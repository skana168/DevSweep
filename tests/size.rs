use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use devsweep::rules::Target;
use devsweep::size::{SizeRequest, checked_size, dir_size, format_size, measure};

const MB: u64 = 1024 * 1024;

fn write(path: &Path, len: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; len]).unwrap();
}

#[test]
fn sizes_format_by_magnitude() {
    assert_eq!(format_size(0), "—");
    assert_eq!(format_size(MB - 1), "—");
    assert_eq!(format_size(MB), "1 MB");
    assert_eq!(format_size(380 * MB), "380 MB");
    assert_eq!(
        format_size((1.2 * 1024.0 * 1024.0 * 1024.0) as u64),
        "1.2 GB"
    );
}

#[test]
fn checked_size_sums_only_checked_targets_and_waits_for_their_sizes() {
    let target = |id: &'static str, size: Option<u64>, checked: bool| Target {
        rule_id: id.to_string(),
        path: PathBuf::from(id),
        size,
        checked,
        dangerous: false,
        label: None,
    };

    // A risky target that is off contributes nothing, even though it is measured.
    let measured = vec![
        target("a", Some(1_000), true),
        target("risky", Some(2_000), false),
    ];
    assert_eq!(checked_size(&measured), Some(1_000));

    // Any checked target still being measured leaves the total unknown.
    let pending = vec![
        target("a", Some(1_000), true),
        target("b", None, true),
        target("risky", Some(2_000), false),
    ];
    assert_eq!(checked_size(&pending), None);

    assert_eq!(checked_size(&[]), Some(0));
}

#[test]
fn nested_file_sizes_are_summed() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("a.bin"), 1_000);
    write(&dir.path().join("nested/deep/b.bin"), 2_000);

    assert_eq!(dir_size(dir.path()), 3_000);
}

#[cfg(unix)]
#[test]
fn symlinks_are_counted_but_not_followed() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside");
    write(&outside.join("big.bin"), 4 * MB as usize);

    let project = dir.path().join("project");
    write(&project.join("small.bin"), 1_000);
    symlink(&outside, project.join("linked")).unwrap();

    let link_len = fs::symlink_metadata(project.join("linked")).unwrap().len();

    assert_eq!(dir_size(&project), 1_000 + link_len);
}

#[cfg(unix)]
#[test]
fn permission_errors_do_not_abort_the_walk() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("readable.bin"), 1_000);
    write(&root.join("locked/secret.bin"), 5_000);

    let locked = root.join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let size = dir_size(root);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(size, 1_000);
}

#[test]
fn measure_reports_a_size_for_every_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("kept/a.bin"), 1_000);
    write(&root.join("risky/b.bin"), 2_000);

    let (tx, rx) = mpsc::channel();
    measure(
        vec![SizeRequest {
            project: root.to_path_buf(),
            paths: vec![root.join("kept"), root.join("risky")],
        }],
        tx,
    );

    let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(event.project, root);
    assert_eq!(
        event.sizes,
        vec![(root.join("kept"), 1_000), (root.join("risky"), 2_000)]
    );
    assert!(rx.recv().is_err());
}
