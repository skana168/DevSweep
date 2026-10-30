//! Unity safeguards shared by discovery, planning and both execution entrypoints.
use std::{fs, path::Path};

pub(crate) fn valid_target(root: &Path, target: &Path) -> bool {
    crate::scan::is_unity(root)
        && ["Library", "Temp"]
            .iter()
            .any(|name| target == root.join(name))
        && fs::symlink_metadata(target)
            .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        && fs::canonicalize(root)
            .ok()
            .zip(fs::canonicalize(target).ok())
            .is_some_and(|(root, target)| target.parent() == Some(root.as_path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_boundary_distinguishes_editor_hub_and_unknown_without_project_arguments() {
        assert_eq!(classify_processes(Ok("Unity\n")), EditorState::Unknown);
        assert_eq!(
            classify_names(Ok("launchd\nUnity\nUnity\n")),
            EditorState::Busy
        );
        assert_eq!(
            classify_names(Ok("Unity Hub\nUnity Hub Helpe\n")),
            EditorState::Idle
        );
        assert_eq!(classify_names(Ok("UnityUnknown\n")), EditorState::Unknown);
        assert_eq!(classify_names(Err(())), EditorState::Unknown);
        assert_eq!(classify_processes(Err(())), EditorState::Unknown);
        assert_eq!(classify_processes(Ok("")), EditorState::Unknown);
        assert_eq!(
            classify_processes(Ok("garbage.exe\n")),
            EditorState::Unknown
        );
        assert_eq!(
            classify_processes(Ok("Unity.exe\tC:\\other.exe\n")),
            EditorState::Unknown
        );
        assert_eq!(
            classify_processes(Ok(
                "Unity.exe\tC:\\Program Files\\Unity\\Editor\\Unity.exe\n"
            )),
            EditorState::Busy
        );
        assert_eq!(
            classify_processes(Ok("Unity.exe\t\n")),
            EditorState::Unknown
        );
        assert_eq!(
            classify_processes(Ok(
                "Unity Hub.exe\tC:\\Program Files\\Unity Hub\\Unity Hub.exe\n"
            )),
            EditorState::Idle
        );
    }
    #[cfg(unix)]
    #[test]
    fn hung_process_query_returns_unknown_within_deadline() {
        let start = std::time::Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let mut command = std::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "echo $$ > \"$1\"; echo Unity; exec /bin/sleep 2",
            "query",
        ]);
        command.arg(&pid_file);
        let output = query_output(&mut command, std::time::Duration::from_millis(50));
        assert_eq!(
            classify_names(output.as_deref().ok_or(())),
            EditorState::Unknown
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        let pid = fs::read_to_string(pid_file).unwrap();
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success(),
            "query child must be terminated and reaped"
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_macos_query_detects_a_headless_editor_named_process() {
        // Synthetic executable-name smoke test, not real Unity Editor acceptance.
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("Unity");
        fs::copy("/bin/sleep", &executable).unwrap();
        let _child = Child(
            std::process::Command::new(executable)
                .arg("10")
                .spawn()
                .unwrap(),
        );
        let mut observed = EditorState::Unknown;
        for _ in 0..50 {
            observed = editor_state();
            if observed == EditorState::Busy {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(observed, EditorState::Busy);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorState {
    Idle,
    Busy,
    Unknown,
}

/// Enumerate all users' processes, including headless Editor instances.
pub fn editor_state() -> EditorState {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/bin/ps");
    #[cfg(target_os = "macos")]
    command.args(["-Aww", "-o", "ucomm="]);
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new("powershell.exe");
        command.creation_flags(0x08000000).args([
            "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new(); Get-CimInstance Win32_Process | ForEach-Object { $_.Name + [char]9 + $_.ExecutablePath }",
        ]);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return EditorState::Unknown;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let text = query_output(&mut command, std::time::Duration::from_secs(3));
        #[cfg(target_os = "macos")]
        {
            classify_names(text.as_deref().ok_or(()))
        }
        #[cfg(target_os = "windows")]
        {
            classify_processes(text.as_deref().ok_or(()))
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn classify_processes(output: Result<&str, ()>) -> EditorState {
    let Ok(text) = output else {
        return EditorState::Unknown;
    };
    if text.trim().is_empty() {
        return EditorState::Unknown;
    }
    let mut state = EditorState::Idle;
    for line in text.lines() {
        let Some((name, path)) = line.split_once('\t') else {
            return EditorState::Unknown;
        };
        let name = name.to_lowercase();
        let path = path.trim().to_lowercase();
        if name.is_empty() || path.contains('\t') {
            return EditorState::Unknown;
        }
        if !name.starts_with("unity") {
            continue;
        }
        let absolute =
            path.as_bytes().get(1) == Some(&b':') && path.as_bytes().get(2) == Some(&b'\\');
        let identified = absolute && path.rsplit('\\').next() == Some(name.as_str());
        if name == "unity.exe" && identified {
            return EditorState::Busy;
        }
        let hub = identified
            && (name == "unity hub.exe"
                || (name.starts_with("unity hub helper") && path.contains("\\unity hub\\")));
        if !hub {
            state = EditorState::Unknown;
        }
    }
    state
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    Version,
    Busy,
    Unknown,
    Path,
}

pub(crate) fn check(project: &Path, target: &Path, state: EditorState) -> Result<(), BlockReason> {
    if !valid_target(project, target) {
        return Err(BlockReason::Path);
    }
    if !matches!(
        crate::scan::unity_version(project),
        crate::scan::UnityVersion::Supported(_)
    ) {
        return Err(BlockReason::Version);
    }
    match state {
        EditorState::Idle => Ok(()),
        EditorState::Busy => Err(BlockReason::Busy),
        EditorState::Unknown => Err(BlockReason::Unknown),
    }
}

pub(crate) fn concerns(project: &Path, target: &Path) -> bool {
    // A vanished version marker must not turn an already discovered Unity Project into a bypass.
    let candidate = |path: &Path| {
        path.ancestors().any(|root| {
            root.join("ProjectSettings/ProjectVersion.txt").exists()
                || (root.join("Assets").is_dir() && root.join("ProjectSettings").is_dir())
        })
    };
    [project, target]
        .iter()
        .any(|path| candidate(path) || fs::canonicalize(path).is_ok_and(|path| candidate(&path)))
}

// ucomm is the kernel accounting name, unlike comm/argv[0], which a process can change.
#[cfg(any(target_os = "macos", test))]
fn classify_names(output: Result<&str, ()>) -> EditorState {
    let Ok(text) = output else {
        return EditorState::Unknown;
    };
    if text.trim().is_empty() {
        return EditorState::Unknown;
    }
    let mut state = EditorState::Idle;
    for name in text.lines().map(str::trim).map(str::to_lowercase) {
        if name == "unity" {
            return EditorState::Busy;
        }
        if name.is_empty()
            || (name.starts_with("unity")
                && name != "unity hub"
                && !name.starts_with("unity hub helpe"))
        {
            state = EditorState::Unknown;
        }
    }
    state
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn query_output(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + timeout;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    // Drain both pipes concurrently so a full pipe cannot stall the process query.
    let (tx, rx) = std::sync::mpsc::channel();
    for (stderr, mut pipe) in [
        (
            false,
            Box::new(child.stdout.take()?) as Box<dyn Read + Send>,
        ),
        (true, Box::new(child.stderr.take()?) as Box<dyn Read + Send>),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
            let _ = tx.send((stderr, result));
        });
    }
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait().ok()? {
                break status;
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if !status.success() {
            return None;
        }
        let mut stdout = None;
        for _ in 0..2 {
            let (stderr, bytes) = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .ok()?;
            let bytes = bytes.ok()?;
            if stderr {
                if !bytes.is_empty() {
                    return None;
                }
            } else {
                stdout = String::from_utf8(bytes).ok();
            }
        }
        stdout
    })();
    // Always terminate and reap an unfinished child, including timeout/error paths.
    let _ = child.kill();
    let _ = child.wait();
    result
}
