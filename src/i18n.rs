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
    stack_flutter: "Flutter",
    stack_android: "Android",
    stack_ios: "iOS",
    stack_node: "Node",
};
