//! Project Clean draft, preview invalidation and fixed confirmation selection.
use std::path::PathBuf;
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
    pub fn cancel_confirmation(&mut self) {
        self.confirmed = false;
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
