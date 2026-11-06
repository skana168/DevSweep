# 08 验证报告（2026-11-05）

结论：**未完成双平台接受验证，禁止切换或移除 egui**。本轮完成 macOS 自动化、部分真实窗口验收与布局、持久化与大体积确认保护修复；Windows 未运行。票据保持未解决。

## 范围与安全

- 工作树 `t3/gpui-cross-platform-verification`，起点 `dbbf56c9bc0e9080a05ffc322bb1a7df1c870e37`。没有子代理、额外 checkout、push、PR 或远程资源。
- 已读 spec、08 全部 Comments、AGENTS、GLOSSARY、三个 ADR、TDD、host 安装的 gpui-kit / gpui-kit-design-guides 及完整 Coding/Design Guides，核对 Kit 0.7.0 当前源码。CodeGraph 命令报告本工作树无可用索引，未自行建索引。
- 测试边界为批准的公开业务/配置接口和生产 GPUI 控件交互。没有新增私有 handler 测试或内部 mock。
- 自动删除仅限 tempfile。原生项目为 `/var/folders/cd/4754c7cd73g1twx1yj5hgql40000gn/T/devsweep-08-native-h8vdqp1_/projects/{alpha,beta}`，各有 30 B payload。只向 Trash 提交 alpha 的 `node_modules`，未提交任何用户缓存删除。
- 原生启动前逐字节备份现有 `config.toml` / `rules.toml` 并记录是否存在；临时配置只扫描上述 fixture，暂时隔离规则覆盖。退出后恢复原文件及原来的不存在状态，并核对字节。未改系统外观、权限、用户项目或真实缓存。

## 修复与回归

1. 最小窗口的控制区与清理结果挤掉项目列表：原生截图中列表仅剩几像素。新增 `confirmation_preserves_filtered_selection_and_failure_keeps_list_usable_at_minimum_size` 通过生产主壳完成跨 Node/Rust 勾选、危险项显式选择、过滤、确认锁定、取消/焦点恢复、刷新发现新项目、真实 Direct 成功和 POSIX 权限 Failure；列表高度断言先红。改为控制区可收缩滚动，主工作区保留 `min_h_24`，随后通过；可见行还能 Down/Space 导航勾选。权限 Failure 子场景仅 Unix，通用确认/筛选部分未加 Unix 条件，但没有 Windows 编译证据。
2. macOS 原生窗口每次重启增高 32 逻辑像素：本轮隔离配置观察到 592 → 624。GPUI macOS `bounds()` 返回 NSWindow 外框，创建使用 `initWithContentRect`；改存 `viewport_size()` 内容尺寸。既有 resize/restart 生产控件测试改为断言内容区尺寸；headless 平台的外框等于内容区，因此它本身不能证明原生标题栏问题。原生连续启动结果见下。
3. 审计发现 GPUI 项目和全局缓存丢失 egui 的 >5 GB 额外勾选确认。项目新增 `large_project_plan_requires_fresh_acknowledgement_before_execute`，固定缓存在既有隔离子进程 `fixed_cache_execution_failure_copy_and_retry` 增补同等场景；真实 5 GB+1 稀疏文件，初始执行无效、勾选、取消/重新审阅必须重新勾选，授权后 Direct 实际清理夹具成功。两处在渲染按钮和 Execute handler 同时加门槛，每个新计划重置 acknowledgement，沿用已有本地化文案。红/绿日志 `/tmp/devsweep-08-large-{project,cache}-{red,green}.log`。
4. 新增 `settings_choices_remain_accessible_at_minimum_window_and_large_text`：880×560、20 px 基准字体、1.5 显示缩放，中英文、System/Light/Dark、设置选项可见且不越过窗口边界、键盘 Tab/Space 打开/关闭后精确恢复设置触发按钮焦点，侧栏同级控件左右边界相等。

## 自动化（不等于原生验收）

命令均在本工作树执行，Cargo 复用 `CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target`。

