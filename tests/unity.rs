use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, mpsc};

use devsweep::delete::{DeleteMode, FailureReason, execute, execute_go_module};
use devsweep::plan::{DeletePlan, PlanItem, PlanProject, SkipReason, plan};
use devsweep::rules::{Target, expand_targets, load};
use devsweep::scan::{Stack, scan};

fn unity(root: &Path) -> PathBuf {
    let path = fs::canonicalize(root).unwrap().join("game");
    fs::create_dir_all(path.join("Assets")).unwrap();
    fs::create_dir_all(path.join("ProjectSettings")).unwrap();
    fs::create_dir_all(path.join("Library")).unwrap();
    fs::write(
        path.join("ProjectSettings/ProjectVersion.txt"),
        "m_EditorVersion: 2022.3.1f1\n",
    )
    .unwrap();
    fs::write(path.join("Library/keep"), "keep").unwrap();
    path
}

#[test]
fn unity_scope_cannot_be_bypassed_at_discovery_or_planning() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(&fs::canonicalize(dir.path()).unwrap());
    let config = dir.path().join("rules.toml");
    fs::write(
        &config,
        "[defaults]\n'dotnet.bin' = true\n[[extra]]\nstack = '*'\npath = 'Library'\n[[extra]]\nstack = 'unity'\npath = 'Library'\n",
    )
    .unwrap();
    let rules = load(Some(&config));
    assert!(rules.error.is_none());
    let project = scan(std::slice::from_ref(&game)).remove(0);
    assert_eq!(expand_targets(&project, &rules.rules).len(), 1);
    let mut forged = project.clone();
    forged.stack = Stack::Node;
    assert!(expand_targets(&forged, &rules.rules).is_empty());
    let targets = [Target {
        rule_id: "extra.0".into(),
        path: game.join("Assets"),
        size: Some(4),
        checked: true,
        default_checked: true,
        dangerous: false,
        label: None,
    }];
    for stack in [Stack::Unity, Stack::Node] {
        let result = plan(
            &[PlanProject {
                path: &game,
                stack,
                targets: &targets,
            }],
            &[],
            &rules.rules,
        );
        assert!(result.items.is_empty());
        assert_eq!(
            result.skipped[0].reason,
            SkipReason::UnityBlocked(devsweep::unity::BlockReason::Path)
        );
    }
}

#[test]
fn execution_rechecks_unity_version_even_for_forged_plans_and_keeps_other_projects_running() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(&fs::canonicalize(dir.path()).unwrap());
    fs::write(
        game.join("ProjectSettings/ProjectVersion.txt"),
        "m_EditorVersion: 2021.3.1f1",
    )
    .unwrap();
    let other = fs::canonicalize(dir.path()).unwrap().join("other");
    fs::create_dir_all(other.join("cache")).unwrap();
    for run in [execute, execute_go_module] {
        for mode in [DeleteMode::Direct, DeleteMode::Trash] {
            fs::create_dir_all(other.join("cache")).unwrap();
            let items = [&game, &other]
                .into_iter()
                .map(|root| PlanItem {
                    project_path: root.clone(),
                    target_path: root.join(if root == &game { "Library" } else { "cache" }),
                    bytes: 4,
                    large: false,
                })
                .collect();
            let (tx, _rx) = mpsc::channel();
            let report = run(
                DeletePlan {
                    items,
                    skipped: vec![],
                    total_bytes: 8,
                },
                mode,
                &AtomicBool::new(false),
                tx,
            );
            assert_eq!(report.failures.len(), 1, "{mode:?}: {report:?}");
            assert_eq!(
                report.failures[0].reason,
                FailureReason::UnityBlocked(devsweep::unity::BlockReason::Version)
            );
            assert_eq!(report.freed_bytes, 4);
            assert!(game.join("Library/keep").is_file());
            assert!(!other.join("cache").exists());
        }
    }
}

