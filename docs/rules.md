# `rules.toml`

DevSweep's project rules are built in. `rules.toml` lets you turn them off, change which
ones start checked, and add directories of your own. The global cache list is fixed and is not
affected by this file.

## Where the file lives

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/DevSweep/rules.toml` |
| Windows | `%APPDATA%\DevSweep\rules.toml` |

If the file does not exist, "Open rules file" in the app writes a commented template. A typo does
not take the app down: the built-in rules stay in effect and the error is shown in the interface.

## What you can do

```toml
# 1. turn built-in entries off
disable = ["node.dist", "ios.pods"]

# 2. change whether an entry is checked by default
[defaults]
"node.dist" = true

# 3. add a directory of your own (stack: flutter / android / ios / java / node / python / rust / dotnet / "*", label optional)
[[extra]]
stack = "*"
path = ".cache"
label = "Cache"
```

Extras are always checked by default and are never marked dangerous. Their ids are generated
(`extra.0`, `extra.1`, …) and cannot be referenced from `disable` or `[defaults]`.

## Built-in rules

`id`s are stable once released: your file depends on them, so they never get renamed.

| Stack | Id | Path | Checked by default |
|---|---|---|---|
| Flutter | `flutter.build` | `build` | yes |
| Flutter | `flutter.dart_tool` | `.dart_tool` | yes |
| Flutter | `flutter.ios_pods` | `ios/Pods` | yes |
| Flutter | `flutter.ios_ephemeral` | `ios/Flutter/ephemeral` | yes |
| Flutter | `flutter.android_gradle` | `android/.gradle` | yes |
| Flutter | `flutter.android_build` | `android/build` | yes |
| Flutter | `flutter.macos_pods` | `macos/Pods` | yes |
| Flutter | `flutter.plugins_dependencies` | `.flutter-plugins-dependencies` | yes |
| Android | `android.build` | `build` | yes |
| Android | `android.module_build` | `*/build` | yes |
| Android | `android.gradle` | `.gradle` | yes |
| Android | `android.cxx` | `.cxx` | yes |
| Android | `android.captures` | `captures` | yes |
| Java | `java.target` | `target` | yes |
| iOS | `ios.pods` | `Pods` | yes |
| iOS | `ios.build` | `build` | yes |
| iOS | `ios.xcuserdata` | `*.xcodeproj/xcuserdata` | yes |
| Node | `node.node_modules` | `node_modules` | yes |
| Node | `node.next` | `.next` | yes |
| Node | `node.out` | `out` | yes |
| Node | `node.nuxt` | `.nuxt` | yes |
| Node | `node.svelte_kit` | `.svelte-kit` | yes |
| Node | `node.turbo` | `.turbo` | yes |
| Node | `node.parcel_cache` | `.parcel-cache` | yes |
| Node | `node.vite` | `.vite` | yes |
| Node | `node.coverage` | `coverage` | yes |
| Node | `node.dist` | `dist` | **no** (dangerous) |
| Python | `python.pytest_cache` | `.pytest_cache` | yes |
| Python | `python.mypy_cache` | `.mypy_cache` | yes |
| Python | `python.ruff_cache` | `.ruff_cache` | yes |
| Python | `python.pycache` | `__pycache__` (recursive) | yes |
| Python | `python.build` | `build` | **no** (dangerous) |
| Python | `python.dist` | `dist` | **no** (dangerous) |
| Python | `python.egg_info` | `*.egg-info` | **no** (dangerous) |
| Python | `python.src_egg_info` | `src/*.egg-info` | **no** (dangerous) |
| Python | `python.dot_venv` | `.venv` | **no** (dangerous) |
| Python | `python.venv` | `venv` | **no** (dangerous) |
| Python | `python.env` | `env` | **no** (dangerous) |
| Rust | `rust.target` | `target` | **no** (dangerous) |
| .NET | `dotnet.bin` | `bin` | **no** (dangerous) |
| .NET | `dotnet.obj` | `obj` | **no** (dangerous) |

A project carries one stack, chosen by priority Flutter > iOS > Android > Java > Node > Python > Rust > .NET, so only that
stack's rules apply to it (see [ADR 0001](adr/0001-stack-is-one-product-type.md)). A `pom.xml`
marks a Java project; a Gradle project counts as Android even when it builds plain JVM code.

## Python projects

A `pyproject.toml`, `setup.py`, `requirements.txt`, or `Pipfile` file marks a Python project.
A lone `.py`, `setup.cfg`, or environment directory does not. A tooling-only `pyproject.toml`
also qualifies; an existing stack such as Node wins when its marker is in the same directory.

Python cleans the three root-level tool caches, recursive `__pycache__` directories, and the
scoped build outputs listed above, plus root-level `.venv/`, `venv/`, and `env/` environments
containing a `pyvenv.cfg` file.
Environments are dangerous and start unchecked; their entire size is shown even when they are
not selected. The marker confirms environment identity, not that manually installed dependencies
can be rebuilt. Custom environment names, nested environments, `.tox`, `.nox`, and Conda
environments are not included in environment cleanup. Build outputs are also dangerous and
unchecked by default: directory names do not prove the contents are reproducible or exclude
manually saved files. Build output patterns do not recurse into arbitrary directories or match
source-adjacent `.so` / `.pyd` files. Coverage reports are not included.
Directories containing `pyvenv.cfg` are excluded from project discovery, including environments
with custom names, so installed packages do not become projects. Ordinary source directories
named `env` or `venv` without that marker are still scanned.

The built-in `python.pycache` rule finds bytecode cache directories without a depth limit, while
project discovery remains limited to five levels below each scan root. Recursion skips existing
generated-directory exclusions, virtual environments, symbolic links, and independent child
projects of any stack. Platform subdirectories retain their existing parent-project ownership.
A child project stops traversal even when it is too deep to appear in the project list; add that
directory or a nearer ancestor as a scan root to discover and clean it separately. Each cache is
one Target, including its contents. These boundaries are checked again when building the final
delete plan. This recursive behavior belongs only to the built-in rule; Extra rule syntax is
unchanged and does not gain a `**` wildcard.

Python built-in targets must be real directories when discovered and when the final delete plan
is built; files and symbolic links are skipped. Environment identity is rechecked in the final
plan too. The `src` parent of a built-in egg-info target must also be a real directory. These
mandatory checks are unaffected by `[defaults]`; explicitly setting a dangerous rule default
to true counts as opting in, but retains the dangerous label. `disable` can remove the rule.
Extra rules keep their existing user-defined behavior. No Python command is executed and
DevSweep does not detect whether an environment is currently in use.

## Rust projects

A `Cargo.toml` file marks a Rust project without parsing its contents, including workspace-only
manifests. A lone `.rs`, `Cargo.lock`, `target/`, or directory named `Cargo.toml` does not qualify.
Existing stack markers take priority in the same directory. Workspace roots and independent
member directories appear separately within the existing scan depth; membership is not resolved.

The `rust.target` rule includes only the whole real root `target/` directory, dangerous and
unchecked by default. Its size includes build caches and final binaries, documentation or packages.
A workspace's shared root target belongs to the root Project; members only match their own root
target, never an ancestor's. Members without targets stay visible. Scanning only a member never
claims output outside that scan root. Nested arbitrary target directories are not matched.

Files and symbolic links are skipped at discovery and checked again in the final Delete plan.
`disable` can remove the rule; `[defaults]` can opt in by setting `"rust.target" = true`, retaining
the dangerous label and mandatory validation. Extra rules keep their existing behavior, including
Rust-scoped extras. Existing protected-path and overlap checks and both delete modes still apply.

No Cargo command is executed, custom target-dir/build-dir settings are not resolved, and source,
`Cargo.lock` and project `.cargo/` are outside this built-in rule. Global caches are unchanged.
Stop builds before cleaning: active builds are not detected or locked, and the existing filesystem
race between validation and deletion is not eliminated.

## .NET projects

C#, F#, and Visual Basic share one `.NET` stack (`dotnet` in `rules.toml`). A real regular
`.csproj`, `.fsproj`, or `.vbproj` file marks its containing directory; extensions are ASCII
case-insensitive and contents are not parsed. Marker directories and symbolic links do not
qualify. Multiple project files in one directory produce one Project. Solutions (`.sln`/`.slnx`),
`global.json`, and source files alone do not qualify or trigger recursive cleanup. Generated
project files, including Unity's, follow the same rules without adding Unity cleanup support.

`dotnet.bin` and `dotnet.obj` match only the whole real, directly contained lowercase `bin/`
and `obj/` directories of a .NET Project. Each is independently measured and selected, dangerous
and unchecked by default. Names are case-sensitive even on case-insensitive filesystems.
Their sizes include all contents, including manually added DLLs, configuration, or data;
selecting them accepts deleting those contents. Files and symbolic links are rejected at
discovery and rechecked in the final plan, including the exact name. Existing protected-path,
overlap, confirmation and Trash/Direct flows apply.

`disable` removes either rule; `[defaults]` can opt in with `"dotnet.bin" = true` or
`"dotnet.obj" = true`, retaining Dangerous and mandatory validation. Extra rules retain their
existing behavior, including file targets and .NET-scoped extras.

These rules do not resolve custom output paths or centralized `artifacts/`, do not include
`.vs/`, `TestResults/` or NuGet global caches, and execute no dotnet/MSBuild commands. This
whole-directory cleanup is not equivalent to `dotnet clean`. Stop builds before cleaning;
active builds are not detected or locked and filesystem changes between validation and deletion
remain possible. Same-directory higher-priority stacks do not receive these rules, and existing
scan-depth and platform-subdirectory ownership limits remain in effect.

## Paths

`path` must be a literal path inside the project: no absolute paths, no `..`, no `.`. A single
`*` may stand in for one path segment, as in `*/build` or `*.xcodeproj/xcuserdata`.

Paths that are invalid, that point outside the project, or that do not exist are skipped; invalid
ones are reported in the interface.

## Go build cache

Go build cache cleanup is a fixed global Cache entry, independent of project Rules. It uses the
current local Go configuration rather than a hardcoded default path, including environment and
persisted Go settings. It does not add a Go Stack or project Marker. Go queries run in the background
with a timeout, without shell startup files or toolchain downloads. Unavailable Go, query errors,
missing directories and unsafe paths are shown explicitly and cannot be cleaned.

Only real cache directories with no links in their path are accepted. Filesystem roots, the home
directory, scan roots and their ancestors, and paths overlapping known Projects or other caches
are rejected. Internal links are not followed. Custom external locations are allowed if validation
passes. These checks protect known locations, not arbitrary manual data inside a cache.

Refresh resolves the location again. Execution rechecks current Go configuration and path safety;
a changed location cancels the old confirmation and requires new measurement and confirmation.
Both Trash and Direct follow the global setting, with no automatic fallback or `go clean` command.
Subsequent builds may be slower. Stop Go builds, tests and downloads first; active tasks are not
locked or terminated. Failures trigger remeasurement, do not count the entire item as freed, and
may leave partial deletions. Filesystem races between checking and deleting are not eliminated.


## Go module cache

Go module cache is a separate fixed Dependency cache entry with the same local-Go discovery,
protected-path, refresh and final-confirmation checks as Go build cache. It is Dangerous: removing
it discards downloaded modules and unpacked dependency sources, so dependencies must be downloaded
again, offline builds may fail, and manual edits inside the cache are lost.

The selected Trash / Direct mode is preserved. On POSIX, Direct adds only missing owner-write
permission to directories, preserving other mode bits; read-only source files need not be changed.
Trash first attempts the normal move without changing POSIX content permissions. Windows clears
read-only attributes only within the validated cache. Internal links and reparse points are not
followed for measurement or permission changes, and deletion does not delete their external targets.
No privilege escalation or Go clean command is used, and Trash failure never falls back to Direct.

A failed item is not reported as fully cleaned or counted as fully freed; remaining contents are
measured again. Partial deletions and permission changes cannot be rolled back. Stop builds, tests
and downloads before cleaning; the filesystem race between validation and deletion still exists.
