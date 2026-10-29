use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crate::delete::{self, DeleteEvent, DeleteMode, DeleteReport, Failure, FailureReason};
use crate::plan::{DeletePlan, LARGE_TARGET_BYTES, PlanItem};

#[derive(Clone)]
pub struct GoCacheContext {
    pub home: PathBuf,
    pub scan_roots: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
    pub other_caches: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoCacheError {
    Unavailable,
    QueryFailed,
    Timeout,
    InvalidOutput,
    Unreadable,
    Disabled,
    Missing,
    NotDirectory,
    Protected,
    Overlap,
    Linked,
    Changed,
}

#[derive(Debug, Clone)]
pub struct GoCacheEntry {
    pub id: &'static str,
    pub path: Option<PathBuf>,
    pub size: Result<u64, GoCacheError>,
}

#[derive(Debug, Clone)]
pub struct GoCacheSnapshot {
    pub entries: [GoCacheEntry; 2],
}

/// A confirmed cache and the protected locations against which it must be rechecked.
pub struct GoCacheClean<'a> {
    pub confirmed: &'a GoCacheSnapshot,
    pub id: &'a str,
    pub context: &'a GoCacheContext,
}

impl GoCacheClean<'_> {
    pub fn execute(
        &self,
        mode: DeleteMode,
        abort: &AtomicBool,
        events: Sender<DeleteEvent>,
        checked: Sender<(GoCacheSnapshot, Option<GoCacheError>)>,
    ) -> DeleteReport {
        self.execute_with_command(
            Command::new("go"),
            Duration::from_secs(3),
            mode,
            abort,
            events,
            checked,
        )
    }

    /// Query and execute through the same seam used by cache discovery.
    pub fn execute_with_command(
        &self,
        command: Command,
        timeout: Duration,
        mode: DeleteMode,
        abort: &AtomicBool,
        events: Sender<DeleteEvent>,
        checked: Sender<(GoCacheSnapshot, Option<GoCacheError>)>,
    ) -> DeleteReport {
        let fresh = discover_with_command(command, self.context, timeout);
        let plan = self.confirmed.plan_against(self.id, &fresh, self.context);
        // Deliver the recheck before Finished, so the UI never loses a rejection reason.
        let _ = checked.send((fresh, plan.as_ref().err().copied()));
        match plan {
            Ok(plan) if self.id == "go.module" => {
                delete::execute_go_module(plan, mode, abort, events)
            }
            Ok(plan) => delete::execute(plan, mode, abort, events),
            Err(_) => {
                let failures = self
                    .confirmed
                    .entry(self.id)
                    .and_then(|entry| entry.path.as_ref())
                    .map(|path| Failure {
                        project_path: path.clone(),
                        path: path.clone(),
                        reason: FailureReason::Other,
                    })
                    .into_iter()
                    .collect();
                let report = DeleteReport {
                    freed_bytes: 0,
                    failures,
                    projects_done: 0,
                    projects_total: usize::from(is_go_cache(self.id)),
                    aborted: abort.load(Ordering::Relaxed),
                };
                let _ = events.send(DeleteEvent::Finished(report.clone()));
                report
            }
        }
    }
}

impl GoCacheSnapshot {
    pub fn entry(&self, id: &str) -> Option<&GoCacheEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Build only a measured, currently safe entry from the fixed Go cache list.
    pub fn plan(&self, id: &str, context: &GoCacheContext) -> Result<DeletePlan, GoCacheError> {
        if !is_go_cache(id) {
            return Err(GoCacheError::InvalidOutput);
        }
        let bytes = self.entry(id).ok_or(GoCacheError::InvalidOutput)?.size?;
        let path = self.validate(id, context)?;
        Ok(DeletePlan {
            items: vec![PlanItem {
                project_path: path.clone(),
                target_path: path,
                bytes,
                large: bytes > LARGE_TARGET_BYTES,
            }],
            skipped: Vec::new(),
            total_bytes: bytes,
        })
    }

    /// A final query may confirm the old path, never authorize a newly configured one.
    pub fn plan_against(
        &self,
        id: &str,
        fresh: &GoCacheSnapshot,
        context: &GoCacheContext,
    ) -> Result<DeletePlan, GoCacheError> {
        let confirmed = self.plan(id, context)?;
        let current = fresh.plan(id, context)?;
        let before = &confirmed.items[0].target_path;
        let after = &current.items[0].target_path;
        if !contains(before, after) || !contains(after, before) {
            return Err(GoCacheError::Changed);
        }
        Ok(confirmed)
    }

    /// Recheck the discovered paths against the current filesystem and protected locations.
    pub fn validate(&self, id: &str, context: &GoCacheContext) -> Result<PathBuf, GoCacheError> {
        let entry = self.entry(id).ok_or(GoCacheError::InvalidOutput)?;
        let path = entry
            .path
            .as_deref()
            .ok_or_else(|| entry.size.err().unwrap_or(GoCacheError::InvalidOutput))?;
        let peer = self
            .entries
            .iter()
            .find(|other| other.id != id)
            .and_then(|other| other.path.as_deref());
        validate_path(path, peer, context)
    }
}

pub fn is_go_cache(id: &str) -> bool {
    matches!(id, "go.build" | "go.module")
}

