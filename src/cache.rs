use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::plan::{DeletePlan, LARGE_TARGET_BYTES, PlanItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::MacOs
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheGroup {
    /// Cache tied to a project's package manager or build.
    Project,
    /// Tools that keep state outside any project.
    Toolchain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    Home,
    LocalAppData,
}

#[derive(Debug, Clone, Copy)]
struct PathSpec {
    base: Base,
    rel: &'static str,
}

pub struct CacheRule {
    pub id: &'static str,
    pub name: &'static str,
    pub group: CacheGroup,
    mac: Option<PathSpec>,
    win: Option<PathSpec>,
    pub dangerous: bool,
}

const fn home(rel: &'static str) -> Option<PathSpec> {
    Some(PathSpec {
        base: Base::Home,
        rel,
    })
}

const fn local(rel: &'static str) -> Option<PathSpec> {
    Some(PathSpec {
        base: Base::LocalAppData,
        rel,
    })
}

/// The whitelist: these twelve entries are the only paths a global-cache
/// cleanup can touch. `id`s are stable once released.
pub fn builtin_caches() -> &'static [CacheRule] {
    &CACHES
}

static CACHES: [CacheRule; 12] = [
    CacheRule {
        id: "gradle.caches",
        name: "Gradle caches",
        group: CacheGroup::Project,
        mac: home(".gradle/caches"),
        win: home(".gradle/caches"),
        dangerous: false,
    },
    CacheRule {
        id: "gradle.dists",
        name: "Gradle distributions",
        group: CacheGroup::Project,
        mac: home(".gradle/wrapper/dists"),
        win: home(".gradle/wrapper/dists"),
        dangerous: false,
    },
    CacheRule {
        id: "npm.cacache",
        name: "npm",
        group: CacheGroup::Project,
        mac: home(".npm/_cacache"),
        win: local("npm-cache"),
        dangerous: false,
    },
    CacheRule {
        id: "pnpm.store",
        name: "pnpm",
        group: CacheGroup::Project,
        mac: home("Library/pnpm/store"),
        win: local("pnpm/store"),
        dangerous: false,
    },
    CacheRule {
        id: "bun.cache",
        name: "bun",
        group: CacheGroup::Project,
        mac: home(".bun/install/cache"),
        win: home(".bun/install/cache"),
        dangerous: false,
    },
    CacheRule {
        id: "yarn.cache",
        name: "Yarn",
        group: CacheGroup::Project,
        mac: home("Library/Caches/Yarn"),
        win: local("Yarn/Cache"),
        dangerous: false,
    },
    CacheRule {
        id: "pub.cache",
        name: "pub",
        group: CacheGroup::Project,
        mac: home(".pub-cache"),
        win: local("Pub/Cache"),
        dangerous: false,
    },
    CacheRule {
        id: "cocoapods.cache",
        name: "CocoaPods",
        group: CacheGroup::Project,
        mac: home("Library/Caches/CocoaPods"),
        win: local("CocoaPods/Cache"),
        dangerous: false,
    },
    CacheRule {
        id: "playwright.browsers",
        name: "Playwright",
        group: CacheGroup::Toolchain,
        mac: home("Library/Caches/ms-playwright"),
        win: local("ms-playwright"),
        dangerous: false,
    },
    CacheRule {
        id: "xcode.derived_data",
        name: "Xcode DerivedData",
        group: CacheGroup::Toolchain,
        mac: home("Library/Developer/Xcode/DerivedData"),
        win: None,
        dangerous: false,
    },
    CacheRule {
        id: "cargo.registry",
        name: "Cargo registry",
        group: CacheGroup::Toolchain,
        mac: home(".cargo/registry"),
        win: home(".cargo/registry"),
        dangerous: true,
    },
    CacheRule {
        id: "pip.cache",
        name: "pip",
        group: CacheGroup::Toolchain,
        mac: home("Library/Caches/pip"),
        win: local("pip/Cache"),
        dangerous: false,
    },
];

