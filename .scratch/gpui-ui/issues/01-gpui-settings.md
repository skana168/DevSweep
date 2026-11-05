# 01: GPUI 启动与设置闭环

**What to build:** 启动 GPUI 窗口，打开独立设置对话框，切换语言、主题和默认删除方式，并在重启后恢复原有配置；迁移期间 egui 仍可运行作对照。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 核实并锁定可获取的 GPUI Kit 版本，仅通过 Kit facade 使用框架，不混配内部依赖；两个平台均能构建启动。
- [ ] 保留品牌资产，使用 Kit 图标、语义主题及平台字体，中文、英文和路径文本可读。
- [ ] 读取现有配置，保存并恢复语言、主题、默认删除方式及窗口尺寸，旧配置无需用户手动迁移。
- [ ] 设置支持键盘进入、Escape 关闭与焦点恢复；通过 GPUI UI integration tests 验证设置更改及配置兼容。
- [ ] 保留可运行的 egui 对照入口和现有业务测试。

## 实施约定

遵循已确认的 GPUI UI 迁移规格与 ADR-0003。遇到 GPUI 用法或设计疑问，查阅已安装的 gpui-kit 与 gpui-kit-design-guides skills，并核实当前版本 API。沿用领域词汇表，保持现有安全语义。