- `cargo test --locked --features gpui-tests -- --test-threads=1`：**233 passed、0 failed、4 ignored，其中 27 个 GPUI UI integration tests**；涵盖业务、配置、egui 与生产 GPUI UI integration tests。
- `cargo fmt --check`、`git diff --check`：通过。
- `cargo clippy --locked --features gpui-tests --lib --bins -- -D warnings`：末次修复后通过。
- `cargo clippy --locked --all-targets --features gpui-tests`：通过，有既有测试 lint。尝试额外的 all-targets `-D warnings` 时，既有 `handle.into()` 同型转换、clone 构造 slice 等使其失败；没有用 allow 隐藏问题，也不将其写成严格 all-targets 通过。生产代码严格检查单独列出。
- `cargo build --locked --features gpui --bins`：macOS arm64 通过。`block 0.1.6` 既有 future-incompat 警告仍存在。
- 默认并行套件首次有 3 个、复核有 12 个 `tests/go_cache.rs` 外部 shell fixture 触发 1 秒 Timeout；串行通过。普通 fixture 查询预算改为 5 秒后，默认并行全量 **233 passed、0 failed、4 ignored**。专用 30 ms 超时测试及其小于 1 秒的断言保持，生产超时未改。红/绿证据 `/tmp/devsweep-08-final-parallel.log`、`/tmp/devsweep-08-final-parallel-green.log`。磁盘耗尽期间的失败/不完整日志不计验收；恢复空间后重新执行。
- 日志：`/tmp/devsweep-08-final-tests.log`、`/tmp/devsweep-08-production-clippy.log`、`/tmp/devsweep-08-all-targets-clippy.log`、`/tmp/devsweep-08-build.log`；布局红/绿分别 `/tmp/devsweep-08-crossflow.log`、`/tmp/devsweep-08-crossflow-green.log`，设置测试 `/tmp/devsweep-08-settings.log`，尺寸 `/tmp/devsweep-08-window.log`。日志为本机临时证据，不是版本化像素基准。

## egui 产品行为覆盖审计

旧 `src/app.rs` 测试按产品承诺归组；Craft 颜色/刷新字形等旧视觉实现由 ADR-0003 替代，不要求复制像素或自绘窗口控件。

| egui 保护的行为 | GPUI / 公开业务覆盖 | 本轮结论 |
| --- | --- | --- |
| 刷新只发现项目、不扰动全局缓存；设置根持久化；父根/covered root | `project_roots_discover_refresh_and_persist_through_real_controls`、`covered_root_survives_parent_removal_and_restart`、`main_navigation_retains_cache_selection_and_updates_settings` | 自动化通过 |
| 后台扫描、失败后重试/复制、根改变拒绝旧结果、未知体积与真实测量进度 | `discovery_failure_can_be_copied_and_refresh_recovers_without_blocking_settings`、`pending_measurement_is_unknown_and_removed_root_cannot_restore_old_results`、`project_size_is_measured_in_background_and_refresh_keeps_known_size`、`tests/refresh_lifecycle.rs` | 自动化通过；大目录原生性能仍待测 |
| 类型筛选含 Unity；跨项目勾选/刷新保留；Project size 不随草稿变化 | `stack_filter_and_keyboard_selection_survive_refresh`、`target_panel_retains_dangerous_draft_across_projects`、`chosen_size_tracks_safe_cross_project_selection_and_clean_request` | 自动化通过；新增主壳跨流程联合覆盖 |
| 行打开清理面板、长路径、复制、虚拟列表 Home/End | `virtual_project_list_keeps_end_navigation_and_long_paths_at_large_text`、真实 row/target panel 测试 | 自动化通过；原生路径复制/粘贴通过 |
| 草稿默认值、Dangerous 显式选择；缓存只列 present/cleaned；分组与体积 | `real_cache_checkboxes_keep_danger_opt_in_and_report_selected_bytes`、`pending_missing_and_invalid_paths_never_become_zero_sized_selections`、`grouped_long_paths_remain_keyboard_accessible_at_large_text`、`fixed_cache_execution_failure_copy_and_retry` | 自动化通过；原生全局执行未通过 |
| 预览/确认隔离、取消不执行、执行前重检、根锁、超大项确认 | `clean_confirmation_cancel_restores_focus_and_execute_rechecks`、`clean_shortcut_reviews_saved_mode_and_never_executes`、`large_project_plan_requires_fresh_acknowledgement_before_execute`、缓存隔离大体积测试、保留 egui 阈值测试及 `project_clean` batch/重检测试 | 自动化通过；macOS 原生项目大体积确认/取消/重置通过，未原生执行大体积项 |
| 中止仅请求、锁至完成/断连、排队刷新、部分结果、Skip/Failure 区别 | `abort_request_keeps_draft_locked_until_worker_disconnect_and_runs_queued_refresh`、`direct_clean_reports_success_and_failure_details_can_be_copied`、`go_cache_abort_waits_for_real_recheck_and_blocks_duplicate_submit` | 自动化通过；原生项目请求中止→已停止及 Failure 通过，不能代替真实长目录/全局中止 |
| Go 查询与项目刷新独立、环境变化保护、只读 module | `tests/go_cache.rs`、`tests/go_cache_paths.rs`、`tests/go_module_delete.rs`、GPUI Go batch 测试 | fixture 自动化通过；真实 Go 未安装，原生未通过 |
| 设置、主题/语言、窗口记忆、浮层焦点、菜单动作 | `settings_changes_survive_restart_and_escape_restores_focus`、`resized_window_and_saved_settings_are_restored_in_a_new_window`、本轮设置缩放测试；复制已为可见按钮 | 自动化通过；原生尺寸修复/语言/主题部分通过；Windows chrome 未验证 |
| 安全拒绝、Unity、rules/config 兼容与删除报告 | 保留所有 plan/delete/unity/refresh/config 业务与 egui 回归 | 自动化通过；真实 Unity Editor 跨平台场景未通过 |

