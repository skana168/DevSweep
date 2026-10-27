use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::rules::{Rule, Target, segment_matches};
use crate::scan::Stack;

/// Single targets above this size are flagged so the user gets a second warning.
pub const LARGE_TARGET_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// A selected project's checked targets. Build the plan from these.
pub struct PlanProject<'a> {
    pub path: &'a Path,
    pub stack: Stack,
    pub targets: &'a [Target],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletePlan {
    pub items: Vec<PlanItem>,
    pub skipped: Vec<Skip>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub project_path: PathBuf,
    /// The path as expanded from the rules, so symlinks are removed as links.
    pub target_path: PathBuf,
    pub bytes: u64,
    pub large: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub path: PathBuf,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    OutsideProject,
    ProjectRoot,
    Home,
    ScanRoot,
    Git,
    OutsideRules,
    Missing,
    /// Exists but cannot be resolved to an absolute path.
    Unresolvable,
    Overlap,
}

/// Build a delete plan from the checked targets of `projects`. Pure logic: it
/// reads metadata to validate paths but changes nothing on disk.
pub fn plan(projects: &[PlanProject<'_>], roots: &[PathBuf], rules: &[Rule]) -> DeletePlan {
    let home = home_dir();
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|root| fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
        .collect();

    // (project, display path, canonical path, is symlink, bytes)
    let mut candidates: Vec<(PathBuf, PathBuf, PathBuf, bool, u64)> = Vec::new();
    let mut skipped = Vec::new();

    for project in projects {
        let project_root =
            fs::canonicalize(project.path).unwrap_or_else(|_| project.path.to_path_buf());
        for target in project.targets.iter().filter(|target| target.checked) {
            let reason = match check_target(
                target,
                project,
                &project_root,
                &roots,
                home.as_deref(),
                rules,
            ) {
                Ok((canonical, is_link)) => {
                    candidates.push((
                        project.path.to_path_buf(),
                        target.path.clone(),
                        canonical,
                        is_link,
                        target.size.unwrap_or(0),
                    ));
                    continue;
                }
                Err(reason) => reason,
            };
            skipped.push(Skip {
                path: target.path.clone(),
                reason,
            });
        }
    }

    // Canonical path first so a parent lands before its descendants; a real
    // directory wins over a symlink that resolves to it.
    candidates.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut items: Vec<PlanItem> = Vec::new();
    let mut kept: Vec<PathBuf> = Vec::new();
    for (project_path, target_path, canonical, _, bytes) in candidates {
        if kept.iter().any(|parent| canonical.starts_with(parent)) {
            skipped.push(Skip {
                path: target_path,
                reason: SkipReason::Overlap,
            });
            continue;
        }
        kept.push(canonical);
        items.push(PlanItem {
            project_path,
            target_path,
            bytes,
            large: bytes > LARGE_TARGET_BYTES,
        });
    }

    items.sort_by(|a, b| {
        a.project_path
            .cmp(&b.project_path)
            .then_with(|| a.target_path.cmp(&b.target_path))
    });
    skipped.sort_by(|a, b| a.path.cmp(&b.path));

    let total_bytes = items.iter().map(|item| item.bytes).sum();
    DeletePlan {
        items,
        skipped,
        total_bytes,
    }
}

/// Validate one target, returning its canonical path and whether it is a
/// symlink when it may be deleted.
fn check_target(
    target: &Target,
    project: &PlanProject<'_>,
    project_root: &Path,
    roots: &[PathBuf],
    home: Option<&Path>,
    rules: &[Rule],
) -> Result<(PathBuf, bool), SkipReason> {
    // A symlink is one entry; canonicalizing it only validates where it points.
    let meta = fs::symlink_metadata(&target.path).map_err(|_| SkipReason::Missing)?;
    let is_link = meta.file_type().is_symlink();

    let canonical = fs::canonicalize(&target.path).map_err(|_| SkipReason::Unresolvable)?;
    let Ok(rel) = target.path.strip_prefix(project.path) else {
        return Err(SkipReason::OutsideProject);
    };
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SkipReason::OutsideProject);
    }

    if home.is_some_and(|home| canonical == home) {
        return Err(SkipReason::Home);
    }
    if roots.contains(&canonical) {
        return Err(SkipReason::ScanRoot);
    }
    if canonical == project_root || target.path == project.path {
        return Err(SkipReason::ProjectRoot);
    }
    if !canonical.starts_with(project_root) {
        return Err(SkipReason::OutsideProject);
    }
    if rel
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
    {
        return Err(SkipReason::Git);
    }
    let rule = rules
        .iter()
        .find(|rule| rule.id == target.rule_id)
        .ok_or(SkipReason::OutsideRules)?;
    let stack_applies = rule.stack.is_none_or(|stack| stack == project.stack);
    if !stack_applies || !matches_rule(rel, &rule.rel) {
        return Err(SkipReason::OutsideRules);
    }

    Ok((canonical, is_link))
}

