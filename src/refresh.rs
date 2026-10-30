//! Refresh lifecycle. Execution adapters perform I/O; this module owns sequencing.
use crate::{
    projects::ProjectRefresh,
    rules::Rule,
    size::{SizeEvent, SizeRequest},
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
};

pub trait RefreshExecutor {
    fn discover(
        &mut self,
        projects: ProjectRefresh,
        roots: Vec<PathBuf>,
        rules: Vec<Rule>,
    ) -> Receiver<ProjectRefresh>;
    fn measure(&mut self, requests: Vec<SizeRequest>) -> Receiver<crate::size::SizeEvent>;
}

pub struct Measurement {
    pub shown: bool,
    pub done: usize,
    pub total: usize,
    pub rx: Receiver<SizeEvent>,
}

#[derive(Default)]
pub struct Refresh {
    pub scan: Option<Receiver<ProjectRefresh>>,
    queued: bool,
    additions: Vec<PathBuf>,
    clean_locked: bool,
    pending_measurements: Vec<SizeRequest>,
    pub measurement: Option<Measurement>,
    invalidated_measurements: HashSet<PathBuf>,
    pub failed: bool,
}

impl Refresh {
    /// Discover only newly added subtrees; superseded additions are retried together.
    pub fn root_added(
        &mut self,
        root: &std::path::Path,
        rules: &[Rule],
        executor: &mut impl RefreshExecutor,
    ) {
        // A superseded full Refresh still needs a follow-up over the final roots.
        self.queued |= self.scan.is_some() && self.additions.is_empty();
        if !self.additions.iter().any(|known| known == root) {
            self.additions.push(root.to_path_buf());
        }
        self.failed = false;
        self.scan = Some(executor.discover(
            ProjectRefresh::default(),
            self.additions.clone(),
            rules.to_vec(),
        ));
    }

    /// Root removal takes effect immediately; recovery discovery runs in the adapter.
    pub fn roots_removed(
        &mut self,
        projects: &mut ProjectRefresh,
        roots: &[PathBuf],
        rules: &[Rule],
        executor: &mut impl RefreshExecutor,
    ) {
        // Invalidation lasts for the active batch, even if the same path is
        // discovered again before its old result arrives. Surviving Projects and
        // independent cache measurements keep their ownership and progress.
        let removed: HashSet<_> = projects
            .entries()
            .iter()
            .filter(|entry| {
                !roots
                    .iter()
                    .any(|root| entry.project.path.starts_with(root))
            })
            .map(|entry| entry.project.path.clone())
            .collect();
        self.invalidated_measurements
            .extend(removed.iter().cloned());
        self.pending_measurements
            .retain(|request| !removed.contains(&request.project));
        projects.retain_roots(roots);
        self.additions.clear();
        self.scan = None;
        self.queued = false;
        self.failed = false;
        self.scan = Some(executor.discover(projects.clone(), roots.to_vec(), rules.to_vec()));
    }

    /// Observe Clean lifecycle state. Only discovery is deferred; running discovery
    /// and measurements may still finish and merge into current Project state.
    pub fn coordinate(
        &mut self,
        clean: Option<&crate::project_clean::ProjectClean>,
        execution: Option<&crate::project_clean::ProjectExecution>,
    ) {
        self.clean_locked = clean.is_some_and(|clean| clean.locked())
            || execution.is_some_and(|execution| execution.running());
    }

    /// Start measurements, including independent cache requests supplied by the adapter.
    pub fn measure(&mut self, requests: Vec<SizeRequest>, executor: &mut impl RefreshExecutor) {
        if requests.is_empty() {
            return;
        }
        if self.measurement.is_some() {
            self.pending_measurements.extend(requests);
            return;
        }
        self.invalidated_measurements.clear();
        let total = requests.len();
        self.measurement = Some(Measurement {
            shown: false,
            done: 0,
            total,
            rx: executor.measure(requests),
        });
    }

    /// Acknowledge a presentation opportunity before consuming even instant results.
    pub fn present_progress(&mut self) {
        if let Some(progress) = &mut self.measurement {
            progress.shown = true;
        }
    }

    /// Return accepted events so the cache presentation adapter can update its rows too.
    pub fn poll_measurement(&mut self, projects: &mut ProjectRefresh) -> Vec<SizeEvent> {
        let mut events = Vec::new();
        let Some(progress) = &mut self.measurement else {
            return events;
        };
        if !progress.shown {
            return events;
        }
        loop {
            match progress.rx.try_recv() {
                Ok(event) => {
                    progress.done += 1;
                    if !self.invalidated_measurements.contains(&event.project) {
                        projects.apply_measurement(&event);
                        events.push(event);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.failed |= progress.done != progress.total;
                    self.measurement = None;
                    break;
                }
            }
        }
        events
    }

    pub fn request(
        &mut self,
        projects: &ProjectRefresh,
        roots: &[PathBuf],
        rules: &[Rule],
        executor: &mut impl RefreshExecutor,
    ) {
        if self.clean_locked || self.scan.is_some() || self.measurement.is_some() {
            self.queued = true;
            return;
        }
        self.queued = false;
        self.failed = false;
        self.scan = Some(executor.discover(projects.clone(), roots.to_vec(), rules.to_vec()));
    }

    pub fn poll_discovery(
        &mut self,
        projects: &mut ProjectRefresh,
        roots: &[PathBuf],
        rules: &[Rule],
        executor: &mut impl RefreshExecutor,
    ) -> bool {
        if self.measurement.is_none() && !self.pending_measurements.is_empty() {
            let mut requests = std::mem::take(&mut self.pending_measurements);
            for request in &mut requests {
                if let Some(entry) = projects.entry(&request.project) {
                    request.paths.retain(|path| {
                        entry
                            .targets
                            .iter()
                            .any(|target| target.path == *path && target.size.is_none())
                    });
                }
            }
            requests.retain(|request| !request.paths.is_empty());
            self.measure(requests, executor);
        }
        let Some(rx) = &self.scan else {
            if self.queued && self.measurement.is_none() {
                self.request(projects, roots, rules, executor);
            }
            return false;
        };
        let changed = match rx.try_recv() {
            Ok(found) => {
                if self.additions.is_empty() {
                    projects.apply_refresh(found);
                } else {
                    projects.apply_addition(found);
                }
                true
            }
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                self.failed = true;
                false
            }
        };
        self.scan = None;
        self.additions.clear();
        if changed {
            self.measure(projects.measurement_requests(), executor);
        }
        if std::mem::take(&mut self.queued) {
            self.request(projects, roots, rules, executor);
        }
        changed
    }
}
