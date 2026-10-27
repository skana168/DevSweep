use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, DeleteReport, FailureReason, execute};
use devsweep::plan::{DeletePlan, PlanProject, plan};
use devsweep::rules::{builtin_rules, expand_targets};
use devsweep::scan::{Project, scan};

const MB: u64 = 1024 * 1024;

fn write(path: &Path, len: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; len]).unwrap();
}

fn project<'a>(projects: &'a [Project], name: &str) -> &'a Project {
    projects
        .iter()
        .find(|project| project.name == name)
        .unwrap_or_else(|| panic!("{name} should be discovered"))
}

/// Node targets with sizes attached, so the plan has byte counts to report.
fn plan_for(project: &Project, root: &Path) -> DeletePlan {
    let mut targets = expand_targets(project, &builtin_rules());
    for target in &mut targets {
        target.size = Some(match target.rule_id.as_str() {
            "node.node_modules" => 2 * MB,
            "node.next" => 3 * MB,
            other => panic!("unexpected target {other}"),
        });
    }
    plan(
        &[PlanProject {
            path: &project.path,
            stack: project.stack,
            targets: &targets,
        }],
        &[root.to_path_buf()],
        &builtin_rules(),
    )
}

fn run(plan: DeletePlan, mode: DeleteMode, aborted: bool) -> DeleteReport {
    let (tx, _rx) = mpsc::channel();
    execute(plan, mode, &AtomicBool::new(aborted), tx)
}

#[cfg(unix)]
struct Restore(PathBuf);

#[cfg(unix)]
impl Drop for Restore {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn direct_delete_removes_targets_and_keeps_sources() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    write(&root.join("app/package.json"), 0);
    write(&root.join("app/package-lock.json"), 0);
    write(&root.join("app/src/index.js"), 0);
    write(&root.join("app/node_modules/react/index.js"), MB as usize);
    write(&root.join("app/.next/chunk.js"), MB as usize);
    write(&root.join("outside/keep.txt"), 0);

    let projects = scan(&[root.clone()]);
    let app = project(&projects, "app");
    let delete_plan = plan_for(app, &root);
    assert_eq!(delete_plan.items.len(), 2);

    let report = run(delete_plan, DeleteMode::Direct, false);

    assert_eq!(report.freed_bytes, 2 * MB + 3 * MB);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.projects_done, 1);
    assert_eq!(report.projects_total, 1);
    assert!(!report.aborted);

    assert!(!app.path.join("node_modules").exists());
    assert!(!app.path.join(".next").exists());
    assert!(app.path.join("package.json").is_file());
    assert!(app.path.join("package-lock.json").is_file());
    assert!(app.path.join("src/index.js").is_file());
    assert!(root.join("outside/keep.txt").is_file());
}

#[test]
fn direct_delete_removes_file_targets_and_keeps_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    write(&root.join("app/pubspec.yaml"), 0);
    write(&root.join("app/.flutter-plugins-dependencies"), 1_200);
    write(&root.join("app/lib/main.dart"), 0);

    let projects = scan(&[root.clone()]);
    let app = project(&projects, "app");
    let mut targets = expand_targets(app, &builtin_rules());
    let target = targets
        .iter_mut()
        .find(|target| target.rule_id == "flutter.plugins_dependencies")
        .expect("the file rule expands");
    target.size = Some(1_200);

    let delete_plan = plan(
        &[PlanProject {
            path: &app.path,
            stack: app.stack,
            targets: &targets,
        }],
        &[root.clone()],
        &builtin_rules(),
    );
    assert_eq!(delete_plan.items.len(), 1);

    let report = run(delete_plan, DeleteMode::Direct, false);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.freed_bytes, 1_200);
    assert!(!app.path.join(".flutter-plugins-dependencies").exists());
    assert!(app.path.join("pubspec.yaml").is_file());
    assert!(app.path.join("lib/main.dart").is_file());
}

#[cfg(unix)]
#[test]
fn one_failure_does_not_stop_the_batch() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    write(&root.join("locked/package.json"), 0);
    write(&root.join("locked/node_modules/x.js"), MB as usize);
    write(&root.join("free/package.json"), 0);
    write(&root.join("free/node_modules/x.js"), MB as usize);

    let projects = scan(&[root.clone()]);
    let mut expanded = Vec::new();
    for name in ["locked", "free"] {
        let found = project(&projects, name);
        let mut targets = expand_targets(found, &builtin_rules());
        assert_eq!(targets.len(), 1, "{name} only has node_modules");
        targets[0].size = Some(MB);
        expanded.push((found, targets));
    }
    let plan_projects: Vec<PlanProject> = expanded
        .iter()
        .map(|(found, targets)| PlanProject {
            path: &found.path,
            stack: found.stack,
            targets,
        })
        .collect();
    let delete_plan = plan(&plan_projects, &[root.clone()], &builtin_rules());

    // A read-only target directory cannot be emptied.
    let locked = root.join("locked/node_modules");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = Restore(locked.clone());

    let report = run(delete_plan, DeleteMode::Direct, false);

    assert!(!root.join("free/node_modules").exists());
    assert!(locked.exists(), "the locked target survives");
    assert_eq!(report.freed_bytes, MB);
    assert_eq!(report.projects_done, 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, locked);
    assert_eq!(report.failures[0].project_path, root.join("locked"));
    assert_eq!(report.failures[0].reason, FailureReason::PermissionDenied);
}

#[test]
fn abort_before_start_deletes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();

    write(&root.join("app/package.json"), 0);
    write(&root.join("app/node_modules/x.js"), MB as usize);

    let projects = scan(&[root.clone()]);
    let app = project(&projects, "app");
    let delete_plan = plan_for(app, &root);

    let report = run(delete_plan, DeleteMode::Direct, true);

    assert!(report.aborted);
    assert_eq!(report.projects_done, 0);
    assert_eq!(report.freed_bytes, 0);
    assert!(app.path.join("node_modules").is_dir());
    assert!(app.path.join("package.json").is_file());
}
