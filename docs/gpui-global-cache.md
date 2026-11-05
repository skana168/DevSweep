# GPUI 全局缓存选择（票据 06）

独立演示入口：

```sh
cargo run --bin devsweep-gpui-cache --features gpui --locked
```

入口读取既有 `Config`（语言、主题、窗口尺寸和扫描根），采用 Kit facade、Kit 图标和原生窗口装饰。它不写配置；设置仍由票据 01 的入口编辑，下次启动演示窗口生效。默认 egui 入口不变。这里只测量和选择，不生成或执行清理计划。

## 主窗口集成

02 的壳拥有导航，在全局缓存页首次激活时创建并保留独立实体：

```rust
let caches = cx.new(|cx| {
    GlobalCacheView::new(&config, project_paths, window, cx)
});
// 在已有 Kit Root 内的内容区域挂载，使用 flex_1/min_h_0/min_w_0。
// 父视图 render: .child(caches.clone())
```

`GlobalCacheView` 不依赖 SettingsApp。`new` 启动后台工作并将焦点放到首行；不要在 render 中重建实体。`project_paths` 应来自当前项目工作区；扫描根、项目集合变化时，应重新创建缓存实体，让 Go 保护上下文与工作区一致，旧实体任务随实体释放取消，不会把旧结果写到新实体。保留实体在导航切换间可保留选择草稿。语言变更调用 `set_language(language, cx)`；主题通过 Kit 全局主题传播。

鼠标 Checkbox、Tab/Space、上下方向键、Home/End 可选择和导航；方向键自动将目标行滚入可见区域。选择与焦点分开。路径截断后可使用可聚焦的“复制路径”按钮取得完整路径。所有体积结果完成前显示测量状态，缺失、不可读和非目录路径显示原因并禁止勾选。正常测量后的非危险条目默认选择；危险项（Go module、Cargo registry）必须主动选择。

分组和顺序来自固定 `cache::builtin_caches`，静态路径来自 `cache::entries`；Windows 不展示不适用的 DerivedData。Go 条目调用 `go_cache::discover`，保留扫描根、项目和其它缓存保护以及原错误分类。普通缓存使用现有 `size::dir_size`（不跟随后代链接；根路径链接沿用该 API 的遍历行为）；根目录读取失败有明确反馈，但与既有测量 API 一样，子目录遍历错误会被跳过，体积仍为估算。进度计数是已返回结果数 / 固定条目数，包含失败结果，不是模拟百分比。

07 可通过 `selection()` 读取固定条目 ID 和所选字节汇总，通过 `go_snapshot()` 取得发现时快照。选择草稿不是清理授权：普通条目必须重新调用 `cache::plan`，Go 必须使用既有 snapshot/context 重检与 execute API，并保留确认、删除方式和失败流程。不得把视图中的路径作为任意删除输入。

## 验证边界

UI integration tests 使用生产实体、真实 Kit 控件、临时文件系统及正常鼠标/键盘事件；不会直接调用选择 handler。现有公开业务测试验证双平台固定路径、分组和 Go 保护。自动化的无头窗口不等于原生平台视觉验收；Windows 构建/运行、macOS 原生目测及完整主壳导航仍由集成与票据 08 补证。
