use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::plan::{DeletePlan, PlanItem};

/// Projects deleted in parallel. Abort only takes effect between projects.
pub const CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeleteMode {
    /// Recoverable: move to the system trash.
    #[default]
    Trash,
    /// Permanent `remove_dir_all`.
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteReport {
    pub freed_bytes: u64,
    pub failures: Vec<Failure>,
    pub projects_done: usize,
    pub projects_total: usize,
    pub aborted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub project_path: PathBuf,
    pub path: PathBuf,
    pub reason: FailureReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    PermissionDenied,
    InUse,
    Missing,
    Other,
}

#[derive(Debug)]
pub enum DeleteEvent {
    /// One project begins.
    Started {
        project: PathBuf,
        total: usize,
    },
    /// A project finished; `deleted` lists the targets removed from disk.
    ProjectDone {
        project: PathBuf,
        deleted: Vec<PathBuf>,
    },
    Finished(DeleteReport),
}

#[derive(Default)]
struct Accumulator {
    freed: u64,
    done: usize,
    failures: Vec<Failure>,
}

/// Delete the plan's items, one project at a time on [`CONCURRENCY`] threads.
/// A single failed item is recorded and skipped; the rest of the batch runs on.
/// Once `abort` is set, no further project starts.
pub fn execute(
    plan: DeletePlan,
    mode: DeleteMode,
    abort: &AtomicBool,
    tx: Sender<DeleteEvent>,
) -> DeleteReport {
    execute_inner(plan, mode, abort, tx, delete_item)
}

/// Execute a validated Go module cache plan, handling its read-only dependencies.
pub fn execute_go_module(
    plan: DeletePlan,
    mode: DeleteMode,
    abort: &AtomicBool,
    tx: Sender<DeleteEvent>,
) -> DeleteReport {
    execute_inner(plan, mode, abort, tx, delete_module_item)
}

fn execute_inner(
    plan: DeletePlan,
    mode: DeleteMode,
    abort: &AtomicBool,
    tx: Sender<DeleteEvent>,
    delete: fn(&PlanItem, DeleteMode) -> Result<(), FailureReason>,
) -> DeleteReport {
    // Plan items are sorted by project, so grouping keeps project order.
    let mut groups: Vec<(PathBuf, Vec<PlanItem>)> = Vec::new();
    for item in plan.items {
        match groups.last_mut() {
            Some((project, items)) if *project == item.project_path => items.push(item),
            _ => groups.push((item.project_path.clone(), vec![item])),
        }
    }

    let total = groups.len();
    let groups = Arc::new(groups);
    let next = Arc::new(AtomicUsize::new(0));
    let acc = Arc::new(Mutex::new(Accumulator::default()));

    thread::scope(|scope| {
        for _ in 0..total.min(CONCURRENCY) {
            let groups = Arc::clone(&groups);
            let next = Arc::clone(&next);
            let acc = Arc::clone(&acc);
            let tx = tx.clone();

            scope.spawn(move || {
                while !abort.load(Ordering::Relaxed) {
                    let Some((project, items)) = groups.get(next.fetch_add(1, Ordering::Relaxed))
                    else {
                        break;
                    };
                    let _ = tx.send(DeleteEvent::Started {
                        project: project.clone(),
                        total,
                    });

                    let mut failures = Vec::new();
                    let mut deleted = Vec::new();
                    let mut freed = 0;
                    for item in items {
                        match delete(item, mode) {
                            Ok(()) => {
                                freed += item.bytes;
                                deleted.push(item.target_path.clone());
                            }
                            Err(reason) => failures.push(Failure {
                                project_path: project.clone(),
                                path: item.target_path.clone(),
                                reason,
                            }),
                        }
                    }

                    {
                        let mut acc = acc.lock().unwrap();
                        acc.freed += freed;
                        acc.done += 1;
                        acc.failures.extend(failures.iter().cloned());
                    }
                    let _ = tx.send(DeleteEvent::ProjectDone {
                        project: project.clone(),
                        deleted,
                    });
                }
            });
        }
    });

    let acc = acc.lock().unwrap();
    let report = DeleteReport {
        freed_bytes: acc.freed,
        failures: acc.failures.clone(),
        projects_done: acc.done,
        projects_total: total,
        aborted: abort.load(Ordering::Relaxed),
    };
    drop(acc);
    let _ = tx.send(DeleteEvent::Finished(report.clone()));
    report
}

fn delete_item(item: &PlanItem, mode: DeleteMode) -> Result<(), FailureReason> {
    let meta = fs::symlink_metadata(&item.target_path).map_err(|_| FailureReason::Missing)?;

    match mode {
        DeleteMode::Trash => move_to_trash(&item.target_path),
        DeleteMode::Direct if meta.file_type().is_symlink() => {
            remove_symlink(&item.target_path).map_err(io_reason)
        }
        DeleteMode::Direct if meta.is_dir() => remove_tree(&item.target_path).map_err(io_reason),
        DeleteMode::Direct => fs::remove_file(&item.target_path).map_err(io_reason),
    }
}

fn delete_module_item(item: &PlanItem, mode: DeleteMode) -> Result<(), FailureReason> {
    let path = &item.target_path;
    if !path.is_absolute() || path.parent().is_none() {
        return Err(FailureReason::Other);
    }
    for ancestor in path.ancestors() {
        let meta = fs::symlink_metadata(ancestor).map_err(io_reason)?;
        if crate::go_cache::linked(&meta) {
            return Err(FailureReason::Other);
        }
    }
    if !fs::symlink_metadata(path).map_err(io_reason)?.is_dir() {
        return Err(FailureReason::Other);
    }
    if mode == DeleteMode::Direct || cfg!(windows) {
        prepare_module_permissions(path).map_err(io_reason)?;
    }
    match mode {
        DeleteMode::Trash => trash::delete(path).map_err(|error| trash_reason(&error)),
        #[cfg(windows)]
        DeleteMode::Direct => fs::remove_dir_all(long_path(path)).map_err(io_reason),
        #[cfg(not(windows))]
        DeleteMode::Direct => fs::remove_dir_all(path).map_err(io_reason),
    }
}

// shortcut: filesystem changes can race with checks; use handle-relative operations for stronger isolation.
fn prepare_module_permissions(path: &Path) -> io::Result<()> {
    let mut entries = walkdir::WalkDir::new(path).follow_links(false).into_iter();
    while let Some(entry) = entries.next() {
        let entry = entry.map_err(io::Error::from)?;
        let meta = fs::symlink_metadata(entry.path())?;
        if crate::go_cache::linked(&meta) {
            entries.skip_current_dir();
            continue;
        }
        #[cfg(unix)]
        if meta.is_dir() {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = meta.permissions();
            let mode = permissions.mode();
            if mode & 0o200 == 0 {
                permissions.set_mode(mode | 0o200);
                fs::set_permissions(entry.path(), permissions)?;
            }
        }
        #[cfg(windows)]
        if meta.permissions().readonly() {
            let mut permissions = meta.permissions();
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions)?;
        }
    }
    Ok(())
}

