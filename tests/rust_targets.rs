use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute};
use devsweep::plan::{PlanProject, SkipReason, plan};
use devsweep::rules::{builtin_rules, expand_targets, load};
use devsweep::scan::scan;
use devsweep::size::{dir_size, total_size};

#[test]
fn rust_target_is_dangerous_and_only_cleaned_after_selection() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let app = root.join("app");
    for name in [
        "target/debug",
        "target/release",
        "src",
        ".cargo",
        "custom-output",
        "nested/target",
    ] {
        fs::create_dir_all(app.join(name)).unwrap();
    }
    fs::write(app.join("target/debug/app"), b"123").unwrap();
    fs::write(app.join("target/release/app"), b"12345").unwrap();
    let kept = [
        "Cargo.toml",
        "Cargo.lock",
        "src/main.rs",
        ".cargo/config.toml",
        "custom-output/artifact",
        "nested/target/artifact",
    ];
    for name in kept {
        fs::write(app.join(name), "keep").unwrap();
    }
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "rust.target");
    assert_eq!(targets[0].path, app.join("target"));
    assert!(targets[0].dangerous && !targets[0].checked && !targets[0].default_checked);
    targets[0].size = Some(dir_size(&targets[0].path));
    assert_eq!(total_size(&targets), Some(8));
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
    let unchecked = make_plan(&targets);
    assert!(unchecked.items.is_empty());
    assert_eq!(unchecked.total_bytes, 0);
    targets[0].checked = true;
    let delete_plan = make_plan(&targets);
    assert_eq!(delete_plan.items.len(), 1);
    assert_eq!(delete_plan.total_bytes, 8);
    assert!(delete_plan.skipped.is_empty());
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 8);
    assert!(!app.join("target").exists());
    for name in kept {
        assert!(app.join(name).is_file(), "{name}");
    }
}

#[test]
fn rust_target_requires_a_directory_at_discovery_and_final_planning() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::write(root.join("target"), "keep").unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    assert!(expand_targets(project, &rules).is_empty());

    fs::remove_file(root.join("target")).unwrap();
    fs::create_dir(root.join("target")).unwrap();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    targets[0].checked = true;
    fs::remove_dir(root.join("target")).unwrap();
    fs::write(root.join("target"), "replacement").unwrap();
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
    assert_eq!(delete_plan.skipped.len(), 1);
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);
    assert_eq!(fs::read(root.join("target")).unwrap(), b"replacement");
}

#[test]
fn rust_rule_overrides_preserve_danger_and_scope_extra_rules() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir(root.join("target")).unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::write(root.join(".custom"), "keep file support for extras").unwrap();
    let config = root.join("rules.toml");
    fs::write(
        &config,
        "[defaults]\n'rust.target' = true\n[[extra]]\nstack = 'rust'\npath = '.custom'\n",
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none(), "{:?}", loaded.error);
    let project = &scan(std::slice::from_ref(&root))[0];
    let targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 2);
    let target = targets.iter().find(|t| t.rule_id == "rust.target").unwrap();
    assert!(target.checked && target.default_checked && target.dangerous);
    let extra = targets.iter().find(|t| t.rule_id == "extra.0").unwrap();
    assert!(extra.checked && !extra.dangerous && extra.path.is_file());

    fs::remove_dir(root.join("target")).unwrap();
    fs::write(root.join("target"), "replacement").unwrap();
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

    fs::write(
        &config,
        "disable = ['rust.target']\n[[extra]]\nstack = 'rust'\npath = '.custom'\n",
    )
    .unwrap();
    let loaded = load(Some(&config));
    assert!(loaded.error.is_none());
    let targets = expand_targets(project, &loaded.rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].rule_id, "extra.0");
    fs::write(root.join("package.json"), "").unwrap();
    let node = &scan(std::slice::from_ref(&root))[0];
    assert!(expand_targets(node, &loaded.rules).is_empty());
}

#[test]
fn workspace_projects_own_only_their_root_target_without_duplicate_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = ['crates/*']\n",
    )
    .unwrap();
    fs::write(root.join("target/shared"), b"123").unwrap();
    let member = root.join("crates/member");
    fs::create_dir_all(member.join("target")).unwrap();
    fs::write(member.join("Cargo.toml"), "").unwrap();
    fs::write(member.join("target/local"), b"12345").unwrap();
    let empty_member = root.join("crates/empty");
    fs::create_dir_all(&empty_member).unwrap();
    fs::write(empty_member.join("Cargo.toml"), "").unwrap();
    let projects = scan(std::slice::from_ref(&root));
    assert_eq!(projects.len(), 3);
    let rules = builtin_rules();
    let mut targets: Vec<_> = projects.iter().map(|p| expand_targets(p, &rules)).collect();
    for (project, targets) in projects.iter().zip(&mut targets) {
        if project.path == empty_member {
            assert!(targets.is_empty());
        } else {
            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].path, project.path.join("target"));
            targets[0].size = Some(dir_size(&targets[0].path));
            targets[0].checked = true;
        }
    }
    assert_eq!(
        targets.iter().map(|t| total_size(t).unwrap()).sum::<u64>(),
        8
    );
    let selected: Vec<_> = projects
        .iter()
        .zip(&targets)
        .map(|(p, targets)| PlanProject {
            path: &p.path,
            stack: p.stack,
            targets,
        })
        .collect();
    let delete_plan = plan(&selected, std::slice::from_ref(&root), &rules);
    assert_eq!(delete_plan.items.len(), 2);
    assert_eq!(delete_plan.total_bytes, 8);
    assert!(delete_plan.skipped.is_empty());
    let only_member = &scan(std::slice::from_ref(&member))[0];
    let local = expand_targets(only_member, &rules);
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].path, member.join("target"));
    let only_empty = &scan(std::slice::from_ref(&empty_member))[0];
    assert!(expand_targets(only_empty, &rules).is_empty());
    let (tx, _rx) = mpsc::channel();
    let report = execute(delete_plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert!(report.failures.is_empty());
    assert_eq!(report.freed_bytes, 8);
    assert!(!root.join("target").exists());
    assert!(!member.join("target").exists());
    for project in &projects {
        assert!(project.path.join("Cargo.toml").is_file());
    }
}

#[cfg(unix)]
#[test]
fn rust_target_links_are_rejected_at_discovery_and_after_selection() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::create_dir(root.join("keep")).unwrap();
    fs::write(root.join("keep/artifact"), "keep").unwrap();
    symlink(root.join("keep"), root.join("target")).unwrap();
    let project = &scan(std::slice::from_ref(&root))[0];
    let rules = builtin_rules();
    assert!(expand_targets(project, &rules).is_empty());
    fs::remove_file(root.join("target")).unwrap();
    fs::create_dir(root.join("target")).unwrap();
    let mut targets = expand_targets(project, &rules);
    assert_eq!(targets.len(), 1);
    targets[0].checked = true;
    fs::remove_dir(root.join("target")).unwrap();
    symlink(root.join("keep"), root.join("target")).unwrap();
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
    assert_eq!(delete_plan.skipped.len(), 1);
    assert_eq!(delete_plan.skipped[0].reason, SkipReason::OutsideRules);

    let config = root.join("rules.toml");
    fs::write(&config, "[[extra]]\nstack = 'rust'\npath = 'target'\n").unwrap();
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
    assert!(extra_plan.skipped.is_empty());
    assert_eq!(fs::read(root.join("keep/artifact")).unwrap(), b"keep");
}