## macOS 原生窗口（通过 CUA 实际操作）

- 临时 `.app` 包装本工作树构建的 `devsweep-gpui`，仅为获得可访问性身份；不是签名/发布包验收。
- 实际发现两个 fixture，测量各 30 B；Node 过滤 1/2；勾选、Command+Shift+Backspace 打开计划、Trash 默认、Escape 取消、方向键/Space 后勾选变化均已观察。
- 设置 Command+,、中文/English、Light/Dark/System 均实际选择。截图核对英文浅色、中文深色、跟随当前浅色系统主题；没有更改 OS 外观，因此“运行期间系统外观切换跟随”仍待测。
- 全局缓存页实际进入、勾选、快捷键审阅计划并 Escape 取消、返回项目保留上下文。全局内容来自真实用户缓存，只读审阅，**从未点击全局执行按钮**，故其清理/中止/恢复未通过。
- 原生路径复制 → 扫描根输入框 Command+V，AX 值等于 alpha 的完整 canonical path；失败详情复制 → 同一未提交输入框粘贴，值含 `alpha/node_modules: 其他错误`。随后清空输入，未添加扫描根。
- 项目 Trash 实际调用 Finder：长时间等待后显示 `Failure: 1`，Freed bytes 0 B，目标 payload 仍在；详情可打开/复制，设置/重试仍可操作。再次执行并点击中止，显示“已请求中止 · 等待当前项目完成”，完成后实际截图为“已停止 · 1/1 个项目 · 已释放（估算）：0 B · 跳过：0 · 失败：1”。没有捏造 Trash 成功或恢复。
- 原生窗口从默认宽度拖至 880；修复前失败/运行结果挤掉列表，修复后同类结果仍有完整 96 逻辑像素项目行，控制区滚动。880 宽截图中长路径在所属区域截断/滚动，复制得到完整路径。
- 内容区尺寸修复后连续两次退出/启动，隔离配置均保持 width=880.0、height=560.0；没有再次增加标题栏高度。退出后 config.toml 已与备份逐字节相等，rules.toml 恢复原来的不存在状态，fixture payload 完整。
- 追加 5 GB+1 稀疏文件后原生重启，确认显示 5.0 GB 和额外 checkbox；未勾选时按钮呈禁用外观、点击没有效果；勾选后取消再打开，checkbox 回到 off。只审阅/取消，未对该大体积项执行删除。

## Design review checklist

1. 任务与下一步：项目/全局导航、Project size / Chosen size、预览后执行区分清楚；原生和自动化均实际走过。
2. 动作承诺：清理快捷键只审阅，取消不删除，危险项主动选择，中止等待当前工作；上述已验证。系统 Trash 恢复仍不可承诺验收通过。
3. 层级：主工作区、右侧 target inspector、底部范围与结果分层；主题语义颜色、Kit 控件、品牌图标保留。未引入 raw color/px 布局修补。
4. 简化：仅修正区域收缩和尺寸单位，不新建入口或改变清理安全语义。
5. 几何：新测试测量同级导航左右边界相等、设置控件不越界、清理入口在 viewport 内、失败后主列表至少 6 rem；实际最小宽度截图复核完整行。并未声称全界面所有边缘/滚动条均已逐像素合格。
6. 组件系统：保留 Kit Dialog、Radio、Button、Resizable、原有滚动 owner；无自绘替代交互。
7. 状态/约束：自动化覆盖 unknown/loading/empty/error/确认/执行/中止/断连，双语言、字体与显示缩放；真实权限/占用、reduced motion、OS IME、大数据压力还有平台待测项。
8. 真实窗口：已实际操作 macOS，发现并修复列表挤压与标题栏尺寸问题，审计补回大体积保护；Windows 无真实窗口证据，清单整体仍未通过。

## 必须补齐的接受条件（均未通过）

