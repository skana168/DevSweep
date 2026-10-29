use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use devsweep::delete::{DeleteMode, execute_go_module};
use devsweep::plan::{DeletePlan, PlanItem};

fn module_plan(path: &Path, bytes: u64) -> DeletePlan {
    DeletePlan {
        items: vec![PlanItem {
            project_path: path.to_path_buf(),
            target_path: path.to_path_buf(),
            bytes,
            large: false,
        }],
        skipped: Vec::new(),
        total_bytes: bytes,
    }
}

#[test]
fn direct_module_cleanup_removes_readonly_dependencies() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let modules = root.join("modules");
    let dependency = modules.join("example.org/pkg@v1");
    fs::create_dir_all(&dependency).unwrap();
    let source = dependency.join("source.go");
    fs::write(&source, b"package example").unwrap();
    let mut readonly = fs::metadata(&source).unwrap().permissions();
    readonly.set_readonly(true);
    fs::set_permissions(&source, readonly).unwrap();
    #[cfg(windows)]
    {
        let mut readonly = fs::metadata(&dependency).unwrap().permissions();
        readonly.set_readonly(true);
        fs::set_permissions(&dependency, readonly).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dependency, fs::Permissions::from_mode(0o555)).unwrap();
    }
    let (tx, _rx) = mpsc::channel();
    let report = execute_go_module(
        module_plan(&modules, 15),
        DeleteMode::Direct,
        &AtomicBool::new(false),
        tx,
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.freed_bytes, 15);
    assert!(!modules.exists());
}

#[cfg(unix)]
#[test]
fn module_cleanup_refuses_a_linked_root_or_ancestor_without_touching_its_target() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(outside.join("modules")).unwrap();
    fs::write(outside.join("modules/keep.go"), b"keep").unwrap();
    let linked_root = root.join("linked-modules");
    let linked_ancestor = root.join("linked-ancestor");
    symlink(outside.join("modules"), &linked_root).unwrap();
    symlink(&outside, &linked_ancestor).unwrap();
    for path in [linked_root, linked_ancestor.join("modules")] {
        let (tx, _rx) = mpsc::channel();
        let report = execute_go_module(
            module_plan(&path, 4),
            DeleteMode::Direct,
            &AtomicBool::new(false),
            tx,
        );
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.freed_bytes, 0);
        assert!(path.exists());
        assert_eq!(fs::read(outside.join("modules/keep.go")).unwrap(), b"keep");
    }
}

#[cfg(unix)]
#[test]
fn a_partial_module_failure_keeps_zero_freed_bytes_and_preserves_external_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let parent = root.join("parent");
    let modules = parent.join("modules");
    let outside = root.join("outside");
    fs::create_dir_all(&modules).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(modules.join("delete.go"), b"delete").unwrap();
    fs::write(outside.join("keep.go"), b"keep").unwrap();
    symlink(&outside, modules.join("linked-dependency")).unwrap();
    fs::set_permissions(&modules, fs::Permissions::from_mode(0o551)).unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o500)).unwrap();
    let _outside_restore = Restore(outside.clone());
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();
    let _parent_restore = Restore(parent.clone());
    let (tx, _rx) = mpsc::channel();
    let report = execute_go_module(
        module_plan(&modules, 6),
        DeleteMode::Direct,
        &AtomicBool::new(false),
        tx,
    );
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.freed_bytes, 0);
    assert!(modules.exists());
    assert!(
        !modules.join("delete.go").exists(),
        "partial deletion is not rolled back"
    );
    assert_eq!(
        fs::metadata(&modules).unwrap().permissions().mode() & 0o777,
        0o751
    );
    assert_eq!(
        fs::metadata(&outside).unwrap().permissions().mode() & 0o777,
        0o500
    );
    assert_eq!(fs::read(outside.join("keep.go")).unwrap(), b"keep");
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires access to the system Trash; uses and restores only a unique temporary fixture"]
fn readonly_module_cache_can_be_trashed_and_restored_without_changing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let name = format!(
        "devsweep-go-module-{}",
        root.file_name().unwrap().to_string_lossy()
    );
    let modules = root.join(&name);
    let dependency = modules.join("dependency");
    fs::create_dir_all(&dependency).unwrap();
    fs::write(
        dependency.join("source.go"),
        b"unique temporary DevSweep module fixture",
    )
    .unwrap();
    fs::set_permissions(&dependency, fs::Permissions::from_mode(0o555)).unwrap();
    let trashed = std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".Trash")
        .join(name);
    assert!(!trashed.exists(), "unique fixture name must not collide");
    let (tx, _rx) = mpsc::channel();
    let report = execute_go_module(
        module_plan(&modules, 39),
        DeleteMode::Trash,
        &AtomicBool::new(false),
        tx,
    );
    if !report.failures.is_empty() {
        assert!(
            modules.exists(),
            "failed Trash must never fall back to permanent deletion"
        );
        fs::set_permissions(&dependency, fs::Permissions::from_mode(0o755)).unwrap();
        panic!("system Trash unavailable: {:?}", report.failures);
    }
    assert!(!modules.exists());
    assert_eq!(
        fs::read(trashed.join("dependency/source.go")).unwrap(),
        b"unique temporary DevSweep module fixture"
    );
    fs::rename(&trashed, &modules).unwrap();
    let mode = fs::metadata(&dependency).unwrap().permissions().mode() & 0o777;
    fs::set_permissions(&dependency, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        mode, 0o555,
        "Trash must not change POSIX module directory permissions"
    );
    assert_eq!(
        fs::read(dependency.join("source.go")).unwrap(),
        b"unique temporary DevSweep module fixture"
    );
}

#[cfg(unix)]
#[test]
fn an_internal_link_does_not_skip_readonly_sibling_dependencies() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let modules = root.join("modules");
    let outside = root.join("outside");
    fs::create_dir(&modules).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.go"), b"keep").unwrap();
    symlink(&outside, modules.join("000-link")).unwrap();
    for index in 0..8 {
        let dependency = modules.join(format!("dependency-{index}"));
        fs::create_dir(&dependency).unwrap();
        fs::write(dependency.join("source.go"), b"source").unwrap();
        fs::set_permissions(dependency, fs::Permissions::from_mode(0o555)).unwrap();
    }
    let (tx, _rx) = mpsc::channel();
    let report = execute_go_module(
        module_plan(&modules, 48),
        DeleteMode::Direct,
        &AtomicBool::new(false),
        tx,
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!modules.exists());
    assert_eq!(fs::read(outside.join("keep.go")).unwrap(), b"keep");
}
