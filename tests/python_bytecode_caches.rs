use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn bytecode_caches_beyond_scan_depth_can_be_measured_and_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    write(&app.join("pyproject.toml"), b"");
    write(&app.join("src/main.py"), b"keep");
    let cache_paths = [
        app.join("__pycache__"),
        app.join("src/pkg/__pycache__"),
        app.join("a/b/c/d/e/f/g/__pycache__"),
    ];
    for path in &cache_paths {
        write(&path.join("module.pyc"), b"cache");
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 3);
    assert!(targets.iter().all(|target| {
        target.rule_id == "python.pycache"
            && target.checked
            && target.default_checked
            && !target.dangerous
    }));
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
    assert!(cache_paths.iter().all(|path| !path.exists()));
    assert!(app.join("pyproject.toml").is_file());
    assert!(app.join("src/main.py").is_file());
}

#[test]
fn bytecode_caches_follow_project_environment_and_platform_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    write(&app.join("pyproject.toml"), b"");
    for rel in [
        "src/__pycache__/module.pyc",
        "env/__pycache__/module.pyc",
        "android/lib/deep/__pycache__/module.pyc",
        "packages/child/__pycache__/module.pyc",
        "tools/__pycache__/module.pyc",
        "a/b/c/d/e/f/nested/__pycache__/module.pyc",
        "custom-env/lib/pkg/__pycache__/module.pyc",
        "build/__pycache__/module.pyc",
    ] {
        write(&app.join(rel), b"cache");
    }
    write(&app.join("android/lib/deep/package.json"), b"");
    write(&app.join("packages/child/setup.py"), b"");
    write(&app.join("tools/package.json"), b"");
    write(&app.join("a/b/c/d/e/f/nested/Pipfile"), b"");
    write(&app.join("custom-env/pyvenv.cfg"), b"");
    let rules = builtin_rules();
    let projects = scan(std::slice::from_ref(&root));
    let project = projects.iter().find(|project| project.path == app).unwrap();
    assert!(!projects.iter().any(|project| project.name == "nested"));
    let targets: Vec<_> = expand_targets(project, &rules)
        .into_iter()
        .filter(|target| target.rule_id == "python.pycache")
        .collect();
    let mut paths: Vec<_> = targets.iter().map(|target| target.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            app.join("android/lib/deep/__pycache__"),
            app.join("env/__pycache__"),
            app.join("src/__pycache__"),
        ]
    );
    let child = projects
        .iter()
        .find(|project| project.name == "child")
        .unwrap();
    assert_eq!(expand_targets(child, &rules).len(), 1);
    let deep_root = app.join("a/b/c/d/e/f/nested");
    let deep_project = &scan(std::slice::from_ref(&deep_root))[0];
    assert_eq!(expand_targets(deep_project, &rules).len(), 1);
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
    assert!(delete_plan.skipped.is_empty());
}

#[test]
fn a_bytecode_cache_is_one_target_including_all_its_contents() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    write(&root.join("setup.py"), b"");
    write(&root.join("__pycache__/nested/__pycache__/cache"), b"cache");
    let project = &scan(std::slice::from_ref(&root))[0];
    let targets = expand_targets(project, &builtin_rules());
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, root.join("__pycache__"));
}

#[test]
fn final_plan_rechecks_environment_and_independent_project_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    write(&root.join("requirements.txt"), b"");
    for rel in ["source", "new-env", "tools", "android/deep"] {
        write(&root.join(rel).join("__pycache__/module.pyc"), b"cache");
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 4);
    write(&root.join("source/Pipfile"), b"");
    write(&root.join("new-env/pyvenv.cfg"), b"");
    write(&root.join("tools/pom.xml"), b"");
    write(&root.join("android/deep/package.json"), b"");
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(
        delete_plan.items[0].target_path,
        root.join("android/deep/__pycache__")
    );
    assert_eq!(delete_plan.skipped.len(), 3);
    assert!(
        delete_plan
            .skipped
            .iter()
            .all(|skip| skip.reason == SkipReason::OutsideRules)
    );
}

#[test]
fn bytecode_rules_can_be_disabled_or_unchecked_without_extending_extra_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    write(&root.join("pyproject.toml"), b"");
    write(&root.join("__pycache__/root.pyc"), b"root");
    write(&root.join("src/pkg/__pycache__/deep.pyc"), b"deep");
    let project = &scan(std::slice::from_ref(&root))[0];
    let config = root.join("rules.toml");
    fs::write(&config, "disable = ['python.pycache']\n").unwrap();
    let disabled = load(Some(&config));
    assert!(disabled.error.is_none());
    assert!(expand_targets(project, &disabled.rules).is_empty());
    fs::write(&config, "[defaults]\n'python.pycache' = false\n").unwrap();
    let unchecked = load(Some(&config));
    let targets = expand_targets(project, &unchecked.rules);
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .all(|target| !target.checked && !target.default_checked)
    );
    fs::write(
        &config,
        "disable = ['python.pycache']\n[[extra]]\nstack = 'python'\npath = '*/__pycache__'\n",
    )
    .unwrap();
    let extra = load(Some(&config));
    assert!(extra.error.is_none());
    assert!(expand_targets(project, &extra.rules).is_empty());
}

#[cfg(unix)]
#[test]
fn bytecode_discovery_and_final_plan_reject_cache_and_ancestor_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    write(&root.join("pyproject.toml"), b"");
    write(&root.join("real/__pycache__/module.pyc"), b"cache");
    symlink(root.join("real"), root.join("linked")).unwrap();
    symlink(root.join("real/__pycache__"), root.join("__pycache__")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, root.join("real/__pycache__"));
    fs::rename(root.join("real"), root.join("moved")).unwrap();
    symlink(root.join("moved"), root.join("real")).unwrap();
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);

    fs::remove_file(root.join("real")).unwrap();
    fs::rename(root.join("moved"), root.join("real")).unwrap();
    let targets = expand_targets(project, &rules);
    fs::rename(root.join("real/__pycache__"), root.join("real/kept")).unwrap();
    symlink(root.join("real/kept"), root.join("real/__pycache__")).unwrap();
    let delete_plan = plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert!(delete_plan.items.is_empty());
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
}
