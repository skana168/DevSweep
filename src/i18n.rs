use devsweep::cache::CacheGroup;
use devsweep::config::LanguageChoice;
use devsweep::delete::FailureReason;
use devsweep::go_cache::GoCacheError;
use devsweep::plan::SkipReason;
use devsweep::scan::Stack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_system() -> Self {
        let locale = sys_locale::get_locale().unwrap_or_default();
        if locale.to_lowercase().starts_with("zh") {
            Self::Zh
        } else {
            Self::En
        }
    }

    pub fn from_choice(choice: LanguageChoice) -> Self {
        match choice {
            LanguageChoice::System => Self::from_system(),
            LanguageChoice::Zh => Self::Zh,
            LanguageChoice::En => Self::En,
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::Zh => &ZH,
            Self::En => &EN,
        }
    }
}

pub struct Strings {
    pub roots: &'static str,
    pub add_directory: &'static str,
    pub remove: &'static str,
    pub projects: &'static str,
    pub empty_title: &'static str,
    pub empty_body: &'static str,
    pub no_projects_title: &'static str,
    pub no_projects_body: &'static str,
    pub measuring: &'static str,
    pub project_column: &'static str,
    pub stack_column: &'static str,
    pub clean: &'static str,
    pub more: &'static str,
    pub open_folder: &'static str,
    pub open_terminal: &'static str,
    pub dangerous: &'static str,
    pub items: &'static str,
    pub total: &'static str,
    pub skipped: &'static str,
    pub skip_outside_project: &'static str,
    pub skip_project_root: &'static str,
    pub skip_home: &'static str,
    pub skip_scan_root: &'static str,
    pub skip_git: &'static str,
    pub skip_outside_rules: &'static str,
    pub skip_missing: &'static str,
    pub skip_unresolvable: &'static str,
    pub skip_overlap: &'static str,
    pub confirm_title: &'static str,
    pub confirm: &'static str,
    pub cancel: &'static str,
    pub large_warning: &'static str,
    pub large_ack: &'static str,
    pub cleaning: &'static str,
    pub abort: &'static str,
    pub aborted: &'static str,
    pub freed: &'static str,
    pub failures: &'static str,
    pub close: &'static str,
    pub done: &'static str,
    pub fail_permission: &'static str,
    pub fail_in_use: &'static str,
    pub fail_missing: &'static str,
    pub fail_other: &'static str,
    pub stack_flutter: &'static str,
    pub stack_android: &'static str,
    pub stack_ios: &'static str,
    pub stack_java: &'static str,
    pub stack_node: &'static str,
    pub stack_python: &'static str,
    pub stack_rust: &'static str,
    pub unity_supported: &'static str,
    pub unity_unsupported: &'static str,
    pub unity_invalid: &'static str,
    pub unity_unknown: &'static str,
    pub unity_read_only: &'static str,
    pub stack_dotnet: &'static str,
    pub dotnet_clean_note: &'static str,
    pub global_caches: &'static str,
    pub cache_column: &'static str,
    pub group_column: &'static str,
    pub cache_group_dependency: &'static str,
    pub cache_group_toolchain: &'static str,
    pub cache_redownload: &'static str,
    pub cache_cargo_note: &'static str,
    pub go_build_cache: &'static str,
    pub go_module_note: &'static str,
    pub go_module_cache: &'static str,
    pub go_build_note: &'static str,
    pub go_stop_tasks: &'static str,
    pub go_cache_changed: &'static str,
    pub go_cache_querying: &'static str,
    pub go_unavailable: &'static str,
    pub go_query_failed: &'static str,
    pub go_timeout: &'static str,
    pub go_invalid_output: &'static str,
    pub go_unreadable: &'static str,
    pub go_disabled: &'static str,
    pub go_missing: &'static str,
    pub go_not_directory: &'static str,
    pub go_protected: &'static str,
    pub go_overlap: &'static str,
    pub go_linked: &'static str,
    pub all: &'static str,
    pub type_filter: &'static str,
    pub search_hint: &'static str,
    pub size_column: &'static str,
    pub no_match_title: &'static str,
    pub no_match_body: &'static str,
    pub clear_filters: &'static str,
    pub settings: &'static str,
    pub minimize: &'static str,
    pub maximize: &'static str,
    pub restore: &'static str,
    pub language: &'static str,
    pub theme: &'static str,
    pub system: &'static str,
    pub dark: &'static str,
    pub light: &'static str,
    pub lang_zh: &'static str,
    pub lang_en: &'static str,
    pub delete_mode: &'static str,
    pub delete_trash: &'static str,
    pub delete_direct: &'static str,
    pub delete_note: &'static str,
    pub rules_file: &'static str,
    pub open_rules: &'static str,
    pub rules_error: &'static str,
    pub rules_restart: &'static str,
    pub root_covered: &'static str,
    pub root_duplicate: &'static str,
}

