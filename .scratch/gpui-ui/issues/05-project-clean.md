# 05: 项目清理完整流程

**What to build:** 将跨项目选择转为可审阅的清理计划，经确认后执行，持续反馈进度、中止与结果，并能查看复制失败详情。

**Blocked by:** 04 — 项目清理项选择与批量汇总

**Status:** ready-for-agent

- [x] 确认界面展示 Delete plan、删除方式、已选体积及 Skip 原因；Trash 默认，Direct 明确不可恢复后果。
- [x] 确认锁定草稿，执行前重新生成计划并应用既有安全检查；鼠标及快捷键均不能绕过确认。
- [x] 后台执行保持界面响应，防止重复提交，保持既有刷新、测量排队及解锁语义。
- [x] 明确区分请求中止与已经停止，保留以项目为单位的中止边界；断连和完成结果正确结束状态。
- [x] 结果区分 Skip 与 Failure，Freed bytes 按既有估算语义呈现，失败详情可查看、复制并继续后续操作。
- [x] 保留业务回归测试，使用 GPUI UI integration tests 覆盖确认、取消、执行、中止、失败和焦点恢复。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。

## Comments

### 2026-11-05 — 集成就绪

- 04 已以 `57a82399c8f33f1deb8c51e6915594efc5d74f39` fast-forward 集成至 `integration/gpui-ui`，合并后六个 GPUI 项目测试、fmt 与 diff 检查通过，实施依赖已满足，可开始本票。
- 接入点为 `ProjectDiscovery::request_clean` 发出的 `CleanRequested` 意图；当前仅提示等待确认，未执行删除。实现时应从现有选择草稿重新生成计划，完成确认锁定、执行前重检及后台执行生命周期，不得把底栏汇总当成删除授权。
- 本票功能及原生平台验收尚未执行，所有验收项保持未勾选；04 的原生布局和双平台流程仍待 08 验收。

### 2026-11-05 — 实施与自动化证据（原生平台验收待 08）

