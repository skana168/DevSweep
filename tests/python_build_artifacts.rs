use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

#[test]
fn python_build_artifacts_are_dangerous_and_only_selected_paths_are_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    for name in ["build", "dist", "app.egg-info", "src/app.egg-info"] {
        fs::create_dir_all(app.join(name)).unwrap();
        fs::write(app.join(name).join("artifact"), b"123").unwrap();
    }
    for name in [
        "nested/build",
        "nested/dist",
        "nested/app.egg-info",
        "src/deep/app.egg-info",
    ] {
        fs::create_dir_all(app.join(name)).unwrap();
        fs::write(app.join(name).join("keep"), b"keep").unwrap();
    }
    for name in ["pyproject.toml", "src/module.so", "src/module.pyd"] {
        fs::write(app.join(name), "keep").unwrap();
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 4);
    assert!(
        targets
            .iter()
            .all(|t| t.dangerous && !t.checked && !t.default_checked)
    );
    for target in &mut targets {
        target.size = Some(dir_size(&target.path));
    }
    assert_eq!(total_size(&targets), Some(12));
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
    targets
        .iter_mut()
        .find(|t| t.path == app.join("build"))
        .unwrap()
        .checked = true;
    let delete_plan = make_plan(&targets);
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.total_bytes, 3);
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 3);
    assert!(!app.join("build").exists());
    for name in [
        "dist/artifact",
        "app.egg-info/artifact",
        "src/app.egg-info/artifact",
        "nested/build/keep",
        "nested/dist/keep",
        "nested/app.egg-info/keep",
        "src/deep/app.egg-info/keep",
        "pyproject.toml",
        "src/module.so",
        "src/module.pyd",
    ] {
        assert!(app.join(name).is_file(), "{name}");
    }
}

#[test]
fn python_build_overrides_preserve_danger_and_skip_same_named_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join("build")).unwrap();
    fs::create_dir_all(app.join("dist")).unwrap();
    fs::write(app.join("setup.py"), "").unwrap();
    fs::write(app.join("file.egg-info"), "keep").unwrap();
    let config = root.join("rules.toml");
    fs::write(
        &config,
        "disable = ['python.dist']\n[defaults]\n'python.build' = true\n",
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none());
    let project = &scan(std::slice::from_ref(&root))[0];
    let targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "python.build");
    assert!(targets[0].checked && targets[0].default_checked && targets[0].dangerous);
    fs::remove_dir(app.join("build")).unwrap();
    fs::write(app.join("build"), "replaced with a file").unwrap();
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &loaded.rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
    assert!(app.join("build").is_file());
}

#[cfg(unix)]
#[test]
fn python_build_plan_rejects_target_and_src_directory_symlink_swaps() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join("build")).unwrap();
    fs::create_dir_all(app.join("src/app.egg-info")).unwrap();
    fs::create_dir_all(app.join("keep/app.egg-info")).unwrap();
    fs::write(app.join("requirements.txt"), "").unwrap();
    symlink(app.join("keep"), app.join("dist")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 2);
    for target in &mut targets {
        target.checked = true;
    }
    fs::remove_dir(app.join("build")).unwrap();
    symlink(app.join("keep"), app.join("build")).unwrap();
    fs::remove_dir_all(app.join("src")).unwrap();
    symlink(app.join("keep"), app.join("src")).unwrap();
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
            .all(|s| s.reason == SkipReason::OutsideRules)
    );
    assert!(expand_targets(project, &rules).is_empty());
    assert!(app.join("keep/app.egg-info").is_dir());
}