pub fn discover(context: &GoCacheContext) -> GoCacheSnapshot {
    discover_with_command(Command::new("go"), context, Duration::from_secs(3))
}

/// The command is an external-process seam; cache paths always come from its output.
pub fn discover_with_command(
    mut command: Command,
    context: &GoCacheContext,
    timeout: Duration,
) -> GoCacheSnapshot {
    command
        .args(["env", "GOCACHE", "GOMODCACHE"])
        .env("GOTOOLCHAIN", "local")
        .env("GOWORK", "off")
        .env("GO111MODULE", "off")
        .current_dir(context.home.ancestors().last().unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = query(&mut command, timeout).and_then(|output| {
        let text = String::from_utf8(output).map_err(|_| GoCacheError::InvalidOutput)?;
        let paths: Vec<_> = text.lines().map(PathBuf::from).collect();
        paths.try_into().map_err(|_| GoCacheError::InvalidOutput)
    });
    let paths: Result<[PathBuf; 2], _> = result;
    GoCacheSnapshot {
        entries: ["go.build", "go.module"].map(|id| {
            let path = paths
                .as_ref()
                .ok()
                .map(|paths| paths[usize::from(id == "go.module")].clone());
            let size = match &path {
                Some(path) => {
                    let peer = paths
                        .as_ref()
                        .ok()
                        .map(|paths| paths[usize::from(id != "go.module")].as_path());
                    validate_path(path, peer, context).and_then(|path| measure(&path))
                }
                None => Err(*paths.as_ref().unwrap_err()),
            };
            GoCacheEntry { id, path, size }
        }),
    }
}

fn measure(path: &Path) -> Result<u64, GoCacheError> {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !fs::symlink_metadata(entry.path()).is_ok_and(|meta| linked(&meta)))
        .try_fold(0, |sum, entry| {
            let entry = entry.map_err(|_| GoCacheError::Unreadable)?;
            let meta = fs::symlink_metadata(entry.path()).map_err(|_| GoCacheError::Unreadable)?;
            Ok(sum + if meta.is_dir() { 0 } else { meta.len() })
        })
}

fn query(command: &mut Command, timeout: Duration) -> Result<Vec<u8>, GoCacheError> {
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GoCacheError::Unavailable
        } else {
            GoCacheError::QueryFailed
        }
    })?;
    let stdout = child.stdout.take().ok_or(GoCacheError::QueryFailed)?;
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = match stdout.take(65_537).read_to_end(&mut bytes) {
            Err(_) => Err(GoCacheError::QueryFailed),
            Ok(_) if bytes.len() > 65_536 => Err(GoCacheError::InvalidOutput),
            Ok(_) => Ok(bytes),
        };
        let _ = tx.send(result);
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if result.is_err() {
                    GoCacheError::QueryFailed
                } else {
                    GoCacheError::Timeout
                });
            }
        }
    };
    if !status.success() {
        return Err(GoCacheError::QueryFailed);
    }
    rx.recv_timeout(timeout.saturating_sub(started.elapsed()))
        .map_err(|_| GoCacheError::Timeout)?
}

fn validate_path(
    path: &Path,
    peer: Option<&Path>,
    context: &GoCacheContext,
) -> Result<PathBuf, GoCacheError> {
    if path == Path::new("off") || path.as_os_str().is_empty() {
        return Err(GoCacheError::Disabled);
    }
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(GoCacheError::InvalidOutput);
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|meta| linked(&meta)) {
            return Err(GoCacheError::Linked);
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GoCacheError::Missing
        } else {
            GoCacheError::Unreadable
        }
    })?;
    if !meta.is_dir() {
        return Err(GoCacheError::NotDirectory);
    }
    let path = path.canonicalize().map_err(|_| GoCacheError::Unreadable)?;
    let home = context
        .home
        .canonicalize()
        .map_err(|_| GoCacheError::Unreadable)?;
    if path.parent().is_none() || contains(&path, &home) {
        return Err(GoCacheError::Protected);
    }
    for root in &context.scan_roots {
        if contains(&path, &normalize_known_path(root)?) {
            return Err(GoCacheError::Protected);
        }
    }
    for other in context
        .projects
        .iter()
        .chain(&context.other_caches)
        .map(PathBuf::as_path)
        .chain(peer.filter(|path| path.is_absolute()))
    {
        let other = normalize_known_path(other)?;
        if contains(&other, &path) || contains(&path, &other) {
            return Err(GoCacheError::Overlap);
        }
    }
    Ok(path)
}

fn normalize_known_path(path: &Path) -> Result<PathBuf, GoCacheError> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(GoCacheError::Unreadable)?;
            let name = path.file_name().ok_or(GoCacheError::Unreadable)?;
            Ok(normalize_known_path(parent)?.join(name))
        }
        Err(_) => Err(GoCacheError::Unreadable),
    }
}

pub(crate) fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn contains(parent: &Path, child: &Path) -> bool {
    #[cfg(windows)]
    {
        let mut child = child.components();
        parent.components().all(|part| {
            child.next().is_some_and(|other| {
                part.as_os_str()
                    .as_encoded_bytes()
                    .eq_ignore_ascii_case(other.as_os_str().as_encoded_bytes())
            })
        })
    }
    #[cfg(not(windows))]
    {
        child.starts_with(parent)
    }
}
