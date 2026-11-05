# 03: 项目测量、筛选与列表导航

**What to build:** 查看后台测量的项目大小，按项目类型筛选并用键盘导航；刷新保持选择，长列表保持流畅。

**Blocked by:** 02 — 扫描根管理与项目发现

**Status:** ready-for-agent

- [x] 展示 Project size 与测量状态，未知体积不显示为零，不将项目大小称为可释放体积。
- [x] 项目类型筛选位于项目视图，结果与既有 Stack 语义一致。
- [ ] 方向键导航与可见焦点有效，项目身份稳定，刷新保留已勾选项且不重算已有体积。
- [ ] 使用适当的虚拟化列表或表格，窗口缩放、长路径与大结果集不会破坏滚动和导航。
- [x] 验证异步测量、刷新期间的状态保持、筛选及键盘导航，过期结果不会污染当前状态。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。


## Comments

### 2026-11-05 — 实施与自动化证据（原生平台验收待 08）

- 起始工作树 `t3/project-measurement` 干净，HEAD 与 `integration/gpui-ui` 均为 `f2366c2b0267d001c75a66de5653442e65263e19`。完成前执行 `git merge integration/gpui-ui -m 'chore(gpui): 同步最新集成分支'`，返回 Already up to date，tip 仍为该 SHA。没有创建 checkout、子代理、PR 或 push；仅修改本票、`src/gpui_projects.rs` 与 `src/projects.rs`。
- 已读规格、01/02/06 实施及集成评论、AGENTS、GLOSSARY、ADR-0001/0002/0003、TDD 及 host 上 GPUI Kit / Design Guides 技能和 Coding/Design、组件约定、应用配方、异步/焦点/键盘/测试指南。CodeGraph 优先尝试后报告当前工作树无可用索引，随后使用源码检索，未创建索引。按 Cargo.lock 的 Kit 0.7.0 与其 facade 依赖源码核实 Checkbox、受控选择、Context::processor、UniformListScrollHandle、ScrollStrategy、焦点、主题及测试 API；没有更改依赖。
- `ProjectDiscovery` 持有既有公开 `ProjectRefresh` 业务模型、项目勾选草稿、当前路径、Stack 筛选、列表焦点/滚动和后台任务。新增 `ProjectRefresh::from_discovered` 仅将已完成的发现结果交给既有 reconcile/Rule expansion，不再次扫描；发现、读取 rules.toml 和展开 Targets 全部在后台执行。测量复用 `size::dir_size` / `total_size` / `CONCURRENCY`（最多四个后台请求），逐 Project 合并真实结果；进度计数来自实际完成请求，不伪造百分比。遵循 rules.toml 覆盖与内置规则回退，读取/解析错误进入已有失败详情及复制路径。
- Project size 包括所有 Targets（含未勾选危险项），未知明确显示 Unknown / 未知和测量状态；已知零显示 0 B，小体积显示精确 B，不称为可释放体积。沿用既有 best-effort 体积估算语义：遍历错误跳过，不增加新的删除权限或改变链接行为。
- Refresh 在发现/测量期间排队；保留现有结果和勾选，随后发现新 Project，只测量未知 Targets。通过既有 apply_refresh 保留同路径、同 Stack 的 Target 体积与 Target 草稿；项目勾选也按路径保留，Stack 改变或项目消失时丢弃旧身份草稿。发现 revision 和测量 epoch 隔离过期任务；移除根立即丢弃其项目及草稿、取消旧测量所有权，即使同路径重新添加也不接受旧结果。新增根期间若项目身份集合发生变化，也使旧测量失效。
- 项目视图有全部类型及九个 Stack 筛选，直接使用已发现 Project 的唯一 Stack，不重新分类；筛选不丢失隐藏项目的勾选。真实 Kit Checkbox 支持鼠标与键盘，点击后焦点进入列表；方向键、Home/End、Space 在完整筛选结果坐标中操作，并用 UniformListScrollHandle 将当前项目滚入视口。勾选背景、悬停及主题焦点环分别呈现。路径为稳定行身份，不使用可变行号或翻译作为业务身份。
- uniform_list 仅构造可见行；筛选索引只在发现或筛选变化时重建，不在每帧克隆全部 Target 数据。行与列表约束为可用宽度，名称和长路径截断；Kit Copy 图标按钮可通过键盘取得完整 Unicode 路径。固定行高使用 rem，缩放与窗口变化由框架重新布局；项目计数与勾选范围可见。未实现 04 的清理项面板/已选体积或 05 的清理入口；egui、设置与缓存导航集成未修改。
- TDD 红→绿：① 真实测量/Refresh 流程先在缺少 project-size 失败，再接入业务模型和后台测量；② Stack/导航流程先在缺少 filter-Rust 失败，再实现筛选与勾选；③ 真实 executor 逐步推进至未知态，先在缺少 project-measurement 失败，再加入真实进度；④ 160 个长名称项目、20px 基础字体、880×560 的测试发现行宽扩张至 1815px，使点击与 End 导航失败，约束行宽并截断后通过；⑤ 首次鼠标点击 Checkbox 后方向键测试先以 1 selected（期望 2）失败，再补列表焦点转移后通过。测试只使用批准的公开业务/配置与生产 GPUI 组件交互 seam，不直接调用 handler，不注入私有测量结果。
- 四个新增 UI integration tests：`project_size_is_measured_in_background_and_refresh_keeps_known_size` 验证真实 4096 B 测量、文件长至 8192 B 后 Refresh 仍保留 4096 B、新增排序靠前项目只测得 2048 B、原项目勾选保留且新项目不误勾；`stack_filter_and_keyboard_selection_survive_refresh` 验证真实 Checkbox、上下/Home/End/Space、Rust/全部筛选及隐藏选择保持；`pending_measurement_is_unknown_and_removed_root_cannot_restore_old_results` 用真实 executor 在发现后、测量前验证未知与 0/1、排队 Refresh 不清草稿、移除并重新添加同一路径后显示新 8192 B 且旧草稿不复活；`virtual_project_list_keeps_end_navigation_and_long_paths_at_large_text` 验证 160 项远端行未挂载、End/Space、完整长中文路径复制、880×560→1200×760、Home/Space 和勾选范围。
- 最终 `cargo test --all-targets --features gpui-tests --locked --no-fail-fast -- --test-threads=1`：220 passed、4 ignored、0 failed，含 15 个 GPUI 真实组件测试以及既有 egui、扫描、Refresh/测量、规则、计划、清理和安全回归。日志 `/tmp/t03-final-full.log`。四个 ignored 是既有系统 Trash / 本地 Go 条件，不声称通过。
- 最终 `cargo clippy --lib --bins --features gpui-tests --locked -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过。`cargo build --bin devsweep-gpui --features gpui --locked` 在 macOS aarch64 成功。日志 `/tmp/t03-final-clippy.log`、`/tmp/t03-final-build.log`。依赖 block 0.1.6 的既有 future-incompat warning 仍存在。
- 第三、四项保持未勾选：自动化已证明导航/保持/虚拟化与上述几何约束，但没有实际操作 macOS 原生窗口验证焦点环、长路径字体、系统缩放和大目录下响应，也没有 Windows 构建/原生运行证据。无头交互与 macOS 构建不等于双平台原生验收；继续由 08 收集，不标记 resolved。测量也仍沿用原 API 的 best-effort 估算限制。
