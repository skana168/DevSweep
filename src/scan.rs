use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

/// Marker priority when several markers land in one directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stack {
    Flutter,
    Android,
    Ios,
    Node,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub stack: Stack,
}

const MAX_DEPTH: usize = 5;

const PRUNED: [&str; 11] = [
    "node_modules",
    ".git",
    ".next",
    ".opencode",
    "build",
    "Pods",
    ".dart_tool",
    "target",
    "dist",
    ".gradle",
    "vendor",
];

const PLATFORM_DIRS: [&str; 6] = ["android", "ios", "macos", "windows", "linux", "web"];

/// Discover projects under `roots`, sorted by name.
pub fn scan(roots: &[PathBuf]) -> Vec<Project> {
    let mut projects = Vec::new();
    for root in roots {
        merge_scan(&mut projects, scan_root(root));
    }
    projects
}

/// Fold a fresh scan of one subtree into an existing list: paths stay unique,
/// and platform subdirectories that now belong to a registered parent disappear.
pub fn merge_scan(projects: &mut Vec<Project>, found: Vec<Project>) {
    for project in found {
        if !projects.iter().any(|known| known.path == project.path) {
            projects.push(project);
        }
    }

    let project_paths: HashSet<PathBuf> = projects.iter().map(|p| p.path.clone()).collect();
    projects.retain(|project| !is_platform_subdir(&project.path, &project_paths));

    projects.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// Drop tracked entries whose project the latest scan no longer contains.
pub fn retain_scanned<T>(tracked: &mut HashMap<PathBuf, T>, projects: &[Project]) {
    tracked.retain(|path, _| projects.iter().any(|project| &project.path == path));
}

fn scan_root(root: &Path) -> Vec<Project> {
    let mut projects = Vec::new();
    let walker = WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_pruned(entry));

    for entry in walker.filter_map(Result::ok) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path().to_path_buf());
        let Some(stack) = detect_stack(&path) else {
            continue;
        };
        projects.push(Project {
            name: entry_name(&path),
            path,
            stack,
        });
    }
    projects
}

fn is_pruned(entry: &DirEntry) -> bool {
    entry.file_type().is_dir() && is_pruned_name(entry.file_name())
}

fn is_pruned_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| PRUNED.contains(&name))
}

fn is_platform_subdir(path: &Path, registered: &HashSet<PathBuf>) -> bool {
    let mut current = path;
    while let Some(parent) = current.parent() {
        let is_platform = current
            .file_name()
            .is_some_and(|name| PLATFORM_DIRS.iter().any(|dir| name == *dir));
        if is_platform && registered.contains(parent) {
            return true;
        }
        current = parent;
    }
    false
}

fn detect_stack(path: &Path) -> Option<Stack> {
    let names = fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned());
    classify(names)
}

fn classify(names: impl IntoIterator<Item = String>) -> Option<Stack> {
    let mut flutter = false;
    let mut ios = false;
    let mut android = false;
    let mut node = false;

    for name in names {
        match name.as_str() {
            "pubspec.yaml" => flutter = true,
            "settings.gradle" | "settings.gradle.kts" | "build.gradle" | "build.gradle.kts" => {
                android = true
            }
            "package.json" => node = true,
            any if any.ends_with(".xcodeproj") => ios = true,
            _ => {}
        }
    }

    if flutter {
        Some(Stack::Flutter)
    } else if ios {
        Some(Stack::Ios)
    } else if android {
        Some(Stack::Android)
    } else if node {
        Some(Stack::Node)
    } else {
        None
    }
}

fn entry_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn classify_uses_flutter_ios_android_node_priority() {
        assert_eq!(
            classify(names(&["pubspec.yaml", "package.json"])),
            Some(Stack::Flutter)
        );
        assert_eq!(
            classify(names(&["App.xcodeproj", "build.gradle", "package.json"])),
            Some(Stack::Ios)
        );
        assert_eq!(
            classify(names(&["settings.gradle.kts", "package.json"])),
            Some(Stack::Android)
        );
        assert_eq!(classify(names(&["package.json"])), Some(Stack::Node));
        assert_eq!(classify(names(&[])), None);
    }

    #[test]
    fn every_pruned_name_is_recognized() {
        for name in PRUNED {
            assert!(is_pruned_name(OsStr::new(name)), "{name} should be pruned");
        }
        assert!(!is_pruned_name(OsStr::new("src")));
    }

    #[test]
    fn platform_dirs_are_merged_under_a_registered_parent() {
        let parent = PathBuf::from("/work/app");
        let registered = HashSet::from([parent.clone()]);

        assert!(is_platform_subdir(&parent.join("android"), &registered));
        assert!(is_platform_subdir(
            &parent.join("android/app"),
            &registered
        ));
        assert!(is_platform_subdir(&parent.join("web"), &registered));
        assert!(!is_platform_subdir(
            &parent.join("packages/web"),
            &registered
        ));
        assert!(!is_platform_subdir(
            Path::new("/work/other/android"),
            &registered
        ));
    }
}
