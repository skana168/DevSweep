use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

#[test]
fn dotnet_outputs_are_independent_dangerous_targets_and_only_selected_output_is_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    for name in [
        "bin/Release",
        "obj/Debug",
        "src",
        "artifacts/bin",
        "nested/bin",
        ".vs",
        "TestResults",
        "Assets",
        "Library",
    ] {
        fs::create_dir_all(app.join(name)).unwrap();
    }
    fs::write(app.join("bin/Release/app.dll"), b"123").unwrap();
    fs::write(app.join("bin/manual.txt"), b"45").unwrap();
    fs::write(app.join("obj/Debug/generated"), b"6789").unwrap();
    let kept = [
        "app.csproj",
        "src/Program.cs",
        "artifacts/bin/app.dll",
        "nested/bin/manual",
        ".vs/settings",
        "TestResults/result",
        "Assets/resource",
        "Library/cache",
    ];
    for name in kept {
        fs::write(app.join(name), "keep").unwrap();
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(
        targets
            .iter()
            .map(|t| t.rule_id.as_str())
            .collect::<Vec<_>>(),
        ["dotnet.bin", "dotnet.obj"]
    );
    assert_eq!(total_size(&targets), None);
    for target in &mut targets {
        assert!(target.dangerous && !target.checked && !target.default_checked);
        target.size = Some(dir_size(&target.path));
    }
    assert_eq!(total_size(&targets), Some(9));
    let make_plan = |targets: &[_]| {
        plan(
            &[PlanProject {
                path: &project.path,
                stack: project.stack,
                targets,
            }],
            std::slice::from_ref(&root),
            &rules,
        )
    };
    assert!(make_plan(&targets).items.is_empty());
    targets[0].checked = true;
    let delete_plan = make_plan(&targets);
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.items[0].target_path, app.join("bin"));
    assert_eq!(delete_plan.total_bytes, 5);
    assert!(delete_plan.skipped.is_empty());
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 5);
    assert!(!app.join("bin").exists());
    assert!(app.join("obj/Debug/generated").is_file());
    for name in kept {
        assert!(app.join(name).is_file(), "{name}");
    }
}

#[test]
fn dotnet_outputs_require_real_directories_at_discovery_and_final_planning() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("app.csproj"), "").unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    for name in ["bin", "obj"] {
        fs::write(root.join(name), "keep").unwrap();
    }
    assert!(expand_targets(project, &rules).is_empty());
    for name in ["bin", "obj"] {
        fs::remove_file(root.join(name)).unwrap();
        fs::create_dir(root.join(name)).unwrap();
    }
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 2);
    for target in &mut targets {
        target.checked = true;
        fs::remove_dir(&target.path).unwrap();
        fs::write(&target.path, "replacement").unwrap();
    }
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped.len(), 2);
    assert!(
        delete_plan
            .skipped
            .iter()
            .all(|skip| skip.reason == SkipReason::OutsideRules)
    );
}

#[test]
fn dotnet_output_names_are_exactly_lowercase_even_on_case_insensitive_filesystems() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("app.fsproj"), "").unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    for name in ["BIN", "OBJ"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    assert!(expand_targets(project, &rules).is_empty());
    for (upper, lower) in [("BIN", "bin"), ("OBJ", "obj")] {
        fs::rename(root.join(upper), root.join("rename-temp")).unwrap();
        fs::rename(root.join("rename-temp"), root.join(lower)).unwrap();
    }
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 2);
    for target in &mut targets {
        target.checked = true;
        let upper = target
            .path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_uppercase();
        fs::rename(&target.path, root.join("rename-temp")).unwrap();
        fs::rename(root.join("rename-temp"), root.join(upper)).unwrap();
    }
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped.len(), 2);
}

#[test]
fn dotnet_rule_overrides_keep_danger_and_validation_while_extras_keep_file_support() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("app.vbproj"), "").unwrap();
    for name in ["bin", "obj"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(root.join(".custom"), "extra file").unwrap();
    let config = root.join("rules.toml");
    fs::write(&config, "disable = ['dotnet.obj']\n[defaults]\n'dotnet.bin' = true\n[[extra]]\nstack = 'dotnet'\npath = '.custom'\n").unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none(), "{:?}", loaded.error);
    let project = &scan(std::slice::from_ref(&root))[0];
    let targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 2);
    let builtin = targets
        .iter()
        .find(|target| target.rule_id == "dotnet.bin")
        .unwrap();
    assert!(builtin.checked && builtin.default_checked && builtin.dangerous);
    let extra = targets
        .iter()
        .find(|target| target.rule_id == "extra.0")
        .unwrap();
    assert!(extra.checked && !extra.dangerous && extra.path.is_file());
    fs::remove_dir(root.join("bin")).unwrap();
    fs::write(root.join("bin"), "replacement").unwrap();
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
    assert_eq!(delete_plan.items[0].target_path, root.join(".custom"));
    assert_eq!(delete_plan.skipped.len(), 1);
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
    fs::write(root.join("package.json"), "").unwrap();
    let node = &scan(std::slice::from_ref(&root))[0];
    assert!(expand_targets(node, &loaded.rules).is_empty());
}

#[cfg(unix)]
#[test]
fn dotnet_output_links_are_rejected_at_discovery_and_after_selection() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("app.csproj"), "").unwrap();
    fs::create_dir(root.join("keep")).unwrap();
    fs::write(root.join("keep/manual"), "keep").unwrap();
    for name in ["bin", "obj"] {
        symlink(root.join("keep"), root.join(name)).unwrap();
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    assert!(expand_targets(project, &rules).is_empty());
    for name in ["bin", "obj"] {
        fs::remove_file(root.join(name)).unwrap();
        fs::create_dir(root.join(name)).unwrap();
    }
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 2);
    for target in &mut targets {
        target.checked = true;
        fs::remove_dir(&target.path).unwrap();
        symlink(root.join("keep"), &target.path).unwrap();
    }
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped.len(), 2);
    assert!(
        delete_plan
            .skipped
            .iter()
            .all(|skip| skip.reason == SkipReason::OutsideRules)
    );
    assert_eq!(fs::read(root.join("keep/manual")).unwrap(), b"keep");
}

#[test]
fn dotnet_plans_keep_protected_path_and_overlap_checks() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("app.csproj"), "").unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    targets[0].checked = true;
    targets.push(targets[0].clone());
    let selected = [PlanProject {
        path: &project.path,
        stack: project.stack,
        targets: &targets,
    }];
    let delete_plan = plan(&selected, std::slice::from_ref(&root), &rules);
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.skipped.len(), 1);
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::Overlap);
    let protected = plan(&selected, &[root.clone(), root.join("bin")], &rules);
    assert!(protected.items.is_empty());
    assert!(
        protected
            .skipped
            .iter()
            .all(|skip| skip.reason == SkipReason::ScanRoot)
    );
}