- Windows：本机仅 `aarch64-apple-darwin` Rust 目标；无 wine/wine64/prlctl/tart/qemu-system-x86_64 或 Windows VM app。工具目录无环境链接工具，尝试 `t3_environment_links` 不可调用。未执行 Windows 编译，更无原生成功证据。需在授权 Windows 环境构建/跑业务和 GPUI 套件，再走扫描→筛选→跨项目选择→预览/取消/确认→Trash/Direct、项目/全局中止与失败恢复，含回收站恢复、占用/权限、长路径、字体、主题/语言、缩放、快捷键/焦点/剪贴板。
- macOS：系统 Trash 成功及恢复（本轮实际 Failure）；只读 Go module Trash 恢复；真实 Go 工具链/环境变化；安全隔离的全局缓存完整执行/中止/失败恢复；原生 Direct 成功/权限恢复；真实长目录与大计划滚动、中止边界/响应性；全局缓存大体积 acknowledgement 原生操作；真实 Unity Editor 保护；reduced motion、IME、OS 外观变化、完整跨主题/语言/最小尺寸几何矩阵。
- 前台切回后的 30 秒自动刷新、Go/缓存联动以及不重复扫描，旧 egui 有保护；本轮 GPUI 只验证了显式刷新、后台队列与导航状态。此项产品行为仍需补齐 GPUI 窗口 activation 交互覆盖及原生接受验证，不能以保留 egui 测试冒充迁移已通过。
- 4 个既有 ignored（大目录性能、本地 Go、两种系统 Trash 恢复）仍未运行，不算通过。没有安装工具链/VM、申请远程资源或修改系统权限来补造环境。


集成同步：末次 `git merge integration/gpui-ui` 返回 Already up to date；本地 tip 仍为 `dbbf56c9bc0e9080a05ffc322bb1a7df1c870e37`。远端无同名 ref，不创建远程资源；父集成者负责后续合入。

## 2026-11-05 — 集成分支复核

- `integration/gpui-ui` 从 `dbbf56c9bc0e9080a05ffc322bb1a7df1c870e37` fast-forward 到实施 HEAD `123b58715fdcad1d070e2999694d300fd6459d79`。本节随后的文档提交记录集成证据，不改变实现；最终分支 HEAD 可由包含本节的提交确定。
- 集成后 `cargo fmt --check`、`git diff --check` 通过；`cargo test --locked --features gpui-tests gpui_ -- --test-threads=1` 退出 0，27 个 GPUI 测试全部通过。未重跑完整套件；上文 233 passed / 4 ignored 是源分支既有证据。既有 `block 0.1.6` future-incompat 警告仍在。
- 自动刷新是**可在本地修复的功能及覆盖缺口**，不只是等待原生证据：`src/gpui_settings.rs::SettingsApp::new` 仅订阅窗口 bounds/appearance、roots/workspace，没有 activation 订阅、最近扫描时间或 30 秒门槛；`Render::render` 的 `RefreshProjects` 仅显式刷新项目。因此缺失失焦→重新激活且距上次扫描至少 30 秒的自动项目扫描，以及同次自动刷新触发 Go 重查、已清理但重新创建缓存的失效/重新测量。旧实现参照 `src/app.rs::DevSweepApp::logic`、`rescan`、`refresh_go_caches`。
- 相应 GPUI 迁移测试还需覆盖：30 秒前不触发、过期切回触发项目与缓存联动、手动刷新后重置时效、已有扫描时不重复启动、清理锁定时安全排队/失效且保留选择与视图。旧保护见 `src/app.rs::tests::sidebar_refresh_click_discovers_projects_without_touching_caches_or_view`、`focus_does_not_start_a_second_scan_while_manual_refresh_is_running`。现有 GPUI 显式刷新/排队测试不能替代 activation 测试。
- 已存在的联动不应误报为全部缺失：`SettingsApp::new` 的 RootsChanged/workspace observer 会在根或项目路径集合变化时调用 `invalidate_caches`；`ensure_caches` 重建视图时，`src/gpui_cache.rs::GlobalCacheView::from_entries` 会重新查询 Go 和测量固定缓存。但路径集合未变化的旧缓存视图不会因前台切回而更新。
- **外部平台/原生证据缺口**另计：Windows 构建和全流程、真实 Go/Unity、系统 Trash 成功与恢复，以及上文未通过的原生 Direct/权限、隔离全局完整流程、长任务、OS 外观/IME/reduced motion/设计矩阵。可在本地增加隔离 fixture/GPUI 自动化，但不能据此声称这些平台/系统集成已验收。补完自动刷新后仍须真实窗口 activation 验收。
- 08 保持 `ready-for-human` 且未完成验收项不勾选；未移除 egui、未开始 09。未 reset/stash/rewrite、push 或创建 PR；保留工作树，原有 `skills-lock.json` 修改及未跟踪 `.claude/` 未纳入提交。

## 2026-11-05 — 激活与过期刷新本地缺口实施

