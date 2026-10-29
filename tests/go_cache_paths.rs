use std::fs;
use std::path::PathBuf;

use devsweep::cache::{self, CacheGroup};
use devsweep::go_cache::{GoCacheContext, GoCacheEntry, GoCacheError, GoCacheSnapshot};

fn fixture() -> (tempfile::TempDir, GoCacheContext, GoCacheSnapshot) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let home = root.join("home");
    let build = root.join("build");
    let module = root.join("modules");
    for path in [&home, &build, &module] {
        fs::create_dir(path).unwrap();
    }
    let snapshot = GoCacheSnapshot {
        entries: [
            GoCacheEntry {
                id: "go.build",
                path: Some(build),
                size: Ok(5),
            },
            GoCacheEntry {
                id: "go.module",
                path: Some(module),
                size: Ok(6),
            },
        ],
    };
    (
        temp,
        GoCacheContext {
            home,
            scan_roots: Vec::new(),
            projects: Vec::new(),
            other_caches: Vec::new(),
        },
        snapshot,
    )
}

#[test]
fn both_go_entries_are_fixed_whitelisted_items_without_guessed_paths_or_cleanup() {
    let entries = cache::entries();
    for (id, group, dangerous) in [
        ("go.build", CacheGroup::Toolchain, false),
        ("go.module", CacheGroup::Dependency, true),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry.rule.id == id)
            .expect("fixed Go entry");
        assert_eq!(entry.rule.group, group);
        assert_eq!(entry.rule.dangerous, dangerous);
        assert!(entry.path.is_none());
        assert!(cache::plan(id, 1).is_none());
    }
}

#[test]
fn valid_real_directories_revalidate_but_new_protected_context_rejects_them() {
    let (_temp, mut context, snapshot) = fixture();
    let build = snapshot.entry("go.build").unwrap().path.clone().unwrap();
    assert_eq!(snapshot.validate("go.build", &context), Ok(build.clone()));
    context.scan_roots.push(build.join("new-scan-root"));
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Protected)
    );
    context.scan_roots.clear();
    context.projects.push(build.join("project"));
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Overlap)
    );
    context.projects.clear();
    context.other_caches.push(build.join("missing-cache"));
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Overlap)
    );
}

#[test]
fn revalidation_rejects_replaced_missing_and_file_directories() {
    let (_temp, context, mut snapshot) = fixture();
    let build = snapshot.entry("go.build").unwrap().path.clone().unwrap();
    fs::remove_dir(&build).unwrap();
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Missing)
    );
    fs::write(&build, b"not a directory").unwrap();
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::NotDirectory)
    );
    snapshot.entries[0].path = Some(context.home.clone());
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Protected)
    );
    snapshot.entries[0].path = Some(PathBuf::from("off"));
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Disabled)
    );
}

#[test]
fn both_nested_go_caches_are_rejected_and_sibling_prefix_names_are_independent() {
    let (_temp, context, mut snapshot) = fixture();
    let build = snapshot.entries[0].path.clone().unwrap();
    let nested = build.join("nested");
    fs::create_dir(&nested).unwrap();
    snapshot.entries[1].path = Some(nested);
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Overlap)
    );
    assert_eq!(
        snapshot.validate("go.module", &context),
        Err(GoCacheError::Overlap)
    );
    let sibling = build.with_file_name("build-other");
    fs::create_dir(&sibling).unwrap();
    snapshot.entries[1].path = Some(sibling.clone());
    assert_eq!(snapshot.validate("go.build", &context), Ok(build));
    assert_eq!(snapshot.validate("go.module", &context), Ok(sibling));
}

#[cfg(windows)]
#[test]
fn windows_overlap_checks_ignore_path_component_case() {
    let (_temp, mut context, snapshot) = fixture();
    let build = snapshot.entry("go.build").unwrap().path.as_ref().unwrap();
    context.other_caches.push(PathBuf::from(
        format!("{}\\MISSING", build.display()).to_ascii_uppercase(),
    ));
    assert_eq!(
        snapshot.validate("go.build", &context),
        Err(GoCacheError::Overlap)
    );
}

#[test]
fn measured_snapshot_builds_a_whitelisted_plan_and_rejects_unknown_or_unmeasured_entries() {
    let (_temp, context, mut snapshot) = fixture();
    let plan = snapshot.plan("go.build", &context).unwrap();
    assert_eq!(plan.items.len(), 1);
    assert_eq!(
        plan.items[0].target_path,
        snapshot.entries[0].path.clone().unwrap()
    );
    assert_eq!(plan.total_bytes, 5);
    assert!(snapshot.plan("arbitrary", &context).is_err());
    snapshot.entries[0].size = Err(GoCacheError::Unreadable);
    assert_eq!(
        snapshot.plan("go.build", &context),
        Err(GoCacheError::Unreadable)
    );
}

