#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use devsweep::delete::{DeleteEvent, DeleteMode, DeleteReport};
use devsweep::go_cache::{
    GoCacheClean, GoCacheContext, GoCacheError, GoCacheSnapshot, discover_with_command,
};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    go: PathBuf,
    context: GoCacheContext,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let build = root.join("build 缓存");
        let module = root.join("modules");
        for path in [&home, &build, &module] {
            fs::create_dir(path).unwrap();
        }
        fs::write(build.join("artifact"), b"build").unwrap();
        fs::write(module.join("dependency"), b"module").unwrap();
        let go = root.join("go");
        fs::write(
            &go,
            "#!/bin/sh\nprintf '%s\\n' \"$FIXTURE_BUILD\" \"$FIXTURE_MODULE\"\n",
        )
        .unwrap();
        fs::set_permissions(&go, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _temp: temp,
            context: GoCacheContext {
                home,
                scan_roots: Vec::new(),
                projects: Vec::new(),
                other_caches: Vec::new(),
            },
            root,
            go,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.go);
        command.env("FIXTURE_BUILD", self.root.join("build 缓存"));
        command.env("FIXTURE_MODULE", self.root.join("modules"));
        command
    }

    fn clean_build(
        &self,
        confirmed: &GoCacheSnapshot,
        command: Command,
        aborted: bool,
    ) -> (DeleteReport, Option<GoCacheError>) {
        let (tx, events) = mpsc::channel();
        let (checked_tx, checked) = mpsc::channel();
        let report = GoCacheClean {
            confirmed,
            id: "go.build",
            context: &self.context,
        }
        .execute_with_command(
            command,
            Duration::from_secs(1),
            DeleteMode::Direct,
            &AtomicBool::new(aborted),
            tx,
            checked_tx,
        );
        assert!(
            matches!(events.into_iter().last(), Some(DeleteEvent::Finished(result)) if result == report)
        );
        (report, checked.recv().unwrap().1)
    }
}

#[test]
fn discovers_and_measures_both_independent_custom_caches() {
    let fixture = Fixture::new();
    let snapshot =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    let build = snapshot.entry("go.build").unwrap();
    let module = snapshot.entry("go.module").unwrap();
    assert_eq!(build.path.as_ref(), Some(&fixture.root.join("build 缓存")));
    assert_eq!(build.size, Ok(5));
    assert_eq!(module.path.as_ref(), Some(&fixture.root.join("modules")));
    assert_eq!(module.size, Ok(6));
}

#[test]
fn confirmed_build_clean_rechecks_and_reports_the_removed_cache() {
    let fixture = Fixture::new();
    let confirmed =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    let (tx, events) = mpsc::channel();
    let (checked_tx, checked) = mpsc::channel();
    let report = GoCacheClean {
        confirmed: &confirmed,
        id: "go.build",
        context: &fixture.context,
    }
    .execute_with_command(
        fixture.command(),
        Duration::from_secs(1),
        DeleteMode::Direct,
        &AtomicBool::new(false),
        tx,
        checked_tx,
    );

    assert_eq!(report.freed_bytes, 5);
    assert_eq!(report.projects_done, 1);
    assert!(report.failures.is_empty());
    assert!(!fixture.root.join("build 缓存").exists());
    assert!(fixture.root.join("modules/dependency").exists());
    let (snapshot, error) = checked.recv().unwrap();
    assert_eq!(error, None);
    assert_eq!(snapshot.entry("go.build").unwrap().size, Ok(5));
    assert!(
        matches!(events.into_iter().last(), Some(DeleteEvent::Finished(result)) if result == report)
    );
}

#[test]
fn confirmed_build_clean_rejects_a_new_path_without_deleting_either_cache() {
    let fixture = Fixture::new();
    let confirmed =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    let replacement = fixture.root.join("replacement");
    fs::create_dir(&replacement).unwrap();
    fs::write(replacement.join("keep"), b"new").unwrap();
    let mut command = fixture.command();
    command.env("FIXTURE_BUILD", &replacement);

    let (report, error) = fixture.clean_build(&confirmed, command, false);

    assert_eq!(error, Some(GoCacheError::Changed));
    assert_eq!(report.freed_bytes, 0);
    assert_eq!(report.projects_done, 0);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, fixture.root.join("build 缓存"));
    assert!(fixture.root.join("build 缓存/artifact").exists());
    assert!(replacement.join("keep").exists());
}

