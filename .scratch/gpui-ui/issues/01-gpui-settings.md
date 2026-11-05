# 01: GPUI 启动与设置闭环

**What to build:** 启动 GPUI 窗口，打开独立设置对话框，切换语言、主题和默认删除方式，并在重启后恢复原有配置；迁移期间 egui 仍可运行作对照。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 核实并锁定可获取的 GPUI Kit 版本，仅通过 Kit facade 使用框架，不混配内部依赖；两个平台均能构建启动。
- [ ] 保留品牌资产，使用 Kit 图标、语义主题及平台字体，中文、英文和路径文本可读。
- [x] 读取现有配置，保存并恢复语言、主题、默认删除方式及窗口尺寸，旧配置无需用户手动迁移。
- [x] 设置支持键盘进入、Escape 关闭与焦点恢复；通过 GPUI UI integration tests 验证设置更改及配置兼容。
- [x] 保留可运行的 egui 对照入口和现有业务测试。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。


## Comments

### 2026-11-05 — 实施与验收证据（待父任务集成 / 最终平台验收）

- 起始分支 `t3/d823d4c4` 干净；HEAD 与 `integration/gpui-ui` 均为 `cb8c12c1d837a472a885da734d78875fa4b11329`，已确认 baseline ancestry。没有创建 checkout、子代理、PR 或 push。
- 按 ADR-0003 保留默认 egui 入口；`cargo run --bin devsweep-gpui --features gpui --locked` 启动仅含本票窗口和设置的 opt-in 前端。共享既有 Config / i18n，无业务语义和清理流程改动。操作与依赖说明见 `docs/gpui-settings.md`。
- 实查 `cargo search gpui-kit` / `cargo info` 与下载的上游源码：最新 0.9.0 因 Fast 可选依赖的 unicode-properties 0.1.3 与 egui 0.37 的 ^0.1.4 冲突，Cargo 解析失败；锁定成功解析的 `gpui-kit = "=0.7.0"` 和 Cargo.lock。只通过 Kit facade 引用框架，未混配内部依赖。来源：[0.9.0](https://crates.io/crates/gpui-kit/0.9.0)、[0.7.0](https://crates.io/crates/gpui-kit/0.7.0)。
- TDD：先运行真实组件测试，得到缺少 SettingsApp / init 和配置位置接口的编译失败，再实现；窗口恢复切片也先得到缺少 window_options 的失败。测试 seam 为已批准的 Config 公开接口和生产 GPUI 组件交互；测试没有直接调用设置变更 handler。UI 测试使用减少动态效果以消除 Kit 入场动画对即时命中的影响，生产仍使用 Kit 默认动画。
- UI integration tests（2 个）：旧配置仅含中文扫描根时正常默认；Command/Control+, 打开；真实 Radio 点击改变语言、主题和默认删除方式；Escape 关闭并恢复工作区焦点；重新从配置创建窗口后 checked 状态恢复；Tab / Space 改变删除方式；窗口调整至 1150×760 后保存并从该尺寸重开；Tab / Enter 从设置按钮进入，Escape 恢复按钮焦点。
- `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：最终全部通过，206 passed、4 ignored、0 failed（包括既有业务 / egui 测试与 2 个 GPUI UI integration tests）。较早在并发编译负载下的 Go 缓存测试出现 Timeout；单独串行 15 passed / 1 ignored，最终完整串行也通过。
- `cargo build --bin devsweep-gpui --features gpui --locked`：macOS aarch64 成功。`cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings` 成功；全目标严格 Clippy 被未改动的 `tests/plan.rs`（4 处）与 `tests/delete.rs`（6 处）既有 cloned_ref_to_slice_refs 阻挡。`cargo fmt --all -- --check` / `git diff --check` 通过。依赖 block 0.1.6 仍有 Rust future-incompat warning。
- macOS 原生验证：实际运行 debug 二进制（临时 app wrapper），看到系统窗口控件、品牌 PNG 与 Kit 设置图标；Command+, 打开、Escape 关闭、Tab / Enter 重开；切换 English 与 Dark，屏幕检查中英文可读及语义浅 / 深主题，随后恢复原 System 语言和主题。路径字符串仅在配置兼容测试中验证保留，本票未实现项目路径列表，未声称其原生视觉验收。
- 第一、二项仍未勾选：本机仅安装 aarch64-apple-darwin target，没有 Windows 构建 / 运行环境，未运行 Windows；Windows 字体 fallback、窗口 / 键盘 / 路径视觉也没有实际证据。CI 已增加 macOS / Windows 的 GPUI build 和 UI integration tests，但未运行的 CI 不算通过。双平台最终验收和 ticket resolution 留给父任务。
