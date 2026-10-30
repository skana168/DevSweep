<p align="center">
  <img src="assets/icon.png" width="128" height="128" alt="DevSweep icon: an electric-blue D with a diagonal sweep cut on an ink-black tile">
</p>

<h1 align="center">DevSweep</h1>

<p align="center">
  <b>Find the tens of gigabytes your build folders and package caches are quietly eating, tick what you
  don't need, and clear it in one go.</b><br>
  A local desktop app for macOS and Windows: Flutter, Android, iOS, Java, Node, Python, Rust and .NET projects, plus the global
  caches behind them. No cloud, no account, nothing leaves your machine.
</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="macOS · Windows" src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows-26282e?style=flat-square">
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-3a3a3a?style=flat-square">
  <img alt="Status: early alpha" src="https://img.shields.io/badge/status-early%20alpha-d69e2e?style=flat-square">
  <br>
  <a href="https://github.com/skana168/DevSweep/actions/workflows/ci.yml"><img alt="CI: macOS and Windows tests" src="https://github.com/skana168/DevSweep/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/skana168/DevSweep/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/skana168/DevSweep?style=flat-square"></a>
</p>

<p align="center">
  <a href="https://github.com/skana168/DevSweep/releases/latest"><b>Download DevSweep</b></a> ·
  <a href="https://github.com/skana168/DevSweep/releases">All releases</a> ·
  <a href="#build-from-source">Build from source</a> ·
  <a href="docs/rules.md">Rules reference</a> ·
  <a href="ROADMAP.md">Roadmap</a> ·
  <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="docs/screenshots/main.png" alt="DevSweep on macOS in the light theme: a sidebar listing the scan root, project types and global caches, and a table of Flutter, Android and Node projects with their stack, cleanable size and a Clean button" width="1000">
</p>

## What it does

DevSweep finds build artifacts, dependencies, and caches in Flutter, Android, iOS, Java, Node, Python, Rust, and .NET projects, plus shared tool caches. Review their sizes, choose what to clean, and confirm.

## Features

<table>
  <tr>
    <td width="33%" valign="top">
      <h3>🧹 A panel per project</h3>
      Every cleanable path in a project with its size. Risky entries start unchecked, and the confirmation dialog spells out the count, the total and any item over 5 GB.
    </td>
    <td width="33%" valign="top">
      <h3>📊 Numbers in the background</h3>
      The list appears first and sizes fill in as they are measured. Filter by stack, search by name or path, sort by name or size.
    </td>
    <td width="33%" valign="top">
      <h3>🌍 The caches outside your projects</h3>
      One view for the package and toolchain caches every project shares, each with its real size. Entries that need a re-download say so.
    </td>
  </tr>
</table>

### Global caches

| Dependency cache | Toolchain cache |
|---|---|
| Gradle caches, Gradle distributions, Maven local repository, npm, pnpm, bun, Yarn, pub, CocoaPods, Go module cache | Playwright, Xcode DerivedData, Cargo registry, pip, Go build cache |

Clearing one of these frees space for every project at once, and the next build downloads or rebuilds
what it needs. The list is fixed: `rules.toml` does not extend or disable it.

The Go build and module caches are queried from the current local Go configuration, including environment and
`go env -w` overrides. Go must be available to the app; failed queries never guess a default path.
Missing or rejected Go caches remain visible with an explanation. Go project recognition is not included.

Go cache paths are checked for protected directories, links and overlaps with known projects or other
caches. Custom locations outside the home directory are supported when these checks pass. Refresh
queries the location again; a changed location at execution cancels the old confirmation. Both caches
follow the selected Trash / Direct setting. Build cache cleanup makes subsequent builds slower.
The module cache is Dangerous: dependencies must be downloaded again, offline builds may fail, and
manual changes inside the cache will be lost. Read-only module directories are handled without
privilege escalation; failures can leave permission changes. Stop Go builds, tests and downloads first. Running tasks are not detected or locked. Failures are shown and
remaining contents are measured again; partial deletion cannot be rolled back.

## Install