impl Strings {
    pub fn unity_note(&self, version: &devsweep::scan::UnityVersion) -> String {
        use devsweep::scan::UnityVersion;
        let qualification = match version {
            UnityVersion::Supported(version) => format!("{version}: {}", self.unity_supported),
            UnityVersion::Unsupported(version) => format!("{version}: {}", self.unity_unsupported),
            UnityVersion::Invalid => self.unity_invalid.to_string(),
            UnityVersion::Unknown => self.unity_unknown.to_string(),
        };
        format!("{} {qualification}", self.unity_read_only)
    }

    pub fn go_cache_error(&self, error: GoCacheError) -> &'static str {
        match error {
            GoCacheError::Changed => self.go_cache_changed,
            GoCacheError::Unavailable => self.go_unavailable,
            GoCacheError::QueryFailed => self.go_query_failed,
            GoCacheError::Timeout => self.go_timeout,
            GoCacheError::InvalidOutput => self.go_invalid_output,
            GoCacheError::Unreadable => self.go_unreadable,
            GoCacheError::Disabled => self.go_disabled,
            GoCacheError::Missing => self.go_missing,
            GoCacheError::NotDirectory => self.go_not_directory,
            GoCacheError::Protected => self.go_protected,
            GoCacheError::Overlap => self.go_overlap,
            GoCacheError::Linked => self.go_linked,
        }
    }

    pub fn stack(&self, stack: Stack) -> &'static str {
        match stack {
            Stack::Flutter => self.stack_flutter,
            Stack::Android => self.stack_android,
            Stack::Ios => self.stack_ios,
            Stack::Java => self.stack_java,
            Stack::Node => self.stack_node,
            Stack::Python => self.stack_python,
            Stack::Rust => self.stack_rust,
            Stack::Dotnet => self.stack_dotnet,
            Stack::Unity => "Unity",
        }
    }

    pub fn cache_group(&self, group: CacheGroup) -> &'static str {
        match group {
            CacheGroup::Dependency => self.cache_group_dependency,
            CacheGroup::Toolchain => self.cache_group_toolchain,
        }
    }

    pub fn skip_reason(&self, reason: SkipReason) -> &'static str {
        match reason {
            SkipReason::UnityReadOnly => self.unity_read_only,
            SkipReason::OutsideProject => self.skip_outside_project,
            SkipReason::ProjectRoot => self.skip_project_root,
            SkipReason::Home => self.skip_home,
            SkipReason::ScanRoot => self.skip_scan_root,
            SkipReason::Git => self.skip_git,
            SkipReason::OutsideRules => self.skip_outside_rules,
            SkipReason::Missing => self.skip_missing,
            SkipReason::Unresolvable => self.skip_unresolvable,
            SkipReason::Overlap => self.skip_overlap,
        }
    }

    pub fn failure_reason(&self, reason: FailureReason) -> &'static str {
        match reason {
            FailureReason::UnityReadOnly => self.unity_read_only,
            FailureReason::PermissionDenied => self.fail_permission,
            FailureReason::InUse => self.fail_in_use,
            FailureReason::Missing => self.fail_missing,
            FailureReason::Other => self.fail_other,
        }
    }
}

