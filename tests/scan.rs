use devsweep::scan::{Stack, merge_scan, retain_scanned, scan};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn write(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

#[test]
fn marker_priority_is_flutter_then_ios_then_android_then_node() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write(&root.join("flutter/pubspec.yaml"));
    write(&root.join("flutter/package.json"));

    write(&root.join("ios/App.xcodeproj/project.pbxproj"));
    write(&root.join("ios/package.json"));
    write(&root.join("ios/build.gradle"));

    write(&root.join("android/settings.gradle.kts"));
    write(&root.join("android/package.json"));

    write(&root.join("node/package.json"));

    let projects = scan(&[root.to_path_buf()]);
    let stacks: Vec<(&str, Stack)> = projects
        .iter()
        .map(|p| (p.name.as_str(), p.stack))
        .collect();

    assert_eq!(
        stacks,
        vec![
            ("android", Stack::Android),
            ("flutter", Stack::Flutter),
            ("ios", Stack::Ios),
            ("node", Stack::Node),
        ]
    );
}

#[test]
fn every_pruned_directory_is_skipped() {
    let pruned = [
        "node_modules",
        ".git",
        ".next",
        ".opencode",
        "build",
        "Pods",
        ".dart_tool",
        "target",
        "dist",
        ".gradle",
        "vendor",
    ];
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write(&root.join("app/package.json"));
    for name in pruned {
        write(&root.join(format!("app/{name}/package.json")));
    }

    let projects = scan(&[root.to_path_buf()]);

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "app");
}

#[test]
fn a_root_named_like_a_pruned_directory_is_still_scanned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("build");
    write(&root.join("package.json"));

    let projects = scan(&[root]);

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "build");
    assert_eq!(projects[0].stack, Stack::Node);
}

#[test]
fn platform_subdirs_of_registered_projects_do_not_get_their_own_row() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write(&root.join("app/pubspec.yaml"));
    write(&root.join("app/android/settings.gradle.kts"));
    write(&root.join("app/android/app/build.gradle"));
    write(&root.join("app/ios/Runner.xcodeproj/project.pbxproj"));

    let projects = scan(&[root.to_path_buf()]);

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "app");
    assert_eq!(projects[0].stack, Stack::Flutter);
}

#[test]
fn platform_merge_does_not_depend_on_root_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("solo/pubspec.yaml"));
    write(&root.join("solo/android/settings.gradle.kts"));

    let child_first = scan(&[root.join("solo/android"), root.join("solo")]);
    let parent_first = scan(&[root.join("solo"), root.join("solo/android")]);

    assert_eq!(child_first.len(), 1);
    assert_eq!(child_first[0].name, "solo");
    assert_eq!(parent_first, child_first);
}

#[test]
fn depth_is_limited_to_five_levels_below_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write(&root.join("a/b/c/d/e/package.json"));
    write(&root.join("a/b/c/d/e/f/package.json"));

    let projects = scan(&[root.to_path_buf()]);

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "e");
}

#[test]
fn rescan_keeps_selection_for_survivors_and_drops_vanished_projects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("alpha/package.json"));
    write(&root.join("beta/package.json"));

    let first = scan(&[root.to_path_buf()]);
    let mut selected: HashMap<PathBuf, bool> =
        first.iter().map(|p| (p.path.clone(), true)).collect();

    fs::remove_dir_all(root.join("beta")).unwrap();
    let second = scan(&[root.to_path_buf()]);
    retain_scanned(&mut selected, &second);

    assert_eq!(selected.len(), 1);
    assert_eq!(selected.get(&first[0].path), Some(&true));
}

#[test]
fn merging_a_new_root_matches_scanning_both_roots_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    write(&one.join("alpha/package.json"));
    write(&two.join("beta/package.json"));

    let mut projects = scan(std::slice::from_ref(&one));
    merge_scan(&mut projects, scan(std::slice::from_ref(&two)));

    assert_eq!(projects, scan(&[one, two]));
}

#[test]
fn rescanning_a_surviving_child_root_restores_a_suppressed_platform_subdir() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("app");
    let child = parent.join("android");
    write(&parent.join("pubspec.yaml"));
    write(&child.join("settings.gradle.kts"));

    let mut projects = scan(&[parent, child.clone()]);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].stack, Stack::Flutter);

    // The UI removes the parent root, keeps projects under the child, then re-walks it.
    projects.retain(|project| project.path.starts_with(&child));
    merge_scan(&mut projects, scan(std::slice::from_ref(&child)));

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].stack, Stack::Android);
}

#[test]
fn scan_finds_realistic_tree_with_types_and_sorted_names() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Flutter app with platform scaffolding.
    write(&root.join("apps/solo/pubspec.yaml"));
    write(&root.join("apps/solo/android/settings.gradle.kts"));
    write(&root.join("apps/solo/ios/Runner.xcodeproj/project.pbxproj"));

    // Node app with dependencies.
    write(&root.join("apps/web/package.json"));
    write(&root.join("apps/web/node_modules/react/package.json"));

    // Android app.
    write(&root.join("apps/mobile/settings.gradle"));

    // Node monorepo: parent and children each count.
    write(&root.join("monorepo/package.json"));
    write(&root.join("monorepo/packages/api/package.json"));
    write(&root.join("monorepo/packages/ui/pubspec.yaml"));

    let projects = scan(&[root.to_path_buf()]);
    let found: Vec<(String, Stack)> = projects
        .iter()
        .map(|project| (project.name.clone(), project.stack))
        .collect();

    assert_eq!(
        found,
        vec![
            ("api".to_string(), Stack::Node),
            ("mobile".to_string(), Stack::Android),
            ("monorepo".to_string(), Stack::Node),
            ("solo".to_string(), Stack::Flutter),
            ("ui".to_string(), Stack::Flutter),
            ("web".to_string(), Stack::Node),
        ]
    );
}

#[test]
fn maven_modules_are_java_projects_of_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Aggregator plus two modules, each with its own build output.
    write(&root.join("repo/pom.xml"));
    write(&root.join("repo/target/generated/pom.properties"));
    write(&root.join("repo/core/pom.xml"));
    write(&root.join("repo/core/target/classes/App.class"));
    write(&root.join("repo/service/pom.xml"));
    write(&root.join("repo/service/target/service.jar"));

    let projects = scan(&[root.to_path_buf()]);
    let found: Vec<(&str, Stack)> = projects
        .iter()
        .map(|project| (project.name.as_str(), project.stack))
        .collect();

    assert_eq!(
        found,
        vec![
            ("core", Stack::Java),
            ("repo", Stack::Java),
            ("service", Stack::Java),
        ]
    );

    for project in &projects {
        let targets = devsweep::rules::expand_targets(project, &devsweep::rules::builtin_rules());
        assert_eq!(
            targets.len(),
            1,
            "{} should only see its own target",
            project.name
        );
        assert_eq!(targets[0].rule_id, "java.target");
        assert_eq!(targets[0].path, project.path.join("target"));
    }
}
