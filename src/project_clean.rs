//! Project Clean draft, confirmation, recheck and Execute lifecycle.
use crate::delete::{self, DeleteEvent, DeleteMode, DeleteReport};
use crate::plan::{PlanItem, Skip};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::plan::{self, DeletePlan, PlanProject};
use crate::rules::{Rule, Target};
use crate::scan::Stack;

pub struct ProjectClean {
    pub project: PathBuf,
    pub plan: DeletePlan,
    pub query: Option<Receiver<DeletePlan>>,
    targets: Vec<Target>,
    confirmed: bool,
    failed: bool,
}

impl ProjectClean {
    pub fn new(project: PathBuf, mut targets: Vec<Target>) -> Self {
        for target in &mut targets {
            target.checked = target.default_checked;
        }
        Self {
            project,
            targets,
            plan: empty(),
            query: None,
            confirmed: false,
            failed: false,
        }
    }
    /// The adapter launches filesystem/process work; the lifecycle owns invalidation.
    pub fn refresh(
        &mut self,
        stack: Stack,
        targets: Vec<Target>,
        roots: Vec<PathBuf>,
        rules: Vec<Rule>,
        launch: impl FnOnce(Box<dyn FnOnce() -> DeletePlan + Send>) -> Receiver<DeletePlan>,
    ) {
        if !self.update(targets) {
            return;
        }
        let task = self.task(stack, roots, rules);
        self.preview(launch(Box::new(task)));
    }
    /// Installing a new preview immediately invalidates the old result.
    pub fn preview(&mut self, receiver: Receiver<DeletePlan>) {
        if self.confirmed {
            return;
        }
        self.plan = empty();
        self.failed = false;
        self.query = Some(receiver);
    }
    pub fn poll(&mut self) {
        if let Some(rx) = &self.query {
            match rx.try_recv() {
                Ok(plan) => {
                    self.plan = plan;
                    self.query = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.failed = true;
                    self.query = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn confirm(&mut self) -> bool {
        if self.confirmed
            || self.query.is_some()
            || self.failed
            || self.plan.items.is_empty()
            || self
                .targets
                .iter()
                .filter(|t| t.checked)
                .any(|t| t.size.is_none())
        {
            return false;
        }
        self.confirmed = true;
        true
    }
    pub fn locked(&self) -> bool {
        self.confirmed
    }
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }
    pub fn edit(&mut self, targets: Vec<Target>) -> bool {
        if self.confirmed {
            return false;
        }
        self.targets = targets;
        self.plan = empty();
        self.query = None;
        true
    }
    /// Refresh sizes/eligibility while retaining the user's current choices.
    pub fn update(&mut self, mut targets: Vec<Target>) -> bool {
        if self.confirmed {
            return false;
        }
        for target in &mut targets {
            if let Some(old) = self
                .targets
                .iter()
                .find(|old| old.path == target.path && old.rule_id == target.rule_id)
            {
                target.checked = old.checked;
            }
        }
        self.edit(targets)
    }
    /// Capture the draft now; executing this task only rechecks that selection.
    pub fn task(
        &self,
        stack: Stack,
        roots: Vec<PathBuf>,
        rules: Vec<Rule>,
    ) -> impl FnOnce() -> DeletePlan + Send + 'static {
        let path = self.project.clone();
        let targets = self.targets.clone();
        move || {
            plan::plan(
                &[PlanProject {
                    path: &path,
                    stack,
                    targets: &targets,
                }],
                &roots,
                &rules,
            )
        }
    }
    /// Recheck only the confirmed preview's items, then retain the final Unity
    /// check in delete::execute. Consuming the confirmation prevents editing or
    /// launching it twice; ProjectExecution owns the lock until it ends.
    /// The adapter controls background scheduling.
    pub fn execute(
        self,
        stack: Stack,
        roots: Vec<PathBuf>,
        rules: Vec<Rule>,
        mode: DeleteMode,
        launch: impl FnOnce(Box<dyn FnOnce() + Send>),
    ) -> Option<ProjectExecution> {
        if !self.confirmed {
            return None;
        }
        let mut selected = self.targets.clone();
        for target in &mut selected {
            target.checked = self
                .plan
                .items
                .iter()
                .any(|item| item.target_path == target.path);
        }
        let path = self.project.clone();
        let items = self.plan.items.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (check_tx, check_rx) = std::sync::mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&abort);
        launch(Box::new(move || {
            let plan = plan::plan(
                &[PlanProject {
                    path: &path,
                    stack,
                    targets: &selected,
                }],
                &roots,
                &rules,
            );
            let _ = check_tx.send(plan.skipped.clone());
            delete::execute(plan, mode, &flag, tx);
        }));
        let mut execution = ProjectExecution::new(rx, items, abort);
        execution.check = Some(check_rx);
        Some(execution)
    }
    pub fn cancel_confirmation(&mut self) {
        self.confirmed = false;
    }
}

/// Execute lifecycle. Partial confirmations survive a worker disconnect; abort
/// remains a request until a report or disconnect ends the task.
pub struct ProjectExecution {
    pub rx: Receiver<DeleteEvent>,
    pub abort: Arc<AtomicBool>,
    pub done: usize,
    pub total: usize,
    pub current: Option<PathBuf>,
    pub deleted: Vec<(PathBuf, Vec<PathBuf>)>,
    pub freed_bytes: u64,
    pub report: Option<DeleteReport>,
    pub interrupted: bool,
    pub skipped: Vec<Skip>,
    applied: usize,
    items: Vec<PlanItem>,
    check: Option<Receiver<Vec<Skip>>>,
}
impl ProjectExecution {
    pub fn new(rx: Receiver<DeleteEvent>, items: Vec<PlanItem>, abort: Arc<AtomicBool>) -> Self {
        Self {
            rx,
            items,
            applied: 0,
            abort,
            done: 0,
            total: 0,
            current: None,
            deleted: vec![],
            freed_bytes: 0,
            report: None,
            interrupted: false,
            skipped: vec![],
            check: None,
        }
    }
    pub fn poll_projects(&mut self, projects: &mut crate::projects::ProjectRefresh) {
        self.poll();
        for (project, deleted) in &self.deleted[self.applied..] {
            if let Some(targets) = projects.targets_mut(project) {
                targets.retain(|target| !deleted.contains(&target.path));
            }
        }
        self.applied = self.deleted.len();
    }
    pub fn running(&self) -> bool {
        self.report.is_none() && !self.interrupted
    }
    pub fn request_abort(&self) {
        self.abort.store(true, Ordering::Relaxed);
    }
    pub fn poll(&mut self) {
        if !self.running() {
            return;
        }
        loop {
            match self.rx.try_recv() {
                Ok(DeleteEvent::Started { project, total }) => {
                    self.current = Some(project);
                    self.total = total;
                }
                Ok(DeleteEvent::ProjectDone { project, deleted }) => {
                    // Count only confirmed paths, once, using their plan-time sizes.
                    let deleted: Vec<_> = deleted
                        .into_iter()
                        .filter(|path| {
                            self.items.iter().any(|item| {
                                item.project_path == project && item.target_path == *path
                            })
                        })
                        .collect();
                    for path in &deleted {
                        self.freed_bytes += self
                            .items
                            .iter()
                            .filter(|item| {
                                item.project_path == project && item.target_path == *path
                            })
                            .map(|item| item.bytes)
                            .sum::<u64>();
                        self.items.retain(|item| {
                            item.project_path != project || item.target_path != *path
                        });
                    }
                    self.deleted.push((project, deleted));
                    self.done += 1;
                }
                Ok(DeleteEvent::Finished(report)) => {
                    self.report = Some(report);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.interrupted = true;
                    break;
                }
            }
        }
        if let Some(rx) = &self.check {
            match rx.try_recv() {
                Ok(skipped) => {
                    self.skipped = skipped;
                    self.check = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.check = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(report) = &mut self.report {
            // The shared delete adapter reports Unity plan refusals as failures
            // for older callers. Project Clean retains their original Skip kind.
            report
                .failures
                .retain(|failure| !self.skipped.iter().any(|skip| skip.path == failure.path));
        }
    }
}

fn empty() -> DeletePlan {
    DeletePlan {
        items: vec![],
        skipped: vec![],
        total_bytes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> Target {
        Target {
            rule_id: "node.modules".into(),
            path: "/project/node_modules".into(),
            size: Some(42),
            checked: true,
            default_checked: true,
            dangerous: false,
            label: None,
        }
    }
    #[test]
    fn execution_disconnect_retains_deleted_bytes_and_abort_lock_until_end() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut execution = ProjectExecution::new(
            rx,
            vec![crate::plan::PlanItem {
                project_path: "/project".into(),
                target_path: target().path,
                bytes: 42,
                large: false,
            }],
            Arc::new(AtomicBool::new(false)),
        );
        tx.send(DeleteEvent::ProjectDone {
            project: "/project".into(),
            deleted: vec![target().path],
        })
        .unwrap();
        execution.request_abort();
        execution.poll();
        assert!(execution.running());
        assert_eq!(execution.freed_bytes, 42);
        drop(tx);
        execution.poll();
        assert!(!execution.running());
        assert!(execution.interrupted);
        assert_eq!(
            execution.deleted,
            vec![(PathBuf::from("/project"), vec![target().path])]
        );
        assert!(execution.report.is_none());
    }
    #[test]
    fn execute_rechecks_only_previewed_targets_and_exposes_refusals() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("node_modules")).unwrap();
        std::fs::create_dir(root.join("dist")).unwrap();
        let project = crate::scan::Project {
            path: root.clone(),
            stack: Stack::Node,
            name: "project".into(),
        };
        let rules = crate::rules::builtin_rules();
        let mut targets = crate::rules::expand_targets(&project, &rules);
        for target in &mut targets {
            target.size = Some(42);
            target.default_checked = true;
        }
        let mut clean = ProjectClean::new(root.clone(), targets);
        let (tx, rx) = std::sync::mpsc::channel();
        clean.preview(rx);
        tx.send(DeletePlan {
            items: vec![PlanItem {
                project_path: root.clone(),
                target_path: root.join("node_modules"),
                bytes: 42,
                large: false,
            }],
            skipped: vec![],
            total_bytes: 42,
        })
        .unwrap();
        clean.poll();
        assert!(clean.confirm());
        let mut task = None;
        let mut execution = clean
            .execute(Stack::Node, vec![], rules, DeleteMode::Direct, |work| {
                task = Some(work)
            })
            .unwrap();
        // The confirmed path disappears before the worker's recheck; dist was
        // selected in the draft but never present in the confirmed preview.
        std::fs::remove_dir(root.join("node_modules")).unwrap();
        task.unwrap()();
        execution.poll();
        assert!(!execution.running());
        assert!(!execution.interrupted);
        assert_eq!(execution.freed_bytes, 0);
        assert_eq!(execution.skipped.len(), 1);
        assert_eq!(
            execution.skipped[0].reason,
            crate::plan::SkipReason::Missing
        );
        assert!(root.join("dist").is_dir());
    }
    #[test]
    fn execution_applies_only_successful_targets_and_retains_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        for path in ["node_modules", "dist"] {
            std::fs::create_dir(root.join(path)).unwrap();
        }
        let mut projects = crate::projects::ProjectRefresh::default();
        projects.refresh(std::slice::from_ref(&root), &crate::rules::builtin_rules());
        let items = vec![PlanItem {
            project_path: root.clone(),
            target_path: root.join("node_modules"),
            bytes: 42,
            large: false,
        }];
        let (tx, rx) = std::sync::mpsc::channel();
        let mut execution = ProjectExecution::new(rx, items, Arc::new(AtomicBool::new(false)));
        let failure = crate::delete::Failure {
            project_path: root.clone(),
            path: root.join("dist"),
            reason: crate::delete::FailureReason::PermissionDenied,
        };
        tx.send(DeleteEvent::ProjectDone {
            project: root.clone(),
            deleted: vec![root.join("node_modules")],
        })
        .unwrap();
        tx.send(DeleteEvent::Finished(DeleteReport {
            freed_bytes: 42,
            failures: vec![failure.clone()],
            projects_done: 1,
            projects_total: 1,
            aborted: false,
        }))
        .unwrap();
        execution.poll_projects(&mut projects);
        assert_eq!(
            projects
                .entry(&root)
                .unwrap()
                .targets
                .iter()
                .map(|target| target.path.clone())
                .collect::<Vec<_>>(),
            vec![root.join("dist")]
        );
        assert_eq!(execution.report.as_ref().unwrap().failures, vec![failure]);
        assert_eq!(execution.freed_bytes, 42);
    }
    #[test]
    fn opening_draft_restores_dangerous_rule_default() {
        let mut risky = target();
        risky.dangerous = true;
        risky.default_checked = false;
        let clean = ProjectClean::new("/project".into(), vec![risky]);
        assert!(!clean.targets()[0].checked);
    }
    #[test]
    fn unknown_selected_size_blocks_confirmation() {
        let mut unknown = target();
        unknown.size = None;
        let mut clean = ProjectClean::new("/project".into(), vec![unknown]);
        let (tx, rx) = std::sync::mpsc::channel();
        clean.preview(rx);
        let mut plan = empty();
        plan.items.push(crate::plan::PlanItem {
            project_path: "/project".into(),
            target_path: "/project/node_modules".into(),
            bytes: 0,
            large: false,
        });
        tx.send(plan).unwrap();
        clean.poll();
        assert!(!clean.confirm());
    }
    #[test]
    fn confirmation_recheck_uses_fixed_targets_on_real_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("node_modules")).unwrap();
        std::fs::create_dir(dir.path().join("build")).unwrap();
        let mut chosen = target();
        chosen.path = dir.path().join("node_modules");
        chosen.rule_id = "node.node_modules".into();
        let clean = ProjectClean::new(dir.path().into(), vec![chosen]);
        let plan = clean.task(Stack::Node, vec![], crate::rules::builtin_rules())();
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].target_path, dir.path().join("node_modules"));
        assert!(dir.path().join("node_modules").is_dir());
    }
    #[test]
    fn measurement_updates_cannot_change_confirmed_selection() {
        let mut clean = ProjectClean::new("/project".into(), vec![target()]);
        let (tx, rx) = std::sync::mpsc::channel();
        clean.preview(rx);
        tx.send(DeletePlan {
            items: vec![crate::plan::PlanItem {
                project_path: "/project".into(),
                target_path: "/project/node_modules".into(),
                bytes: 42,
                large: false,
            }],
            skipped: vec![],
            total_bytes: 42,
        })
        .unwrap();
        clean.poll();
        assert!(clean.confirm());
        assert!(!clean.update(vec![]));
        assert_eq!(clean.targets(), &[target()]);
        clean.cancel_confirmation();
        assert!(clean.update(vec![]));
        assert!(clean.plan.items.is_empty());
    }
    #[test]
    fn only_latest_draft_can_confirm_and_confirmation_locks_selection() {
        let mut clean = ProjectClean::new("/project".into(), vec![target()]);
        let (old_tx, old_rx) = std::sync::mpsc::channel();
        clean.refresh(Stack::Node, vec![target()], vec![], vec![], |_| old_rx);
        let (tx, rx) = std::sync::mpsc::channel();
        clean.refresh(Stack::Node, vec![target()], vec![], vec![], |_| rx);
        assert!(!clean.confirm());
        let plan = DeletePlan {
            items: vec![crate::plan::PlanItem {
                project_path: "/project".into(),
                target_path: "/project/node_modules".into(),
                bytes: 42,
                large: false,
            }],
            skipped: vec![],
            total_bytes: 42,
        };
        tx.send(plan.clone()).unwrap();
        clean.poll();
        let _ = old_tx.send(empty());
        clean.poll();
        assert_eq!(clean.plan, plan);
        assert!(clean.confirm());
        assert!(!clean.edit(vec![]));
        assert_eq!(clean.targets(), &[target()]);
        clean.cancel_confirmation();
        assert!(clean.edit(vec![]));
        assert!(!clean.confirm());
    }
    #[test]
    fn disconnected_preview_cannot_confirm_and_reports_failure() {
        let mut clean = ProjectClean::new(PathBuf::new(), vec![]);
        let (tx, rx) = std::sync::mpsc::channel();
        clean.preview(rx);
        drop(tx);
        clean.poll();
        assert!(clean.failed());
        assert!(!clean.confirm());
    }
}