- 工作树 `t3/gpui-activation-stale-refresh`，起点为指定集成提交 `9fd66c27d8b8af3db1c074b6548625623b4a6026`，开始时干净。无 children、checkout 创建、PR/push；未改 egui、用户配置、默认入口或无关文件。CodeGraph MCP 明确返回本工作树没有可查询索引，随后使用源码检索；未建索引。
- 读取 spec/08/本报告、AGENTS/GLOSSARY/三个 ADR、TDD 与 git-commit，以及 host 的 GPUI Kit / Design Guides、Coding Guides 和 testing 指引。核对锁定 Kit 0.7.0 的 `observe_window_activation`、`Window::is_window_active` / `activate_window`、`BackgroundExecutor::now` / `advance_clock` 和 `Task::is_ready` 源码。生产使用执行器单调时钟，测试用同一执行器虚拟时钟，无 30 秒 wall-clock sleep。
- `ActivationRefresh` 公开业务接口负责切回、30 秒边界和扫描中忽略激活；手动刷新请求、实际发现（含根改变）更新时效。主壳保留 activation subscription，以单个 pending 标志合并清理确认/执行期间的激活刷新，项目或缓存锁释放后由已有 observer 释放；项目测量中的刷新继续沿用既有 queued_refresh。没有替换运行中的清理实体/计划，没有绕过确认或中止边界。
- 自动刷新沿用项目 `refresh` / `apply_refresh`，保留选择、危险项草稿、筛选和已知 Project size。全局缓存原位重查 Go、后台检查/重新测量 Cleaned 固定缓存；未重建仍显示 Cleaned，已知固定缓存保持原体积，显式草稿与滚动/实体所有权保留，成功清理记录不选中的草稿。进行中的测量不会重复启动，完成 Task 在下次刷新时回收。现有 RootsChanged/路径集合变化的 `invalidate_caches -> ensure_caches -> GlobalCacheView::from_entries` 完整保留，没有新增重复的根变化处理。

### 新增回归及红/绿证据

- `tests/activation_refresh.rs` 两个公开业务测试：持续激活不触发、29/30 秒边界、手动扫描重置时效、扫描中忽略激活且不追加 follow-up。首个红为公开类型尚不存在，随后通过；日志 `/tmp/activation-{red,green}.log`。
- 生产 GPUI `foreground_activation_refreshes_only_after_grace_period`：真正激活两个 headless 平台窗口，29 秒无新项目、30 秒发现；手动刷新后再次验证 1/30 秒边界；保留 Node 筛选、跨刷新选择、危险项显式勾选、普通项取消、24 B Project size。原始缺少 activation 接线时失败（列表未出现），接线后通过，日志 `/tmp/activation-ui-{red,green}.log`。
- 生产 GPUI `activation_defers_cache_refresh_until_cancel_and_remeasures_recreated_cache`：独立子进程隔离 HOME/USERPROFILE/LOCALAPPDATA/PATH，真实固定 catalogue 路径及 shell Go 边界 fixture；Direct 只删除临时 Gradle，未重建刷新仍 Cleaned；重建为 256 B，Go 从 64 B 改为 512 B，已知 npm 仍显示旧 128 B。缓存确认期间切回不改变旧快照；取消后释放刷新，保留 npm/Go/Gradle 勾选草稿与当前缓存页，项目同路径从 Node 变为 Rust 被发现；紧接着未过期切回不重新查询已变为 1024 B 的 Go fixture。缺少 Cleaned 恢复分支时明确失败：实际 `Cache directory does not exist`、预期 `Cleaned · 0 B`，修复后通过；日志 `/tmp/activation-cleaned-{red,green}.log`。
- headless 新建辅助窗口本身不会激活；测试显式调用 `activate_window` 并 drain 调度器。上述为真实生产 GPUI UI integration tests，不是私有 handler 调用，也不是原生 OS 接受证据。缓存/Go 子场景使用 Unix shell fixture；跨平台公开时效测试和项目激活测试未加 Unix 限制，但 Windows 未编译/运行。

### 本轮检查

所有 Cargo 命令使用 `CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target`。大构建前磁盘约 55 GiB 可用，无磁盘耗尽失败。

- `cargo test --locked --features gpui-tests -- --test-threads=1`：237 passed、0 failed、4 ignored（含 29 个 GPUI UI integration tests），日志 `/tmp/activation-final-tests.log`。末次固定缓存 missing 恢复仅作用固定条目后，另跑全体 GPUI 回归，见下方提交前结果。
- `cargo clippy --locked --features gpui-tests --lib --bins -- -D warnings`：通过，日志 `/tmp/activation-final-clippy.log`。
- `cargo clippy --locked --all-targets --features gpui-tests`：通过；既有测试 lint 仍存在，不称严格 all-targets 通过。日志 `/tmp/activation-final-all-targets-clippy.log`。新增测试的同型 `.into()` 已移除。
- `cargo build --locked --features gpui --bins`：macOS arm64 通过，日志 `/tmp/activation-final-build.log`。既有 `block 0.1.6` future-incompat 警告仍在。
- `cargo fmt --check`、`git diff --check`：通过。

