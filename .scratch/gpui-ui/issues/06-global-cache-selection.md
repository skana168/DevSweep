# 06: 全局缓存发现、测量与选择

**What to build:** 在全局缓存视图独立查看固定缓存清单、测量体积、了解清理代价并选择缓存条目。

**Blocked by:** 01 — GPUI 启动与设置闭环

**Status:** ready-for-agent

- [ ] 全局缓存导航与视图可独立使用，按既有 Cache group 展示当前平台的 Cache entry。
- [ ] 后台测量显示真实状态，支持空状态及失败反馈；未知体积不显示为零。
- [ ] 清楚展示风险与 Needs re-download 提示，危险条目默认不勾选。
- [ ] 支持鼠标、方向键和空格选择，显示所选范围及体积，长路径和窗口缩放下仍可操作。
- [ ] 验证平台路径解析、分组、测量状态、风险默认值及选择交互；保持固定清单与项目 Rule 的边界。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。

## Comments

### 2026-11-05 — 独立缓存实体实现与自动化证据（待主壳集成 / 平台验收）

- 开始前确认准备好的 `t3/global-cache-selection` 工作树干净，HEAD 与 `integration/gpui-ui` 均为 `1839078ddb89e7751d318e29aec41a685096b53e`。没有创建 checkout、子代理、push 或 PR。完成前执行 `git merge --no-edit integration/gpui-ui`，返回 Already up to date。
- 按 TDD、host 路径的 gpui-kit / gpui-kit-design-guides 和当前 0.7.0 依赖源码实现。CodeGraph 首次尝试 `codegraph explore` 报告工作树没有可用索引，之后使用源码检索，未创建索引。未修改依赖版本或 Cargo.lock。
- `src/gpui_cache.rs` 的 `GlobalCacheView` 独立持有固定清单、后台任务、逐条结果、焦点、滚动和选择草稿。静态路径复用 `cache::entries`，Go 复用 `go_cache::discover` 及扫描根 / 项目 / 其它缓存保护上下文；没有引入项目 Rule，也没有删除、计划或执行入口。普通目录复用 `size::dir_size`，未知、缺失、非目录和不可读状态不显示为零；完成计数来自真实返回结果，失败也计入已完成。
- 按既有 Cache group 展示；普通条目成功测量后默认选择，危险 Go module / Cargo registry 必须主动选择。显示重新下载、Rust 构建影响、Go module 丢失手工修改与离线构建风险、Go build 重建代价。已选范围与字节汇总固定在底部；小于 MB 的体积使用真实 B 数，避免用破折号掩盖零或小缓存。
- 鼠标 Checkbox、Tab/Space、上下方向键、Home/End；方向键滚动至目标行，焦点与勾选分开。长路径截断且有 Kit 图标的可聚焦“复制路径”按钮，能取得完整 Unicode 路径。语义主题和 rem 布局继承现有 Kit 配置。
- 未修改 02 所属 SettingsApp / 共享 shell。`cargo run --bin devsweep-gpui-cache --features gpui --locked` 是独立 opt-in 演示窗口，读取相同 Config 的语言、主题、窗口尺寸和扫描根；不写配置。共享壳接入、实体保留/重建、语言更新以及 07 清理重检接口说明在 `docs/gpui-global-cache.md`。默认 egui 与现有 GPUI 设置入口保留。
- TDD 红→绿证据：首个真实 Checkbox 流程先因缺少 GlobalCacheView 编译失败；随后缺失/非目录与加载状态测试先因缺少进度目标失败；分组/放大/路径复制先因缺少分组目标失败；空状态先因缺少空状态目标失败；Cargo 明确重新下载提示先因缺少风险目标失败，再逐片实现通过。链接兼容测试起初错误假设根链接不遍历，实查既有公开 API 后修正夹具与预期，保留原行为，未更改业务 API。
- 最终 5 个 GPUI UI integration tests 使用生产实体、真实 Kit 组件和临时文件系统，通过正常鼠标/键盘事件验证：`real_cache_checkboxes_keep_danger_opt_in_and_report_selected_bytes`（4096 B × 2 的选择汇总、危险默认和风险文案、鼠标/上键/空格）；`pending_missing_and_invalid_paths_never_become_zero_sized_selections`（后台完成前后状态、0/2→2/2、缺失与非目录反馈、不可选）；`grouped_long_paths_remain_keyboard_accessible_at_large_text`（中文分组、640×480、基础字体 20px、End/Space、固定汇总区与完整长中文路径复制）；`empty_catalogue_explains_why_there_are_no_choices`（空状态和零选择）；Unix 上的 `linked_fixed_cache_keeps_existing_measurement_semantics`（原测量 API 的根链接行为）。测试未直接调用选择 handler，没有模拟私有业务协作者。
- `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：211 passed、4 ignored、0 failed，含上述 5 个 UI 测试、原设置 UI 测试、egui 与固定路径/分组、Go 发现/保护和其它业务回归。
- `cargo build --bin devsweep-gpui-cache --features gpui --locked`：macOS aarch64 成功。`cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过。依赖 block 0.1.6 仍有既有 future-incompat warning。
- 验收限制：主壳全局缓存导航由 02/合并者接入，本票提供独立演示入口；未实际进行 macOS 原生窗口目测、Windows 编译/运行、双平台字体/主题/焦点/系统缩放验收，本机仅安装 aarch64-apple-darwin target。以上自动化证据不替代平台验收，顶层清单保持未勾选且不标记 resolved，由 08 与集成补证。普通测量沿用现有 API：子目录遍历错误被跳过、体积是估算；根路径链接沿用既有 WalkDir 行为，后代链接不跟随。清理仍须 07 使用固定 ID 重新建计划与 Go snapshot/context 重检，不可直接删除视图路径。

