//! Opening a directory in the user's terminal.
//!
//! The app has no terminal of its own, so each platform is asked for its own:
//! macOS opens Terminal.app, Windows a console, and Linux tries the usual names.
//! Failures are silent — this is a convenience action, not a state change.

use std::path::Path;
use std::process::Command;

pub fn open(dir: &Path) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(dir)
        .spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd")
        .arg("/K")
        .arg("cd")
        .arg("/d")
        .arg(dir)
        .spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    linux(dir);
}

/// Try `$TERMINAL` first, then the common emulators, stopping at the first that starts.
#[cfg(all(unix, not(target_os = "macos")))]
fn linux(dir: &Path) {
    let mut names: Vec<std::ffi::OsString> = Vec::new();
    if let Some(term) = std::env::var_os("TERMINAL") {
        names.push(term);
    }
    names
        .extend(["x-terminal-emulator", "gnome-terminal", "konsole"].map(std::ffi::OsString::from));
    for name in names {
        if Command::new(&name).current_dir(dir).spawn().is_ok() {
            return;
        }
    }
}
