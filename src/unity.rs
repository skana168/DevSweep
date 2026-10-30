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
    let output = std::process::Command::new("/bin/ps")
        .args(["-Aww", "-o", "ucomm="])
        .output();
    #[cfg(target_os = "windows")]
    let output = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell.exe").creation_flags(0x08000000).args([
            "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new(); Get-CimInstance Win32_Process | ForEach-Object { $_.Name + [char]9 + $_.ExecutablePath }",
        ]).output()
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return EditorState::Unknown;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let text = output
            .ok()
            .filter(|out| out.status.success() && out.stderr.is_empty())
            .and_then(|out| String::from_utf8(out.stdout).ok());
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