static ZH: Strings = Strings {
    roots: "扫描根",
    add_directory: "添加目录",
    remove: "移除",
    projects: "项目",
    empty_title: "还没有扫描根",
    empty_body: "添加一个目录，DevSweep 会找出其中的 Flutter、Android、iOS、Java 和 Node 项目。",
    no_projects_title: "没有找到项目",
    no_projects_body: "这个目录下没有识别到 Flutter、Android、iOS、Java 或 Node 项目。",
    measuring: "体积计算中",
    project_column: "项目",
    stack_column: "类型",
    clean: "清理",
    more: "更多",
    open_folder: "打开文件夹",
    open_terminal: "在终端打开",
    dangerous: "危险",
    items: "项",
    total: "合计",
    skipped: "被拒绝",
    skip_outside_project: "超出项目范围",
    skip_project_root: "项目根目录",
    skip_home: "用户主目录",
    skip_scan_root: "扫描根",
    skip_git: ".git 目录",
    skip_outside_rules: "规则外路径",
    skip_missing: "不存在",
    skip_unresolvable: "路径无法解析",
    skip_overlap: "重叠目标",
    confirm_title: "确认清理",
    confirm: "确认清理",
    cancel: "取消",
    large_warning: "以下条目超过 5 GB：",
    large_ack: "我确认删除这些大条目",
    cleaning: "正在清理",
    abort: "中止",
    aborted: "已中止",
    freed: "已释放",
    failures: "失败清单",
    close: "关闭",
    done: "完成",
    fail_permission: "权限不足",
    fail_in_use: "被占用",
    fail_missing: "不存在",
    fail_other: "其他错误",
    stack_flutter: "Flutter",
    stack_android: "Android",
    stack_ios: "iOS",
    stack_java: "Java",
    stack_node: "Node",
    stack_python: "Python",
    stack_rust: "Rust",
    unity_supported: "符合 Unity 2022.3+ 版本范围，仍未开放清理。",
    unity_unsupported: "低于支持下限 Unity 2022.3；这不表示所有早期版本都不安全。",
    unity_invalid: "版本信息损坏，无法确认是否符合 Unity 2022.3+ 范围。",
    unity_unknown: "版本未知，无法确认是否符合 Unity 2022.3+ 范围。",
    unity_read_only: "Unity 仅支持只读展示，清理能力尚未开放。",
    stack_dotnet: ".NET",
    dotnet_clean_note: "将删除所选 bin/obj 整个目录，包括手工放入的内容。请先停止构建。",
    global_caches: "全局缓存",
    cache_column: "缓存",
    group_column: "分组",
    cache_group_dependency: "依赖",
    cache_group_toolchain: "工具链",
    cache_redownload: "清理后需重新下载",
    cache_cargo_note: "清理会影响本机 Rust 构建",
    go_build_cache: "Go 构建缓存",
    go_module_cache: "Go 模块缓存",
    go_module_note: "需重新下载依赖；离线构建可能失败；缓存内手工修改会丢失",
    go_build_note: "清理后，后续构建会变慢。",
    go_stop_tasks: "清理前请停止 Go 构建、测试和依赖下载；并发任务可能导致失败或立即重建缓存。",
    go_cache_changed: "缓存位置已变化，本次清理已取消。请等待重新测量后再次确认。",
    go_cache_querying: "正在查询 Go 缓存位置…",
    go_unavailable: "无法确定缓存位置：未找到 Go",
    go_query_failed: "无法确定缓存位置：Go 查询失败",
    go_timeout: "无法确定缓存位置：Go 查询超时",
    go_invalid_output: "无法确定缓存位置：Go 返回值无效",
    go_unreadable: "无法读取缓存路径或测量体积",
    go_disabled: "缓存未启用",
    go_missing: "缓存目录不存在",
    go_not_directory: "安全检查拒绝：缓存路径不是目录",
    go_protected: "安全检查拒绝：受保护目录",
    go_overlap: "安全检查拒绝：与项目或其他缓存重叠",
    go_linked: "安全检查拒绝：缓存路径包含链接或重定向",
    all: "全部",
    type_filter: "类型",
    search_hint: "搜索名称或路径 (⌘F)",
    size_column: "大小",
    no_match_title: "没有匹配的项目",
    no_match_body: "换一个关键词或类型过滤再试一次。",
    clear_filters: "清空筛选",
    settings: "设置",
    minimize: "最小化",
    maximize: "最大化",
    restore: "还原",
    language: "语言",
    theme: "主题",
    system: "跟随系统",
    dark: "深色",
    light: "浅色",
    lang_zh: "中文",
    lang_en: "English",
    delete_mode: "删除方式",
    delete_trash: "移入废纸篓",
    delete_direct: "直接删除",
    delete_note: "废纸篓可恢复；直接删除不可恢复。",
    rules_file: "规则文件",
    open_rules: "打开规则文件",
    rules_error: "规则文件有误：",
    rules_restart: "修改规则后需重启生效",
    root_covered: "已被父根覆盖",
    root_duplicate: "这个目录已在列表中",
};