#[test]
fn scanned_unity_reports_version_support_numerically_and_exposes_cache_targets() {
    use devsweep::scan::{UnityVersion, unity_version};
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    for (version, supported) in [
        ("2022.3.0f1", true),
        ("2022.2.99f1", false),
        ("2021.3.45f1", false),
        ("2022.10.0f1", true),
        ("6000.0.1f1", true),
        ("6000.1.0b2", true),
    ] {
        fs::write(
            game.join("ProjectSettings/ProjectVersion.txt"),
            format!(
                "m_EditorVersion: {version}\nm_EditorVersionWithRevision: {version} (revision)\n"
            ),
        )
        .unwrap();
        let project = scan(std::slice::from_ref(&game)).remove(0);
        assert_eq!(project.stack, Stack::Unity);
        assert_eq!(
            unity_version(&project.path),
            if supported {
                UnityVersion::Supported(version.into())
            } else {
                UnityVersion::Unsupported(version.into())
            }
        );
        assert_eq!(
            expand_targets(&project, &devsweep::rules::builtin_rules()).len(),
            1
        );
    }
    for invalid in [
        "2022.3",
        "2022.3.0garbage",
        "2022.3.0f",
        "6.0.0",
        "2022.3.0f1 trailing",
        "2022.x.0f1",
    ] {
        fs::write(
            game.join("ProjectSettings/ProjectVersion.txt"),
            format!("m_EditorVersion: {invalid}\n"),
        )
        .unwrap();
        assert_eq!(unity_version(&game), UnityVersion::Invalid);
    }
    for unknown in ["", "m_EditorVersionWithRevision: 6000.0.1f1 (revision)\n"] {
        fs::write(game.join("ProjectSettings/ProjectVersion.txt"), unknown).unwrap();
        assert_eq!(scan(std::slice::from_ref(&game))[0].stack, Stack::Unity);
        assert_eq!(unity_version(&game), UnityVersion::Unknown);
    }
}

#[test]
fn refresh_drops_previously_cleanable_targets_when_project_becomes_unity() {
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    fs::create_dir_all(game.join("node_modules")).unwrap();
    fs::write(game.join("package.json"), "{}").unwrap();
    let mut projects = devsweep::projects::ProjectRefresh::default();
    let rules = devsweep::rules::builtin_rules();
    projects.refresh(std::slice::from_ref(&game), &rules);
    assert_eq!(projects.entries()[0].targets.len(), 1);
    unity(dir.path());
    projects.refresh(&[game], &rules);
    assert_eq!(projects.entries()[0].targets.len(), 1);
    assert_eq!(projects.entries()[0].targets[0].rule_id, "unity.library");
}

#[test]
fn supported_unity_exposes_only_root_caches_with_safe_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    for path in ["Temp", "obj", "Logs", "UserSettings", "Packages/Library"] {
        fs::create_dir_all(game.join(path)).unwrap();
    }
    let project = scan(&[game]).remove(0);
    let targets = expand_targets(&project, &devsweep::rules::builtin_rules());
    assert_eq!(targets.len(), 2);
    let library = targets
        .iter()
        .find(|t| t.rule_id == "unity.library")
        .unwrap();
    assert!(library.dangerous);
    assert!(!library.checked);
    assert!(
        targets
            .iter()
            .find(|t| t.rule_id == "unity.temp")
            .unwrap()
            .checked
    );
}

#[test]
fn planning_rejects_forged_unity_paths_and_unsupported_versions() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    let project = scan(std::slice::from_ref(&game)).remove(0);
    let rules = devsweep::rules::builtin_rules();
    let mut targets = expand_targets(&project, &rules);
    targets[0].checked = true;
    fs::write(
        game.join("ProjectSettings/ProjectVersion.txt"),
        "m_EditorVersion: 2021.3.1f1",
    )
    .unwrap();
    let result = plan(
        &[PlanProject {
            path: &game,
            stack: Stack::Unity,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert_eq!(
        result.skipped[0].reason,
        SkipReason::UnityBlocked(devsweep::unity::BlockReason::Version)
    );
}

#[test]
fn unity_overrides_change_selection_but_never_expand_scope_or_remove_danger() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    fs::create_dir_all(game.join("Temp")).unwrap();
    let config = dir.path().join("rules.toml");
    fs::write(&config, "disable = ['unity.temp']\n[defaults]\n'unity.library' = true\n[[extra]]\nstack = 'unity'\npath = 'Assets'\n[[extra]]\nstack = '*'\npath = 'Packages'\n").unwrap();
    let rules = load(Some(&config));
    let project = scan(std::slice::from_ref(&game)).remove(0);
    let targets = expand_targets(&project, &rules.rules);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, game.join("Library"));
    assert!(targets[0].checked && targets[0].dangerous);
}

