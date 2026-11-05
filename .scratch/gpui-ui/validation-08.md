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