/// One `*` per segment, matching a literal path exactly otherwise. No `**`.
fn matches_rule(rel: &Path, pattern: &str) -> bool {
    let rel: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let pattern: Vec<&str> = pattern.split('/').collect();
    rel.len() == pattern.len()
        && rel
            .iter()
            .zip(&pattern)
            .all(|(segment, pattern)| segment_matches(segment, pattern))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .and_then(|home| fs::canonicalize(home).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(rule_id: &str, path: PathBuf) -> Target {
        Target {
            rule_id: rule_id.to_string(),
            path,
            size: Some(1_000),
            checked: true,
            dangerous: false,
            label: None,
        }
    }

    fn rule(id: &str, rel: &str) -> Rule {
        Rule {
            id: id.to_string(),
            stack: None,
            rel: rel.to_string(),
            label: None,
            default_checked: true,
            dangerous: false,
        }
    }

    fn reasons(plan: &DeletePlan) -> Vec<(String, SkipReason)> {
        plan.skipped
            .iter()
            .map(|skip| (skip.path.display().to_string(), skip.reason))
            .collect()
    }

    #[test]
    fn rule_patterns_match_exactly_one_level_of_wildcards() {
        assert!(matches_rule(Path::new("build"), "build"));
        assert!(matches_rule(Path::new("app/build"), "*/build"));
        assert!(!matches_rule(Path::new("app/app/build"), "*/build"));
        assert!(matches_rule(
            Path::new("App.xcodeproj/xcuserdata"),
            "*.xcodeproj/xcuserdata"
        ));
        assert!(!matches_rule(
            Path::new("nested/App.xcodeproj/xcuserdata"),
            "*.xcodeproj/xcuserdata"
        ));
    }

    #[test]
    fn protected_paths_are_skipped_and_files_are_plannable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("package.json"), "").unwrap();

        let rules = [rule("test.git", ".git"), rule("test.file", "package.json")];
        let targets = vec![
            target("test.git", root.join(".git")),
            target("test.file", root.join("package.json")),
            target("test.file", root.clone()),
        ];
        let plan = plan(
            &[PlanProject {
                path: &root,
                stack: Stack::Node,
                targets: &targets,
            }],
            std::slice::from_ref(&root),
            &rules,
        );

        // A scan root is rejected before the project-root check.
        assert_eq!(
            reasons(&plan),
            vec![
                (root.display().to_string(), SkipReason::ScanRoot),
                (root.join(".git").display().to_string(), SkipReason::Git),
            ]
        );
        // A Target may be a file; the whole length counts.
        assert_eq!(
            plan.items
                .iter()
                .map(|item| item.target_path.clone())
                .collect::<Vec<_>>(),
            vec![root.join("package.json")]
        );
        assert_eq!(plan.total_bytes, 1_000);
    }

    #[test]
    fn targets_outside_the_rules_or_the_project_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        fs::create_dir_all(project.join("secret")).unwrap();
        fs::create_dir_all(project.join("node_modules")).unwrap();

        let targets = vec![
            target("node.node_modules", project.join("node_modules")),
            target("node.node_modules", project.join("secret")),
        ];
        let plan = plan(
            &[PlanProject {
                path: &project,
                stack: Stack::Node,
                targets: &targets,
            }],
            &[dir.path().to_path_buf()],
            &crate::rules::builtin_rules(),
        );

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].target_path, project.join("node_modules"));
        assert_eq!(
            reasons(&plan),
            vec![(
                project.join("secret").display().to_string(),
                SkipReason::OutsideRules
            )]
        );
    }

    #[test]
    fn nested_targets_are_pruned_and_duplicates_reported() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        fs::create_dir_all(project.join("sub/deep")).unwrap();
        fs::create_dir_all(project.join("other")).unwrap();

        let rules = [rule("test.sub", "sub"), rule("test.deep", "sub/deep")];
        let targets = vec![
            target("test.sub", project.join("sub")),
            target("test.deep", project.join("sub/deep")),
            target("test.deep", project.join("sub/deep")),
            target("test.sub", project.join("other")),
        ];
        // `other` does not match a rule, so it is skipped for that reason instead.
        let plan = plan(
            &[PlanProject {
                path: &project,
                stack: Stack::Node,
                targets: &targets,
            }],
            &[dir.path().to_path_buf()],
            &rules,
        );

        assert_eq!(
            plan.items
                .iter()
                .map(|item| item.target_path.clone())
                .collect::<Vec<_>>(),
            vec![project.join("sub")]
        );
        assert_eq!(
            reasons(&plan),
            vec![
                (
                    project.join("other").display().to_string(),
                    SkipReason::OutsideRules
                ),
                (
                    project.join("sub/deep").display().to_string(),
                    SkipReason::Overlap
                ),
                (
                    project.join("sub/deep").display().to_string(),
                    SkipReason::Overlap
                ),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_outside_the_project_are_rejected_and_inside_ones_kept_as_links() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        let outside = dir.path().join("outside");
        fs::create_dir_all(project.join("real")).unwrap();
        fs::create_dir_all(project.join("plain")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, project.join("escape")).unwrap();
        symlink(project.join("plain"), project.join("inside")).unwrap();
        symlink(project.join("real"), project.join("twin")).unwrap();

        let rules = [
            rule("test.escape", "escape"),
            rule("test.inside", "inside"),
            rule("test.real", "real"),
            rule("test.twin", "twin"),
        ];
        let targets = vec![
            target("test.escape", project.join("escape")),
            target("test.inside", project.join("inside")),
            target("test.real", project.join("real")),
            target("test.twin", project.join("twin")),
        ];
        let plan = plan(
            &[PlanProject {
                path: &project,
                stack: Stack::Node,
                targets: &targets,
            }],
            &[dir.path().to_path_buf()],
            &rules,
        );

        assert_eq!(
            plan.items
                .iter()
                .map(|item| item.target_path.clone())
                .collect::<Vec<_>>(),
            vec![project.join("inside"), project.join("real")]
        );
        assert_eq!(
            reasons(&plan),
            vec![
                (
                    project.join("escape").display().to_string(),
                    SkipReason::OutsideProject
                ),
                (
                    project.join("twin").display().to_string(),
                    SkipReason::Overlap
                ),
            ]
        );
    }

    #[test]
    fn missing_targets_are_reported_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        fs::create_dir_all(&project).unwrap();

        let rules = [rule("test.gone", "gone")];
        let targets = vec![target("test.gone", project.join("gone"))];
        let plan = plan(
            &[PlanProject {
                path: &project,
                stack: Stack::Node,
                targets: &targets,
            }],
            &[dir.path().to_path_buf()],
            &rules,
        );

        assert_eq!(
            reasons(&plan),
            vec![(
                project.join("gone").display().to_string(),
                SkipReason::Missing
            )]
        );
    }

    #[test]
    fn large_targets_are_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("app");
        fs::create_dir_all(project.join("huge")).unwrap();
        fs::create_dir_all(project.join("small")).unwrap();

        let rules = [rule("test.huge", "huge"), rule("test.small", "small")];
        let mut huge = target("test.huge", project.join("huge"));
        huge.size = Some(LARGE_TARGET_BYTES + 1);
        let targets = vec![huge, target("test.small", project.join("small"))];

        let plan = plan(
            &[PlanProject {
                path: &project,
                stack: Stack::Node,
                targets: &targets,
            }],
            &[dir.path().to_path_buf()],
            &rules,
        );

        assert_eq!(plan.items.len(), 2);
        assert!(plan.items[0].large);
        assert!(!plan.items[1].large);
        assert_eq!(
            plan.total_bytes,
            LARGE_TARGET_BYTES + 1_001,
            "both checked targets count"
        );
    }
}
