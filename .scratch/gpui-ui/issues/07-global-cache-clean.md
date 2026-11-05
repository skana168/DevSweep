# 07: 全局缓存清理完整流程

**What to build:** 将所选全局缓存接入计划确认、后台清理、中止及结果反馈，沿用项目清理中已验证的呈现模式。

**Blocked by:** 05 — 项目清理完整流程；06 — 全局缓存发现、测量与选择

**Status:** ready-for-agent

- [x] 确认显示所选缓存、删除方式、范围、体积及风险，不误称为项目 Target 或项目 Rule。
- [x] 复用既有全局缓存安全检查，执行只能作用于固定缓存清单所解析的路径。
- [x] 后台执行、防重复提交、中止、完成和失败反馈遵循既有缓存业务语义，不套用不适合缓存的项目执行边界。
- [x] 失败详情可查看和复制，关闭确认与结果浮层正确恢复焦点。
- [x] 业务测试及 GPUI UI integration tests 覆盖确认取消、执行、中止、路径拒绝与失败恢复。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。

## Comments

### 2026-11-05 — 05 集成后的复用边界

- 项目清理实施提交 `654f3a5c8a017147dc460a77581f8f6733ac5ab6` 已 fast-forward 集成；07 可复用计划确认、删除方式、后台进度、中止请求/完成区分、Skip/Failure 结果、失败详情复制及关闭恢复焦点的呈现模式。
- 复用不包含项目执行边界：缓存必须继续通过既有固定缓存清单解析路径及缓存专属安全检查，沿用各缓存（包括 Go 缓存）的执行、中止及体积报告语义，不将缓存称为项目 Target/Rule，也不套用按 Project 停止的边界。
- 本记录不表示 07 已完成，也不替代 06 依赖及 macOS/Windows 原生验收；本票验收项保持未勾选。


### 2026-11-05 — 实施与自动化证据（原生平台验收待 08）

