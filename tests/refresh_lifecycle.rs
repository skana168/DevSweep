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
    roots: Vec<Vec<PathBuf>>,
}
impl RefreshExecutor for Controlled {
    fn discover(
        &mut self,
        _: ProjectRefresh,
        roots: Vec<PathBuf>,
        _: Vec<Rule>,
    ) -> Receiver<ProjectRefresh> {
        let (tx, rx) = mpsc::channel();
        self.roots.push(roots);
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

#[test]
fn root_edits_reject_old_discovery_and_restore_retained_child_in_background() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let child = root.join("android");
    std::fs::create_dir_all(child.join("build")).unwrap();
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::write(root.join("pubspec.yaml"), "").unwrap();
    std::fs::write(child.join("build.gradle"), "").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    let old = projects.clone();
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.measure(projects.measurement_requests(), &mut executor);
    // Root edits supersede discovery even while an earlier measurement is alive.
    refresh.roots_removed(
        &mut projects,
        std::slice::from_ref(&root),
        &rules,
        &mut executor,
    );
    refresh.roots_removed(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    assert!(
        projects.entries().is_empty(),
        "removed ownership changes immediately"
    );
    executor.measurements[0]
        .send(SizeEvent {
            project: root.clone(),
            sizes: vec![(root.join("build"), 99)],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    refresh.poll_measurement(&mut projects);
    assert!(
        projects.entries().is_empty(),
        "late sizes cannot restore a removed Project"
    );
    let _ = executor.discoveries[0].send(old);
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    assert!(projects.entries().is_empty());
    let mut found = ProjectRefresh::default();
    found.refresh(std::slice::from_ref(&child), &rules);
    executor.discoveries[1].send(found).unwrap();
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    assert_eq!(projects.entries().len(), 1);
    assert!(projects.entry(&child).is_some());
}

#[test]
fn repeated_additions_scan_only_new_subtrees_and_merge_latest_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let a = root.join("a");
    let b = root.join("b");
    let c = root.join("c");
    for path in [&a, &b, &c] {
        std::fs::create_dir_all(path.join("node_modules")).unwrap();
        std::fs::write(path.join("package.json"), "{}").unwrap();
    }
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&a), &rules);
    projects.targets_mut(&a).unwrap()[0].size = Some(41);
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.root_added(&b, &rules, &mut executor);
    refresh.root_added(&c, &rules, &mut executor);
    assert_eq!(
        executor.roots,
        vec![vec![b.clone()], vec![b.clone(), c.clone()]]
    );
    projects.targets_mut(&a).unwrap()[0].checked = false;
    let mut found = ProjectRefresh::default();
    found.refresh(&[b.clone(), c.clone()], &rules);
    let _ = executor.discoveries[0].send(ProjectRefresh::default());
    executor.discoveries[1].send(found).unwrap();
    refresh.poll_discovery(&mut projects, &[a.clone(), b, c], &rules, &mut executor);
    assert_eq!(projects.entries().len(), 3);
    let target = &projects.entry(&a).unwrap().targets[0];
    assert_eq!(target.size, Some(41));
    assert!(!target.checked);
}

#[test]
fn confirmation_defers_repeated_refresh_until_cancel_and_idle_does_not_refresh() {
    use devsweep::project_clean::ProjectClean;
    let mut clean = ProjectClean::new("project".into(), vec![]);
    clean.plan.items.push(devsweep::plan::PlanItem {
        project_path: "project".into(),
        target_path: "project/target".into(),
        bytes: 10,
        large: false,
    });
    assert!(clean.confirm());
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    let mut projects = ProjectRefresh::default();
    refresh.coordinate(Some(&clean), None);
    for _ in 0..3 {
        refresh.request(&projects, &[], &[], &mut executor);
        refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    }
    assert!(executor.discoveries.is_empty());
    clean.cancel_confirmation();
    refresh.coordinate(Some(&clean), None);
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert_eq!(executor.discoveries.len(), 1);
    executor.discoveries[0]
        .send(ProjectRefresh::default())
        .unwrap();
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
    assert_eq!(executor.discoveries.len(), 1);
}

#[test]
fn execution_holds_refresh_through_abort_and_releases_on_report_or_disconnect() {
    use devsweep::{
        delete::{DeleteEvent, DeleteReport},
        project_clean::ProjectExecution,
    };
    use std::sync::{Arc, atomic::AtomicBool};
    for disconnect in [false, true] {
        for requested in [false, true] {
            let (tx, rx) = mpsc::channel();
            let mut execution = ProjectExecution::new(rx, vec![], Arc::new(AtomicBool::new(false)));
            let mut refresh = Refresh::default();
            let mut executor = Controlled::default();
            let mut projects = ProjectRefresh::default();
            refresh.coordinate(None, Some(&execution));
            if requested {
                for _ in 0..3 {
                    refresh.request(&projects, &[], &[], &mut executor);
                }
            }
            execution.request_abort();
            execution.poll_projects(&mut projects);
            refresh.coordinate(None, Some(&execution));
            refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
            assert!(executor.discoveries.is_empty());
            if !disconnect {
                tx.send(DeleteEvent::Finished(DeleteReport {
                    aborted: true,
                    freed_bytes: 0,
                    failures: vec![],
                    projects_done: 0,
                    projects_total: 1,
                }))
                .unwrap();
            }
            drop(tx);
            execution.poll_projects(&mut projects);
            assert!(!execution.running());
            refresh.coordinate(None, Some(&execution));
            refresh.poll_discovery(&mut projects, &[], &[], &mut executor);
            assert_eq!(executor.discoveries.len(), usize::from(requested));
        }
    }
}

#[test]
fn running_discovery_and_late_measurement_keep_successful_deletions() {
    use devsweep::{
        delete::DeleteEvent,
        plan::PlanItem,
        project_clean::{ProjectClean, ProjectExecution},
    };
    use std::sync::{Arc, atomic::AtomicBool};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("node_modules")).unwrap();
    std::fs::write(root.join("package.json"), "{}").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    // Ordinary preview does not defer discovery.
    let clean = ProjectClean::new(root.clone(), projects.entry(&root).unwrap().targets.clone());
    refresh.coordinate(Some(&clean), None);
    refresh.request(
        &projects,
        std::slice::from_ref(&root),
        &rules,
        &mut executor,
    );
    let stale = projects.clone();
    refresh.measure(projects.measurement_requests(), &mut executor);
    let target = root.join("node_modules");
    let (tx, rx) = mpsc::channel();
    let mut execution = ProjectExecution::new(
        rx,
        vec![PlanItem {
            project_path: root.clone(),
            target_path: target.clone(),
            bytes: 37,
            large: false,
        }],
        Arc::new(AtomicBool::new(false)),
    );
    tx.send(DeleteEvent::ProjectDone {
        project: root.clone(),
        deleted: vec![target.clone()],
    })
    .unwrap();
    drop(tx);
    execution.poll_projects(&mut projects);
    executor.discoveries[0].send(stale).unwrap();
    refresh.coordinate(None, Some(&execution));
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&root),
        &rules,
        &mut executor,
    );
    executor.measurements[0]
        .send(SizeEvent {
            project: root.clone(),
            sizes: vec![(target, 37)],
        })
        .unwrap();
    executor.measurements.clear();
    refresh.present_progress();
    refresh.poll_measurement(&mut projects);
    assert!(projects.entry(&root).unwrap().targets.is_empty());
    assert_eq!(execution.freed_bytes, 37);
    assert!(execution.interrupted);
    assert_eq!(executor.discoveries.len(), 1);
}