/// Resolve a rule's path for `platform`. `None` when the entry does not exist
/// on that platform (Xcode DerivedData on Windows).
pub fn resolve(
    rule: &CacheRule,
    platform: Platform,
    home: &Path,
    local_app_data: Option<&Path>,
) -> Option<PathBuf> {
    let spec = match platform {
        Platform::MacOs => rule.mac,
        Platform::Windows => rule.win,
    }?;
    let base = match spec.base {
        Base::Home => home,
        Base::LocalAppData => local_app_data?,
    };
    join_rel(base, spec.rel)
}

/// Only plain relative segments: no absolute paths, `..`, or `.`.
fn join_rel(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    if rel.is_absolute() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(base.join(rel))
}

struct Env {
    platform: Platform,
    home: PathBuf,
    local_app_data: Option<PathBuf>,
}

impl Env {
    fn current() -> Option<Self> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(Self {
            platform: Platform::current(),
            home: PathBuf::from(home),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        })
    }

    fn path(&self, rule: &CacheRule) -> Option<PathBuf> {
        resolve(
            rule,
            self.platform,
            &self.home,
            self.local_app_data.as_deref(),
        )
    }
}

#[derive(Clone)]
pub struct CacheEntry {
    pub rule: &'static CacheRule,
    /// `None` when the platform has no such cache or the home directory is unknown.
    pub path: Option<PathBuf>,
}

/// All twelve entries with their path on this machine. Paths are not checked
/// for existence here; the UI renders missing ones as "not found".
pub fn entries() -> Vec<CacheEntry> {
    let env = Env::current();
    builtin_caches()
        .iter()
        .map(|rule| CacheEntry {
            rule,
            path: env.as_ref().and_then(|env| env.path(rule)),
        })
        .collect()
}

/// The plan for one whitelisted cache entry, or `None` for an unknown id or a
/// path that is no longer a directory. Never accepts a caller-supplied path.
pub fn plan(id: &str, bytes: u64) -> Option<DeletePlan> {
    plan_in(&Env::current()?, id, bytes)
}