- 准备工作树 `t3/0d233a82` 起始干净，HEAD 正是指定 `integration/gpui-ui` 基线 `2862e55dc30dadd25f779c78c2a7d15e89b97375`。未创建 checkout、子代理、PR 或 push。已读 spec、05/06/07 全部备注、AGENTS、GLOSSARY、ADR-0001/0002/0003、issue tracker/domain 约定，以及指定 TDD 和 host GPUI Kit / Design Guides、Coding Guides、组件约定/配方、测试/异步/焦点参考。CodeGraph 首先调用但报告工作树没有可用索引，随后使用源码检索，未创建索引。
- Kit 依赖仍锁定 0.7.0，未修改 Cargo.toml/Cargo.lock。核实该版本 Dialog `on_ok`、`on_close`、`footer`、`overlay_closable`、WindowExt `close_dialog`、Radio `on_change`、Button disabled/键盘及 test-support 输入/剪贴板 API。
- `GlobalCacheView` 内独立保留缓存确认/执行生命周期，呈现沿用项目清理的 Kit Dialog、Radio、固定 footer、真实结果、详情复制及 Root 焦点恢复模式，但不复用 ProjectClean 或项目中止边界。确认展示全局缓存范围、固定 ID 对应路径、体积、大体积提示、重新下载/危险代价、删除方式和 Skip；Trash 为未配置默认值，尊重已有 Config 删除方式，Direct 明确不可恢复。只有确认中的 Clean 才授权执行；主壳 Command/Control+Shift+Backspace 同样进入缓存确认。
- 普通缓存在确认和执行前均调用原 `cache::plan(id, bytes)`，并要求解析路径与测量/确认路径相同，视图路径从不成为任意删除计划。Go 继续调用原 snapshot/context 计划及 `GoCacheClean::execute` 重查询，拒绝新配置路径继承旧授权，保留 `execute_go_module` 的只读依赖处理；重检原因本地化显示，回写新快照，避免重复计算同一 Failure。没有改变 cache/go_cache/delete 业务 API、安全规则、配置格式或 egui。
- 执行使用真实独立工作线程、原删除函数及共享 abort flag，UI 消费缓存条目实际完成结果。逐个执行缓存，已开始的单个缓存操作继续遵循既有业务中止语义；请求中止和 Finished 后的已停止明确区分，没有项目专用中止文案。确认/执行锁定草稿并防重复提交；主壳阻止并发项目清理，语言/主题/默认删除方式仍可通过设置更新。扫描根/项目集合改变时拒绝旧确认或请求中止，保留结果，显式重开缓存页后用当前保护上下文测量。视图销毁请求中止；断连释放锁并保留已收到的结果。成功条目显示已清理、0 B 且不可再次选择，Freed bytes 使用原报告的计划时体积估算，不重新测量成功项。
- TDD 红→绿实证：取消流程首次缺少 `cache-clean` 失败（`/tmp/t07-red-cancel.log`），实现后通过；路径拒绝先缺少反馈元素失败（`/tmp/t07-red-rejection.log`）；Go 中止曾错误显示 1/1，断言 0/1 失败（`/tmp/t07-red-abort.log`），改用原报告的完成计数后通过；Go 路径变更曾重复累计 Failure，新增断言失败（`/tmp/t07-red-go-changed.log`），单条具体原因及新测量回写后通过（`/tmp/t07-green-go-changed.log`）；成功项曾显示“缓存目录不存在”，已清理状态断言失败（`/tmp/t07-red-cleaned.log`），修正后通过；静态路径确认后被文件替换曾误计 Failure，Skip 断言失败（`/tmp/t07-red-static-recheck.log`），改为计划拒绝后通过。以上早期日志包含当时的单缓存版本，最终用例扩展为缓存批次。
- 新增 3 个生产 GPUI UI integration tests：`cache_confirmation_cancel_keeps_selection_and_restores_focus` 通过真实 Tab/Space/取消/Escape 验证焦点、选择保留与任意视图路径拒绝；`fixed_cache_execution_failure_copy_and_retry` 验证真实 Direct 删除、只读父目录产生权限 Failure、1024 B 估算、详情/剪贴板/精确焦点恢复、修复后重试及确认后固定路径变为文件时 Skip 且文件保留；`go_cache_abort_waits_for_real_recheck_and_blocks_duplicate_submit` 通过临时外部 Go 命令验证真实重查询期间中止、草稿锁和重复提交拒绝、0/2 停止、变更路径拒绝/新测量回写，以及同一批次只读 Go module 使用专属执行成功。扩展原主壳导航测试，验证中文/深色/已保存 Direct 设置同步及缓存快捷键只进入确认。Unix 权限/脚本用例不宣称 Windows 行为已通过。
- 所有删除测试仅操作 tempfile 隔离夹具。固定路径测试通过独立测试子进程设置临时 HOME/USERPROFILE/LOCALAPPDATA，并只装载所需固定条目；Go 测试临时 HOME/PATH 和外部命令只返回夹具路径，不调用用户 Go 或删除用户缓存/项目。测试边界沿用批准的既有公开业务/config 接口及真实生产 GPUI 控件事件，没有新增私有 handler 单元测试或内部 mock。
- 最终 `CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：**230 passed、4 ignored、0 failed**，包含 **24 个 GPUI UI integration tests**，覆盖 egui、配置、扫描/测量、固定清单路径、Go 保护/只读 module、计划/删除、项目锁/排队/生命周期回归。完整日志 `/tmp/t07-accepted-full.log`。4 个 ignored 为已有大目录性能探针、本地 Go 配置测试、macOS Trash 恢复、只读 module Trash 恢复，不视为通过。Go 批次额外独立运行也通过（`/tmp/t07-go-batch.log`）。
- 最后收紧主壳保护上下文失效后的结果保留：只在显式重开缓存页时重建，不由后续项目通知立即覆盖完成结果。此后 `cargo test --lib --features gpui-tests --locked gpui_ -- --test-threads=1` 再次 24 passed、0 failed（同一 CARGO_TARGET_DIR，`/tmp/t07-last-ui.log`），Clippy 再次通过（`/tmp/t07-last-clippy.log`）。
- `CARGO_TARGET_DIR=/Users/yoyun/DevSweep/target cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings` 通过（`/tmp/t07-accepted-clippy.log`）；`cargo fmt --all -- --check`、`git diff --check` 通过。`cargo build --bin devsweep-gpui --features gpui --locked` 在 macOS arm64 成功（同一共享 CARGO_TARGET_DIR，`/tmp/t07-build.log`）。本机仅安装 aarch64-apple-darwin target；依赖 block 0.1.6 的既有 future-incompat warning 仍存在。
- 完成前执行 `git merge integration/gpui-ui -m 'chore(gpui): 同步最新集成分支'`，返回 Already up to date；integration tip 仍为 `2862e55dc30dadd25f779c78c2a7d15e89b97375`。不 push、不创建 PR，由父 merger 集成。
- 顶层勾选记录实现和自动化覆盖，不代表原生视觉/系统行为通过。Status 保持原样，由父 merger 集成；平台验收继续由 08 收集。

尚未执行，不视为通过：

- [ ] macOS 原生窗口人工验证中英文、浅/深色、大字体/窗口缩放、长确认列表滚动、键盘焦点环及系统剪贴板。
- [ ] macOS 系统 Trash、真实长目录进度/中止及完整缓存清理原生流程。
- [ ] Windows 编译/运行、回收站/Direct、占用/权限失败、焦点/剪贴板及同样完整流程。
- [ ] 双平台真实 Go 安装、环境配置变化及只读 module 系统 Trash 恢复的原生接受验证。
