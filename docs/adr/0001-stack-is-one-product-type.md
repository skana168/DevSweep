# Stack 是单一产品类型

每个 Project 只带一个 Stack（Flutter / iOS / Android / Java / Node / Python / Rust），按 Flutter > iOS > Android > Java > Node > Python > Rust 的优先级取自项目目录自身的标记；它决定哪些 Rule 适用。

另一个方案是记录目录里所有命中的标记并应用所有类型的规则（v0.1 SPEC 曾这样写），但同一条路径会被多条规则命中（Flutter 的 `build` 与 Android 的 `build`），清理清单和确认框会出现重复项，去重时"哪条规则拥有这条路径"也变得含糊，因此弃用。

代价是混合形态的项目只走一套规则：Flutter 的规则显式列出 `android/`、`ios/` 下的路径；React Native 这类 Node + Android 项目目前只能清 Node 项。这是规则表的缺口，补规则，不要把 `classify` 改成返回多个 Stack。平台子目录里的标记不改变父项目的 Stack。

Java 只由 `pom.xml` 触发：Gradle 项目缺少可靠的 Android / 非 Android 判别（现代 Android 用 version catalog，`build.gradle` 里可能看不到 `com.android`），所以 Gradle JVM 项目仍归 Android——类型标签不准，但它的 `build/` 与 `.gradle` 本来就被 Android 规则覆盖，不丢空间。

Python 支持：将 Python 追加到优先级末尾，成为 Flutter > iOS > Android > Java > Node > Python，以保留已有项目的归类与清理行为。代价是同目录同时有 `package.json` 与 Python 标记时仍归 Node，不自动合并 Python 规则；独立子目录中的 Python Project 可单独识别。

Rust 支持：将 Rust 追加到优先级末尾，成为 Flutter > iOS > Android > Java > Node > Python > Rust，仍以保留已有混合项目的归类与清理行为为先。同目录的 `Cargo.toml` 不叠加 Rust 规则；独立子目录中的 Rust Project 可单独识别。代价是同目录同时有更高优先级 Marker 时，Rust 构建产物不被自动覆盖。