#[test]
fn final_query_must_match_the_confirmed_path_and_cannot_fall_back_after_query_failure() {
    let (_temp, context, snapshot) = fixture();
    let mut fresh = snapshot.clone();
    assert!(snapshot.plan_against("go.build", &fresh, &context).is_ok());
    let replacement = context.home.parent().unwrap().join("new-build");
    fs::create_dir(&replacement).unwrap();
    fresh.entries[0].path = Some(replacement.clone());
    assert_eq!(
        snapshot.plan_against("go.build", &fresh, &context),
        Err(GoCacheError::Changed)
    );
    assert!(snapshot.entries[0].path.as_ref().unwrap().is_dir());
    assert!(replacement.is_dir());
    fresh.entries[0].path = None;
    fresh.entries[0].size = Err(GoCacheError::QueryFailed);
    assert_eq!(
        snapshot.plan_against("go.build", &fresh, &context),
        Err(GoCacheError::QueryFailed)
    );
}

#[test]
fn confirmed_build_cache_executes_directly_and_a_missing_target_never_counts_as_freed() {
    use devsweep::delete::{self, DeleteMode, FailureReason};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    let (_temp, context, snapshot) = fixture();
    let build = snapshot.entries[0].path.as_ref().unwrap();
    fs::write(build.join("artifact"), b"build").unwrap();
    let (tx, _) = mpsc::channel();
    let plan = snapshot.plan("go.build", &context).unwrap();
    let report = delete::execute(plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert_eq!(report.freed_bytes, 5);
    assert!(report.failures.is_empty());
    assert!(!build.exists());
    assert!(snapshot.entries[1].path.as_ref().unwrap().is_dir());

    fs::create_dir(build).unwrap();
    let plan = snapshot.plan("go.build", &context).unwrap();
    fs::remove_dir(build).unwrap();
    let (tx, _) = mpsc::channel();
    let report = delete::execute(plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    assert_eq!(report.freed_bytes, 0);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].reason, FailureReason::Missing);
}

#[cfg(unix)]
#[test]
fn a_partly_unremovable_build_cache_reports_failure_without_claiming_its_whole_size() {
    use devsweep::delete::{self, DeleteMode};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    let (_temp, context, snapshot) = fixture();
    let build = snapshot.entries[0].path.as_ref().unwrap();
    let blocked = build.join("blocked");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("artifact"), b"build").unwrap();
    let plan = snapshot.plan("go.build", &context).unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o500)).unwrap();
    let (tx, _) = mpsc::channel();
    let report = delete::execute(plan, DeleteMode::Direct, &AtomicBool::new(false), tx);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(report.freed_bytes, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(blocked.join("artifact").exists());
}

#[cfg(unix)]
#[test]
fn direct_build_cleanup_does_not_follow_internal_links() {
    use devsweep::delete::{self, DeleteMode};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    let (_temp, context, snapshot) = fixture();
    let build = snapshot.entries[0].path.as_ref().unwrap();
    let external = context.home.join("keep");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("source"), b"keep").unwrap();
    std::os::unix::fs::symlink(&external, build.join("link")).unwrap();
    let (tx, _) = mpsc::channel();
    let report = delete::execute(
        snapshot.plan("go.build", &context).unwrap(),
        DeleteMode::Direct,
        &AtomicBool::new(false),
        tx,
    );
    assert!(report.failures.is_empty());
    assert!(!build.exists());
    assert_eq!(fs::read(external.join("source")).unwrap(), b"keep");
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "uses the real macOS Trash with a unique disposable fixture; run explicitly"]
fn macos_build_cache_trash_can_be_restored() {
    use devsweep::delete::{self, DeleteMode};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    let (_temp, context, mut snapshot) = fixture();
    let name = format!(
        "devsweep-go-build-trash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let build = context.home.parent().unwrap().join(&name);
    fs::create_dir(&build).unwrap();
    fs::write(build.join("artifact"), b"build").unwrap();
    snapshot.entries[0].path = Some(build.clone());
    let (tx, _) = mpsc::channel();
    let report = delete::execute(
        snapshot.plan("go.build", &context).unwrap(),
        DeleteMode::Trash,
        &AtomicBool::new(false),
        tx,
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!build.exists());
    let trashed = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".Trash")
        .join(name);
    fs::rename(&trashed, &build).expect("restore this unique fixture from the system Trash");
    assert_eq!(fs::read(build.join("artifact")).unwrap(), b"build");
}
