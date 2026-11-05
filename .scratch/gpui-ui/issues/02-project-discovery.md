# 02: 扫描根管理与项目发现

**What to build:** 在项目视图管理扫描根，后台发现项目并展示项目列表，用户能理解扫描进度及异常并发起刷新。

**Blocked by:** 01 — GPUI 启动与设置闭环

**Status:** ready-for-agent

- [x] 左侧提供项目 / 全局缓存导航，扫描根管理位于项目视图；支持添加、移除扫描根并持久化。
- [x] 复用既有发现语义，包括被覆盖的扫描根及 Project、Stack 识别规则。
- [ ] 扫描在后台执行，界面保持响应；进度只反映真实工作，空状态和扫描错误提供明确的后续操作。
- [x] 支持刷新入口及平台约定的快捷键；测试扫描根变更、项目发现、空状态与失败路径。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。


## Comments

### 2026-11-05 — 实施与自动化证据（平台验收仍待完成）

- 开始前确认工作树 `t3/19c698cc` 干净，HEAD 与 `integration/gpui-ui` 均为 `1839078ddb89e7751d318e29aec41a685096b53e`；没有创建 checkout、子代理、push 或 PR。完成前执行 `git merge integration/gpui-ui`，结果 `Already up to date`（tip 仍为 `1839078`）。
- 已读本规格、01 实施/集成评论、AGENTS、GLOSSARY、ADRs、host 上 gpui-kit / gpui-kit-design-guides 技能及 Coding/Design/测试/组件约定指南；按当前 Cargo.lock 的 Kit 0.7.0 源码核实 Input、Button、异步任务、虚拟列表与测试 API。CodeGraph 优先尝试后报告工作树无可用索引，转用源码检索，未生成索引。
- `gpui_projects::ProjectDiscovery` 独立持有输入、后台发现任务与结果。窗口壳继续持有 Config 和设置；扫描根变更事件通过既有 `config::save_to` 保存。新增根在后台规范化，重复根提示，被覆盖根保留配置并标识；移除父根会重新发现保留子根，旧请求结果不会覆盖新请求。项目结果复用 `scan::active_roots`、Marker / Stack / 排序 / 深度 / 剪枝 / Unity 边界；虚拟列表只渲染可见范围。刷新保留当前可用结果直到新结果返回，不测量也不改清理草稿。
- 左侧项目 / 全局缓存导航由本票维护，`gpui_settings::render_global_cache_content` 是 06 的小型内容接入点，目前只有占位标题；本票没有全局缓存发现、测量、选择或清理行为。设置对话框、配置字段及默认 egui 入口保留。
- 原 `scan` 是忽略遍历错误的 best-effort API；新增 `scan_with_errors` 共用同一遍历，保留部分 Projects 和失败路径 / OS 错误，原 API 的项目发现结果保持兼容。UI 提供失败详情、复制、刷新重试及移除根的恢复路径；不显示虚构百分比，仅在实际后台任务等待时显示发现状态。
- TDD red → green 切片：真实生产窗口测试先在缺少 `discovery-empty` 失败，再接入发现 UI；公开业务接口测试先在缺少 `scan_with_errors` 编译失败，再实现报告；失败恢复 UI 测试先在缺少复制控件失败，再接入复制；覆盖根测试先暴露 `/var` 与 `/private/var` 未规范化的持久化差异，再后台规范化；之后同一测试发现覆盖提示被长路径挤出可见范围，修正根行布局后通过。测试只使用已批准的业务 / Config 公开接口和真实生产 GPUI 输入、按钮、快捷键，没有直接调用 UI handler 或伪造扫描结果。
- 3 个新增 GPUI UI integration tests：① 空状态 → 输入中文扫描根 → Tab / Enter 添加 → 真实异步发现 Rust Project → 重复根提示及配置不重复 → 缓存占位 / 项目导航 → Command/Control+R → 移除并持久化；② 启动缺失根 → 设置仍可打开 / Escape 关闭 → 显示错误而非成功空状态 → 复制具体失败路径 → 修复目录并通过快捷键刷新为空状态 → 创建 Node Marker 后按钮刷新发现 Project；③ 子根启动 → 添加父根 → 规范化配置与 Covered root 提示 / 两个 Project → 移除父根 → 子根配置保留并在新窗口重启后发现。原 2 个 SettingsApp 设置 / 主题 / 删除方式 / 窗口尺寸 / 焦点测试全部保留并通过。
- `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：210 passed、4 ignored、0 failed（含 egui / 业务 / 安全回归、24 个 scan 测试及 5 个 GPUI 真实组件测试）。最后补强键盘添加、重复提示及真实等待的按钮状态后，再运行 `cargo test --lib --features gpui-tests --locked gpui_settings::tests -- --test-threads=1`：5 passed。完整日志临时保存于 `/tmp/devsweep-ticket02-tests.log`。
- `cargo build --bin devsweep-gpui --features gpui --locked`：macOS aarch64 构建成功。
- `cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过。依赖 `block 0.1.6` 仍有既有 future-incompat warning；4 个 ignored 测试涉及系统 Trash / 本地 Go 等既有手动条件，不声称通过。
- 第三项保持未勾选：后台线程、空状态和失败恢复已由自动化验证，但本票未实际操作 macOS 原生窗口验证大目录扫描期间的响应、长路径 / 字体 / 缩放和系统权限失败，也未在 Windows 构建或运行。真实 macOS / Windows 流程验收继续由 08 收集，未运行的 CI 不算验收；不标记 resolved。无清理 / 中止 / 测量验收声明。
