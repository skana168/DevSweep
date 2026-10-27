# Stack 是单一产品类型

每个 Project 只带一个 Stack（Flutter / iOS / Android / Node），按 Flutter > iOS > Android > Node 的优先级取自项目目录自身的标记；它决定哪些 Rule 适用。

另一个方案是记录目录里所有命中的标记并应用所有类型的规则（v0.1 SPEC 曾这样写），但同一条路径会被多条规则命中（Flutter 的 `build` 与 Android 的 `build`），清理清单和确认框会出现重复项，去重时"哪条规则拥有这条路径"也变得含糊，因此弃用。

代价是混合形态的项目只走一套规则：Flutter 的规则显式列出 `android/`、`ios/` 下的路径；React Native 这类 Node + Android 项目目前只能清 Node 项。这是规则表的缺口，补规则，不要把 `classify` 改成返回多个 Stack。平台子目录里的标记不改变父项目的 Stack。