fn plan_in(env: &Env, id: &str, bytes: u64) -> Option<DeletePlan> {
    let rule = builtin_caches().iter().find(|rule| rule.id == id)?;
    let path = env.path(rule)?;
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.is_dir() && !meta.file_type().is_symlink() {
        return None;
    }

    Some(DeletePlan {
        items: vec![PlanItem {
            project_path: path.clone(),
            target_path: path,
            bytes,
            large: bytes > LARGE_TARGET_BYTES,
        }],
        skipped: Vec::new(),
        total_bytes: bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn find(id: &str) -> &'static CacheRule {
        builtin_caches()
            .iter()
            .find(|rule| rule.id == id)
            .unwrap_or_else(|| panic!("{id} is missing from the table"))
    }

    fn env(root: &Path, platform: Platform) -> Env {
        Env {
            platform,
            home: root.to_path_buf(),
            local_app_data: Some(root.join("LocalAppData")),
        }
    }

    #[test]
    fn table_has_twelve_unique_entries_and_one_dangerous_one() {
        let rules = builtin_caches();
        let ids: HashSet<&str> = rules.iter().map(|rule| rule.id).collect();

        assert_eq!(rules.len(), 12);
        assert_eq!(ids.len(), 12);
        assert_eq!(
            rules
                .iter()
                .filter(|rule| rule.group == CacheGroup::Project)
                .count(),
            8
        );
        assert_eq!(
            rules
                .iter()
                .filter(|rule| rule.dangerous)
                .map(|rule| rule.id)
                .collect::<Vec<_>>(),
            vec!["cargo.registry"]
        );
    }

    #[test]
    fn every_entry_resolves_to_its_documented_path() {
        let home = Path::new("/home/dev");
        let local = Path::new("/win/local");
        let expected: [(&str, PathBuf, Option<PathBuf>); 12] = [
            (
                "gradle.caches",
                home.join(".gradle/caches"),
                Some(home.join(".gradle/caches")),
            ),
            (
                "gradle.dists",
                home.join(".gradle/wrapper/dists"),
                Some(home.join(".gradle/wrapper/dists")),
            ),
            (
                "npm.cacache",
                home.join(".npm/_cacache"),
                Some(local.join("npm-cache")),
            ),
            (
                "pnpm.store",
                home.join("Library/pnpm/store"),
                Some(local.join("pnpm/store")),
            ),
            (
                "bun.cache",
                home.join(".bun/install/cache"),
                Some(home.join(".bun/install/cache")),
            ),
            (
                "yarn.cache",
                home.join("Library/Caches/Yarn"),
                Some(local.join("Yarn/Cache")),
            ),
            (
                "pub.cache",
                home.join(".pub-cache"),
                Some(local.join("Pub/Cache")),
            ),
            (
                "cocoapods.cache",
                home.join("Library/Caches/CocoaPods"),
                Some(local.join("CocoaPods/Cache")),
            ),
            (
                "playwright.browsers",
                home.join("Library/Caches/ms-playwright"),
                Some(local.join("ms-playwright")),
            ),
            (
                "xcode.derived_data",
                home.join("Library/Developer/Xcode/DerivedData"),
                None,
            ),
            (
                "cargo.registry",
                home.join(".cargo/registry"),
                Some(home.join(".cargo/registry")),
            ),
            (
                "pip.cache",
                home.join("Library/Caches/pip"),
                Some(local.join("pip/Cache")),
            ),
        ];

        for (id, mac, win) in expected {
            let rule = find(id);
            assert_eq!(
                resolve(rule, Platform::MacOs, home, Some(local)),
                Some(mac.clone()),
                "{id} on macOS"
            );
            assert_eq!(
                resolve(rule, Platform::Windows, home, Some(local)),
                win,
                "{id} on Windows"
            );
        }
    }

    #[test]
    fn paths_follow_the_platform_table() {
        let home = Path::new("/home/dev");
        let local = Path::new("/win/local");

        assert_eq!(
            resolve(find("npm.cacache"), Platform::MacOs, home, Some(local)),
            Some(home.join(".npm/_cacache"))
        );
        assert_eq!(
            resolve(find("npm.cacache"), Platform::Windows, home, Some(local)),
            Some(local.join("npm-cache"))
        );
        assert_eq!(
            resolve(find("gradle.caches"), Platform::Windows, home, Some(local)),
            Some(home.join(".gradle/caches")),
            "Gradle stays under the user profile"
        );
        assert_eq!(
            resolve(
                find("xcode.derived_data"),
                Platform::Windows,
                home,
                Some(local)
            ),
            None
        );
        assert_eq!(
            resolve(find("xcode.derived_data"), Platform::MacOs, home, None),
            Some(home.join("Library/Developer/Xcode/DerivedData"))
        );
        assert_eq!(
            resolve(find("pnpm.store"), Platform::Windows, home, None),
            None,
            "no LOCALAPPDATA means no Windows path"
        );
    }

    #[test]
    fn relative_paths_cannot_escape_their_base() {
        let base = Path::new("/home/dev");

        assert_eq!(join_rel(base, "../etc"), None);
        assert_eq!(join_rel(base, "/etc"), None);
        assert_eq!(join_rel(base, "./cache"), None);
        assert_eq!(
            join_rel(base, "Library/Caches/pip"),
            Some(base.join("Library/Caches/pip"))
        );
    }

    #[test]
    fn plan_rejects_ids_outside_the_whitelist_or_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(dir.path(), Platform::MacOs);
        fs::create_dir_all(dir.path().join(".gradle/caches")).unwrap();

        for id in [
            "",
            "..",
            "../cargo.registry",
            "gradle",
            "gradle.caches/../..",
            "/etc",
            "xcode.derived_data",
        ] {
            assert!(plan_in(&env, id, 1_000).is_none(), "{id} must be rejected");
        }

        let plan = plan_in(&env, "gradle.caches", 1_000).expect("whitelisted path exists");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].target_path, dir.path().join(".gradle/caches"));
        assert_eq!(plan.total_bytes, 1_000);
        assert!(!plan.items[0].large);
    }

    #[test]
    fn plan_only_accepts_directory_like_entries() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(dir.path(), Platform::MacOs);
        fs::create_dir_all(dir.path().join(".npm")).unwrap();
        fs::write(dir.path().join(".npm/_cacache"), "not a directory").unwrap();

        assert!(plan_in(&env, "npm.cacache", 1_000).is_none());
    }
}
