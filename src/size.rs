use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use walkdir::WalkDir;

use crate::rules::Target;

pub const CONCURRENCY: usize = 4;

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

/// `1.2 GB`, `380 MB`, and `—` below one megabyte.
pub fn format_size(bytes: u64) -> String {
    if bytes < MB {
        "—".to_string()
    } else if bytes < GB {
        format!("{} MB", bytes / MB)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

/// Sum file sizes under `path`. Symlinks count as links and are never followed;
/// unreadable entries are skipped instead of aborting the walk.
pub fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_type().is_dir())
        .filter_map(|entry| fs::symlink_metadata(entry.path()).ok())
        .map(|meta| meta.len())
        .sum()
}

/// Sum of every target's size, or `None` while any of them is still being measured.
pub fn total_size(targets: &[Target]) -> Option<u64> {
    targets
        .iter()
        .try_fold(0, |sum, target| target.size.map(|size| sum + size))
}

pub struct SizeRequest {
    pub project: PathBuf,
    pub paths: Vec<PathBuf>,
}

pub struct SizeEvent {
    pub project: PathBuf,
    /// One entry per requested path, in request order.
    pub sizes: Vec<(PathBuf, u64)>,
}

/// Measure `requests` on a fixed pool of [`CONCURRENCY`] threads, reporting each
/// project as it finishes. The receiver's channel closes once every project is sent.
pub fn measure(requests: Vec<SizeRequest>, tx: Sender<SizeEvent>) {
    let requests = Arc::new(requests);
    let next = Arc::new(AtomicUsize::new(0));

    for _ in 0..requests.len().min(CONCURRENCY) {
        let requests = Arc::clone(&requests);
        let next = Arc::clone(&next);
        let tx = tx.clone();
        thread::spawn(move || {
            while let Some(request) = requests.get(next.fetch_add(1, Ordering::Relaxed)) {
                let sizes = request
                    .paths
                    .iter()
                    .map(|path| (path.clone(), dir_size(path)))
                    .collect();
                let event = SizeEvent {
                    project: request.project.clone(),
                    sizes,
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
    }
}