#[test]
fn confirmed_build_clean_never_falls_back_when_the_final_query_fails() {
    let fixture = Fixture::new();
    let confirmed =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    let (report, error) = fixture.clean_build(
        &confirmed,
        Command::new(fixture.root.join("missing-go")),
        false,
    );

    assert_eq!(error, Some(GoCacheError::Unavailable));
    assert_eq!(report.freed_bytes, 0);
    assert_eq!(report.projects_done, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(fixture.root.join("build 缓存/artifact").exists());
}

#[test]
fn cancelled_build_clean_finishes_without_deleting_the_confirmed_cache() {
    let fixture = Fixture::new();
    let confirmed =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    let (report, error) = fixture.clean_build(&confirmed, fixture.command(), true);

    assert_eq!(error, None);
    assert!(report.aborted);
    assert_eq!(report.projects_done, 0);
    assert_eq!(report.freed_bytes, 0);
    assert!(report.failures.is_empty());
    assert!(fixture.root.join("build 缓存/artifact").exists());
}

#[test]
fn a_partly_failed_build_clean_does_not_claim_the_whole_cache_size() {
    let fixture = Fixture::new();
    let blocked = fixture.root.join("build 缓存/blocked");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("keep"), b"blocked").unwrap();
    let confirmed =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o500)).unwrap();
    let (report, error) = fixture.clean_build(&confirmed, fixture.command(), false);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(error, None);
    assert_eq!(report.freed_bytes, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(blocked.join("keep").exists());
}

#[test]
fn a_hung_go_query_times_out_without_guessing_paths() {
    let fixture = Fixture::new();
    fs::write(&fixture.go, "#!/bin/sh\nexec sleep 5\n").unwrap();
    let started = std::time::Instant::now();
    let snapshot = discover_with_command(
        fixture.command(),
        &fixture.context,
        Duration::from_millis(30),
    );
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.path.is_none() && entry.size.is_err())
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn rejects_home_directory_even_when_go_reports_it_as_a_cache() {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    command.env("FIXTURE_BUILD", &fixture.context.home);
    let snapshot = discover_with_command(command, &fixture.context, Duration::from_secs(1));
    assert!(snapshot.entry("go.build").unwrap().size.is_err());
    assert_eq!(snapshot.entry("go.module").unwrap().size, Ok(6));
}

#[test]
fn rejects_scan_root_ancestors_and_project_or_cache_overlap_in_either_direction() {
    let mut fixture = Fixture::new();
    let build = fixture.root.join("build 缓存");
    for case in 0..3 {
        fixture.context.scan_roots.clear();
        fixture.context.projects.clear();
        fixture.context.other_caches.clear();
        match case {
            0 => fixture.context.scan_roots.push(build.join("scan-root")),
            1 => fixture.context.projects.push(build.join("project")),
            _ => fixture
                .context
                .other_caches
                .push(build.join("not-created-yet")),
        }
        let snapshot =
            discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
        assert!(
            snapshot.entry("go.build").unwrap().size.is_err(),
            "case {case}"
        );
        assert_eq!(snapshot.entry("go.module").unwrap().size, Ok(6));
    }
    fixture.context.other_caches.clear();
    let nested = build.join("module-cache");
    fs::create_dir(&nested).unwrap();
    let mut command = fixture.command();
    command.env("FIXTURE_MODULE", nested);
    let snapshot = discover_with_command(command, &fixture.context, Duration::from_secs(1));
    assert!(snapshot.entries.iter().all(|entry| entry.size.is_err()));
}

#[test]
fn rejects_cache_root_links_and_links_in_their_ancestors() {
    let fixture = Fixture::new();
    let alias = fixture.root.join("linked-cache");
    std::os::unix::fs::symlink(fixture.root.join("build 缓存"), &alias).unwrap();
    let ancestor = fixture.root.join("linked-ancestor");
    std::os::unix::fs::symlink(&fixture.root, &ancestor).unwrap();
    for path in [alias, ancestor.join("build 缓存")] {
        let mut command = fixture.command();
        command.env("FIXTURE_BUILD", path);
        let snapshot = discover_with_command(command, &fixture.context, Duration::from_secs(1));
        assert!(snapshot.entry("go.build").unwrap().size.is_err());
        assert_eq!(snapshot.entry("go.module").unwrap().size, Ok(6));
    }
}

