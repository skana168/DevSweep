# DevSweep

DevSweep 找出开发机器上项目内的构建/依赖产物与项目之外的全局缓存占用的空间，让用户勾选、确认后清理。本文件定义这套领域里每个词的确切含义。

## 发现

**Scan root（扫描根）**：
用户添加的、在其下方寻找项目的目录。
_Avoid_: 根目录、工作区、路径

**Covered root（被覆盖的扫描根）**：
位于另一个扫描根之下的扫描根；它不被单独遍历，其内容由父根发现。
_Avoid_: 无效根、重复根

**Project（项目）**：
扫描根之下的一个带技术栈标记的目录；可以是应用、库，或 monorepo 里的一个包。
_Avoid_: 仓库、代码库、模块

**Marker（项目标记）**：
让一个目录被认作 Project 的文件：`pubspec.yaml`、`settings.gradle(.kts)` / `build.gradle(.kts)`、`*.xcodeproj`、`pom.xml`、`package.json`、`pyproject.toml`、`setup.py`、`requirements.txt`、`Pipfile`、`Cargo.toml`。
_Avoid_: 项目文件、特征

**Stack（项目类型）**：
Project 所属的、唯一的产品技术栈家族：Flutter / iOS / Android / Java / Node / Python / Rust；它决定哪些 Rule 适用。多种标记共存时的选择约定见 [ADR 0001](docs/adr/0001-stack-is-one-product-type.md)。
_Avoid_: 框架、平台、子项目类型

**Platform directory（平台子目录）**：
Project 内的 `android/`、`ios/`、`macos/`、`windows/`、`linux/`、`web/`；属于所在项目，从不单独成为 Project。
_Avoid_: 子项目

**Refresh（刷新）**：
重新发现 Project，但不重算已有体积、不丢失已勾选项。
_Avoid_: 重扫、重载

## 规则与清理项

**Rule（规则）**：
一条描述项目内某类可清理路径的配方，按 Stack 限定范围或对全部 Stack 生效。它的 id 一旦发布即是稳定标识，用户的规则文件依赖它。
_Avoid_: 配置、白名单、过滤条件

**Rule override（规则覆盖）**：
用户通过 `rules.toml` 对 Rule 做的三种调整：关闭、改默认勾选、追加 Extra rule。它只作用于项目内的 Rule，不作用于全局缓存。
_Avoid_: 全局规则、配置

**Extra rule（自定义规则）**：
用户在 `rules.toml` 中追加的 Rule，其路径必须留在项目内。
_Avoid_: 插件、扩展

**Target（清理项）**：
一条 Rule 在一个 Project 中展开出的、当前存在的那个路径（目录或文件）；带体积、危险标记，以及清理面板里的勾选草稿。
_Avoid_: 条目、文件、缓存、项目

**Dangerous（危险项）**：
默认不勾选、必须用户主动选择的 Target 或全局缓存条目；删除它的代价（丢掉可能仍需的产物、被迫重新下载）要用户自己判断。
_Avoid_: 不安全、错误

**Project size（项目大小）**：
一个 Project 全部 Target 的体积合计，危险项也算在内；任一 Target 还在测量时它是未知，而不是 0。
_Avoid_: 可释放、可回收体积、磁盘占用

**Chosen size（已选体积）**：
一个 Project 中当前勾选、且通过了安全检查的 Target 的体积合计，即这一次 Clean 将释放的量。
_Avoid_: 可回收体积、项目大小

## 清理

**Clean（清理）**：
用户发起的一次操作：先把勾选的 Target 变成 Delete plan，再执行删除。
_Avoid_: 删除（指整个流程时）、清空

**Delete plan（清理计划）**：
勾选的 Target 中通过安全检查并去掉重叠后得到的删除清单，另附被拒的 Skip 与总体积。构建计划不写磁盘。
_Avoid_: 删除列表、任务

**Skip（跳过）**：
在计划阶段被拒绝的 Target 及其原因：在项目外、指向项目根/家目录/扫描根、`.git`、不在规则内、已不存在、路径无法解析、被更上层的目标覆盖。
_Avoid_: 失败、错误

**Execute（执行）**：
按 Delete plan 删除磁盘内容；以 Project 为单位，多个项目并行，中止只在项目之间生效。
_Avoid_: 运行、清空

**Failure（失败）**：
通过了计划、却在执行时没删掉的条目及原因：权限不足、被占用、已不存在、其它。
_Avoid_: 跳过

**Delete mode（删除方式）**：
Trash（默认，移入废纸篓/回收站，可恢复）或 Direct（直接删除，不可恢复）。
_Avoid_: 删除策略、模式

**Freed bytes（已释放）**：
执行成功的目标按其计划时体积累加出的总量；它是估算，删除后不重新测量。
_Avoid_: 节省空间、实际释放

## 全局缓存

**Global cache（全局缓存）**：
不属于任何 Project、由某个包管理器或工具链维护的目录；清掉后靠重新构建或重新下载恢复。
_Avoid_: 项目缓存、系统缓存

**Cache entry（全局缓存条目）**：
全局缓存清单中的一条，按平台解析成一个具体路径。清单固定，不来自 Rule，用户也不能通过 `rules.toml` 扩展或关闭它；全局缓存清理只能作用于其中条目解析出的路径。
_Avoid_: 规则、Target

**Cache group（缓存分组）**：
Cache entry 的展示分组：Dependency cache（依赖缓存：Gradle、npm、pnpm、bun、Yarn、pub、CocoaPods）与 Toolchain cache（工具链缓存：Playwright、DerivedData、Cargo registry、pip）。分组只决定展示与顺序，不改变清理行为。
_Avoid_: 项目缓存、项目相关

**Needs re-download（需重新下载）**：
该 Cache entry 清理后，下一次构建必须重新下载依赖或浏览器。
_Avoid_: 影响构建（太笼统）
