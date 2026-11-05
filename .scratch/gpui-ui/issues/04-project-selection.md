# 04: 项目清理项选择与批量汇总

**What to build:** 在右侧面板查看项目清理项及风险，跨项目勾选，并在底部看到此次清理范围和已选体积。

**Blocked by:** 03 — 项目测量、筛选与列表导航

**Status:** ready-for-agent

- [ ] 选中项目打开可调整宽度的右侧清理面板，主列表上下文保留；区域有合理最小宽度且缩放后可用。
- [x] 展示 Target、体积、危险标记和必要说明；Dangerous 默认不勾选，只能由用户主动选择。
- [x] 支持鼠标和空格勾选、跨项目选择，按稳定身份保持选择；切换当前项目不会丢失选择草稿。
- [x] 底部固定展示选择范围、Chosen size 和清理入口，汇总使用通过安全检查的已选项，与 Project size 明确区分。
- [x] 测试面板切换、危险项选择、跨项目汇总、键盘操作和清理入口的可用状态。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。

## Comments

### 2026-11-05 — 实施与自动化证据（原生平台验收待 08）

- 起始工作树 `t3/gpui-project-selection` 干净，HEAD 与 `integration/gpui-ui` 均为要求的 `59fe4032f5c03a360df5349b2644e22e4c900988`。已读规格、03 实施及集成证据、AGENTS、GLOSSARY、ADR-0001/0002/0003，以及批准的 TDD、host GPUI Kit / Design Guides 技能、完整 Design/Coding Guides、组件约定、应用配方和测试/异步/焦点/键盘指南。CodeGraph 优先运行后报告该工作树无可用索引，随后用源码检索，没有创建索引。
- 核实 Cargo.toml / lock 固定 Kit 0.7.0，以及已安装 facade/base 源码的 Checkbox `on_change`、Tab/Space、ResizableState、h_resizable、resizable_panel、size_range 和真实测试 drag API，没有变更依赖。Kit 的 ResizablePanelGroup 不实现 Styled，布局由外层 flex 区域约束；面板范围接口接受 Pixels，因此用当前 `window.rem_size()` 解析最小/首选宽度（列表 14rem，面板最小 16rem、首选 21rem），不是固定像素产品布局。
- 项目列表和右侧面板共享保留的 Kit `Entity<ResizableState>`，切换项目及重绘不重建分隔状态。列表仍使用原 uniform_list、方向键与滚动句柄。扫描根和筛选区域限制至窗口一半高度并独立滚动，避免较小窗口/大字体挤掉工作区；错误详情与复制按钮保持固定可见。Target 面板独立滚动，显示名称、完整路径 tooltip、真实体积或 Unknown/未知、Dangerous 文字及删除代价说明，并沿用 Unity/.NET 现有注意文案。
- 继续以 `ProjectRefresh::targets_mut` 保存 Target.checked 草稿，不引入第二套选择模型；Target 修改按项目路径和 Target 路径定位，刷新仍沿用 apply_refresh。新发现的危险项强制未选（即使规则覆盖改变默认），显式选择会在刷新和项目切换后保留。真实 Kit Checkbox 负责鼠标、Tab 和 Space；主动勾选 Target 也把所属 Project 纳入此次范围。取消项目范围保留其 Target 草稿。
- 固定底栏显示项目勾选范围、安全检查后 Project/Target 数、Skip 数、Chosen size 与 Clean… 入口。后台复用 `plan::plan` 及当前规则快照，统一处理 Unity/路径/规则保护和重叠去重，没有在 UI 复制安全规则。选择、刷新、根移除与测量更新使汇总失效并重新检查；epoch 拒绝旧汇总结果。安全检查中显示 Checking…/检查中…，被接受的 Target 有未知体积时显示 Unknown/未知，已知零才是 0 B；Project size 仍是原有全部 Target 体积，不代替 Chosen size。
- `request_clean` 是 05 的明确接入点，仅在存在通过检查的 Target 时发出 `CleanRequested` 意图，并明确显示“尚未删除任何内容”的等待计划确认状态；它不调用执行/删除 API，也不把汇总视为执行授权。05 必须重新构建计划、展示删除方式和确认，并执行前重检。egui、设置/缓存导航和既有扫描/刷新/测量业务接口未修改。
- TDD 红→绿：① `target_panel_retains_dangerous_draft_across_projects` 首次在缺少 target-node.dist 失败（`/tmp/t04-red-panel.log`），随后真实面板验证危险默认、鼠标/Tab/Space、跨项目草稿；追加真实拖动分隔线验证宽度改变且切换项目后保持。② `chosen_size_tracks_safe_cross_project_selection_and_clean_request` 首次在缺少 clean-projects 失败（`/tmp/t04-red-summary.log`），随后通过真实复选框验证两项目三 Target 合计 4096 B、Refresh 保持、已消失危险 Target 被现有计划拒绝后合计 2048 B/1 Skip；清理请求后其余磁盘产物仍存在。入口状态验证使用真实点击是否产生等待确认反馈，未把组件未公开的 disabled accessibility 标志当作证据。既有 pending_measurement 测试追加“检查中”时入口不响应断言。
- 回归发现并修复：880×560、20px 字体下工作区过小导致 End 导航失败，限制顶部控件滚动区域后原测试通过；首次完整回归发现复制错误按钮被滚动区域裁剪，将错误区域移回固定可见位置后通过。没有放宽原测试断言。最终六个项目 UI integration tests 全通过，覆盖原有大结果集虚拟化、中文长路径、方向键与窗口缩放。
- 最终 `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：222 passed、4 ignored、0 failed，含 17 个真实 GPUI 组件测试及既有 egui、配置、发现、Refresh、规则、计划、删除与安全回归。日志 `/tmp/t04-final-full.log`。四个 ignored 是既有系统 Trash / 本地 Go 条件，不声称通过。
- `cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过。Clippy 日志 `/tmp/t04-clippy.log`。`cargo build --bin devsweep-gpui --features gpui --locked` 在 macOS aarch64 成功，日志 `/tmp/t04-build.log`。既有 `block 0.1.6` future-incompat warning 仍存在。
- 完成前执行 `git merge integration/gpui-ui -m 'chore(gpui): 同步最新集成分支'`，返回 Already up to date，最新本地 integration tip 仍为 `59fe4032f5c03a360df5349b2644e22e4c900988`。未创建 checkout、子代理、PR 或 push；仅修改本票与 `src/gpui_projects.rs`，交由父 merger 集成。

待验收（未执行，不视为通过）：

- [ ] macOS 原生窗口人工验证面板拖动、缩放、主题/中文字体、键盘焦点环和滚动可用性。
- [ ] Windows 构建及原生窗口执行相同流程。
- [ ] 双平台大目录下真实响应与完整选择流程验收（08）；首项产品布局验收保持未勾选，自动化几何与交互证据不替代原生验收。
