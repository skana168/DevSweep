use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

#[test]
fn python_root_caches_can_be_measured_planned_and_cleaned_without_touching_sources() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join("src/.pytest_cache")).unwrap();
    fs::write(app.join("pyproject.toml"), "").unwrap();
    fs::write(app.join("src/main.py"), "print('keep')").unwrap();
    fs::write(app.join("src/.pytest_cache/keep"), "nested").unwrap();
    for (name, bytes) in [(".pytest_cache", 3), (".mypy_cache", 5), (".ruff_cache", 7)] {
        fs::create_dir(app.join(name)).unwrap();
        fs::write(app.join(name).join("cache"), vec![0; bytes]).unwrap();
    }
    let projects = scan(std::slice::from_ref(&root));
    let project = &projects[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);

    assert_eq!(targets.len(), 3);
    assert!(
        targets
            .iter()
            .all(|t| t.checked && t.default_checked && !t.dangerous)
    );
    assert_eq!(total_size(&targets), None);
    for target in &mut targets {
        target.size = Some(dir_size(&target.path));
    }
    assert_eq!(total_size(&targets), Some(15));
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        std::slice::from_ref(&root),
        &rules,
    );
    assert_eq!(delete_plan.items.len(), 3);
    assert_eq!(delete_plan.total_bytes, 15);
    assert!(delete_plan.skipped.is_empty());
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 15);
    for name in [".pytest_cache", ".mypy_cache", ".ruff_cache"] {
        assert!(!app.join(name).exists());
    }
    assert!(app.join("pyproject.toml").is_file());
    assert!(app.join("src/main.py").is_file());
    assert!(app.join("src/.pytest_cache/keep").is_file());
}

#[test]
fn python_rules_support_user_overrides_and_stack_scoped_extras() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = root.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("requirements.txt"), "").unwrap();
    for name in [".pytest_cache", ".mypy_cache", ".ruff_cache", ".custom"] {
        fs::create_dir(app.join(name)).unwrap();
    }
    let config = root.join("rules.toml");
    fs::write(
        &config,
        r#"
disable = ["python.pytest_cache"]
[defaults]
"python.mypy_cache" = false
[[extra]]
stack = "python"
path = ".custom"
label = "Custom Python cache"
"#,
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none(), "{:?}", loaded.error);
    let projects = scan(&[root.to_path_buf()]);
    let targets = expand_targets(&projects[0], &loaded.rules);
    assert_eq!(targets.len(), 3);
    assert!(!targets.iter().any(|t| t.rule_id == "python.pytest_cache"));
    let mypy = targets
        .iter()
        .find(|t| t.rule_id == "python.mypy_cache")
        .unwrap();
    assert!(!mypy.checked && !mypy.default_checked && !mypy.dangerous);
    let extra = targets.iter().find(|t| t.rule_id == "extra.0").unwrap();
    assert!(extra.checked && !extra.dangerous);
    assert_eq!(extra.label.as_deref(), Some("Custom Python cache"));

    fs::write(app.join("package.json"), "").unwrap();
    let node = scan(&[root.to_path_buf()]);
    assert!(expand_targets(&node[0], &loaded.rules).is_empty());
}

#[test]
fn python_builtin_caches_require_directories_at_discovery_and_final_planning() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join(".mypy_cache")).unwrap();
    fs::write(app.join("setup.py"), "").unwrap();
    fs::write(app.join(".pytest_cache"), "keep this file").unwrap();
    let projects = scan(std::slice::from_ref(&root));
    let project = &projects[0];
    let rules = builtin_rules();
    let targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "python.mypy_cache");

    fs::remove_dir(app.join(".mypy_cache")).unwrap();
    fs::write(app.join(".mypy_cache"), "replaced with a file").unwrap();
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
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
    assert!(app.join(".mypy_cache").is_file());
}

#[cfg(unix)]
#[test]
fn python_builtin_caches_reject_symlinks_without_changing_extra_rules() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join(".pytest_cache")).unwrap();
    fs::create_dir(app.join("keep")).unwrap();
    fs::write(app.join("Pipfile"), "").unwrap();
    symlink(app.join("keep"), app.join(".mypy_cache")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "python.pytest_cache");

    fs::remove_dir(app.join(".pytest_cache")).unwrap();
    symlink(app.join("keep"), app.join(".pytest_cache")).unwrap();
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
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);

    let config = root.join("rules.toml");
    fs::write(
        &config,
        "[[extra]]\nstack = 'python'\npath = '.mypy_cache'\n",
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none());
    let extras = expand_targets(project, &loaded.rules);
    assert_eq!(extras.len(), 1);
    assert_eq!(extras[0].rule_id, "extra.0");
    let extra_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &extras,
        }],
        std::slice::from_ref(&root),
        &loaded.rules,
    );
    assert_eq!(extra_plan.items.len(), 1);
    assert!(app.join("keep").is_dir());
}
