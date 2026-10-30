use devsweep::{
    projects::ProjectRefresh,
    refresh::{Refresh, RefreshExecutor},
    rules::Rule,
    size::{SizeEvent, SizeRequest},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Default)]
struct Controlled {
    discoveries: Vec<Sender<ProjectRefresh>>,
    measurements: Vec<Sender<SizeEvent>>,
}
impl RefreshExecutor for Controlled {
    fn discover(
        &mut self,
        _: ProjectRefresh,
        _: Vec<PathBuf>,
        _: Vec<Rule>,
    ) -> Receiver<ProjectRefresh> {
        let (tx, rx) = mpsc::channel();
        self.discoveries.push(tx);
        rx
    }
    fn measure(&mut self, _: Vec<SizeRequest>) -> Receiver<SizeEvent> {
        let (tx, rx) = mpsc::channel();
        self.measurements.push(tx);
        rx
    }
}

#[test]
fn repeated_requests_coalesce_and_keep_one_followup_discovery() {
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    let mut projects = ProjectRefresh::default();
    refresh.request(&projects, &[], &[], &mut executor);
    refresh.request(&projects, &[], &[], &mut executor);
    refresh.request(&projects, &[], &[], &mut executor);
    assert_eq!(executor.discoveries.len(), 1);
    executor.discoveries[0]
        .send(ProjectRefresh::default())
        .unwrap();
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert_eq!(executor.discoveries.len(), 2);
    executor.discoveries[1]
        .send(ProjectRefresh::default())
        .unwrap();
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert!(refresh.scan.is_none());
}

#[test]
fn discovery_merges_latest_draft_then_measures_only_unknown_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("node_modules")).unwrap();
    std::fs::write(root.join("package.json"), "{}").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    let mut executor = Controlled::default();
    let mut refresh = Refresh::default();
    refresh.request(
        &projects,
        std::slice::from_ref(&root),
        &rules,
        &mut executor,
    );
    let found = projects.clone();
    projects.targets_mut(&root).unwrap()[0].checked = false;
    executor.discoveries[0].send(found).unwrap();
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&root),
        &rules,
        &mut executor,
    );
    assert!(!projects.entry(&root).unwrap().targets[0].checked);
    assert!(refresh.measurement.is_some());
    executor.measurements[0]
        .send(SizeEvent {
            project: root.clone(),
            sizes: vec![(root.join("node_modules"), 37)],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.poll_measurement(&mut projects);
    assert_eq!(projects.entry(&root).unwrap().targets[0].size, None);
    refresh.present_progress();
    refresh.poll_measurement(&mut projects);
    assert_eq!(projects.entry(&root).unwrap().targets[0].size, Some(37));
    assert!(refresh.measurement.is_none());
    assert!(!refresh.failed);
}

#[test]
fn disconnected_measurement_keeps_partial_results_and_allows_retry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for name in ["a", "b"] {
        std::fs::create_dir_all(root.join(name).join("node_modules")).unwrap();
        std::fs::write(root.join(name).join("package.json"), "{}").unwrap();
    }
    let mut projects = ProjectRefresh::default();
    projects.refresh(
        std::slice::from_ref(&root),
        &devsweep::rules::builtin_rules(),
    );
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.measure(projects.measurement_requests(), &mut executor);
    executor.measurements[0]
        .send(SizeEvent {
            project: root.join("a"),
            sizes: vec![(root.join("a/node_modules"), 23)],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    assert_eq!(refresh.poll_measurement(&mut projects).len(), 1);
    assert_eq!(
        projects.entry(&root.join("a")).unwrap().targets[0].size,
        Some(23)
    );
    assert_eq!(
        projects.entry(&root.join("b")).unwrap().targets[0].size,
        None
    );
    assert!(refresh.failed);
    assert!(refresh.measurement.is_none());
    refresh.request(&projects, &[], &[], &mut executor);
    assert!(!refresh.failed);
    executor.discoveries.clear();
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert!(refresh.failed);
    assert!(refresh.scan.is_none());
    assert_eq!(projects.entries().len(), 2);
}

#[test]
fn refresh_during_measurement_waits_without_discarding_inflight_results() {
    let mut projects = ProjectRefresh::default();
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.measure(
        vec![SizeRequest {
            project: "a".into(),
            paths: vec![],
        }],
        &mut executor,
    );
    refresh.request(&projects, &[], &[], &mut executor);
    assert!(executor.discoveries.is_empty());
    executor.measurements[0]
        .send(SizeEvent {
            project: "a".into(),
            sizes: vec![],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    assert_eq!(refresh.poll_measurement(&mut projects).len(), 1);
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert_eq!(executor.discoveries.len(), 1);
}

#[test]
fn additional_measurements_do_not_replace_an_active_batch() {
    let mut projects = ProjectRefresh::default();
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.measure(
        vec![SizeRequest {
            project: "a".into(),
            paths: vec!["target".into()],
        }],
        &mut executor,
    );
    refresh.measure(
        vec![SizeRequest {
            project: "b".into(),
            paths: vec!["target".into()],
        }],
        &mut executor,
    );
    assert_eq!(executor.measurements.len(), 1);
    executor.measurements[0]
        .send(SizeEvent {
            project: "a".into(),
            sizes: vec![],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    assert_eq!(
        refresh.poll_measurement(&mut projects)[0].project,
        PathBuf::from("a")
    );
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert!(refresh.measurement.is_some());
    assert_eq!(executor.measurements.len(), 1);
}

#[test]
fn queued_measurements_skip_targets_that_became_known() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("node_modules")).unwrap();
    std::fs::write(root.join("package.json"), "{}").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    let mut executor = Controlled::default();
    let mut refresh = Refresh::default();
    refresh.measure(projects.measurement_requests(), &mut executor);
    refresh.measure(projects.measurement_requests(), &mut executor);
    executor.measurements[0]
        .send(SizeEvent {
            project: root.clone(),
            sizes: vec![(root.join("node_modules"), 19)],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    refresh.poll_measurement(&mut projects);
    refresh.poll_discovery(&mut projects, &[root], &rules, &mut executor);
    assert!(refresh.measurement.is_none());
    assert!(executor.measurements.is_empty());
}
