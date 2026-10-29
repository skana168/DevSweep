# 开发计划（Roadmap）

本文记录 DevSweep 项目类型与配套能力的开发状态；Rust / Cargo 已随 **v0.3.0** 发布，.NET 已实现并合入 `main`、尚未发布，Go 全局缓存也已实现、尚未发布，其余条目仍为**待开发**。
优先级是待开发条目的建议顺序，不代表发布日期承诺；具体清理规则需在实现前验证误删边界。
当前已支持的能力见 [README](README.zh-CN.md)，现有清理行为见[规则参考](docs/rules.md)。

## 已完成：Rust / Cargo 与 .NET

| 项目类型 | 状态 | 识别标记 | 初步清理目标与价值 | 安全边界 |
|---|---|---|---|---|
| Rust / Cargo | 已发布（[v0.3.0](https://github.com/skana168/DevSweep/releases/tag/v0.3.0)） | `Cargo.toml` | 已支持识别、筛选、体积统计与项目根目录真实 `target/` 的清理 | 危险项默认不勾选；Workspace 共享产物归根项目，成员只处理自身 `target/`；发现与最终计划均拒绝文件和符号链接；不追踪自定义输出位置，不清理项目外路径 |
| .NET / C# / F# / Visual Basic | 已合入 `main`（`fdaf2c7`），未发布 | `*.csproj`、`*.fsproj`、`*.vbproj`（真实普通文件，扩展名大小写不敏感） | 已支持统一 .NET 类型识别、筛选、体积统计及项目根直属、精确小写的真实 `bin/`、`obj/` 整目录清理 | 两项独立、危险且默认不勾选；手工内容也会被删除；发现与最终计划拒绝文件、符号链接；解决方案不触发递归清理；不追踪自定义或集中输出，清理前应停止构建 |

参考：[Cargo 构建缓存](https://doc.rust-lang.org/cargo/reference/build-cache.html)、
[cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)、
[dotnet clean](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-clean)。

## 下一优先项：Swift Package

| 项目类型 | 状态 | 识别标记 | 初步清理目标与价值 | 安全边界 |
|---|---|---|---|---|
| Swift Package | 待开发 | `Package.swift` | 评估 `.build/`；覆盖没有 `.xcodeproj` 的库和命令行项目 | 区分构建产物与依赖工作副本，保护可能存在的手工修改；不把整个目录直接视为可丢弃缓存 |

参考：[Swift Package 清理命令](https://docs.swift.org/latest/documentation/userdocs/commands/)、
[Swift Package 标记与结构](https://www.swift.org/getting-started/library-swiftpm/)。

## 后续候选：Unity 与 CMake

| 项目类型 | 状态 | 识别标记 | 初步清理目标与价值 | 安全边界 |
|---|---|---|---|---|
| Unity | 待开发 | 联合检查 `ProjectSettings/ProjectVersion.txt` 与 `Assets/` | 评估 `Library/` 中可重建的导入缓存，拓展游戏开发场景 | 默认不勾选，提示关闭编辑器后清理及重新导入成本；保护源资源、资源元数据和项目设置 |
| C / C++（先限 CMake） | 待开发 | `CMakeLists.txt`，结合候选构建目录内的 `CMakeCache.txt` 等标记验证 | 评估已确认构建目录中的产物，覆盖常见原生开发场景 | 默认不勾选，提示缓存内用户配置可能丢失；不因目录名为 `build/` 就删除，不清理项目外路径 |

参考：[Unity 资源数据库与缓存](https://docs.unity3d.com/6000.0/Documentation/Manual/asset-database-contents.html)、
[CMake 缓存与用户配置](https://cmake.org/cmake/help/book/mastering-cmake/chapter/CMake%20Cache.html)。

## 配套计划

| 能力 | 状态 | 初步范围与价值 | 边界 |
|---|---|---|---|
| Go 全局缓存 | 已实现，未发布 | 已支持独立定位、统计和清理 `GOCACHE` 构建缓存与 `GOMODCACHE` 模块缓存，读取当前本机 Go 的实际配置 | 不新增 Go 项目类型；拒绝危险、链接和重叠路径，位置变化需重新确认；保留废纸篓 / 直接删除方式，模块缓存提示重新下载、离线与手工修改丢失风险；macOS 临时夹具删除及恢复已验证，Windows 系统删除与恢复尚待实机验证 |
| 混合项目覆盖 | 待开发 | 补齐 React Native 的原生目录规则，并验证 Tauri 的 Node 与 Rust 清理覆盖 | 遵循 [ADR 0001](docs/adr/0001-stack-is-one-product-type.md)：每个 Project 只有一个 Stack；Rust 支持发布不代表同目录 Node 与 Rust 规则自动叠加；补充有条件的规则并验证独立子项目的归属与去重，不改为同时应用多种类型的全部规则 |

参考：[Go 缓存位置与清理](https://go.dev/doc/manage-install)、
[Go 模块缓存与权限](https://go.dev/ref/mod#module-cache)。

以上待开发条目的顺序尚未通过实际项目的空间收益测量验证；清理目标、默认勾选和验证条件应在各项实现前明确。