#[cfg(unix)]
#[test]
fn unity_rejects_files_symlinks_nested_targets_and_replaced_directories() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    let project = scan(std::slice::from_ref(&game)).remove(0);
    let rules = devsweep::rules::builtin_rules();
    let mut targets = expand_targets(&project, &rules);
    targets[0].checked = true;
    fs::remove_dir_all(game.join("Library")).unwrap();
    fs::write(game.join("Library"), "file").unwrap();
    assert!(expand_targets(&project, &rules).is_empty());
    fs::remove_file(game.join("Library")).unwrap();
    symlink(game.join("Assets"), game.join("Library")).unwrap();
    assert!(expand_targets(&project, &rules).is_empty());
    let result = plan(
        &[PlanProject {
            path: &game,
            stack: Stack::Unity,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert!(result.items.is_empty());
    assert_eq!(
        result.skipped[0].reason,
        SkipReason::UnityBlocked(devsweep::unity::BlockReason::Path)
    );
    fs::create_dir_all(game.join("Packages/Library")).unwrap();
    for run in [execute, execute_go_module] {
        for mode in [DeleteMode::Direct, DeleteMode::Trash] {
            for path in [
                game.join("Library"),
                game.join("Assets"),
                game.join("Packages/Library"),
                game.clone(),
                game.join("Library/../Assets"),
            ] {
                let (tx, _) = mpsc::channel();
                let report = run(
                    DeletePlan {
                        items: vec![PlanItem {
                            project_path: game.clone(),
                            target_path: path,
                            bytes: 1,
                            large: false,
                        }],
                        skipped: vec![],
                        total_bytes: 1,
                    },
                    mode,
                    &AtomicBool::new(false),
                    tx,
                );
                assert_eq!(report.freed_bytes, 0);
                assert_eq!(
                    report.failures[0].reason,
                    FailureReason::UnityBlocked(devsweep::unity::BlockReason::Path)
                );
                assert!(game.join("Assets").is_dir());
            }
        }
    }
}

#[test]
fn disappearing_unity_target_uses_existing_missing_reports() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    let rules = devsweep::rules::builtin_rules();
    let project = scan(std::slice::from_ref(&game)).remove(0);
    let mut targets = expand_targets(&project, &rules);
    targets[0].checked = true;
    fs::remove_dir_all(game.join("Library")).unwrap();
    let result = plan(
        &[PlanProject {
            path: &game,
            stack: Stack::Unity,
            targets: &targets,
        }],
        &[],
        &rules,
    );
    assert_eq!(result.skipped[0].reason, SkipReason::Missing);
    for run in [execute, execute_go_module] {
        let (tx, _) = mpsc::channel();
        let report = run(
            DeletePlan {
                items: vec![PlanItem {
                    project_path: game.clone(),
                    target_path: game.join("Library"),
                    bytes: 4,
                    large: false,
                }],
                skipped: vec![],
                total_bytes: 4,
            },
            DeleteMode::Direct,
            &AtomicBool::new(false),
            tx,
        );
        assert_eq!(report.failures[0].reason, FailureReason::Missing);
    }
}

#[test]
fn missing_version_marker_at_execution_cannot_turn_unity_into_an_unprotected_project() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    fs::remove_file(game.join("ProjectSettings/ProjectVersion.txt")).unwrap();
    for run in [execute, execute_go_module] {
        let (tx, _) = mpsc::channel();
        let report = run(
            DeletePlan {
                items: vec![PlanItem {
                    project_path: game.clone(),
                    target_path: game.join("Library"),
                    bytes: 4,
                    large: false,
                }],
                skipped: vec![],
                total_bytes: 4,
            },
            DeleteMode::Direct,
            &AtomicBool::new(false),
            tx,
        );
        assert_eq!(report.freed_bytes, 0);
        assert_eq!(report.failures.len(), 1);
        assert!(game.join("Library/keep").exists());
    }
}

#[test]
fn unity_cache_sizes_include_unchecked_library_and_survive_refresh() {
    use devsweep::{projects::ProjectRefresh, size};
    let dir = tempfile::tempdir().unwrap();
    let game = unity(dir.path());
    fs::create_dir_all(game.join("Temp")).unwrap();
    fs::write(game.join("Temp/cache"), "123").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&game), &rules);
    let (tx, rx) = mpsc::channel();
    size::measure(projects.measurement_requests(), tx);
    projects.apply_measurement(&rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap());
    projects.refresh(&[game], &rules);
    let targets = &projects.entries()[0].targets;
    assert_eq!(size::total_size(targets), Some(7));
    assert_eq!(
        targets
            .iter()
            .find(|t| t.rule_id == "unity.library")
            .unwrap()
            .size,
        Some(4)
    );
    assert_eq!(
        targets
            .iter()
            .filter(|t| t.checked)
            .map(|t| t.size.unwrap())
            .sum::<u64>(),
        3
    );
}
