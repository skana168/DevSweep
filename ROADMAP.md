# 开发计划（Roadmap）

本文记录 DevSweep 后续支持的项目类型与配套能力，以下所有条目均为**待开发**。
优先级是建议顺序，不代表发布日期承诺；具体清理规则需在实现前验证误删边界。
当前已支持的能力见 [README](README.zh-CN.md)，现有清理行为见[规则参考](docs/rules.md)。

## 优先：Rust / Cargo 与 .NET

| 项目类型 | 状态 | 识别标记 | 初步清理目标与价值 | 安全边界 |
|---|---|---|---|---|
| Rust / Cargo | 待开发 | `Cargo.toml` | 项目内默认 `target/`；集中清理构建产物，补齐已有 Cargo 全局缓存之外的项目支持 | Workspace 成员通常共用根目录的输出，避免重复归属；首版不追踪自定义输出位置，不清理项目外路径 |
| .NET / C# / F# / Visual Basic | 待开发 | `*.csproj`、`*.fsproj`、`*.vbproj` | 具体项目内的 `bin/`、`obj/`；覆盖最终输出与中间产物，补充 Windows 开发场景 | 按项目标记限定清理范围，不因发现解决方案文件就递归删除所有同名目录；目录中的手工内容风险需在实现前验证 |

参考：[Cargo 构建缓存](https://doc.rust-lang.org/cargo/reference/build-cache.html)、
[cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)、
[dotnet clean](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-clean)。

## 随后：Swift Package

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
| Go 全局缓存 | 待开发 | 优先评估 `GOCACHE` 构建缓存与 `GOMODCACHE` 模块缓存 | 暂不承诺新增 Go 项目类型；需验证自定义路径、模块缓存只读权限及清理后的重新下载行为 |
| 混合项目覆盖 | 待开发 | 补齐 React Native 的原生目录规则，并验证未来 Tauri 的 Node 与 Rust 清理覆盖 | 遵循 [ADR 0001](docs/adr/0001-stack-is-one-product-type.md)：每个 Project 只有一个 Stack；补充有条件的规则并验证独立子项目的归属与去重，不改为同时应用多种类型的全部规则 |

参考：[Go 缓存位置与清理](https://go.dev/doc/manage-install)、
[Go 模块缓存与权限](https://go.dev/ref/mod#module-cache)。

以上顺序尚未通过实际项目的空间收益测量验证；清理目标、默认勾选和验证条件应在各项实现前明确。
