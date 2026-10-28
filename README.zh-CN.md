<p align="center">
  <img src="assets/icon.png" width="128" height="128" alt="DevSweep 图标：墨黑底板上的电光蓝 D，以斜向留白表现扫掠">
</p>

<h1 align="center">DevSweep</h1>

<p align="center">
  <b>找出项目里悄悄吃掉几十 GB 的构建产物和依赖缓存，勾选不需要的部分，一次清掉。</b><br>
  macOS 与 Windows 上的本地应用：Flutter、Android、iOS、Java、Node、Python 项目，加上它们背后的全局缓存。
  没有云端，没有账号，数据不出本机。
</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="macOS · Windows" src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows-26282e?style=flat-square">
  <img alt="许可：MIT" src="https://img.shields.io/badge/license-MIT-3a3a3a?style=flat-square">
  <img alt="状态：早期 alpha" src="https://img.shields.io/badge/status-early%20alpha-d69e2e?style=flat-square">
  <br>
  <a href="https://github.com/skana168/DevSweep/actions/workflows/ci.yml"><img alt="CI：macOS 与 Windows 测试" src="https://github.com/skana168/DevSweep/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/skana168/DevSweep/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/skana168/DevSweep?style=flat-square"></a>
</p>

<p align="center">
  <a href="https://github.com/skana168/DevSweep/releases/latest"><b>下载 DevSweep</b></a> ·
  <a href="https://github.com/skana168/DevSweep/releases">全部版本</a> ·
  <a href="#从源码构建">从源码构建</a> ·
  <a href="docs/rules.md">规则参考</a> ·
  <a href="README.md">English</a>
</p>

<p align="center">
  <img src="docs/screenshots/main.png" alt="DevSweep 在 macOS 浅色主题下的界面：侧栏列出扫描根、项目类型与全局缓存，主表列出 Flutter、Android、Node 项目及其类型、可清理体积和「清理」按钮" width="1000">
</p>

## 它做什么

每个 Flutter、Android、iOS、Java、Node、Python 项目都会留下东西：`build/`、`node_modules/`、
`.dart_tool/`、`Pods/`、`.next/`、`target/`。DevSweep 遍历你指定的目录，认出每个项目，并用一张表
告诉你其中有多少是可以丢掉的。打开某个项目，勾选要删的条目，确认面板，空间就回来了。同一个窗口里
还列着与单个项目无关的全局缓存。

Python 项目由 `pyproject.toml`、`setup.py`、`requirements.txt` 或 `Pipfile` 识别。当前支持清理
项目根目录的 `.pytest_cache/`、`.mypy_cache/`、`.ruff_cache/`，以及递归发现的 `__pycache__/`。
字节码缓存发现会避开虚拟环境、生成目录、符号链接与独立子项目，不受项目发现五层深度的限制。
即使独立子项目超过五层也不会被父项目代清，需要添加更近的扫描根后单独发现并清理。
根目录的 `build/`、`dist/`、`*.egg-info/` 及 `src/*.egg-info/` 也可清理，均为危险项、
默认不勾选，需自行判断其中是否有人工保存的内容。暂不包含环境清理和覆盖率报告。

它从不自作主张地删东西：你不确认清理计划，就什么都不会动；而项目外的路径、`.git`、家目录、
规则之外的东西永远不会进入这份计划。

## 功能

<table>
  <tr>
    <td width="33%" valign="top">
      <h3>🧹 每个项目一个面板</h3>
      列出项目里每一条可清理路径及其体积。危险项默认不勾，确认框会写明条目数、总大小，以及任何超过 5 GB 的单项。
    </td>
    <td width="33%" valign="top">
      <h3>📊 体积在后台算</h3>
      列表先出现，体积随后填入。可按类型过滤、按名称或路径搜索、按名称或体积排序。
    </td>
    <td width="33%" valign="top">
      <h3>🌍 项目之外的缓存</h3>
      一个页面汇总所有项目共用的包管理器与工具链缓存，各带真实体积；需要重新下载的条目会标注出来。
    </td>
  </tr>
</table>

### 全局缓存

| 依赖缓存 | 工具链缓存 |
|---|---|
| Gradle caches、Gradle distributions、Maven local repository、npm、pnpm、bun、Yarn、pub、CocoaPods | Playwright、Xcode DerivedData、Cargo registry、pip |

清理其中一条会同时为所有项目腾出空间，下一次构建会重新下载或重建它需要的东西。这份清单固定，
`rules.toml` 不能扩展也不能关闭它。

