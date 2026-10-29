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