fn move_to_trash(path: &Path) -> Result<(), FailureReason> {
    #[cfg(windows)]
    clear_readonly(path);
    // The crate canonicalizes to verbatim paths itself, so long paths are covered.
    trash::delete(path).map_err(|err| trash_reason(&err))
}

#[cfg(windows)]
fn remove_tree(path: &Path) -> io::Result<()> {
    clear_readonly(path);
    fs::remove_dir_all(long_path(path))
}

#[cfg(not(windows))]
fn remove_tree(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(windows)]
fn remove_symlink(path: &Path) -> io::Result<()> {
    fs::remove_dir(path).or_else(|_| fs::remove_file(path))
}

#[cfg(not(windows))]
fn remove_symlink(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

/// `\\?\` prefix so Windows accepts paths past `MAX_PATH`; UNC shares need `\\?\UNC\`.
#[cfg(windows)]
fn long_path(path: &Path) -> PathBuf {
    let path = path.as_os_str().to_string_lossy();
    if path.starts_with(r"\\?\") {
        PathBuf::from(path.into_owned())
    } else if let Some(share) = path.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{share}"))
    } else {
        PathBuf::from(format!(r"\\?\{path}"))
    }
}

/// Windows refuses to delete read-only entries, so clear the bit before removing
/// the tree. Links are left alone: only the link itself is removed.
#[cfg(windows)]
fn clear_readonly(path: &Path) {
    for entry in walkdir::WalkDir::new(long_path(path))
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = fs::set_permissions(entry.path(), permissions);
        }
    }
}

fn io_reason(err: io::Error) -> FailureReason {
    match err.kind() {
        io::ErrorKind::NotFound => FailureReason::Missing,
        io::ErrorKind::PermissionDenied => FailureReason::PermissionDenied,
        // EBUSY on unix, ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION on Windows.
        _ if matches!(err.raw_os_error(), Some(16) | Some(32) | Some(33)) => FailureReason::InUse,
        _ => FailureReason::Other,
    }
}

fn trash_reason(err: &trash::Error) -> FailureReason {
    match err {
        // The crate cannot tell "gone" from "no permission"; treat it as access denied.
        trash::Error::CouldNotAccess { .. } => FailureReason::PermissionDenied,
        trash::Error::Os { code, .. } => match code {
            2 | 3 => FailureReason::Missing,
            5 | 13 => FailureReason::PermissionDenied,
            32 | 33 => FailureReason::InUse,
            _ => FailureReason::Other,
        },
        _ => FailureReason::Other,
    }
}
