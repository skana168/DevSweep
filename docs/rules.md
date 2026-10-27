# `rules.toml`

DevSweep's project rules are built in. `rules.toml` lets you turn them off, change which
ones start checked, and add directories of your own. The global cache list is fixed and is not
affected by this file.

## Where the file lives

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/DevSweep/rules.toml` |
| Windows | `%APPDATA%\DevSweep\rules.toml` |

If the file does not exist, "Open rules file" in the app writes a commented template. A typo does
not take the app down: the built-in rules stay in effect and the error is shown in the interface.

## What you can do

```toml
# 1. turn built-in entries off
disable = ["node.dist", "ios.pods"]

# 2. change whether an entry is checked by default
[defaults]
"node.dist" = true

# 3. add a directory of your own (stack: flutter / android / ios / node / "*", label optional)
[[extra]]
stack = "*"
path = ".cache"
label = "Cache"
```

Extras are always checked by default and are never marked dangerous. Their ids are generated
(`extra.0`, `extra.1`, …) and cannot be referenced from `disable` or `[defaults]`.

## Built-in rules

`id`s are stable once released: your file depends on them, so they never get renamed.

| Stack | Id | Path | Checked by default |
|---|---|---|---|
| Flutter | `flutter.build` | `build` | yes |
| Flutter | `flutter.dart_tool` | `.dart_tool` | yes |
| Flutter | `flutter.ios_pods` | `ios/Pods` | yes |
| Flutter | `flutter.ios_ephemeral` | `ios/Flutter/ephemeral` | yes |
| Flutter | `flutter.android_gradle` | `android/.gradle` | yes |
| Flutter | `flutter.android_build` | `android/build` | yes |
| Flutter | `flutter.macos_pods` | `macos/Pods` | yes |
| Flutter | `flutter.plugins_dependencies` | `.flutter-plugins-dependencies` | yes |
| Android | `android.build` | `build` | yes |
| Android | `android.module_build` | `*/build` | yes |
| Android | `android.gradle` | `.gradle` | yes |
| Android | `android.cxx` | `.cxx` | yes |
| Android | `android.captures` | `captures` | yes |
| iOS | `ios.pods` | `Pods` | yes |
| iOS | `ios.build` | `build` | yes |
| iOS | `ios.xcuserdata` | `*.xcodeproj/xcuserdata` | yes |
| Node | `node.node_modules` | `node_modules` | yes |
| Node | `node.next` | `.next` | yes |
| Node | `node.out` | `out` | yes |
| Node | `node.nuxt` | `.nuxt` | yes |
| Node | `node.svelte_kit` | `.svelte-kit` | yes |
| Node | `node.turbo` | `.turbo` | yes |
| Node | `node.parcel_cache` | `.parcel-cache` | yes |
| Node | `node.vite` | `.vite` | yes |
| Node | `node.coverage` | `coverage` | yes |
| Node | `node.dist` | `dist` | **no** (dangerous) |

A project carries one stack, chosen by priority Flutter > iOS > Android > Node, so only that
stack's rules apply to it (see [ADR 0001](adr/0001-stack-is-one-product-type.md)).

## Paths

`path` must be a literal path inside the project: no absolute paths, no `..`, no `.`. A single
`*` may stand in for one path segment, as in `*/build` or `*.xcodeproj/xcuserdata`.

Paths that are invalid, that point outside the project, or that do not exist are skipped; invalid
ones are reported in the interface.