Download the build for your computer from the [latest release](https://github.com/skana168/DevSweep/releases/latest), or browse [all releases](https://github.com/skana168/DevSweep/releases). `<version>` is the version number, for example `0.3.0`.

| Platform | File | What it is |
|---|---|---|
| macOS | `DevSweep_<version>_macos.dmg` | Open it and drag DevSweep into Applications |
| Windows | `devsweep_<version>_windows.zip` | Portable: unzip and run `devsweep.exe`, no installer |

> [!IMPORTANT]
> **The builds are unsigned and not notarized**, so the OS will stop them once. That is expected,
> not corruption.
>
> **macOS (Gatekeeper)**: if macOS says it cannot check the app, open System Settings → Privacy &
> Security and click "Open Anyway" at the bottom, or right-click the app, choose Open and confirm.
> You can also drop the quarantine flag:
>
> ```sh
> xattr -dr com.apple.quarantine /Applications/DevSweep.app
> ```
>
> **Windows (SmartScreen)**: if "Windows protected your PC" appears, click "More info" → "Run
> anyway". Some antivirus tools flag new unsigned executables; allow the build if you trust it.

## Usage

1. Click `+ Add directory` in the sidebar and add the folders your projects live in, e.g. `~/Projects` or `D:\code`
2. Click a row's `Clean` to open its panel and tick the entries to delete; dangerous ones start off
3. Click `Clean N · 380 MB` at the bottom of the panel, check the dialog, confirm
4. Global caches are under "Global caches" in the sidebar, cleared one entry at a time
5. A row's `...` menu opens the project folder or a terminal there

Deletions go to the **Trash** (Recycle Bin on Windows) by default, so a mistake is recoverable;
Settings can switch to permanent deletion. The list refreshes when the window regains focus after
more than 30 seconds away: measured sizes survive that refresh, the panel's ticks do not.

### Cargo projects

A directory containing a `Cargo.toml` file is recognized as a Rust project without parsing the
manifest. This includes workspace roots with no package of their own. Workspace roots and member
directories appear separately, even when they have no cleanable items; membership is not resolved.
Discovery stays within the scan roots, up to five levels deep, and skips build and dependency folders.
Each project has one stack: Unity > Flutter > iOS > Android > Java > Node > Python > Rust > .NET. Higher-priority markers
win when they share a directory with `Cargo.toml`; independent Rust subdirectories can still appear.

Rust cleanup covers the whole real `target/` directory directly under each Rust project. It is
marked dangerous and starts unchecked: it can contain final binaries, documentation and packages
as well as build caches. A workspace's shared root `target/` belongs to the root project; members
only clean their own local `target/`, without searching ancestors. Files and symbolic links are
excluded, and these checks run again in the final delete plan. Rule overrides retain these checks
and the dangerous label.

DevSweep does not run Cargo or resolve custom target-dir/build-dir settings. Rust built-in cleanup
leaves source, `Cargo.lock` and `.cargo/` untouched; Cargo registry global cache behavior is unchanged.
Stop builds before cleaning: active builds are not detected or locked, and filesystem changes
between the final check and deletion remain possible.

### .NET projects

C#, F#, and Visual Basic share the `.NET` project type and filter. A real regular file ending in
`.csproj`, `.fsproj`, or `.vbproj` identifies its directory as a project; extensions are matched
without ASCII case sensitivity. Project contents are not parsed, so SDK-style, legacy, empty,
and invalid project files all qualify. Marker directories and symbolic links do not qualify.
Multiple project files in one directory produce one project; `.sln`, `.slnx`, `global.json`, and
source files alone do not identify a project.

.NET has the lowest stack priority, preserving existing mixed-project classifications; an
independent .NET subdirectory can still appear. Discovery keeps the five-level depth and platform
subdirectory ownership rules. Unity projects take priority over generated .NET project files;
see Unity support below.

Cleanup includes only the whole real `bin/` and `obj/` directories directly under each .NET
project, matched with exact lowercase names. They are separate dangerous entries, unchecked by
default; their full sizes are shown even when unchecked. Deleting a selected directory also
deletes any manually added files inside it. Files and symbolic links are rejected at discovery
and checked again in the final delete plan. Rule overrides can disable an entry or opt in to
default selection, but retain the dangerous label and validation.

Custom output paths, centralized `artifacts/`, `.vs/`, `TestResults/`, and NuGet global caches
are outside these built-in rules. DevSweep does not run dotnet or MSBuild; this is not equivalent
to `dotnet clean`. Stop builds before cleaning: active builds are not detected or locked, and
filesystem changes between validation and deletion remain possible. .NET support is implemented
in source and has not yet been released.

### Unity projects

Unity is recognized by `Assets/`, `ProjectSettings/`, and `ProjectSettings/ProjectVersion.txt`,
without generated `.csproj` files. It takes priority over other stacks and owns its entire tree,
including nested scan roots. The Unity filter and project row show version qualification.

Only whole, real root-level `Library/` and `Temp/` directories are offered, with their full sizes.
Library is dangerous and unchecked by default: the next project open reimports resources and may
be slow. Temp starts checked. Rule overrides can disable either rule or change default selection;
generic and extra rules cannot add Unity targets or remove the dangerous label. Assets, Packages,
ProjectSettings, obj, Logs, UserSettings, builds, installations and global caches are excluded.

Cleanup requires Unity 2022.3 or later, including Unity 6's numeric `6000.x` format. Earlier,
malformed and unknown versions remain visible but cannot be cleaned; this floor describes support
scope, not a claim that all earlier versions are unsafe. Planning and both execution entrypoints
validate paths and version, rejecting symbolic links and manually constructed out-of-scope plans.

Any running Unity Editor blocks all Unity cleanup, including batch mode and Editors without a
projectPath argument. Hub alone is allowed. Enumeration failure or ambiguous candidate identity
blocks cleanup, without a force option. macOS uses `ps -Aww -o ucomm=` (kernel accounting names);
Windows uses local PowerShell `Get-CimInstance Win32_Process` Name/ExecutablePath. Unsupported
platforms report unknown. Processes are checked while planning and before each project's execution.
Lockfiles are neither an idle signal nor automatically removed. Other projects continue when Unity
is blocked. Close all Editors and do not start one during cleanup: preflight is not a cross-process
lock, and a later Editor start or filesystem change can still race with deletion.

Trash remains the default regardless of size; empty Trash yourself to release disk space. Direct
remains permanent. **Real Editor acceptance on macOS and Windows is still outstanding**; automated
system-boundary tests are not field validation. See [the acceptance protocol](docs/unity-validation.md).

## Custom rules

Project rules are built in; `rules.toml` turns them off, changes which ones start checked, or adds a
directory of your own:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/DevSweep/rules.toml` |
| Windows | `%APPDATA%\DevSweep\rules.toml` |

"Open rules file" in the app writes a commented template if the file is missing. The full reference
— every built-in rule id, the path restrictions, examples — is in [`docs/rules.md`](docs/rules.md).

## Build from source

Rust stable is required; macOS also needs Xcode Command Line Tools.

```sh
cargo run                    # run in debug
cargo test                   # core tests only, no window needed
./scripts/bundle-macos.sh    # writes target/DevSweep_<version>_macos.dmg
cargo build --release        # Windows portable exe: target\release\devsweep.exe
```

CI runs `cargo test` on macOS and Windows for every push. Icons: `assets/icon.svg` is the source,
rendered by `scripts/make-icons.sh` into `assets/icon.png` (256 px, displayed at 128×128 in this
README), `assets/icon-macos.png` (1024 px, with a centered 824 px tile), `assets/icon.icns`
(macOS bundle) and `assets/icon.ico` (nine sizes from 16–256 px, embedded by `build.rs`). macOS
bundles keep their system icon; direct development launches use the macOS PNG. The script checks
sizes, transparency and margins. Rendered files are committed, so normal builds need no icon tools.

Testing boundary: automated tests cover the core in `src/` (scan, rule matching, sizing, plan,
delete, config). The UI layer has one test: that every font family can draw Chinese (`src/theme.rs`).

## License

[MIT](LICENSE).

The interface follows [Craft Launcher](https://github.com/storytold/craft-launcher)'s ArtCraft design
(Apache-2.0). Fonts: Archivo, Inter and Geist Mono, with Chinese from a GB2312 subset of
[Maple Mono](https://github.com/subframe7536/maple-font) CN, all under the
[SIL OFL 1.1](assets/fonts/). See [NOTICE](NOTICE) and
[ADR 0002](docs/adr/0002-craft-theme-and-bundled-cjk-font.md).
