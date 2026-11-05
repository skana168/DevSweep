# GPUI 设置入口（迁移中）

默认入口仍为 egui：`cargo run --bin devsweep`（或 `cargo run`）。GPUI 的扫描和清理视图由后续票据实现，此入口仅提供窗口和设置闭环。

```sh
cargo run --bin devsweep-gpui --features gpui --locked
cargo test --lib --features gpui-tests --locked
```

点击「设置…」或按 Command+,（macOS）/ Control+,（Windows）打开独立模态对话框。Tab / Shift+Tab 移动焦点，Space / Enter 选择选项，Escape 关闭并恢复原焦点。语言、主题和默认删除方式即时保存；窗口尺寸在调整时保存。两个入口沿用 `config.toml`、字段和配置目录，不需要迁移文件；扫描根不被设置界面改写。配置读取或保存失败继续沿用既有的默认值 / best-effort 语义。

GPUI 仅依赖 facade `gpui-kit = "=0.7.0"`；内部配套版本由 Kit 和 Cargo.lock 管理，不直接添加 GPUI / Base / Component 依赖。2026-11-05 实查 crates.io 最新 0.9.0 的 Fast 可选依赖经 gpui-fast-web 锁定 `unicode-properties = 0.1.3`，与保留的 egui 0.37 所需 `^0.1.4` 无法解析。0.8.0 也引入 Fast 依赖系列，因此本票使用成功解析、构建和测试的 0.7.0。升级应重新验证双前端依赖解析。

保留现有 PNG / ICNS / ICO 品牌资产，界面图标来自 Kit。使用语义主题和系统 UI 字体（macOS `.SystemUIFont`，Windows `Segoe UI`，由系统提供 CJK fallback），不复用旧 Craft 字体策略。默认发布脚本继续打包 egui；本票不切换默认产品入口。

CI 为 macOS / Windows 增加 GPUI 构建与真实组件 UI integration tests；未运行的 CI 和平台不计作验收。当前执行证据和限制见 [票据 01](../.scratch/gpui-ui/issues/01-gpui-settings.md)。
