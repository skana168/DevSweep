use devsweep::scan::{Stack, scan};
use std::fs;
use std::path::Path;

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
