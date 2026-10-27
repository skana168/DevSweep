use std::fs;
use std::path::{Path, PathBuf};

use crate::scan::{Project, Stack};

#[derive(Clone)]
pub struct Rule {
    pub id: &'static str,
    /// `None` applies to every stack.
    pub stack: Option<Stack>,
    /// Literal relative path, with one `*` allowed inside a segment.
    pub rel: &'static str,
    pub default_checked: bool,
    pub dangerous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub rule_id: &'static str,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub checked: bool,
    pub dangerous: bool,
}

pub fn builtin_rules() -> &'static [Rule] {
    &[
        Rule {
            id: "flutter.build",
            stack: Some(Stack::Flutter),
            rel: "build",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.dart_tool",
            stack: Some(Stack::Flutter),
            rel: ".dart_tool",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.ios_pods",
            stack: Some(Stack::Flutter),
            rel: "ios/Pods",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.ios_ephemeral",
            stack: Some(Stack::Flutter),
            rel: "ios/Flutter/ephemeral",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.android_gradle",
            stack: Some(Stack::Flutter),
            rel: "android/.gradle",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.android_build",
            stack: Some(Stack::Flutter),
            rel: "android/build",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.macos_pods",
            stack: Some(Stack::Flutter),
            rel: "macos/Pods",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "flutter.plugins_dependencies",
            stack: Some(Stack::Flutter),
            rel: ".flutter-plugins-dependencies",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "android.build",
            stack: Some(Stack::Android),
            rel: "build",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "android.module_build",
            stack: Some(Stack::Android),
            rel: "*/build",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "android.gradle",
            stack: Some(Stack::Android),
            rel: ".gradle",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "android.cxx",
            stack: Some(Stack::Android),
            rel: ".cxx",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "android.captures",
            stack: Some(Stack::Android),
            rel: "captures",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "ios.pods",
            stack: Some(Stack::Ios),
            rel: "Pods",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "ios.build",
            stack: Some(Stack::Ios),
            rel: "build",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "ios.xcuserdata",
            stack: Some(Stack::Ios),
            rel: "*.xcodeproj/xcuserdata",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.node_modules",
            stack: Some(Stack::Node),
            rel: "node_modules",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.next",
            stack: Some(Stack::Node),
            rel: ".next",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.out",
            stack: Some(Stack::Node),
            rel: "out",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.nuxt",
            stack: Some(Stack::Node),
            rel: ".nuxt",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.svelte_kit",
            stack: Some(Stack::Node),
            rel: ".svelte-kit",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.turbo",
            stack: Some(Stack::Node),
            rel: ".turbo",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.parcel_cache",
            stack: Some(Stack::Node),
            rel: ".parcel-cache",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.vite",
            stack: Some(Stack::Node),
            rel: ".vite",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.coverage",
            stack: Some(Stack::Node),
            rel: "coverage",
            default_checked: true,
            dangerous: false,
        },
        Rule {
            id: "node.dist",
            stack: Some(Stack::Node),
            rel: "dist",
            default_checked: false,
            dangerous: true,
        },
    ]
}

/// Resolve the rules that apply to `project` into existing paths.
pub fn expand_targets(project: &Project, rules: &[Rule]) -> Vec<Target> {
    let mut targets = Vec::new();
    for rule in rules {
        if rule.stack.is_some_and(|stack| stack != project.stack) {
            continue;
        }
        for path in resolve(&project.path, rule.rel) {
            targets.push(Target {
                rule_id: rule.id,
                path,
                size: None,
                checked: rule.default_checked,
                dangerous: rule.dangerous,
            });
        }
    }
    targets
}

/// One `*` inside a segment, matching a literal name otherwise.
pub(crate) fn segment_matches(name: &str, pattern: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
        None => name == pattern,
    }
}

/// Split `rel` into segments, expanding a single `*` in a segment against
/// directory entries. No `**`, character classes, or regex.
fn resolve(root: &Path, rel: &str) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for segment in rel.split('/') {
        let mut next = Vec::new();
        for base in &current {
            if segment.contains('*') {
                let Ok(entries) = fs::read_dir(base) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if segment_matches(&name, segment) {
                        next.push(base.join(name));
                    }
                }
            } else {
                next.push(base.join(segment));
            }
        }
        current = next;
    }
    current.sort();
    current.retain(|path| fs::symlink_metadata(path).is_ok());
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(root: &Path) -> Project {
        Project {
            path: root.to_path_buf(),
            name: "app".into(),
            stack: Stack::Android,
        }
    }

    #[test]
    fn wildcard_matches_one_level_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app/build")).unwrap();
        fs::create_dir_all(root.join("app/app/build")).unwrap();
        fs::create_dir_all(root.join("app/build/nested")).unwrap();

        let paths: Vec<String> = resolve(root, "*/build")
            .iter()
            .map(|path| path.strip_prefix(root).unwrap().display().to_string())
            .collect();

        assert_eq!(paths, vec!["app/build"]);
    }

    #[test]
    fn expansion_skips_missing_paths_and_applies_default_checked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app/build")).unwrap();
        fs::create_dir_all(root.join("app/.gradle")).unwrap();
        fs::create_dir_all(root.join("app/dist")).unwrap();

        let mut node = project(root);
        node.stack = Stack::Node;
        fs::create_dir_all(root.join("node_modules")).unwrap();

        let targets = expand_targets(&node, builtin_rules());

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].rule_id, "node.node_modules");
        assert!(targets[0].checked);
    }

    #[test]
    fn expansion_carries_dangerous_flag_and_its_default_checked_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();

        let mut node = project(root);
        node.stack = Stack::Node;

        let targets = expand_targets(&node, builtin_rules());
        let find = |id: &str| targets.iter().find(|target| target.rule_id == id).unwrap();

        assert!(find("node.node_modules").checked);
        assert!(!find("node.node_modules").dangerous);
        assert!(!find("node.dist").checked);
        assert!(find("node.dist").dangerous);
        assert!(targets.iter().all(|target| target.size.is_none()));
    }

    #[test]
    fn xcodeproj_wildcard_resolves_xcuserdata() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("App.xcodeproj/xcuserdata")).unwrap();
        fs::create_dir_all(root.join("Other.xcodeproj/xcuserdata")).unwrap();
        fs::create_dir_all(root.join("nested/Deep.xcodeproj/xcuserdata")).unwrap();

        let mut ios = project(root);
        ios.stack = Stack::Ios;

        let paths: Vec<PathBuf> = expand_targets(&ios, builtin_rules())
            .into_iter()
            .filter(|target| target.rule_id == "ios.xcuserdata")
            .map(|target| target.path)
            .collect();

        // `nested/Deep.xcodeproj/xcuserdata` is one level too deep and must not match.
        assert_eq!(
            paths,
            vec![
                root.join("App.xcodeproj/xcuserdata"),
                root.join("Other.xcodeproj/xcuserdata")
            ]
        );
    }
}