## 安装

从 [Releases](https://github.com/skana168/DevSweep/releases/latest) 下载对应文件，或在
[全部版本](https://github.com/skana168/DevSweep/releases) 里挑选。`<版本>` 是版本号，例如 `0.2.0`。

| 平台 | 文件 | 用法 |
|---|---|---|
| macOS | `DevSweep_<版本>_macos.dmg` | 打开后把 DevSweep 拖进 Applications |
| Windows | `devsweep_<版本>_windows.zip` | 便携版：解压后直接运行 `devsweep.exe`，无需安装 |

> [!IMPORTANT]
> **发行包未签名、未公证**，所以系统会拦一次。这是预期行为，不是文件损坏。
>
> **macOS（Gatekeeper）**：双击提示「无法打开，因为 Apple 无法检查…」时，打开「系统设置 →
> 隐私与安全性」，在页面底部点「仍要打开」；或右键点 App 选「打开」再确认。也可以在终端去掉
> 隔离标记：
>
> ```sh
> xattr -dr com.apple.quarantine /Applications/DevSweep.app
> ```
>
> **Windows（SmartScreen）**：运行 exe 时若出现「Windows 已保护你的电脑」，点「更多信息」→
> 「仍要运行」。个别杀软会对未签名的新 exe 报未知发布者，你信任这个构建就放行。

## 使用

1. 侧栏点 `+ 添加目录`，加入放项目的父目录，如 `~/Projects`、`D:\code`
2. 点某一行的 `清理` 打开面板，逐项勾选要删的内容；危险项默认不勾
3. 点面板底部的 `清理 N 项 · 380 MB`，在确认框里核对后执行
4. 全局缓存在侧栏「全局缓存」，那里逐条清理
5. 每行的 `...` 菜单可直接打开项目文件夹或在终端打开

删除默认进入 **废纸篓**（Windows 为回收站），误删可恢复；设置里可改为直接删除。窗口重新获得
焦点且距上次扫描超过 30 秒会自动刷新：已有的体积会保留，面板里的勾选不跨次保留。

## 自定义规则

项目规则是内置的；`rules.toml` 可以关闭某条、改默认勾选，或追加自己的目录：

| 平台 | 路径 |
|---|---|
| macOS | `~/Library/Application Support/DevSweep/rules.toml` |
| Windows | `%APPDATA%\DevSweep\rules.toml` |

文件不存在时，点界面里的「打开规则文件」会按模板生成带注释的默认文件。完整参考——每条内置规则
的 id、路径限制、示例——见 [`docs/rules.md`](docs/rules.md)。

## 从源码构建

需要 Rust stable；macOS 另需 Xcode Command Line Tools。

```sh
cargo run                    # 调试运行
cargo test                   # 只测 core，不开窗口
./scripts/bundle-macos.sh    # 产出 target/DevSweep_<版本>_macos.dmg
cargo build --release        # Windows 便携 exe：target\release\devsweep.exe
```

CI 在每次推送时于 macOS 与 Windows 上跑 `cargo test`。图标：`assets/icon.svg` 是源文件，由
`scripts/make-icons.sh` 渲染为 `assets/icon.png`（256 像素，在 README 中显示为 128×128）、
`assets/icon-macos.png`（1024 像素，底板 824 像素、四边留白 100 像素）、`assets/icon.icns`（macOS
应用包）与 `assets/icon.ico`（16–256 像素共九档，由 `build.rs` 嵌入 exe）。macOS 应用包保留系统
图标，直接启动开发二进制时使用 macOS PNG。脚本检查尺寸、透明度和留白。渲染结果已提交，
常规构建无需图标工具。

测试边界：自动化测试只覆盖 `src/` 里的 core（扫描、规则匹配、体积、计划、删除、配置）；UI 层
只测一件事——中文字形在每套字体里都画得出来（`src/theme.rs`）。

## 许可

[MIT](LICENSE)。

界面视觉取自 [Craft Launcher](https://github.com/storytold/craft-launcher) 的 ArtCraft 设计
（Apache-2.0）。字体为 Archivo、Inter、Geist Mono，中文字形是
[Maple Mono](https://github.com/subframe7536/maple-font) CN 的 GB2312 子集，均由
[SIL OFL 1.1](assets/fonts/) 授权。详见 [NOTICE](NOTICE) 与
[ADR 0002](docs/adr/0002-craft-theme-and-bundled-cjk-font.md)。