### 仍未通过

本节补齐的是此前报告第 82 行所列本地实现/自动化缺口。**原生 macOS 窗口的失焦/重新激活、真实 Go 环境、Windows 编译与全流程、系统 Trash 成功/恢复以及原报告其余平台/设计矩阵接受条件仍未验证**。4 个 ignored 仍不算通过。未操控原生用户配置或真实缓存，08 保持 `ready-for-human`，不勾选平台验收项，不切换/移除 egui，不开始 09。

提交前复核：`git merge integration/gpui-ui` 返回 `Already up to date.`，集成 tip 仍为指定 `9fd66c27d8b8af3db1c074b6548625623b4a6026`。最后补充释放 activation 队列时复用已排队/正在运行的手动项目 Refresh，避免清理取消时追加一次发现；最终生产代码另跑 `cargo test --locked --features gpui-tests gpui_ -- --test-threads=1`，29 passed / 0 failed，日志 `/tmp/activation-final-gpui.log`；生产严格 Clippy、bins 构建、fmt/diff 复核通过。完整 237/4/0 为上述全量运行记录，末次 GPUI 专项覆盖最终 GPUI 改动。按 git-commit skill 提交，最终 SHA 由包含本节的提交确定；无 push/PR。


## 2026-11-05 — 激活刷新集成复核

- 集成分支 `integration/gpui-ui` 核实 `9fd66c27d8b8af3db1c074b6548625623b4a6026` 为指定实施提交 `78778b9f98a7e64b81c26faa09d15fb9d9ba5749` 的祖先后，`git merge --ff-only` 成功，无冲突、无额外实现修改。源工作树及其他工作树保留。
- 集成后 `cargo fmt --check`、`git diff --check` 通过；`CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target cargo test --locked --features gpui-tests gpui_ -- --test-threads=1` 退出 0，29 个 GPUI 测试全部通过，包含前台激活时效及缓存确认延迟刷新/重建测量。日志 `/tmp/gpui-activation-integration-gpui.log`。
- 同环境运行 `cargo test --locked --features gpui-tests --test activation_refresh --test refresh_lifecycle -- --test-threads=1` 退出 0，公开激活时效测试 2/2、刷新生命周期测试 14/14 通过。日志 `/tmp/gpui-activation-integration-refresh.log`。本轮未重复完整套件；237 passed / 4 ignored、生产严格 Clippy 和 macOS bins 构建为上节源分支证据，既有 block future-incompat 警告仍在。
- 本地 activation 实现/自动化缺口已补齐；仍需真实 macOS 失焦/切回与 Go/缓存联动验收。Windows 编译和同等原生完整流程、系统 Trash 成功/恢复、真实 Go/Unity、原生 Direct/权限恢复、隔离全局执行/中止/失败恢复、真实长任务/大计划及完整主题/语言/缩放/焦点/剪贴板/OS 外观/IME/reduced motion 设计矩阵仍未通过；4 个 ignored 不计成功。沿用本报告逐项接受条件。
- 08 保持 `ready-for-human`，未完成框不勾选，保留 egui 与默认入口，不开始 09。无 children、stash/reset/rewrite、push/PR；原有 `skills-lock.json` 和 `.claude/` 保持未提交。使用 git-commit skill 仅提交集成证据。

## 2026-11-05 — P2 固定缓存失败恢复与确认计划虚拟化

