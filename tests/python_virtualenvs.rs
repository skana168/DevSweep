use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

#[test]
fn python_virtualenvs_are_dangerous_and_cleaned_only_after_selection() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("pyproject.toml"), "").unwrap();
    fs::write(app.join("main.py"), "keep source").unwrap();
    for (name, bytes) in [(".venv", 3), ("venv", 5), ("env", 7)] {
        fs::create_dir(app.join(name)).unwrap();
        fs::write(app.join(name).join("pyvenv.cfg"), "").unwrap();
        fs::write(app.join(name).join("payload"), vec![0; bytes]).unwrap();
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 3);
    assert!(
        targets
            .iter()
            .all(|t| t.dangerous && !t.checked && !t.default_checked)
    );
    for target in &mut targets {
        target.size = Some(dir_size(&target.path));
    }
    assert_eq!(total_size(&targets), Some(15));
    let selected = |targets: &[_]| {
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
    assert!(selected(&targets).items.is_empty());
    targets
        .iter_mut()
        .find(|t| t.path == app.join(".venv"))
        .unwrap()
        .checked = true;
    let delete_plan = selected(&targets);
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.total_bytes, 3);
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 3);
    assert!(!app.join(".venv").exists());
    assert!(app.join("venv/payload").is_file());
    assert!(app.join("env/payload").is_file());
    assert!(app.join("main.py").is_file());
}

#[test]
fn virtualenv_identity_is_rechecked_when_the_final_plan_is_built() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join(".venv")).unwrap();
    fs::write(app.join("setup.py"), "").unwrap();
    fs::write(app.join(".venv/pyvenv.cfg"), "").unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    targets[0].checked = true;
    fs::remove_file(app.join(".venv/pyvenv.cfg")).unwrap();
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
    assert!(app.join(".venv").is_dir());
}

#[test]
fn virtualenv_cleanup_requires_a_marker_file_and_a_supported_root_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = root.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("requirements.txt"), "").unwrap();
    for name in [
        ".venv",
        "venv",
        "env",
        "custom",
        ".tox",
        ".nox",
        "nested/.venv",
    ] {
        fs::create_dir_all(app.join(name)).unwrap();
    }
    fs::create_dir(app.join(".venv/pyvenv.cfg")).unwrap();
    for name in ["custom", ".tox", ".nox", "nested/.venv"] {
        fs::write(app.join(name).join("pyvenv.cfg"), "").unwrap();
        fs::write(app.join(name).join("setup.py"), "").unwrap();
    }
    let projects = scan(&[root.to_path_buf()]);
    assert_eq!(projects.len(), 1);
    assert!(expand_targets(&projects[0], &builtin_rules()).is_empty());
}

#[test]
fn explicit_virtualenv_defaults_keep_danger_and_identity_checks() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("Pipfile"), "").unwrap();
    for name in [".venv", "venv", "env"] {
        fs::create_dir(app.join(name)).unwrap();
    }
    for name in [".venv", "venv"] {
        fs::write(app.join(name).join("pyvenv.cfg"), "").unwrap();
    }
    let config = root.join("rules.toml");
    fs::write(
        &config,
        "disable = ['python.venv']\n[defaults]\n'python.dot_venv' = true\n'python.env' = true\n",
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none());
    let project = &scan(std::slice::from_ref(&root))[0];
    let targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "python.dot_venv");
    assert!(targets[0].checked && targets[0].default_checked && targets[0].dangerous);
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
    assert_eq!(delete_plan.items[0].target_path, app.join(".venv"));

    let mut misplaced = targets;
    fs::create_dir_all(app.join("nested/.venv")).unwrap();
    fs::write(app.join("nested/.venv/pyvenv.cfg"), "").unwrap();
    misplaced[0].path = app.join("nested/.venv");
    let invalid_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &misplaced,
        }],
        std::slice::from_ref(&root),
        &loaded.rules,
    );
    assert!(invalid_plan.items.is_empty());
    assert_eq!(invalid_plan.skipped[0].reason, SkipReason::OutsideRules);
}

#[cfg(unix)]
#[test]
fn virtualenv_links_are_rejected_at_discovery_and_final_planning() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    fs::create_dir_all(app.join(".venv")).unwrap();
    fs::create_dir(app.join("keep")).unwrap();
    fs::write(app.join("pyproject.toml"), "").unwrap();
    fs::write(app.join(".venv/pyvenv.cfg"), "").unwrap();
    fs::write(app.join("keep/pyvenv.cfg"), "").unwrap();
    symlink(app.join("keep"), app.join("venv")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "python.dot_venv");
    targets[0].checked = true;
    fs::remove_file(app.join(".venv/pyvenv.cfg")).unwrap();
    fs::remove_dir(app.join(".venv")).unwrap();
    symlink(app.join("keep"), app.join(".venv")).unwrap();
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
    assert!(app.join("keep/pyvenv.cfg").is_file());
}
