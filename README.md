# DevSweep

找出开发机器上被项目缓存和全局缓存占掉的几十 GB，勾选后一次清掉。
A local GUI for finding and cleaning development caches on macOS and Windows.

[中文](#中文) · [English](#english)

---

## 中文

### 功能

- 添加多个扫描根目录，自动识别 **Flutter / Android / iOS / Node** 项目（含 monorepo 子项目）
- 按项目列出可回收体积，可展开查看每一条清理项；体积在后台计算，列表先出现
- 按类型过滤、按名称或路径搜索、按名称或体积排序
- 勾选单个或批量清理；删除前有确认框（项目数、条目数、总大小、完整路径），另有 Dry-run 预览
- 默认把内容移入**废纸篓**（macOS）/ 回收站（Windows），可在设置中改为直接删除
- **全局缓存**页列出 npm、Gradle、pnpm、bun、Yarn、pub、CocoaPods、Playwright、Xcode DerivedData、Cargo registry、pip 等缓存的路径与体积，逐条清理
- 中文 / English 界面（默认跟随系统），深色 / 浅色主题
- 项目内的清理规则可用 `rules.toml` 关闭、改默认勾选、追加自定义目录；全局缓存清单固定，不受其影响

### 安装

从 [Releases](../../releases) 下载：

| 平台 | 文件 | 用法 |
|---|---|---|
| macOS | `DevSweep_<版本>_macos.dmg` | 打开后把 DevSweep 拖进 Applications |
| Windows | `devsweep_<版本>_windows.zip` | 解压后直接运行 `devsweep.exe`（便携，无需安装） |

发布包**未签名、未公证**，所以系统会拦一次。这是预期行为，不是文件损坏。

**macOS（Gatekeeper）**：双击提示「无法打开，因为 Apple 无法检查…」时，打开「系统设置 → 隐私与安全性」，在页面底部点「仍要打开」；或右键点 App 选「打开」再确认。也可以在终端去掉隔离标记：

```sh
xattr -dr com.apple.quarantine /Applications/DevSweep.app
```

**Windows（SmartScreen）**：运行 exe 时若出现「Windows 已保护你的电脑」，点「更多信息」→「仍要运行」。
个别杀软会对未签名的新 exe 报未知发布者，按需加入信任。

### 使用

1. 左下角 `+ 添加目录` 加入你的项目父目录（如 `~/Projects`、`D:\code`）
2. 列表出现后，展开行确认要删的条目，勾选项目
3. 点 `清理 N 项 · 380 MB`，在确认框里核对路径后执行
4. 全局缓存在侧栏「全局缓存」，那里逐条清理；标了「需重新下载」的条目清理后要重新拉依赖

窗口重新获得焦点且距上次扫描超过 30 秒会自动刷新；已有的体积与勾选会保留。

### 规则文件 `rules.toml`

- macOS：`~/Library/Application Support/DevSweep/rules.toml`
- Windows：`%APPDATA%\DevSweep\rules.toml`

文件不存在时，点界面里的「打开规则文件」会按模板生成带注释的默认文件。三种用法：

```toml
# 1. 关闭不信任的内置条目
disable = ["node.dist", "ios.pods"]

# 2. 改条目的默认勾选状态
[defaults]
"node.dist" = true

# 3. 追加自己的清理目录（stack: flutter / android / ios / node / "*"，label 可选）
[[extra]]
stack = "*"
path = ".cache"
label = "Cache"
```

内置条目 id 从类型前缀开始，例如 `flutter.build`、`flutter.dart_tool`、`android.build`、`android.module_build`、`ios.pods`、`ios.xcuserdata`、`node.node_modules`、`node.next`、`node.dist`（默认不勾）。已发布的 id 不会改名。

`path` 只接受项目内的字面相对路径，可含单段通配 `*`（如 `*/build`）；写错或指向项目外时该条被忽略并在界面上提示。

### 从源码构建

需要 Rust stable。macOS 另需 Xcode Command Line Tools。

```sh
cargo run              # 调试运行
cargo test             # 只测 core，不开窗口
./scripts/bundle-macos.sh   # 产出 target/DevSweep_<版本>_macos.dmg
cargo build --release  # Windows 便携 exe：target\release\devsweep.exe
```

图标：`assets/icon.svg` 是源文件，由 `scripts/make-icons.sh` 渲染为 `assets/icon.icns`（macOS bundle 用）与 `assets/icon.ico`（构建期由 `build.rs` 嵌入 exe）。渲染结果已提交，常规构建不需要 rsvg 或 ImageMagick。

测试边界：自动化测试只覆盖 `src/` 里的 core（扫描、规则匹配、体积、计划、删除、配置）；UI 层只测一件事——中文字形在每套字体里都画得出来（`src/theme.rs`）。

### 字体与致谢

界面视觉取自 [Craft Launcher](https://github.com/storytold/craft-launcher) 的 ArtCraft 设计（Apache-2.0），字体为 Archivo、Inter、Geist Mono，中文字形是 [Maple Mono](https://github.com/subframe7536/maple-font) CN 的 GB2312 子集，均由 [SIL OFL 1.1](assets/fonts/) 授权。详见 [NOTICE](NOTICE) 与 [ADR 0002](docs/adr/0002-craft-theme-and-bundled-cjk-font.md)。

### 许可

[MIT](LICENSE)

---

## English

### Features

- Add one or more scan roots; Flutter, Android, iOS and Node projects are detected automatically, including monorepo sub-projects
- Every project shows its reclaimable size, expandable into individual cleanable entries; sizes are computed in the background so the list appears first
- Filter by project type, search by name or path, sort by name or size
- Select one or many projects and clean them; a confirmation dialog shows project count, entry count, total size and full paths, plus a dry-run preview
- Cleaning moves things to the **Trash** by default (Recycle Bin on Windows); switch to permanent deletion in settings
- A **Global caches** view lists npm, Gradle, pnpm, bun, Yarn, pub, CocoaPods, Playwright, Xcode DerivedData, Cargo registry and pip caches with sizes, cleared one entry at a time
- Chinese / English UI (follows the system by default), dark / light theme
- Project rules can be disabled, re-defaulted or extended with `rules.toml`; the global cache list is fixed and unaffected

### Install

Download from [Releases](../../releases):

| Platform | File | Usage |
|---|---|---|
| macOS | `DevSweep_<version>_macos.dmg` | Open it and drag DevSweep into Applications |
| Windows | `devsweep_<version>_windows.zip` | Unzip and run `devsweep.exe` (portable, no installer) |

The binaries are **unsigned and not notarized**, so the OS will stop them once. That is expected, not corruption.

**macOS (Gatekeeper)**: if macOS says it cannot verify the app, open System Settings → Privacy & Security and click "Open Anyway" at the bottom, or right-click the app and choose Open. You can also drop the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/DevSweep.app
```

**Windows (SmartScreen)**: if "Windows protected your PC" appears, click "More info" → "Run anyway". Some antivirus tools flag new unsigned executables; allow it if you trust the build.

### Usage

1. Click `+ Add folder` in the sidebar and add your project parents (e.g. `~/Projects`, `D:\code`)
2. Expand a row to check which entries will go, then select projects
3. Click `Clean N · 380 MB`, review the paths in the dialog, confirm
4. Global caches live under "Global caches" in the sidebar; clear them one by one. Entries marked as needing a re-download will have to be fetched again

The list refreshes when the window regains focus after more than 30 seconds away; measured sizes and your selection survive the refresh.

### `rules.toml`

- macOS: `~/Library/Application Support/DevSweep/rules.toml`
- Windows: `%APPDATA%\DevSweep\rules.toml`

If the file does not exist, "Open rules file" in the app generates a commented template. Three things you can do:

```toml
# 1. turn built-in entries off
disable = ["node.dist", "ios.pods"]

# 2. change whether an entry is checked by default
[defaults]
"node.dist" = true

# 3. add a directory of your own (stack: flutter / android / ios / node / "*", label optional)
[[extra]]
stack = "*"
path = ".cache"
label = "Cache"
```

Built-in ids start with their stack, e.g. `flutter.build`, `flutter.dart_tool`, `android.build`, `android.module_build`, `ios.pods`, `ios.xcuserdata`, `node.node_modules`, `node.next`, `node.dist` (unchecked by default). Published ids never get renamed.

`path` must be a literal relative path inside the project, optionally with a single-segment wildcard such as `*/build`. Invalid or escaping paths are ignored and reported in the UI.

### Build from source

Rust stable is required; macOS also needs Xcode Command Line Tools.

```sh
cargo run                   # run in debug
cargo test                  # core tests only, no window needed
./scripts/bundle-macos.sh   # writes target/DevSweep_<version>_macos.dmg
cargo build --release       # Windows portable exe: target\release\devsweep.exe
```

Icons: `assets/icon.svg` is the source, rendered by `scripts/make-icons.sh` into `assets/icon.icns` (macOS app bundle) and `assets/icon.ico` (embedded into the exe by `build.rs`). The rendered files are committed, so normal builds need neither rsvg nor ImageMagick.

Testing boundary: automated tests cover the core in `src/` (scan, rule matching, sizing, plan, delete, config). The UI layer has one test: that every font family can draw Chinese (`src/theme.rs`).

### Theme and credits

The interface follows Craft Launcher's ArtCraft design (Apache-2.0). Fonts: Archivo, Inter and Geist Mono, with Chinese from a GB2312 subset of [Maple Mono](https://github.com/subframe7536/maple-font) CN — all under the [SIL OFL 1.1](assets/fonts/). See [NOTICE](NOTICE) and [ADR 0002](docs/adr/0002-craft-theme-and-bundled-cjk-font.md).

### License

[MIT](LICENSE)
