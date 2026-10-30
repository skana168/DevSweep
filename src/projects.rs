//! Project discovery and the Target state preserved by Refresh.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::rules::{self, Rule, Target};
use crate::scan::{self, Project};
use crate::size::{SizeEvent, SizeRequest};

pub struct ProjectEntry {
    pub project: Project,
    pub targets: Vec<Target>,
}

#[derive(Default)]
pub struct ProjectRefresh {
    entries: Vec<ProjectEntry>,
}

impl ProjectRefresh {
    pub fn entries(&self) -> &[ProjectEntry] {
        &self.entries
    }

    pub fn entry(&self, path: &Path) -> Option<&ProjectEntry> {
        self.entries.iter().find(|entry| entry.project.path == path)
    }

    pub fn targets_mut(&mut self, path: &Path) -> Option<&mut Vec<Target>> {
        self.entries
            .iter_mut()
            .find(|entry| entry.project.path == path)
            .map(|entry| &mut entry.targets)
    }

    /// Rediscover Projects without recomputing surviving Target sizes or drafts.
    pub fn refresh(&mut self, roots: &[PathBuf], rules: &[Rule]) {
        self.reconcile(scan::scan(&scan::active_roots(roots)), rules);
    }

    /// Discover only the new subtree; merge ownership before retaining Target state.
    pub fn add_root(&mut self, root: &Path, rules: &[Rule]) {
        let mut projects = self
            .entries
            .iter()
            .map(|entry| entry.project.clone())
            .collect();
        scan::merge_scan(&mut projects, scan::scan(&[root.to_path_buf()]));
        self.reconcile(projects, rules);
    }

    /// A surviving child Scan root can regain Projects suppressed by its former parent.
    pub fn remove_root(&mut self, roots: &[PathBuf], rules: &[Rule]) {
        let mut projects = self
            .entries
            .iter()
            .filter(|entry| {
                roots
                    .iter()
                    .any(|root| entry.project.path.starts_with(root))
            })
            .map(|entry| entry.project.clone())
            .collect();
        scan::merge_scan(&mut projects, scan::scan(&scan::active_roots(roots)));
        self.reconcile(projects, rules);
    }

    fn reconcile(&mut self, projects: Vec<Project>, rules: &[Rule]) {
        let mut targets: HashMap<_, _> = self
            .entries
            .drain(..)
            .map(|entry| (entry.project.path, entry.targets))
            .collect();
        self.entries = projects
            .into_iter()
            .map(|project| {
                let targets = if project.stack == scan::Stack::Unity {
                    Vec::new()
                } else {
                    targets
                        .remove(&project.path)
                        .unwrap_or_else(|| rules::expand_targets(&project, rules))
                };
                ProjectEntry { project, targets }
            })
            .collect();
    }

    pub fn measurement_requests(&self) -> Vec<SizeRequest> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let paths: Vec<_> = entry
                    .targets
                    .iter()
                    .filter(|target| target.size.is_none())
                    .map(|target| target.path.clone())
                    .collect();
                (!paths.is_empty()).then(|| SizeRequest {
                    project: entry.project.path.clone(),
                    paths,
                })
            })
            .collect()
    }

    /// A result for a vanished Project or Target cannot recreate its state.
    pub fn apply_measurement(&mut self, event: &SizeEvent) {
        if let Some(targets) = self.targets_mut(&event.project) {
            for (path, size) in &event.sizes {
                if let Some(target) = targets.iter_mut().find(|target| &target.path == path) {
                    target.size = Some(*size);
                }
            }
        }
    }
}
