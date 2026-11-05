# 08: 双平台完整流程验收与交互补齐

**What to build:** 在 macOS 与 Windows 完成真实窗口中的核心工作流，补齐跨流程交互问题并记录可核查的验收证据。

**Blocked by:** 05 — 项目清理完整流程；07 — 全局缓存清理完整流程

**Status:** ready-for-agent

- [ ] 现有业务测试、迁移后的 GPUI 核心 UI integration tests 及相关构建检查通过。
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