#[test]
fn query_errors_and_missing_or_disabled_caches_have_distinct_states() {
    use devsweep::go_cache::GoCacheError;
    let fixture = Fixture::new();
    let snapshot = discover_with_command(
        Command::new(fixture.root.join("no-go")),
        &fixture.context,
        Duration::from_secs(1),
    );
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.path.is_none() && entry.size == Err(GoCacheError::Unavailable))
    );
    fs::write(&fixture.go, "#!/bin/sh\nexit 3\n").unwrap();
    let snapshot =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.path.is_none() && entry.size == Err(GoCacheError::QueryFailed))
    );
    fs::write(&fixture.go, "#!/bin/sh\nprintf 'unexpected output\\n'\n").unwrap();
    let snapshot =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.path.is_none() && entry.size == Err(GoCacheError::InvalidOutput))
    );
    fs::write(
        &fixture.go,
        "#!/bin/sh\nprintf '%s\\n' \"$FIXTURE_BUILD\" \"$FIXTURE_MODULE\"\n",
    )
    .unwrap();
    let mut command = fixture.command();
    command
        .env("FIXTURE_BUILD", "off")
        .env("FIXTURE_MODULE", fixture.root.join("missing"));
    let snapshot = discover_with_command(command, &fixture.context, Duration::from_secs(1));
    assert_eq!(
        snapshot.entry("go.build").unwrap().size,
        Err(GoCacheError::Disabled)
    );
    assert_eq!(
        snapshot.entry("go.module").unwrap().size,
        Err(GoCacheError::Missing)
    );
}

#[test]
fn measurement_does_not_follow_internal_links() {
    let fixture = Fixture::new();
    let outside = fixture.root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("large"), vec![0; 100_000]).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.root.join("build 缓存/external")).unwrap();
    let snapshot =
        discover_with_command(fixture.command(), &fixture.context, Duration::from_secs(1));
    assert_eq!(snapshot.entry("go.build").unwrap().size, Ok(5));
    assert_eq!(fs::metadata(outside.join("large")).unwrap().len(), 100_000);
}

#[test]
#[ignore = "requires a local Go installation; only writes a temporary GOENV and temporary caches"]
fn real_go_respects_persisted_configuration_environment_and_gopath() {
    let fixture = Fixture::new();
    let goenv = fixture.root.join("go-env");
    let gopath = fixture.root.join("gopath");
    let module = gopath.join("pkg/mod");
    fs::create_dir_all(&module).unwrap();
    fs::write(module.join("dependency"), b"module").unwrap();
    let command = || {
        let mut command = Command::new("go");
        command
            .env("GOENV", &goenv)
            .env("GOPATH", &gopath)
            .env("GOTOOLCHAIN", "local")
            .env("GOWORK", "off")
            .env("GO111MODULE", "off")
            .env_remove("GOCACHE")
            .env_remove("GOMODCACHE")
            .current_dir(&fixture.context.home);
        command
    };
    assert!(
        command()
            .args([
                "env",
                "-w",
                &format!("GOCACHE={}", fixture.root.join("build 缓存").display())
            ])
            .status()
            .unwrap()
            .success()
    );
    let snapshot = discover_with_command(command(), &fixture.context, Duration::from_secs(3));
    assert_eq!(snapshot.entry("go.build").unwrap().size, Ok(5));
    assert_eq!(
        snapshot.entry("go.module").unwrap().path.as_ref(),
        Some(&module)
    );
    assert_eq!(snapshot.entry("go.module").unwrap().size, Ok(6));
    let mut overridden = command();
    overridden
        .env("GOCACHE", &module)
        .env("GOMODCACHE", fixture.root.join("build 缓存"));
    let snapshot = discover_with_command(overridden, &fixture.context, Duration::from_secs(3));
    assert_eq!(snapshot.entry("go.build").unwrap().size, Ok(6));
    assert_eq!(snapshot.entry("go.module").unwrap().size, Ok(5));
}
