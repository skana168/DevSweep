//! Refresh lifecycle. Execution adapters perform I/O; this module owns sequencing.
use crate::{
    projects::ProjectRefresh,
    rules::Rule,
    size::{SizeEvent, SizeRequest},
};
use std::{
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
    pending_measurements: Vec<SizeRequest>,
    pub measurement: Option<Measurement>,
    pub failed: bool,
}

impl Refresh {
    /// Start measurements, including independent cache requests supplied by the adapter.
    pub fn measure(&mut self, requests: Vec<SizeRequest>, executor: &mut impl RefreshExecutor) {
        if requests.is_empty() {
            return;
        }
        if self.measurement.is_some() {
            self.pending_measurements.extend(requests);
            return;
        }
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
                    projects.apply_measurement(&event);
                    events.push(event);
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
        if self.scan.is_some() || self.measurement.is_some() {
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
                projects.apply_refresh(found);
                true
            }
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                self.failed = true;
                false
            }
        };
        self.scan = None;
        if changed {
            self.measure(projects.measurement_requests(), executor);
        }
        if std::mem::take(&mut self.queued) {
            self.request(projects, roots, rules, executor);
        }
        changed
    }
}
