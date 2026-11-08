//! Presentation catalogue; absent cache IDs never authorize backend cleanup.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub(super) struct Tool {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) cache_id: Option<&'static str>,
    pub(super) brand: &'static str,
    pub(super) low_risk: bool,
    pub(super) demo_bytes: u64,
    pub(super) demo_label: &'static str,
}
pub(super) const TOOLS: &[Tool] = &[
    Tool {
        id: "gradle-build",
        name: "Gradle 构建缓存",
        description: "Gradle 保存的可复用构建结果",
        cache_id: Some("gradle.caches"),
        brand: "gradle",
        low_risk: true,
        demo_bytes: 4_520_000_000,
        demo_label: "4.52 GB",
    },
    Tool {
        id: "codex-archived",
        name: "较早的 Codex 归档会话",
        description: "超过 30 天未修改的 Codex 已归档对话记录",
        cache_id: None,
        brand: "openai",
        low_risk: false,
        demo_bytes: 4_240_000_000,
        demo_label: "4.24 GB",
    },
    Tool {
        id: "cargo-sources",
        name: "Cargo 解压源码缓存",
        description: "Cargo 下载并解压的依赖源码",
        cache_id: None,
        brand: "rust",
        low_risk: false,
        demo_bytes: 1_740_000_000,
        demo_label: "1.74 GB",
    },
    Tool {
        id: "rust-toolchains",
        name: "附加 Rust 工具链",
        description: "不再活动或项目不再需要的 Rust 版本",
        cache_id: None,
        brand: "rust",
        low_risk: false,
        demo_bytes: 1_550_000_000,
        demo_label: "1.55 GB",
    },
    Tool {
        id: "npm-download",
        name: "npm 下载缓存",
        description: "npm 之前下载的软件包缓存",
        cache_id: Some("npm.cacache"),
        brand: "npm",
        low_risk: true,
        demo_bytes: 1_020_000_000,
        demo_label: "1.02 GB",
    },
    Tool {
        id: "npx-tools",
        name: "npx 临时工具缓存",
        description: "npx 临时下载并执行的软件包目录",
        cache_id: None,
        brand: "npm",
        low_risk: true,
        demo_bytes: 1_015_000_000,
        demo_label: "1,015 MB",
    },
    Tool {
        id: "nuget",
        name: "NuGet 包与请求缓存",
        description: "NuGet 保留的软件包和请求缓存",
        cache_id: None,
        brand: "nuget",
        low_risk: true,
        demo_bytes: 955_000_000,
        demo_label: "955 MB",
    },
    Tool {
        id: "cargo-rustup",
        name: "Cargo 与 Rustup 缓存",
        description: "Cargo 注册表、Git 依赖及 Rustup 下载缓存",
        cache_id: None,
        brand: "rust",
        low_risk: true,
        demo_bytes: 204_000_000,
        demo_label: "204 MB",
    },
    Tool {
        id: "vscode",
        name: "VS Code 缓存",
        description: "VS Code 产生的扩展和工作区缓存",
        cache_id: None,
        brand: "vscode",
        low_risk: true,
        demo_bytes: 24_600_000,
        demo_label: "24.6 MB",
    },
];
pub(super) fn primary_cache(id: &str) -> bool {
    TOOLS.iter().any(|tool| tool.cache_id == Some(id))
}
pub(super) fn initial_selection() -> std::collections::HashSet<String> {
    TOOLS
        .iter()
        .filter(|t| t.low_risk)
        .map(|t| t.id.to_owned())
        .collect()
}
