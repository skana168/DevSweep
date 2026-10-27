use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use devsweep::rules::Target;
use devsweep::size::{SizeRequest, dir_size, format_size, measure};

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
    assert_eq!(format_size((1.2 * 1024.0 * 1024.0 * 1024.0) as u64), "1.2 GB");
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
fn measure_counts_only_checked_targets() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("kept/a.bin"), 1_000);
    write(&root.join("risky/b.bin"), 2_000);

    let (tx, rx) = mpsc::channel();
    measure(
        vec![SizeRequest {
            project: root.to_path_buf(),
            targets: vec![
                Target {
                    rule_id: "kept",
                    path: root.join("kept"),
                    checked: true,
                },
                Target {
                    rule_id: "risky",
                    path: root.join("risky"),
                    checked: false,
                },
            ],
        }],
        tx,
    );

    let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(event.project, root);
    assert_eq!(event.size, 1_000);
    assert!(rx.recv().is_err());
}
