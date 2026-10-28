<p align="center">
  <img src="assets/icon.png" width="128" height="128" alt="DevSweep icon: an electric-blue D with a diagonal sweep cut on an ink-black tile">
</p>

<h1 align="center">DevSweep</h1>

<p align="center">
  <b>Find the tens of gigabytes your build folders and package caches are quietly eating, tick what you
  don't need, and clear it in one go.</b><br>
  A local desktop app for macOS and Windows: Flutter, Android, iOS, Java, Node and Python projects, plus the global
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
  <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="docs/screenshots/main.png" alt="DevSweep on macOS in the light theme: a sidebar listing the scan root, project types and global caches, and a table of Flutter, Android and Node projects with their stack, cleanable size and a Clean button" width="1000">
</p>

## What it does

Every Flutter, Android, iOS, Java, Node and Python project leaves something behind: `build/`, `node_modules/`,
`.dart_tool/`, `Pods/`, `.next/`, `target/`. DevSweep walks the folders you choose, recognises each
project, and shows how much of it is expendable in one table. Open a project, tick the entries you
want gone, confirm the panel, and the space comes back. The same window also lists the global caches
that have nothing to do with any single project.

Python projects are recognised by `pyproject.toml`, `setup.py`, `requirements.txt`, or `Pipfile`.
Python cleanup covers root-level `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`, and recursive
`__pycache__/` directories. Bytecode cache discovery skips virtual environments, generated
directories, symbolic links, and independent child projects, even beyond the five-level project
discovery limit. Add a nearer scan root to discover and clean those deep child projects.
Root-level `build/`, `dist/`, `*.egg-info/`, and `src/*.egg-info/` are also listed as dangerous
items, unchecked by default: their contents may include files you saved manually.
Root-level `.venv/`, `venv/`, and `env/` environments containing `pyvenv.cfg` are also listed
as dangerous and start unchecked: manually installed dependencies may not be reproducible.
Their contents are excluded from project discovery. Coverage reports are not included.

It never deletes anything on its own: nothing is touched until you confirm a plan, and nothing
outside the rules (`.git`, your home folder, the project root, paths outside the project) can ever
be part of that plan.

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
| Gradle caches, Gradle distributions, Maven local repository, npm, pnpm, bun, Yarn, pub, CocoaPods | Playwright, Xcode DerivedData, Cargo registry, pip |

Clearing one of these frees space for every project at once, and the next build downloads or rebuilds
what it needs. The list is fixed: `rules.toml` does not extend or disable it.

## Install

Download the build for your computer from the [latest release](https://github.com/skana168/DevSweep/releases/latest), or browse [all releases](https://github.com/skana168/DevSweep/releases). `<version>` is the version number, for example `0.2.0`.

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
