use std::fs;
use std::path::Path;

use devsweep::projects::ProjectRefresh;
use devsweep::rules::builtin_rules;
use devsweep::size::SizeEvent;

fn project(path: &Path) {
    fs::create_dir_all(path.join("node_modules")).unwrap();
    fs::write(path.join("package.json"), "{}").unwrap();
}

#[test]
fn refresh_preserves_surviving_target_drafts_and_sizes_and_measures_only_new_projects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let alpha = root.join("alpha");
    project(&alpha);
    let rules = builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    let requests = projects.measurement_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].project, alpha);
    projects.apply_measurement(&SizeEvent {
        project: alpha.clone(),
        sizes: vec![(alpha.join("node_modules"), 37)],
    });
    projects.targets_mut(&alpha).unwrap()[0].checked = false;
    project(&root.join("beta"));
    // Refresh discovers Projects, not newly created Targets inside surviving Projects.
    fs::create_dir(alpha.join("dist")).unwrap();

    projects.refresh(std::slice::from_ref(&root), &rules);

    assert_eq!(projects.entries().len(), 2);
    assert_eq!(projects.entry(&alpha).unwrap().targets.len(), 1);
    let target = &projects.entry(&alpha).unwrap().targets[0];
    assert_eq!(target.size, Some(37));
    assert!(!target.checked);
    let requests = projects.measurement_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].project, root.join("beta"));
}

#[test]
fn adding_a_scan_root_keeps_existing_target_state_and_merges_platform_projects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let flutter = root.join("flutter");
    let android = flutter.join("android");
    fs::create_dir_all(android.join("build")).unwrap();
    fs::write(android.join("build.gradle"), "").unwrap();
    fs::write(flutter.join("pubspec.yaml"), "").unwrap();
    let alpha = root.join("alpha");
    project(&alpha);
    let rules = builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.add_root(&alpha, &rules);
    projects.targets_mut(&alpha).unwrap()[0].checked = false;
    projects.apply_measurement(&SizeEvent {
        project: alpha.clone(),
        sizes: vec![(alpha.join("node_modules"), 19)],
    });
    projects.add_root(&android, &rules);
    assert_eq!(projects.entries().len(), 2);

    projects.add_root(&flutter, &rules);

    assert_eq!(projects.entries().len(), 2);
    assert!(projects.entry(&android).is_none());
    assert!(projects.entry(&flutter).is_some());
    let target = &projects.entry(&alpha).unwrap().targets[0];
    assert!(!target.checked);
    assert_eq!(target.size, Some(19));
}

#[test]
fn removing_a_parent_scan_root_restores_the_surviving_platform_project_and_drops_late_results() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let parent = root.join("flutter");
    let child = parent.join("android");
    fs::create_dir_all(child.join("build")).unwrap();
    fs::write(parent.join("pubspec.yaml"), "").unwrap();
    fs::write(child.join("build.gradle"), "").unwrap();
    let rules = builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(&[parent.clone(), child.clone()], &rules);
    assert_eq!(projects.entries().len(), 1);
    assert!(projects.entry(&child).is_none());

    projects.remove_root(std::slice::from_ref(&child), &rules);

    assert_eq!(projects.entries().len(), 1);
    assert_eq!(
        projects.entries()[0].project.stack,
        devsweep::scan::Stack::Android
    );
    assert!(projects.entry(&parent).is_none());
    assert_eq!(projects.measurement_requests()[0].project, child);
    projects.remove_root(&[], &rules);
    projects.apply_measurement(&SizeEvent {
        project: child.clone(),
        sizes: vec![(child.join("build"), 400)],
    });
    assert!(projects.entries().is_empty());
    assert!(projects.measurement_requests().is_empty());
}

#[test]
fn refresh_drops_vanished_projects_and_late_results_cannot_restore_projects_or_deleted_targets() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let alpha = root.join("alpha");
    let beta = root.join("beta");
    project(&alpha);
    project(&beta);
    let rules = builtin_rules();
    let mut projects = ProjectRefresh::default();
    projects.refresh(std::slice::from_ref(&root), &rules);
    fs::remove_dir_all(&beta).unwrap();
    projects.targets_mut(&alpha).unwrap().clear();

    projects.refresh(std::slice::from_ref(&root), &rules);
    for path in [&alpha, &beta] {
        projects.apply_measurement(&SizeEvent {
            project: path.clone(),
            sizes: vec![(path.join("node_modules"), 123)],
        });
    }

    assert_eq!(projects.entries().len(), 1);
    assert_eq!(projects.entries()[0].project.path, alpha);
    assert!(projects.entries()[0].targets.is_empty());
    assert!(projects.entry(&beta).is_none());
    assert!(projects.measurement_requests().is_empty());
}
