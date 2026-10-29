use std::fs;
use std::path::{Path, PathBuf};

use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{Rule, builtin_rules, expand_targets, load};
use devsweep::scan::scan;

const MB: u64 = 1024 * 1024;

fn write(path: &Path, len: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; len]).unwrap();
}

fn extra_rule(id: &'static str, rel: &'static str) -> Rule {
    Rule {
        id: id.to_string(),
        stack: None,
        rel: rel.to_string(),
        label: None,
        default_checked: true,
        dangerous: false,
    }
}

#[test]
fn an_extra_rule_uses_the_same_path_segments_at_discovery_and_final_planning() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    write(&root.join("app/package.json"), 0);
    write(&root.join("app/artifacts/main.cache"), 7);
    write(&root.join("app/artifacts/nested/keep.cache"), 11);
    let overrides = root.join("rules.toml");
    fs::write(
        &overrides,
        "[[extra]]\nstack = \"node\"\npath = \"artifacts//*.cache\"\n",
    )
    .unwrap();
    let loaded = load(Some(&overrides));
    assert!(loaded.error.is_none());
    let projects = scan(std::slice::from_ref(&root));
    let project = &projects[0];
    let mut targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, project.path.join("artifacts/main.cache"));
    targets[0].size = Some(7);
    let mut nested = targets[0].clone();
    nested.path = project.path.join("artifacts/nested/keep.cache");
    nested.size = Some(11);
    targets.push(nested);

    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &loaded.rules,
    );

    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.total_bytes, 7);
    assert_eq!(delete_plan.skipped.len(), 1);
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
    assert_eq!(
        delete_plan.skipped[0].path,
        project.path.join("artifacts/nested/keep.cache")
    );
}

#[test]
fn a_file_rule_is_plannable_and_counts_its_own_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    write(&root.join("app/pubspec.yaml"), 0);
    write(&root.join("app/.flutter-plugins-dependencies"), 1_200);
    write(&root.join("app/lib/main.dart"), 0);

    let projects = scan(&[root.clone()]);
    let project = projects
        .iter()
        .find(|project| project.name == "app")
        .expect("app is discovered");
    let mut targets = expand_targets(project, &builtin_rules());
    let target = targets
        .iter_mut()
        .find(|target| target.rule_id == "flutter.plugins_dependencies")
        .expect("the file rule expands");
    target.size = Some(1_200);

    let plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[root.clone()],
        &builtin_rules(),
    );

    assert_eq!(
        plan.items
            .iter()
            .map(|item| item.target_path.clone())
            .collect::<Vec<_>>(),
        vec![project.path.join(".flutter-plugins-dependencies")]
    );
    assert_eq!(plan.total_bytes, 1_200);
    assert!(plan.skipped.is_empty(), "{:?}", plan.skipped);
}

#[test]
fn plan_lists_checked_targets_and_rejects_escaping_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    // Node project with two cleanable targets and a risky default-off one.
    write(&root.join("app/package.json"), 0);
    write(
        &root.join("app/node_modules/react/index.js"),
        2 * MB as usize,
    );
    write(&root.join("app/.next/chunk.js"), 3 * MB as usize);
    write(&root.join("app/dist/bundle.js"), 4 * MB as usize);
    fs::create_dir_all(root.join("app/real")).unwrap();

    // A directory outside the project that rules try to reach.
    write(&root.join("outside/big.bin"), 5 * MB as usize);

    let mut rules = builtin_rules();
    rules.extend([
        extra_rule("test.escape", "../outside"),
        extra_rule("test.link", "escape_link"),
        extra_rule("test.inside_link", "inside_link"),
    ]);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(root.join("outside"), root.join("app/escape_link")).unwrap();
        symlink(root.join("app/real"), root.join("app/inside_link")).unwrap();
    }

    let projects = scan(&[root.clone()]);
    let project = projects
        .iter()
        .find(|project| project.name == "app")
        .expect("app is discovered");

    let mut targets = expand_targets(project, &rules);
    for target in &mut targets {
        target.size = Some(match target.rule_id.as_str() {
            "node.node_modules" => 2 * MB,
            "node.next" => 3 * MB,
            _ => 0,
        });
    }

    let plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[root.clone()],
        &rules,
    );

    let planned: Vec<PathBuf> = plan
        .items
        .iter()
        .map(|item| item.target_path.clone())
        .collect();

    assert!(planned.contains(&project.path.join("node_modules")));
    assert!(planned.contains(&project.path.join(".next")));
    // `dist` is risky and off by default.
    assert!(!planned.contains(&project.path.join("dist")));
    // `../escape` resolves outside the project and must be rejected.
    assert!(!planned.contains(&root.join("outside")));
    assert!(
        plan.skipped.iter().any(|skip| {
            skip.path.ends_with("outside") && skip.reason == SkipReason::OutsideProject
        }),
        "escaping rule should be reported, got {:?}",
        plan.skipped
    );

    #[cfg(unix)]
    {
        assert!(
            plan.skipped.iter().any(|skip| {
                skip.path == project.path.join("escape_link")
                    && skip.reason == SkipReason::OutsideProject
            }),
            "escaping symlink should be reported, got {:?}",
            plan.skipped
        );
        // A symlink inside the project is planned as the link itself, never its target.
        assert!(planned.contains(&project.path.join("inside_link")));
    }

    assert_eq!(plan.total_bytes, 5 * MB);

    // A preview is read-only.
    assert!(project.path.join("dist").is_dir());
    assert!(root.join("outside/big.bin").is_file());
}
