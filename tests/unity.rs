use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, mpsc};

use devsweep::delete::{DeleteMode, FailureReason, execute, execute_go_module};
use devsweep::plan::{DeletePlan, PlanItem, PlanProject, SkipReason, plan};
use devsweep::rules::{Target, expand_targets, load};
use devsweep::scan::{Stack, scan};

fn unity(root: &Path) -> PathBuf {
    let path = root.join("game");
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
fn unity_read_only_cannot_be_bypassed_at_discovery_or_planning() {
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
    assert!(expand_targets(&project, &rules.rules).is_empty());
    let mut forged = project.clone();
    forged.stack = Stack::Node;
    assert!(expand_targets(&forged, &rules.rules).is_empty());
    let targets = [Target {
        rule_id: "extra.0".into(),
        path: game.join("Library"),
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
        assert_eq!(result.skipped[0].reason, SkipReason::UnityReadOnly);
    }
}

#[test]
fn execution_rechecks_read_only_unity_even_for_forged_plans_and_keeps_other_projects_running() {
    let dir = tempfile::tempdir().unwrap();
    let game = unity(&fs::canonicalize(dir.path()).unwrap());
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
            assert_eq!(report.failures[0].reason, FailureReason::UnityReadOnly);
            assert_eq!(report.freed_bytes, 4);
            assert!(game.join("Library/keep").is_file());
            assert!(!other.join("cache").exists());
        }
    }
}

#[test]
fn scanned_unity_reports_version_support_numerically_without_enabling_cleanup() {
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
        assert!(expand_targets(&project, &devsweep::rules::builtin_rules()).is_empty());
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
    assert!(projects.entries()[0].targets.is_empty());
}