static EN: Strings = Strings {
    roots: "Scan roots",
    add_directory: "Add directory",
    remove: "Remove",
    projects: "Projects",
    empty_title: "No scan roots yet",
    empty_body: "Add a directory and DevSweep will find the Flutter, Android, iOS, Java and Node projects inside it.",
    no_projects_title: "No projects found",
    no_projects_body: "No Flutter, Android, iOS, Java or Node projects were found in this directory.",
    measuring: "Measuring sizes",
    project_column: "Project",
    stack_column: "Type",
    clean: "Clean",
    more: "More",
    open_folder: "Open folder",
    open_terminal: "Open in terminal",
    dangerous: "Dangerous",
    items: "items",
    total: "Total",
    skipped: "Rejected",
    skip_outside_project: "Outside the project",
    skip_project_root: "Project root",
    skip_home: "Home directory",
    skip_scan_root: "Scan root",
    skip_git: ".git directory",
    skip_outside_rules: "Not covered by a rule",
    skip_missing: "Missing",
    skip_unresolvable: "Unresolvable path",
    skip_overlap: "Overlaps another target",
    confirm_title: "Confirm cleanup",
    confirm: "Clean now",
    cancel: "Cancel",
    large_warning: "Items above 5 GB:",
    large_ack: "I understand these large items will be deleted",
    cleaning: "Cleaning",
    abort: "Stop",
    aborted: "Stopped",
    freed: "Freed",
    failures: "Failures",
    close: "Close",
    done: "Done",
    fail_permission: "Permission denied",
    fail_in_use: "In use",
    fail_missing: "Missing",
    fail_other: "Other error",
    stack_flutter: "Flutter",
    stack_android: "Android",
    stack_ios: "iOS",
    stack_java: "Java",
    stack_node: "Node",
    stack_python: "Python",
    stack_rust: "Rust",
    unity_supported: "meets the Unity 2022.3+ version range; cleanup remains unavailable.",
    unity_unsupported: "below the Unity 2022.3 support floor; this does not mean all earlier versions are unsafe.",
    unity_invalid: "Malformed version; cannot confirm the Unity 2022.3+ version range.",
    unity_unknown: "Unknown version; cannot confirm the Unity 2022.3+ version range.",
    unity_read_only: "Unity is read-only; cleanup is not available yet.",
    stack_dotnet: ".NET",
    dotnet_clean_note: "Deletes each selected bin/obj directory in full, including files you added. Stop builds first.",
    global_caches: "Global caches",
    cache_column: "Cache",
    group_column: "Group",
    cache_group_dependency: "Dependency",
    cache_group_toolchain: "Toolchain",
    cache_redownload: "Downloads again next time",
    cache_cargo_note: "Affects Rust builds on this machine",
    go_build_cache: "Go build cache",
    go_module_cache: "Go module cache",
    go_module_note: "Dependencies must be downloaded again; offline builds may fail; manual edits inside the cache will be lost",
    go_build_note: "Subsequent builds will be slower after cleaning.",
    go_stop_tasks: "Stop Go builds, tests and downloads before cleaning; concurrent tasks may cause failure or recreate caches immediately.",
    go_cache_changed: "Cache location changed; cleanup cancelled. Wait for measurement and confirm again.",
    go_cache_querying: "Querying Go cache locations…",
    go_unavailable: "Cache location unknown: Go was not found",
    go_query_failed: "Cache location unknown: Go query failed",
    go_timeout: "Cache location unknown: Go query timed out",
    go_invalid_output: "Cache location unknown: invalid Go output",
    go_unreadable: "Cannot read the cache path or measure its size",
    go_disabled: "Cache is disabled",
    go_missing: "Cache directory does not exist",
    go_not_directory: "Safety check rejected: cache path is not a directory",
    go_protected: "Safety check rejected: protected directory",
    go_overlap: "Safety check rejected: overlaps a project or another cache",
    go_linked: "Safety check rejected: cache path contains a link or redirection",
    all: "All",
    type_filter: "Type",
    search_hint: "Search name or path (⌘F)",
    size_column: "Size",
    no_match_title: "No matching projects",
    no_match_body: "Try another keyword or type filter.",
    clear_filters: "Clear filters",
    settings: "Settings",
    minimize: "Minimize",
    maximize: "Maximize",
    restore: "Restore",
    language: "Language",
    theme: "Theme",
    system: "System",
    dark: "Dark",
    light: "Light",
    lang_zh: "中文",
    lang_en: "English",
    delete_mode: "Delete method",
    delete_trash: "Move to Trash",
    delete_direct: "Delete permanently",
    delete_note: "Trash is recoverable; a permanent delete cannot be undone.",
    rules_file: "Rules file",
    open_rules: "Open rules file",
    rules_error: "Rules file has problems:",
    rules_restart: "Restart to apply rule changes",
    root_covered: "Covered by a parent root",
    root_duplicate: "This directory is already listed",
};

#[cfg(test)]
mod tests {
    use super::*;
    use devsweep::scan::UnityVersion;

    #[test]
    fn unity_messages_explain_read_only_and_version_qualification_in_both_languages() {
        let zh = Lang::Zh.strings();
        let en = Lang::En.strings();
        assert_eq!(zh.stack(Stack::Unity), "Unity");
        assert_eq!(en.stack(Stack::Unity), "Unity");
        for (strings, read_only, supported, unsupported, invalid, unknown) in [
            (zh, "只读", "符合", "低于", "损坏", "未知"),
            (en, "read-only", "meets", "below", "Malformed", "Unknown"),
        ] {
            for (version, expected) in [
                (UnityVersion::Supported("6000.0.1f1".into()), supported),
                (UnityVersion::Unsupported("2021.3.0f1".into()), unsupported),
                (UnityVersion::Invalid, invalid),
                (UnityVersion::Unknown, unknown),
            ] {
                let note = strings.unity_note(&version);
                assert!(note.contains(read_only), "{note}");
                assert!(note.contains(expected), "{note}");
                assert!(note.contains("2022.3"), "{note}");
            }
        }
    }
}
