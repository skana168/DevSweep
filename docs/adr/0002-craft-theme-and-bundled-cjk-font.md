# 主题照搬 craft-launcher，中文用内置 Maple Mono 子集

视觉语言整套取自 Craft Launcher 的 ArtCraft 设计（storytold/craft-launcher，Apache-2.0）：暖黑/米白底色、1px hairline 分隔、零圆角、Archivo 标题 + Inter 正文 + Geist Mono 标签。`src/theme.rs` 从它的 `crates/ui-egui/src/theme.rs` 改写而来；面板、栏宽、布局与交互是 DevSweep 自己的，不随主题变化。

中文字形单独解决：Inter、Archivo、Geist Mono 都没有 CJK，而界面默认中文。二进制内嵌 Maple Mono CN v7.9 的 **GB2312 子集**（`scripts/subset-maple.sh` 生成，5.2 MB），作为每个字体族的兜底；拉丁与等宽仍用 craft 的三款字体。

另一个方案是运行时读系统 CJK 字体（macOS PingFang、Windows 微软雅黑）。放弃它的原因是外观会随机器变化，且需要按字族名解析 `.ttc`（~60 行平台相关代码，找不到时要静默降级）；内置子集在两个平台渲染一致，也不增加运行时依赖。

没有内嵌完整 Maple CN：单个字重 18.6 MB，app 体积会翻三倍。子集覆盖 6763 个常用汉字，足以显示任意项目路径；生僻字会缺字形，届时加宽子集并重跑脚本即可。只打包 Regular 一个字重，因为 egui 的 `strong()` 只换颜色、不做合成加粗，标题用同一个字重。

代价：字体让二进制多出约 6.9 MB（三款拉丁字体 1.7 MB + 中文子集 5.2 MB）；换字体版本要重跑子集脚本，并同步脚本里的版本号与 `NOTICE`。