- 工作分支 `t3/gpui-cache-retry-virtualized-plan`，干净起点为指定 `86d8a9c7501bbabb853861ea831369e335b97a10`；`git merge-base --is-ancestor 86d8a9c7501bbabb853861ea831369e335b97a10 HEAD` 退出 0。实施前及提交前 `git merge integration/gpui-ui` 均返回 `Already up to date.`，本地集成 tip 仍为该 SHA。没有 children、checkout 创建、PR/push；未修改用户配置、egui 或默认入口，无无关变更。
- 已读 AGENTS、GLOSSARY、三个 ADR、spec、08/本报告、TDD（批准 seams：公开业务/配置与真实生产 GPUI 交互）、git-commit，以及 `/Users/yoyun/DevSweep/.agents/skills` 下 Kit 的 Coding/Design/Testing 指南。CodeGraph explore 明确报告当前工作树无可用索引，未建索引，随后使用源码检索。
- 固定缓存 `Failed` 现在随刷新重新进入后台测量，包括 Missing/Unreadable/路径类型恢复；保留已存在的显式 draft，初次失败且无 draft 的固定条目恢复后沿用其安全默认选择。Ready 固定条目不重算，Go 的既有草稿逻辑与 Cleaned 语义不变。清理锁定时拒绝刷新，进行中的测量不重复启动；仍由主壳 pending activation 和已有路径集合失效/实体重建边界处理排队及过期结果，没有引入第二套 generation owner。
- 项目确认计划改用锁定 Kit re-export 的 GPUI `uniform_list`，合并 Target/Skip 索引范围，仅在框架请求的可见范围构建行，无全计划元素集合或每帧全数据克隆。核对 `gpui-kit-0.7.0` re-export 和 `gpui-pre-0.3.7/src/elements/uniform_list.rs` 的签名、每帧 `measure_item`、宽度约束、visible range、`track_scroll` 和 `scroll_to_item` 源码。等高 `h_12` 行由当前 rem 每帧测量，文字在当前宽度截断，无应用层换行高度缓存；完整路径可通过 Kit Copy 图标按钮的 tooltip 和剪贴板读取。行内继续显示 bytes/Large Target 或本地化 Skip reason。
- Target ID 来自路径；Skip ID 来自路径、原因及同类出现序号（不使用全列表位置），处理多条 Rule 产生相同 Skip 的情况。Skip keys 只在后台计划完成后构造一次。保留一个滚动 owner/handle，每次新审阅重置；只读计划可获焦点、Home/End 跳转，滚轮走 GPUI 原生虚拟列表。Copy 有本地化 accessible name，点击不窃走按钮焦点；Escape 沿用 Kit Dialog 关闭/恢复，Enter 不授权清理。

### TDD 与生产 UI regression 证据

- 扩展 `gpui_settings::tests::activation_defers_cache_refresh_until_cancel_and_remeasures_recreated_cache`：独立子进程隔离 HOME/USERPROFILE/LOCALAPPDATA/PATH，真实 catalogue 的 Gradle distributions 起初 Missing、bun cache 用临时目录 000 权限得到 Unreadable；修复文件系统后真实双窗口切回触发 30 秒刷新，分别显示 **24 B / 48 B** 并恢复安全默认选择。既有场景继续验证 Cleaned、Go、Ready npm 的旧 **128 B** 测量与显式选择、清理锁定期间排队、取消后释放，以及未到期切回不查。红日志 `/tmp/devsweep-p2-cache-red.log` 在旧刷新实现下实际仍为 `Cache directory does not exist`，预期 `24 B`；绿日志 `/tmp/devsweep-p2-cache-green.log` 退出 0，包含两种失败恢复断言。Unix 权限 fixture 不是 Windows 权限验收。
- 新增 `gpui_projects::tests::large_confirmation_bounds_target_and_skip_rendering_and_preserves_selection`：通过生产项目行的 Space/Down 选中 **120** 个临时 Node 项目；测量后移除其中 **60** 个 node_modules，真实 Clean 按钮产生 **60 Target + 60 Skip**。旧 eager 实现在 `distant Targets must not be built` 断言失败，日志 `/tmp/devsweep-p2-plan-red.log`；虚拟化后通过，日志 `/tmp/devsweep-p2-plan-green.log`。
- 同一 UI regression 的扩展版（`/tmp/devsweep-p2-plan-regression.log`，及最终套件）：逐个查询真实行 ID，断言当前构建行 **1..16（不含 16）**、远端 Target/Skip 不存在；首行 **16 B** 内容与完整路径 Copy，End 到末尾 **Skip: Missing**，只读 Enter 不执行；缩至 **880×560** 并将字体设为 **20 px** 后仍少于 16 行，Home 首行可见且高度 **60 px**，真实滚轮使首行离开已构建范围；Escape 恢复项目列表焦点、**120 projects selected** 保留；重新审阅从首项开始，显式 Direct/Execute 只删除 tempfile 中 60 个剩余 Target，报告 **960 B、Skip: 60、Failure: 0**。没有以私有 renderer/helper 调用或模拟列表替代生产交互。

### 最终检查

Cargo 均复用 `CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target`。磁盘从约 54 GiB 到约 53 GiB 可用，未出现磁盘耗尽。

- `cargo test --locked --features gpui-tests -- --test-threads=1`：**238 passed / 0 failed / 4 ignored**，含 **30 个 GPUI UI integration tests**；日志 `/tmp/devsweep-p2-final-tests.log`。
- `cargo test --locked --features gpui-tests gpui_ -- --test-threads=1`：**30 passed / 0 failed**；日志 `/tmp/devsweep-p2-final-gpui.log`。
- `cargo clippy --locked --features gpui-tests --lib --bins -- -D warnings`：退出 0；日志 `/tmp/devsweep-p2-final-clippy.log`。
- `cargo clippy --locked --all-targets --features gpui-tests`：退出 0，既有测试 lint 仍在，不称严格 all-targets 通过；日志 `/tmp/devsweep-p2-all-targets-clippy.log`。
- `cargo build --locked --features gpui --bins`：macOS arm64 退出 0；日志 `/tmp/devsweep-p2-final-build.log`。既有 `block 0.1.6` future-incompat 警告仍在。
- `cargo fmt --check`、`git diff --check`：通过。提交遵循 git-commit skill，最终 SHA 为包含本节的提交。