- 准备工作树 `t3/f0dafb3e` 起始无修改，HEAD 正是要求的 `integration/gpui-ui` 基线 `b140f7e494018eb1ac3822dd2ea28d76d990b152`。未创建 checkout 或子代理。已读本票、spec、04/05 集成备注、AGENTS、GLOSSARY、ADR-0001/0002/0003，批准的 TDD 技能及 host GPUI Kit / Design Guides 技能、Design/Coding Guides、组件约定、应用配方、测试/异步/焦点/键盘参考。CodeGraph 优先执行，但报告此工作树无可用索引，随后使用源码检索，未创建索引。
- Cargo.toml/lock 仍固定 Kit 0.7.0。对照安装版本源码核实 Dialog 的 `on_close` / `on_ok`、`footer`、`overlay_closable`，WindowExt 的对话框关闭及焦点恢复、Radio `on_change`、Button 键盘/焦点行为与 test-support 的真实输入/剪贴板 API，没有改变依赖。Kit 鼠标 Button 不抢焦点；取消恢复先前列表焦点，键盘打开/关闭则恢复触发按钮焦点。
- `CleanRequested` 现由保留的 GPUI subscription 接入：捕获 ProjectRefresh 中跨项目草稿，在后台重新生成 Delete plan；汇总不是删除授权。ProjectClean 新增 batch 入口，保留每个 Project 的 Stack 和明确勾选，不复制路径/规则/Unity 安全检查。预览锁定快照，只有确认对话框中的 Clean 才调用原 `ProjectClean::execute`；执行只重检预览接受的路径，仍由原 delete::execute 保留项目边界及最终 Unity 检查。预览和重检产生的 Skip 保留在最终结果中。
- 确认对话框显示安全计划路径、体积、大体积标记、Skip 原因及删除方式；默认 Trash，尊重已保存的默认删除方式。Direct 明确显示永久不可恢复后果。Cancel/Escape 释放草稿，迟到的预览结果由 epoch 拒绝。按钮固定在 Dialog footer，计划列表按当前窗口高度限制并独立滚动。新增 Command/Control+Shift+Backspace 清理意图及入口提示；快捷键、重复请求和未聚焦执行按钮时的 Enter 均不能绕过确认。
- GPUI 后台调度原 ProjectExecution，前台定时消费真实事件并用 poll_projects 回写成功删除的 Target。通过原 Refresh::coordinate 及新增 is_clean_locked 读取接口共享确认/执行锁语义，已有 GPUI Refresh/measurement 队列继续在释放锁、测量结束后推进。扫描根和草稿在确认/执行期间不可改，重复提交被阻止；刷新请求排队。中止只设置原 abort flag，显示“已请求中止、等待当前项目完成”，收到 Finished 才显示“已停止”；断连显示未正常完成并解锁，保留已确认删除的估算。视图销毁会请求中止，活动 Project 仍遵循原项目边界。
- 结果明确区分 Skip 与 Failure，Freed bytes 明示估算并使用原报告/计划时体积，不重新测量成功删除项；失败详情可查看和复制，关闭恢复焦点，后续刷新及再次清理可继续。最终项目计数取 Finished 报告，覆盖未收到 Started 就中止的 `0/1` 情形。没有接入任何全局缓存删除，egui、缓存选择及配置文件格式保持兼容。
- TDD 红→绿：`batch_confirmation_rechecks_only_reviewed_targets_and_unlocks_after_completion` 首次因缺少 batch API 编译失败（`/tmp/t05-red-batch.log`），随后通过（`/tmp/t05-green-batch.log`）；真实 GPUI `clean_confirmation_cancel_restores_focus_and_execute_rechecks` 首次因缺少 clean-mode-trash 失败（`/tmp/t05-red-ui.log`），随后经真实 Radio/按钮/Space/Escape 通过；中止最终计数断言首次发现 `0/0`（`/tmp/t05-red-stopped.log`），改用最终报告后通过（`/tmp/t05-green-stopped.log`）。测试边界沿用批准的公开业务接口与真实生产 GPUI 组件交互。
- 新增交互覆盖：真实临时两项目 Direct 删除，其中一个只读目录产生真实权限 Failure，核对成功项 1024 B 估算、Skip/Failure 区分、详情及剪贴板、焦点恢复、刷新后重试；此用例仅在 Unix 编译，不宣称 Windows 权限行为通过。中止用例在既有公开 ProjectExecution::new/DeleteEvent 工作线程边界安装可控事件通道，随后只通过真实 UI 请求中止、尝试改草稿、重复清理和刷新，验证锁持续至断连/Finished、排队刷新及 Stopped 显示。该边界用例不冒充真实长目录原生响应测试。全部删除测试仅操作 tempfile 目录，没有清理用户项目或全局缓存。
- 最终 `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：227 passed、4 ignored、0 failed，包含 21 个真实 GPUI UI integration tests，及 egui、配置、Refresh、扫描、测量、计划、删除、安全回归。日志 `/tmp/t05-final-full.log`。4 个 ignored 为既有系统 Trash、本地 Go 条件及大型目录性能探针，不视为通过。确认 footer 调整后 21 个 GPUI 测试也单独通过（`/tmp/t05-final-ui.log`）；最终完整回归包含之后的中止报告计数断言。
- `cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过；Clippy 日志 `/tmp/t05-final-clippy.log`。`cargo build --bin devsweep-gpui --features gpui --locked` 在 macOS arm64 成功（`/tmp/t05-build.log`）；这只是构建证据，不是原生流程验收。既有 `block 0.1.6` future-incompat warning 仍存在。
- 完成前执行 `git merge integration/gpui-ui -m 'chore(gpui): 同步项目清理集成基线'`，返回 Already up to date；本地 integration tip 仍为 `b140f7e494018eb1ac3822dd2ea28d76d990b152`。不 push、不创建 PR，由父 merger 集成。

待原生验收（未执行，不视为通过）：

- [ ] macOS 原生窗口人工验证主题、中文字体、大字体/窗口缩放、长计划滚动、键盘焦点环、系统 Trash、真实长目录进度和中止边界。
- [ ] Windows 构建、原生窗口、回收站/直接删除、占用失败、剪贴板和同样完整流程。
- [ ] 双平台真实 Unity Editor 保护及大目录完整流程；本票自动化不替代 08 的平台验收。
