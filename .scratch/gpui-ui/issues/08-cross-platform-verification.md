# 08: 双平台完整流程验收与交互补齐

**What to build:** 在 macOS 与 Windows 完成真实窗口中的核心工作流，补齐跨流程交互问题并记录可核查的验收证据。

**Blocked by:** 05 — 项目清理完整流程；07 — 全局缓存清理完整流程

**Status:** ready-for-human

- [x] 现有业务测试、迁移后的 GPUI 核心 UI integration tests 及相关构建检查通过。
- [ ] 两个平台实际验证扫描、筛选、勾选、计划确认、项目及全局缓存清理、中止和失败恢复。
- [ ] 验证中英文、系统 / 浅色 / 深色、窗口缩放与最小窗口约束、长路径、键盘导航和浮层焦点恢复。
- [ ] 核对并覆盖原 egui 交互测试所保护的产品行为，区分自动化结果与真实窗口人工验证。
- [ ] 按 GPUI Kit Design review checklist 审核并修复发现的问题；未实际验证的平台或场景明确记为未通过，阻止切换。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。


## Comments

### 2026-11-05 — 07 集成后验收准备

- `integration/gpui-ui` 已 fast-forward 合入 07 实施提交 `3fd233a84881434c70049ac9b798ed2f3ad10bed`；05/07 的项目和全局缓存清理流程现已共同位于集成分支，可开始本票跨流程验收。详见 [07 集成证据](07-global-cache-clean.md)。
- 集成后 fmt/diff 检查通过；缓存 GPUI UI integration tests 8/8、主壳/设置 GPUI UI integration tests 7/7 通过。完整套件此前记录为 230 passed、4 ignored、0 failed，末次 GPUI 回归 24/24；这些为既有实施证据，本轮仅复跑上述相关测试。
- 本票全部验收框保持未勾选。仍需 macOS 真实窗口视觉/主题/语言/缩放/长路径/键盘与焦点/剪贴板、系统 Trash、长任务进度及中止、真实 Go 环境，以及 Windows 编译和同等完整流程与回收站/权限/占用失败验证。自动化不能代替这些接受验证；未验证场景继续阻止切换，保留 egui。


### 2026-11-05 — 本轮实施及未完成验收

- 独立报告：[08 验证报告](../validation-08.md)，明确分开自动化、真实 macOS 操作、审计缺口及双平台未通过标准；本票不 resolved，egui/默认入口不变，未开始 09。
- 修复最小窗口清理结果挤掉项目列表（控制区可收缩滚动，主工作区保留一行）、macOS 外框尺寸误当内容区造成每次重启增高 32 px（改存 viewport）、项目/全局缓存遗漏 >5 GB 额外勾选确认（新计划重置，按钮和执行 handler 双重限制）。新增 3 个生产 GPUI 交互测试并扩展固定缓存隔离测试；覆盖跨类型/危险项选择、筛选/确认/取消/刷新/真实成功及权限 Failure、设置大字体/1.5 显示缩放/焦点/几何、大体积确认重置和授权执行。布局与大体积保护保留红/绿记录，标题栏问题用原生连续重启复核，未称 headless 已复现原生标题栏。
- 末次完整串行套件 233 passed、4 ignored、0 failed，含 27 个 GPUI UI integration tests。fmt/diff、生产 lib/bins 严格 Clippy、macOS GPUI/egui bins 构建通过。all-targets 普通 Clippy 通过但既有测试 lint 尚在；严格 all-targets 不是通过。默认并行 Go fixture 的 1 秒超时复现后，普通 shell 夹具预算改为 5 秒，保留专用短超时测试和生产超时；默认并行全量随后同为 233/4/0。磁盘耗尽期间失败均单列，不计成功。
- macOS 原生实操：临时项目发现/测量、类型筛选、勾选、清理快捷键预览/取消、中英文及三种主题、缩窄与重启尺寸、方向键/Space、实际路径/失败详情复制粘贴；项目 Trash 调用实际 Failure，Freed bytes 0、文件完整；请求中止持续等待当前项目，最终显示已停止及 Failure，仍可重试/设置。修复版最小窗口截图保留完整项目行；原生大体积额外确认禁用/取消重置通过。全局页仅选择/审阅/取消，未执行用户缓存。已退出验收 app，config 字节与原备份一致，rules 恢复原先不存在状态。
- 尚未通过：Windows 编译及全部原生流程（无 Windows target/VM/可用环境链接工具）；macOS Trash 成功/系统恢复、原生 Direct/权限恢复、隔离全局完整执行/中止/失败恢复、真实 Go、真实长任务/大计划、前台切回自动刷新/Go 缓存联动迁移覆盖，以及完整 Design checklist 跨平台状态矩阵。详见报告的逐项可执行接受标准。故双平台流程、完整交互迁移审计和设计验收框仍保持未勾选，不允许切换。
- 提交前 `git merge integration/gpui-ui`：Already up to date，tip `dbbf56c9bc0e9080a05ffc322bb1a7df1c870e37`。使用 git-commit skill；无 children、push、PR 或新远程资源。

### 2026-11-05 — 激活与 30 秒刷新本地缺口

- 从指定集成基线 `9fd66c27d8b8af3db1c074b6548625623b4a6026` 补齐 GPUI 窗口 activation：30 秒切回扫描、手动刷新重置时效、扫描中不重复启动、清理期间合并排队并在取消/结束后释放，联动 Go 重查与重建的 Cleaned 固定缓存后台测量。保留项目选择/草稿/已知体积、缓存选择和当前视图；保留现有根/项目路径改变的失效与重建流程。
- 新增 2 个公开业务时效回归和 2 个生产 GPUI UI integration tests；隔离目录真实 Direct 清理/重建与 Go shell fixture，不触碰用户缓存。完整串行套件 237 passed / 4 ignored / 0 failed，生产严格 Clippy、普通 all-targets Clippy、fmt/diff 与 macOS bins 构建通过；all-targets 既有测试 lint 保留。详细命令、红/绿、末次回归与集成同步见 [报告新增章节](../validation-08.md#2026-11-05--激活与过期刷新本地缺口实施)。
- 仅本地功能/自动化缺口完成；原生 activation、真实 Go、Windows 与其它原生/设计接受缺口仍未通过，状态和验收框保持不变。无 children、checkout 创建、PR/push，egui/用户配置/默认入口不变。
