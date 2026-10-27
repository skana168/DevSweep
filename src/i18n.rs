use devsweep::delete::FailureReason;
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
    pub no_targets: &'static str,
    pub unchecked: &'static str,
    pub clean: &'static str,
    pub items: &'static str,
    pub preview: &'static str,
    pub preview_total: &'static str,
    pub skipped: &'static str,
    pub skip_outside_project: &'static str,
    pub skip_project_root: &'static str,
    pub skip_home: &'static str,
    pub skip_scan_root: &'static str,
    pub skip_git: &'static str,
    pub skip_outside_rules: &'static str,
    pub skip_missing: &'static str,
    pub skip_not_a_directory: &'static str,
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
    pub fail_permission: &'static str,
    pub fail_in_use: &'static str,
    pub fail_missing: &'static str,
    pub fail_other: &'static str,
    pub stack_flutter: &'static str,
    pub stack_android: &'static str,
    pub stack_ios: &'static str,
    pub stack_node: &'static str,
}

impl Strings {
    pub fn stack(&self, stack: Stack) -> &'static str {
        match stack {
            Stack::Flutter => self.stack_flutter,
            Stack::Android => self.stack_android,
            Stack::Ios => self.stack_ios,
            Stack::Node => self.stack_node,
        }
    }

    pub fn skip_reason(&self, reason: SkipReason) -> &'static str {
        match reason {
            SkipReason::OutsideProject => self.skip_outside_project,
            SkipReason::ProjectRoot => self.skip_project_root,
            SkipReason::Home => self.skip_home,
            SkipReason::ScanRoot => self.skip_scan_root,
            SkipReason::Git => self.skip_git,
            SkipReason::OutsideRules => self.skip_outside_rules,
            SkipReason::Missing => self.skip_missing,
            SkipReason::NotADirectory => self.skip_not_a_directory,
            SkipReason::Overlap => self.skip_overlap,
        }
    }

    pub fn failure_reason(&self, reason: FailureReason) -> &'static str {
        match reason {
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
    empty_body: "添加一个目录，DevSweep 会找出其中的 Flutter、Android、iOS 和 Node 项目。",
    no_projects_title: "没有找到项目",
    no_projects_body: "这个目录下没有识别到 Flutter、Android、iOS 或 Node 项目。",
    measuring: "体积计算中",
    no_targets: "无可清理项",
    unchecked: "未勾选",
    clean: "清理",
    items: "项",
    preview: "预览",
    preview_total: "合计",
    skipped: "被拒绝",
    skip_outside_project: "超出项目范围",
    skip_project_root: "项目根目录",
    skip_home: "用户主目录",
    skip_scan_root: "扫描根",
    skip_git: ".git 目录",
    skip_outside_rules: "规则外路径",
    skip_missing: "不存在",
    skip_not_a_directory: "不是目录",
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
    fail_permission: "权限不足",
    fail_in_use: "被占用",
    fail_missing: "不存在",
    fail_other: "其他错误",
    stack_flutter: "Flutter",
    stack_android: "Android",
    stack_ios: "iOS",
    stack_node: "Node",
};

static EN: Strings = Strings {
    roots: "Scan roots",
    add_directory: "Add directory",
    remove: "Remove",
    projects: "Projects",
    empty_title: "No scan roots yet",
    empty_body: "Add a directory and DevSweep will find the Flutter, Android, iOS and Node projects inside it.",
    no_projects_title: "No projects found",
    no_projects_body: "No Flutter, Android, iOS or Node projects were found in this directory.",
    measuring: "Measuring sizes",
    no_targets: "Nothing to clean",
    unchecked: "Unchecked",
    clean: "Clean",
    items: "items",
    preview: "Preview",
    preview_total: "Total",
    skipped: "Rejected",
    skip_outside_project: "Outside the project",
    skip_project_root: "Project root",
    skip_home: "Home directory",
    skip_scan_root: "Scan root",
    skip_git: ".git directory",
    skip_outside_rules: "Not covered by a rule",
    skip_missing: "Missing",
    skip_not_a_directory: "Not a directory",
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
    fail_permission: "Permission denied",
    fail_in_use: "In use",
    fail_missing: "Missing",
    fail_other: "Other error",
    stack_flutter: "Flutter",
    stack_android: "Android",
    stack_ios: "iOS",
    stack_node: "Node",
};