#[test]
fn root_change_after_cancel_supersedes_old_scan_and_queued_refresh_scope() {
    use devsweep::{plan::PlanItem, project_clean::ProjectClean};
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let child = parent.join("android");
    std::fs::create_dir_all(child.join("build")).unwrap();
    std::fs::create_dir(parent.join("build")).unwrap();
    std::fs::write(parent.join("pubspec.yaml"), "").unwrap();
    std::fs::write(child.join("build.gradle"), "").unwrap();
    let rules = devsweep::rules::builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&parent), &rules);
    for target in projects.targets_mut(&parent).unwrap() {
        target.size = Some(12);
    }
    let mut refresh = Refresh::default();
    let mut executor = Controlled::default();
    refresh.request(
        &projects,
        std::slice::from_ref(&parent),
        &rules,
        &mut executor,
    );
    let stale = projects.clone();
    let mut clean = ProjectClean::new(
        parent.clone(),
        projects.entry(&parent).unwrap().targets.clone(),
    );
    clean.plan.items.push(PlanItem {
        project_path: parent.clone(),
        target_path: parent.join("build"),
        bytes: 12,
        large: false,
    });
    assert!(clean.confirm());
    refresh.coordinate(Some(&clean), None);
    for _ in 0..3 {
        refresh.request(
            &projects,
            std::slice::from_ref(&parent),
            &rules,
            &mut executor,
        );
    }
    assert_eq!(executor.discoveries.len(), 1);
    clean.cancel_confirmation();
    refresh.coordinate(Some(&clean), None);
    refresh.roots_removed(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    assert!(projects.entries().is_empty());
    assert!(
        executor.discoveries[0].send(stale).is_err(),
        "superseded result is disconnected"
    );
    let mut recovered = ProjectRefresh::default();
    recovered.refresh(std::slice::from_ref(&child), &rules);
    for target in recovered.targets_mut(&child).unwrap() {
        target.size = Some(7);
    }
    executor.discoveries[1].send(recovered).unwrap();
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    refresh.poll_discovery(
        &mut projects,
        std::slice::from_ref(&child),
        &rules,
        &mut executor,
    );
    assert!(projects.entry(&parent).is_none());
    assert!(projects.entry(&child).is_some());
    assert_eq!(executor.roots, vec![vec![parent], vec![child]]);
    assert!(
        refresh.scan.is_none(),
        "scope recovery satisfies the deferred Refresh"
    );
}