上述仅解决两项本地 P2 实现及自动化覆盖问题。**Windows 编译/原生流程、macOS 原生大计划滚动/响应性及权限恢复、真实 Go/Unity、系统 Trash 成功/恢复，以及既有平台/设计接受条件均继续未通过**；没有新原生操作或像素证据，4 个 ignored 不计成功。08 保持 `ready-for-human`，未勾选的平台/设计验收框保持不变，保留 egui，不开始 09。


## 2026-11-05 — P2 最终集成复核与清理清单

- 集成基线 `86d8a9c7501bbabb853861ea831369e335b97a10` 是 `ace29721c33a5193cf02fcd6cb5e4cc1572fd696` 的祖先；`integration/gpui-ui` 通过 `git merge --ff-only ace29721c33a5193cf02fcd6cb5e4cc1572fd696` 无冲突集成，无额外实现修改。本节所在后续文档提交为最终集成 HEAD。
- Code-review findings 状态：**fixed**。P2 固定缓存 Failed 不恢复已由刷新重新后台测量修复；P2 确认计划 eager 渲染已由 Target/Skip 共用虚拟列表修复。两项对应生产 GPUI 回归在集成 checkout 复跑均通过；此状态仅指这两项实现问题，不代表平台接受验证通过。
- `cargo fmt --check`、`git diff --check` 退出 0。复用共享 target，`cargo test --locked --features gpui-tests activation_defers_cache_refresh_until_cancel_and_remeasures_recreated_cache -- --test-threads=1` 和 `cargo test --locked --features gpui-tests large_confirmation_bounds_target_and_skip_rendering_and_preserves_selection -- --test-threads=1` 各 1 passed / 0 failed；日志 `/tmp/gpui-p2-integration-cache.log`、`/tmp/gpui-p2-integration-plan.log`。本轮不重复全量；238 passed / 4 ignored、30 个 GPUI 测试、Clippy 和 macOS bins 构建均为上一节源分支证据。既有 block 0.1.6 future-incompat 警告仍在。
- 剩余精确 gates：Windows 编译及项目/全局缓存完整原生流程与回收站恢复、占用/权限/长路径；macOS 原生 activation 与缓存/Go 联动、大计划滚动/响应性、Direct 成功/权限恢复、隔离全局执行/中止/失败恢复、大体积缓存确认；系统 Trash 成功/恢复及只读 Go module 恢复、真实 Go/Unity；真实长任务/中止边界；完整主题/语言/最小窗口/缩放/键盘/焦点/剪贴板/OS 外观/IME/reduced motion 设计矩阵。4 个 ignored 不计通过。08 保持 ready-for-human、不 resolved；保留 egui/默认入口，不开始 09。
- 清理候选已逐个核实：以下 10 个任务工作树均 clean，其 HEAD 及全部祖先均可达集成 HEAD，关联分支可交由父代理使用 orchestrator lifecycle 清理；本轮未删除。公共前缀 `/Users/yoyun/.t3/worktrees/DevSweep/`：

| 工作树目录 | 分支 |
| --- | --- |
| t3-0d233a82 | t3/gpui-cache-clean |
| t3-19c698cc | t3/gpui-project-discovery |
| t3-1b2df80c | t3/gpui-cache-retry-virtualized-plan |
| t3-2bcbdc36 | t3/global-cache-selection |
| t3-59eb2508 | t3/gpui-activation-stale-refresh |
| t3-b87ccada | t3/project-measurement |
| t3-bc53e485 | t3/gpui-project-selection |
| t3-d823d4c4 | t3/gpui-settings-ui |
| t3-e44152ae | t3/gpui-cross-platform-verification |
| t3-f0dafb3e | t3/implement-project-clean |

- 上表为完整任务清单；集成 checkout 因原有 `skills-lock.json` 修改和未跟踪 `.claude/` 并非 clean，不是清理候选。`main` 虽已可达，也不是本任务清理对象。清理前父代理应再次核实 clean/可达状态，防止后续并发改动。
- 无 children、stash/reset/rewrite、push/PR。仅按 git-commit skill 提交本集成证据，原有 skills-lock.json 与 .claude/ 不纳入提交。