### 2026-11-05 — integration/gpui-ui 顺序集成证据

- 在共享 checkout 将 02 的 `c26aff449e672c0b688659f7bba79742ecd86882` 以 `--ff-only` 快进合入，再将 06 的 `5f53efcbb59a2167e86d47979ae9cd2ef15bb213` 普通合并；合并提交 `deafc6b33602c667ba118c2cdae5a09a15a7d015`，无冲突。未 reset / stash / 改写历史，未 push / PR / 创建子代理，保留各工作树；未修改并发实现者的工作树或分支。用户原有 `skills-lock.json` 和 `.claude/` 保持未提交。
- 主壳首次进入缓存页时创建 GlobalCacheView，导航切换保留同一实体及选择草稿；扫描根变化或发现的项目路径集合变化时丢弃旧实体，下次使用当前 Config / project_paths 重建，避免沿用旧 Go 保护上下文。语言变更同步项目和缓存实体；主题继续使用 Kit 全局 Theme。缓存仍只提供选择草稿，无清理执行入口。
- 新增真实主壳 UI integration test `main_navigation_retains_cache_selection_and_updates_settings`：固定缓存条目与临时目录实际后台测量，鼠标取消选择 → 项目页 → 缓存页保持取消；设置切换中文 / 深色、Escape 返回后分组语言、主题与选择仍正确。更新原导航测试以验证实际缓存进度控件，不再断言占位标题。
- 本次执行：`cargo test --lib --features gpui-tests --locked gpui_ -- --test-threads=1` 11 passed；`cargo test --test scan --features gpui-tests --locked` 24 passed；`cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 均通过。临时日志 `/tmp/devsweep-integration-{gpui,scan,clippy}.log`。全量测试沿用各实现评论证据，本次没有重跑全量或原生窗口构建/操作；依赖 block 0.1.6 的既有 future-incompat warning 仍存在。
- 未验收：Windows 构建/运行，macOS 原生大目录扫描响应及视觉检查，双平台字体/缩放/长路径/焦点与系统权限场景、完整清理链路。无头测试不等于原生验收，顶层未勾选项维持原状，不标记 resolved；最终平台接受仍由 08 收集。
- 06 主壳接入缺口已补齐；其余原生 Windows/macOS 验收仍待 08，07 必须继续按固定 ID 建计划并重检 Go snapshot/context，不能把视图路径当删除授权。
