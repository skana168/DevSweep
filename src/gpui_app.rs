#[path = "gpui_app/developer_tools.rs"]
mod developer_tools;
use crate::gpui_theme::*;
use developer_tools::{TOOLS, primary_cache};
use devsweep::{
    cache,
    config::{self, Config},
    delete::{self, DeleteEvent},
    go_cache,
    plan::{self, DeletePlan, PlanProject},
    projects::ProjectRefresh,
    rules,
    scan::Stack,
    size,
};
use gpui_kit::{
    component::{
        Disableable, WindowExt,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu, PopupMenuItem},
    },
    *,
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

// Material Symbols code points from egui_icons; loaded as a GPUI font.
const BROOM: &str = egui_icons::icons::ICON_CLEANING_SERVICES.codepoint;
const FOLDER: &str = "\u{e2c7}";
const DOWN: &str = "\u{e5cf}";
const UP: &str = "\u{e5ce}";
const NAV: [(&str, &str); 9] = [
    ("深度清理", BROOM),
    ("大文件清理", "\u{e873}"),
    ("重复文件清理", "\u{e14d}"),
    ("磁盘空间分析", "\u{e6c4}"),
    ("隐私清理", "\u{e9e0}"),
    ("应用卸载", "\u{f720}"),
    ("启动项管理", "\u{e8ac}"),
    ("系统优化", "\u{eb9b}"),
    ("系统维护", "\u{e869}"),
];
const CATEGORIES_DATA: [(&str, &str, &str); 9] = [
    ("系统缓存", "\u{f20e}", "21.2 GB · 1842 项"),
    ("用户缓存文件", "\u{e2cc}", "34.4 GB · 20 项"),
    ("应用缓存", "\u{e1b0}", "512 MB · 6 项"),
    ("浏览器数据", "\u{e051}", "2.84 GB · 318 项"),
    ("应用优化", "\u{e322}", "3.23 GB · 77 项"),
    ("AI 模型与缓存", "\u{e80c}", "3.73 GB · 3 项"),
    ("开发工具", "\u{eb8e}", "15.2 GB · 11 项"),
    ("Xcode 数据", "\u{e86f}", "27.7 GB · 3 项"),
    ("项目构建产物", "\u{e1bd}", "63 GB · 8 项"),
];
#[derive(Clone)]
struct Group {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    stack: Option<Stack>,
    demo_size: u64,
    demo_path: &'static str,
}
fn groups() -> Vec<Group> {
    [
        (
            "rust",
            "Rust",
            "Cargo target 目录与跨平台编译缓存",
            Some(Stack::Rust),
            45_000_000_000,
            "/Users/demo/Projects/Desktop App/target",
        ),
        (
            "node",
            "Node.js",
            "项目依赖目录与前端框架构建缓存",
            Some(Stack::Node),
            16_600_000_000,
            "/Users/demo/Projects/Web App/node_modules",
        ),
        (
            "gradle",
            "Gradle",
            "项目构建输出和项目级 Gradle 缓存",
            Some(Stack::Android),
            811_000_000,
            "/Users/demo/Projects/Android App/.gradle",
        ),
        (
            "godot",
            "Godot",
            "Godot 生成的导入资源和编辑器缓存",
            None,
            333_000_000,
            "",
        ),
        (
            "swift",
            "Swift",
            "Swift Package Manager 生成的 .build 目录",
            None,
            117_000_000,
            "",
        ),
        (
            "python",
            "Python",
            "Python 项目的测试、类型检查与字节码缓存",
            Some(Stack::Python),
            116_000_000,
            "",
        ),
        (
            "dotnet",
            ".NET",
            ".NET 项目生成的 bin 与 obj 编译目录",
            Some(Stack::Dotnet),
            12_700_000,
            "",
        ),
        (
            "cmake",
            "CMake",
            "CMake 项目常见的独立构建目录",
            None,
            188_000,
            "",
        ),
        (
            "flutter",
            "Flutter",
            "Flutter 项目的构建与依赖产物",
            Some(Stack::Flutter),
            0,
            "",
        ),
        ("ios", "iOS", "iOS 项目的构建产物", Some(Stack::Ios), 0, ""),
        (
            "java",
            "Java",
            "Java 项目的构建产物",
            Some(Stack::Java),
            0,
            "",
        ),
        (
            "unity",
            "Unity",
            "Unity 项目的可重建产物",
            Some(Stack::Unity),
            0,
            "",
        ),
    ]
    .into_iter()
    .map(
        |(id, name, description, stack, demo_size, demo_path)| Group {
            id,
            name,
            description,
            stack,
            demo_size,
            demo_path,
        },
    )
    .collect()
}
#[derive(Clone)]
struct CacheRow {
    entry: cache::CacheEntry,
    bytes: u64,
    checked: bool,
}
enum ScanResult {
    Projects(ProjectRefresh),
    Disk(Option<(u64, u64)>),
    Cache(Vec<CacheRow>, go_cache::GoCacheSnapshot),
}
pub struct SweepWindow {
    config: Config,
    projects: ProjectRefresh,
    caches: Vec<CacheRow>,
    go_snapshot: Option<go_cache::GoCacheSnapshot>,
    rules: Vec<rules::Rule>,
    demo: bool,
    disk: Option<(u64, u64)>,
    collapsed: bool,
    category: usize,
    expanded: HashSet<String>,
    scan: Option<mpsc::Receiver<ScanResult>>,
    execution: Option<mpsc::Receiver<DeleteEvent>>,
    abort: Arc<AtomicBool>,
    status: String,
    settings: bool,
    search: Entity<InputState>,
    show_search: bool,
    _search_subscription: Subscription,
    root_picker_pending: bool,
    demo_tool_selection: HashSet<String>,
    other_tools_open: bool,
}
impl SweepWindow {
    pub fn new(config: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let demo_tools = std::env::args().any(|arg| arg == "--demo-tools");
        let mut view = Self::new_with_mode(
            config,
            demo_tools || std::env::args().any(|arg| arg == "--demo"),
            window,
            cx,
        );
        if demo_tools {
            view.category = 6;
        }
        view
    }
    pub fn new_with_mode(
        config: Config,
        demo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let loaded = rules::load(config::rules_path().as_deref());
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索项目名称或路径"));
        let subscription =
            cx.subscribe_in(&search, window, |_, _, _: &InputEvent, _, cx| cx.notify());
        let mut this = Self {
            config,
            projects: ProjectRefresh::default(),
            caches: vec![],
            go_snapshot: None,
            rules: loaded.rules,
            demo,
            disk: None,
            collapsed: false,
            category: 8,
            expanded: ["rust", "node", "gradle"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            scan: None,
            execution: None,
            abort: Arc::default(),
            status: loaded.error.map_or(String::new(), |e| {
                format!("规则文件读取失败，使用内置规则：{e}")
            }),
            settings: false,
            search,
            show_search: false,
            _search_subscription: subscription,
            root_picker_pending: false,
            demo_tool_selection: developer_tools::initial_selection(),
            other_tools_open: false,
        };
        cx.observe_window_bounds(window, |this, window, _| {
            let bounds = window.bounds().size;
            this.config.window = Some(config::Window {
                width: bounds.width.into(),
                height: bounds.height.into(),
            });
            if !this.demo {
                config::save(&this.config);
            }
        })
        .detach();
        if !demo {
            this.refresh(cx);
        }
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if weak.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        this
    }
    fn locked(&self) -> bool {
        self.scan.is_some() || self.execution.is_some() || self.root_picker_pending
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.demo || self.locked() {
            return;
        }
        let roots = self.config.roots.clone();
        let rules = self.rules.clone();
        let mut projects = self.projects.clone();
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.status = "正在发现项目…".into();
        std::thread::spawn(move || {
            let _ = tx.send(ScanResult::Disk(disk_space()));
            projects.refresh(&roots, &rules);
            let (size_tx, size_rx) = mpsc::channel();
            size::measure(projects.measurement_requests(), size_tx);
            for event in size_rx {
                projects.apply_measurement(&event);
            }
            let project_paths = projects
                .entries()
                .iter()
                .map(|e| e.project.path.clone())
                .collect();
            let _ = tx.send(ScanResult::Projects(projects));
            let entries = cache::entries();
            let context = go_cache::GoCacheContext {
                home: directories::BaseDirs::new()
                    .map(|b| b.home_dir().to_path_buf())
                    .unwrap_or_default(),
                scan_roots: roots,
                projects: project_paths,
                other_caches: entries.iter().filter_map(|e| e.path.clone()).collect(),
            };
            let snapshot = go_cache::discover(&context);
            let caches = entries
                .into_iter()
                .map(|mut entry| {
                    let bytes = if let Some(go) = snapshot.entry(entry.rule.id) {
                        entry.path = go.path.clone();
                        go.size.unwrap_or(0)
                    } else {
                        entry.path.as_ref().map_or(0, |p| size::dir_size(p))
                    };
                    CacheRow {
                        entry,
                        bytes,
                        checked: false,
                    }
                })
                .collect();
            let _ = tx.send(ScanResult::Cache(caches, snapshot));
        });
        cx.notify();
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(rx) = &self.scan {
            let mut disconnected = false;
            loop {
                let event = match rx.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                };
                match event {
                    ScanResult::Disk(disk) => self.disk = disk,
                    ScanResult::Projects(p) => {
                        let measurements: Vec<_> = p
                            .entries()
                            .iter()
                            .map(|e| size::SizeEvent {
                                project: e.project.path.clone(),
                                sizes: e
                                    .targets
                                    .iter()
                                    .filter_map(|t| t.size.map(|b| (t.path.clone(), b)))
                                    .collect(),
                            })
                            .collect();
                        self.projects.apply_refresh(p);
                        for event in measurements {
                            self.projects.apply_measurement(&event);
                        }
                        self.status = "正在测量全局缓存…".into();
                    }
                    ScanResult::Cache(mut c, snapshot) => {
                        for row in &mut c {
                            if let Some(old) = self
                                .caches
                                .iter()
                                .find(|old| old.entry.rule.id == row.entry.rule.id)
                            {
                                row.checked = old.checked;
                            } else {
                                row.checked = primary_cache(row.entry.rule.id)
                                    && row.bytes > 0
                                    && !row.entry.rule.dangerous;
                            }
                        }
                        self.go_snapshot = Some(snapshot);
                        self.caches = c;
                        self.status = "扫描完成".into();
                    }
                }
                changed = true;
            }
            if self.status == "扫描完成" || disconnected {
                if disconnected && self.status != "扫描完成" {
                    self.status = "扫描未完成，请重新扫描。".into();
                }
                self.scan = None;
                changed = true;
            }
        }
        if let Some(rx) = &self.execution {
            let mut done = false;
            while let Ok(event) = rx.try_recv() {
                match event {
                    DeleteEvent::Started { project, .. } => {
                        self.status = format!("正在清理 {}", project.display())
                    }
                    DeleteEvent::ProjectDone { project, deleted } => {
                        if let Some(targets) = self.projects.targets_mut(&project) {
                            for t in targets {
                                if deleted.contains(&t.path) {
                                    t.size = Some(0);
                                    t.checked = false;
                                }
                            }
                        }
                        for c in &mut self.caches {
                            if c.entry.path.as_ref().is_some_and(|p| deleted.contains(p)) {
                                c.bytes = 0;
                                c.checked = false;
                            }
                        }
                    }
                    DeleteEvent::Finished(report) => {
                        self.status = format!(
                            "{}：已释放 {}，失败 {} 项",
                            if report.aborted {
                                "已中止"
                            } else {
                                "清理完成"
                            },
                            bytes(report.freed_bytes),
                            report.failures.len()
                        );
                        if !report.failures.is_empty() {
                            self.status.push_str(&format!("；{:?}", report.failures));
                        }
                        done = true;
                    }
                }
                changed = true;
            }
            if done {
                self.execution = None;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
    fn notice(
        window: &mut Window,
        cx: &mut App,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
    ) {
        let title = title.into();
        let message = message.into();
        window.open_dialog(cx, move |d, _, _| {
            d.title(title.clone()).child(message.clone())
        });
    }
    fn unavailable(name: &str, window: &mut Window, cx: &mut App) {
        Self::notice(
            window,
            cx,
            name.to_owned(),
            format!("“{name}”暂未实现，敬请期待。"),
        );
    }
    fn total(&self) -> u64 {
        self.projects
            .entries()
            .iter()
            .flat_map(|e| &e.targets)
            .map(|t| t.size.unwrap_or(0))
            .sum()
    }
    fn cache_in_scope(&self, c: &CacheRow) -> bool {
        if self.category == 7 {
            c.entry.rule.id == "xcode.derived_data"
        } else {
            c.entry.rule.id != "xcode.derived_data"
        }
    }
    fn selected(&self) -> (usize, u64) {
        if self.category == 6 || self.category == 7 {
            self.caches
                .iter()
                .filter(|c| c.checked && self.cache_in_scope(c))
                .fold((0, 0), |(n, b), c| (n + 1, b + c.bytes))
        } else {
            self.projects
                .entries()
                .iter()
                .flat_map(|e| &e.targets)
                .filter(|t| t.checked)
                .fold((0, 0), |(n, b), t| (n + 1, b + t.size.unwrap_or(0)))
        }
    }
    fn toggle_group(&mut self, stack: Stack, value: bool) {
        let paths: Vec<_> = self
            .projects
            .entries()
            .iter()
            .filter(|e| e.project.stack == stack)
            .map(|e| e.project.path.clone())
            .collect();
        for p in paths {
            if let Some(ts) = self.projects.targets_mut(&p) {
                for t in ts {
                    t.checked = value;
                }
            }
        }
    }
    fn go_context(&self) -> go_cache::GoCacheContext {
        go_cache::GoCacheContext {
            home: directories::BaseDirs::new()
                .map(|b| b.home_dir().to_path_buf())
                .unwrap_or_default(),
            scan_roots: self.config.roots.clone(),
            projects: self
                .projects
                .entries()
                .iter()
                .map(|e| e.project.path.clone())
                .collect(),
            other_caches: self
                .caches
                .iter()
                .filter(|c| !go_cache::is_go_cache(c.entry.rule.id))
                .filter_map(|c| c.entry.path.clone())
                .collect(),
        }
    }
    fn build_plan(&self) -> DeletePlan {
        let projects: Vec<_> = self
            .projects
            .entries()
            .iter()
            .map(|e| PlanProject {
                path: &e.project.path,
                stack: e.project.stack,
                targets: &e.targets,
            })
            .collect();
        let mut result = plan::plan(&projects, &self.config.roots, &self.rules);
        for c in self.caches.iter().filter(|c| c.checked) {
            let plan = if go_cache::is_go_cache(c.entry.rule.id) {
                self.go_snapshot
                    .as_ref()
                    .and_then(|s| s.plan(c.entry.rule.id, &self.go_context()).ok())
            } else {
                cache::plan(c.entry.rule.id, c.bytes)
            };
            if let Some(plan) = plan {
                result.items.extend(plan.items);
                result.skipped.extend(plan.skipped);
            }
        }
        // A selected project output can overlap a global tool directory. Keep one owner.
        result
            .items
            .sort_by(|a, b| a.target_path.cmp(&b.target_path));
        let mut paths: Vec<std::path::PathBuf> = Vec::new();
        let skipped = &mut result.skipped;
        result.items.retain(|item| {
            let path = std::fs::canonicalize(&item.target_path)
                .unwrap_or_else(|_| item.target_path.clone());
            if paths.iter().any(|parent| path.starts_with(parent)) {
                skipped.push(plan::Skip {
                    path: item.target_path.clone(),
                    reason: plan::SkipReason::Overlap,
                });
                false
            } else {
                paths.push(path);
                true
            }
        });
        result.total_bytes = result.items.iter().map(|i| i.bytes).sum();
        result
    }
    fn global_selected(&self) -> (usize, u64) {
        let projects = self
            .projects
            .entries()
            .iter()
            .flat_map(|e| &e.targets)
            .filter(|t| t.checked)
            .fold((0, 0), |(n, b), t| (n + 1, b + t.size.unwrap_or(0)));
        self.caches
            .iter()
            .filter(|c| c.checked)
            .fold(projects, |(n, b), c| (n + 1, b + c.bytes))
    }
    fn demo_tools_selected(&self) -> (usize, u64) {
        let selected = TOOLS
            .iter()
            .filter(|t| self.demo_tool_selection.contains(t.id));
        let count = selected.clone().count();
        // Reference totals are display fixtures, not authorization or measured bytes.
        let bytes = if count == TOOLS.len() {
            15_200_000_000
        } else if self.demo_tool_selection == developer_tools::initial_selection() {
            7_690_000_000
        } else {
            selected.map(|t| t.demo_bytes).sum()
        };
        (count, bytes)
    }
    fn set_category_selection(&mut self, value: bool) {
        if self.demo {
            if self.category == 6 {
                self.demo_tool_selection = if value {
                    TOOLS.iter().map(|t| t.id.to_owned()).collect()
                } else {
                    HashSet::new()
                };
            }
            return;
        }
        if self.category == 8 {
            let paths: Vec<_> = self
                .projects
                .entries()
                .iter()
                .map(|e| e.project.path.clone())
                .collect();
            for path in paths {
                if let Some(targets) = self.projects.targets_mut(&path) {
                    for t in targets {
                        t.checked = value;
                    }
                }
            }
        } else {
            let category = self.category;
            for c in &mut self.caches {
                if (category == 7) == (c.entry.rule.id == "xcode.derived_data") {
                    c.checked = value && c.bytes > 0;
                }
            }
        }
    }
    fn clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo {
            Self::notice(
                window,
                cx,
                "样例模式",
                "这里展示参考图样例数据，不会执行清理。",
            );
            return;
        }
        if self.locked() {
            return;
        }
        if self
            .projects
            .entries()
            .iter()
            .flat_map(|e| &e.targets)
            .any(|t| t.checked && t.size.is_none())
        {
            Self::notice(window, cx, "等待测量", "所选清理项仍在测量，请稍后重试。");
            return;
        }
        if self
            .caches
            .iter()
            .any(|c| c.checked && go_cache::is_go_cache(c.entry.rule.id))
            && self.global_selected().0 > 1
        {
            Self::notice(
                window,
                cx,
                "单独清理 Go 缓存",
                "Go 缓存需要工具链复查，请每次只选择一项 Go 缓存。",
            );
            return;
        }
        let plan = self.build_plan();
        if plan.items.is_empty() {
            Self::notice(
                window,
                cx,
                "没有可清理项目",
                "请先选择清理项；所选路径可能已不存在或未通过安全检查。",
            );
            return;
        }
        let allowed: HashSet<_> = plan.items.iter().map(|i| i.target_path.clone()).collect();
        let message = format!(
            "将{} {} 项，预计释放 {}。\n危险项可能需要重新下载依赖或重建产物。\n跳过 {} 项。\n\n{}",
            if self.config.delete_mode == delete::DeleteMode::Trash {
                "移入废纸篓"
            } else {
                "永久删除（不可恢复）"
            },
            plan.items.len(),
            bytes(plan.total_bytes),
            plan.skipped.len(),
            plan.items
                .iter()
                .map(|i| i.target_path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |d, _, _| {
            let weak = weak.clone();
            let allowed = allowed.clone();
            d.title("确认清理所选项目")
                .child(
                    div()
                        .id("cleanup-preview")
                        .max_h(px(360.))
                        .overflow_y_scroll()
                        .child(message.clone()),
                )
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .show_cancel(true)
                        .ok_text("清理")
                        .cancel_text("取消"),
                )
                .on_ok(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.locked() {
                            return;
                        }
                        let mut plan = this.build_plan();
                        plan.items.retain(|i| allowed.contains(&i.target_path));
                        plan.total_bytes = plan.items.iter().map(|i| i.bytes).sum();
                        let go = this
                            .caches
                            .iter()
                            .find(|c| c.checked && go_cache::is_go_cache(c.entry.rule.id))
                            .and_then(|c| {
                                this.go_snapshot
                                    .clone()
                                    .map(|s| (s, c.entry.rule.id, this.go_context()))
                            });
                        let mode = this.config.delete_mode;
                        let abort = Arc::new(AtomicBool::new(false));
                        this.abort = abort.clone();
                        let (tx, rx) = mpsc::channel();
                        this.execution = Some(rx);
                        this.status = "正在复查并执行清理…".into();
                        std::thread::spawn(move || {
                            if let Some((snapshot, id, context)) = go {
                                let (check_tx, _) = mpsc::channel();
                                go_cache::GoCacheClean {
                                    confirmed: &snapshot,
                                    id,
                                    context: &context,
                                }
                                .execute(mode, &abort, tx, check_tx);
                            } else {
                                delete::execute(plan, mode, &abort, tx);
                            }
                        });
                        cx.notify();
                    });
                    true
                })
        });
    }
    fn add_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked() || self.demo {
            return;
        }
        self.prompt_for_root(window, cx);
    }
    fn prompt_for_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.root_picker_pending {
            return;
        }
        // Synchronous modal loops re-enter GPUI while its App is mutably borrowed.
        // Await the platform picker only after this event handler has returned.
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("添加扫描根".into()),
        });
        self.root_picker_pending = true;
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            let selected = selected.await;
            let _ = cx.update(|window, cx| {
                weak.update(cx, |this, cx| {
                    this.root_picker_pending = false;
                    match selected {
                        Ok(Ok(Some(paths))) => {
                            if let Some(path) = paths.into_iter().next() {
                                if this.config.roots.contains(&path) {
                                    this.status = "此扫描根已添加".into();
                                } else {
                                    this.config.roots.push(path);
                                    if !this.demo {
                                        config::save(&this.config);
                                    }
                                    this.refresh(cx);
                                }
                            }
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => {
                            Self::notice(window, cx, "无法打开目录选择框", error.to_string())
                        }
                        Err(error) => Self::notice(window, cx, "目录选择未完成", error.to_string()),
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }
    fn nav(&self, cx: &mut Context<Self>) -> Div {
        let p = palette();
        let mut content = column()
            .w(px(if self.collapsed { 64. } else { SIDEBAR }))
            .flex_shrink_0()
            .h_full()
            .bg(p.sidebar)
            .pt(px(45.))
            .px(px(12.))
            .pb(px(14.));
        for (ix, (name, glyph)) in NAV.iter().enumerate() {
            if [0, 4, 5].contains(&ix) && !self.collapsed {
                content = content.child(
                    text(
                        match ix {
                            0 => "存储空间",
                            4 => "隐私与安全",
                            _ => "系统工具",
                        },
                        11.,
                    )
                    .text_color(p.muted)
                    .mt(px(if ix == 0 { 2. } else { 18. }))
                    .mb(px(6.))
                    .pl(px(12.)),
                );
            }
            let name = *name;
            let glyph = *glyph;
            content = content.child(
                div()
                    .relative()
                    .child(
                        Button::new(SharedString::from(format!("nav-{ix}")))
                            .ghost()
                            .accessibility_label(name)
                            .w_full()
                            .h(px(46.))
                            .justify_start()
                            .font_weight(if ix == 0 {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if ix == 0 { p.accent } else { p.foreground })
                            .bg(if ix == 0 { p.selection } else { p.sidebar })
                            .rounded(px(14.))
                            .child(
                                row()
                                    .w_full()
                                    .gap(px(14.))
                                    .child(icon(glyph, 25.))
                                    .children((!self.collapsed).then(|| text(name, 17.))),
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if ix == 0 {
                                    this.settings = false;
                                    cx.notify();
                                } else {
                                    Self::unavailable(name, w, cx);
                                }
                            })),
                    )
                    .children((ix == 0).then(|| selection_marker(28.))),
            );
        }
        content.child(div().flex_1()).children(
            [
                ("操作历史", "\u{e889}"),
                ("设置", "\u{e8b8}"),
                (
                    if self.collapsed {
                        "展开侧边栏"
                    } else {
                        "收起侧边栏"
                    },
                    "\u{e9b0}",
                ),
            ]
            .into_iter()
            .enumerate()
            .map(|(ix, (name, glyph))| {
                Button::new(SharedString::from(format!("nav-bottom-{ix}")))
                    .accessibility_label(name)
                    .font_weight(FontWeight::NORMAL)
                    .ghost()
                    .w_full()
                    .h(px(48.))
                    .justify_start()
                    .child(
                        row()
                            .w_full()
                            .gap(px(14.))
                            .child(icon(glyph, 25.))
                            .children((!self.collapsed).then(|| text(name, 16.))),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| match ix {
                        0 => Self::unavailable(name, w, cx),
                        1 => {
                            this.settings = !this.settings;
                            cx.notify();
                        }
                        _ => {
                            this.collapsed = !this.collapsed;
                            cx.notify();
                        }
                    }))
            }),
        )
    }
    fn categories(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = palette();
        let mut panel = column()
            .id("categories")
            .w(px(CATEGORIES))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .p(px(7.))
            .border_r_1()
            .border_color(p.border);
        for (ix, (name, glyph, demo_meta)) in CATEGORIES_DATA.iter().enumerate() {
            let (name, glyph) = (*name, *glyph);
            let meta = if self.demo {
                demo_meta.to_string()
            } else {
                match ix {
                    8 => format!(
                        "{} · {} 项",
                        bytes(self.total()),
                        self.projects
                            .entries()
                            .iter()
                            .map(|e| e.targets.len())
                            .sum::<usize>()
                    ),
                    6 | 7 => format!(
                        "{} · 全局缓存",
                        bytes(
                            self.caches
                                .iter()
                                .filter(|c| (ix == 7) == (c.entry.rule.id == "xcode.derived_data"))
                                .map(|c| c.bytes)
                                .sum()
                        )
                    ),
                    _ => "暂未实现".into(),
                }
            };
            panel = panel.child(
                div()
                    .relative()
                    .child(
                        Button::new(SharedString::from(format!("category-{ix}")))
                            .ghost()
                            .w_full()
                            .h(px(61.))
                            .justify_start()
                            .rounded(px(14.))
                            .bg(if ix == self.category {
                                p.category
                            } else {
                                p.surface
                            })
                            .child(
                                row()
                                    .w_full()
                                    .gap(px(20.))
                                    .child(icon(glyph, 19.).text_color(p.muted))
                                    .child(
                                        column()
                                            .flex_1()
                                            .min_w_0()
                                            .items_start()
                                            .gap(px(3.))
                                            .child(
                                                text(name, 16.).font_weight(FontWeight::SEMIBOLD),
                                            )
                                            .child(text(meta, 12.).text_color(p.muted)),
                                    )
                                    .child(div().flex_1())
                                    .children(
                                        (ix == 6 || (self.demo && [0, 1, 2, 3].contains(&ix)))
                                            .then(|| {
                                                text(
                                                    if ix == 6 {
                                                        if self.demo {
                                                            tools_bytes(
                                                                self.demo_tools_selected().1,
                                                            )
                                                        } else {
                                                            tools_bytes(
                                                    self.caches
                                                        .iter()
                                                        .filter(|c| {
                                                            c.checked
                                                                && c.entry.rule.id
                                                                    != "xcode.derived_data"
                                                        })
                                                        .map(|c| c.bytes)
                                                        .sum(),
                                                )
                                                        }
                                                    } else {
                                                        [
                                                            "21.2 GB", "26.3 GB", "512 MB",
                                                            "2.84 GB", "", "", "", "", "",
                                                        ][ix]
                                                            .to_owned()
                                                    },
                                                    12.,
                                                )
                                                .text_color(if ix == 6 && self.category == 6 {
                                                    p.accent
                                                } else {
                                                    p.muted
                                                })
                                            }),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if [6, 7, 8].contains(&ix) {
                                    if !this.locked() {
                                        this.category = ix;
                                        cx.notify();
                                    }
                                } else {
                                    Self::unavailable(name, w, cx);
                                }
                            })),
                    )
                    .children((ix == self.category).then(|| selection_marker(41.))),
            );
        }
        panel
    }
    fn project_results(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = palette();
        let query = self.search.read(cx).value().to_lowercase();
        let mut result = column()
            .id("project-results")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for g in groups() {
            let entries: Vec<_> = self
                .projects
                .entries()
                .iter()
                .filter(|e| {
                    Some(e.project.stack) == g.stack
                        && (query.is_empty()
                            || e.project.name.to_lowercase().contains(&query)
                            || e.project
                                .path
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&query))
                })
                .collect();
            if !self.demo && g.stack.is_some() && entries.is_empty() {
                continue;
            }
            if self.demo && g.demo_size == 0 {
                continue;
            }
            let size = if self.demo {
                g.demo_size
            } else {
                entries
                    .iter()
                    .flat_map(|e| &e.targets)
                    .map(|t| t.size.unwrap_or(0))
                    .sum()
            };
            let opened = self.expanded.contains(g.id);
            let checked =
                !entries.is_empty() && entries.iter().flat_map(|e| &e.targets).all(|t| t.checked);
            let group_id = g.id;
            let name = g.name;
            let stack = g.stack;
            let checkbox = Checkbox::new(SharedString::from(format!("group-check-{}", g.id)))
                .accessibility_label(format!("选择 {name} 构建产物"))
                .checked(checked)
                .disabled(self.locked())
                .on_change(cx.listener(move |this, v, w, cx| {
                    if let Some(stack) = stack {
                        if !this.demo {
                            this.toggle_group(stack, *v);
                            cx.notify();
                        }
                    } else {
                        Self::unavailable(name, w, cx);
                    }
                }));
            result = result.child(
                row()
                    .h(px(ROW))
                    .flex_shrink_0()
                    .px(px(20.))
                    .gap(px(18.))
                    .child(checkbox)
                    .child(brand(g.id))
                    .child(
                        Button::new(SharedString::from(format!("group-open-{}", g.id)))
                            .accessibility_label(format!("展开 {} 构建产物", g.name))
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .justify_start()
                            .child(
                                column()
                                    .w_full()
                                    .items_start()
                                    .gap(px(3.))
                                    .child(
                                        row()
                                            .gap(px(8.))
                                            .child(
                                                text(format!("{} 构建产物", g.name), 15.)
                                                    .font_weight(FontWeight::SEMIBOLD),
                                            )
                                            .child(
                                                text("需确认", 10.)
                                                    .px(px(7.))
                                                    .py(px(2.))
                                                    .rounded(px(12.))
                                                    .bg(p.category)
                                                    .text_color(p.muted),
                                            ),
                                    )
                                    .child(text(g.description, 12.).text_color(p.muted)),
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if stack.is_none() {
                                    Self::unavailable(name, w, cx);
                                } else {
                                    if !this.expanded.remove(group_id) {
                                        this.expanded.insert(group_id.into());
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        column()
                            .w(px(87.))
                            .items_end()
                            .gap(px(9.))
                            .child(
                                text(
                                    if self.demo || g.stack.is_some() {
                                        bytes(size)
                                    } else {
                                        "—".into()
                                    },
                                    14.,
                                )
                                .font_weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                text(
                                    if !self.demo && g.stack.is_none() {
                                        "暂未实现"
                                    } else {
                                        "发现可清理"
                                    },
                                    10.,
                                )
                                .text_color(p.muted),
                            ),
                    )
                    .child(icon(if opened { UP } else { DOWN }, 17.).text_color(p.muted)),
            );
            if opened && self.demo && !g.demo_path.is_empty() {
                result = result.child(
                    row()
                        .h(px(54.))
                        .pl(px(65.))
                        .pr(px(20.))
                        .gap(px(18.))
                        .child(
                            Checkbox::new(SharedString::from(format!("demo-{}", g.id)))
                                .accessibility_label(g.demo_path)
                                .on_change(|_, w, cx| {
                                    Self::notice(w, cx, "样例模式", "样例数据仅用于视觉展示。")
                                }),
                        )
                        .child(icon(FOLDER, 21.).text_color(p.muted))
                        .child(
                            column()
                                .flex_1()
                                .min_w_0()
                                .child(text(g.demo_path, 14.).font_weight(FontWeight::SEMIBOLD))
                                .child(
                                    text("1 个文件 · 最近修改 2026/08/03 10:20", 11.)
                                        .text_color(p.muted),
                                ),
                        )
                        .child(text(bytes(size), 14.)),
                );
            }
            if opened && !self.demo {
                for e in entries {
                    for t in &e.targets {
                        let project = e.project.path.clone();
                        let path = t.path.clone();
                        let label = t.path.display().to_string();
                        result = result.child(
                            row()
                                .min_h(px(54.))
                                .pl(px(65.))
                                .pr(px(20.))
                                .gap(px(18.))
                                .child(
                                    Checkbox::new(SharedString::from(format!(
                                        "target-{}",
                                        t.path.display()
                                    )))
                                    .accessibility_label(format!("选择 {label}"))
                                    .checked(t.checked)
                                    .disabled(self.locked())
                                    .on_change(cx.listener(move |this, value, _, cx| {
                                        if let Some(ts) = this.projects.targets_mut(&project)
                                            && let Some(t) = ts.iter_mut().find(|t| t.path == path)
                                        {
                                            t.checked = *value;
                                        }
                                        cx.notify();
                                    })),
                                )
                                .child(icon(FOLDER, 21.).text_color(p.muted))
                                .child(
                                    column()
                                        .flex_1()
                                        .min_w_0()
                                        .child(text(label, 14.).truncate())
                                        .child(
                                            text(
                                                if t.dangerous {
                                                    "需确认 · 清理后可能需要重新下载或重建"
                                                } else {
                                                    "可重新生成的项目产物"
                                                },
                                                11.,
                                            )
                                            .text_color(p.muted),
                                        ),
                                )
                                .child(text(t.size.map_or("测量中…".into(), bytes), 14.)),
                        );
                    }
                }
            }
        }
        if !self.demo && self.projects.entries().is_empty() {
            result = result.child(
                column()
                    .p(px(24.))
                    .gap(px(12.))
                    .child(text(
                        if self.scan.is_some() {
                            "正在扫描项目…"
                        } else {
                            "尚未发现项目"
                        },
                        18.,
                    ))
                    .child(text("在设置中添加扫描根，然后重新扫描。", 14.).text_color(p.muted))
                    .child(
                        Button::new("empty-add-root")
                            .label("添加扫描根")
                            .disabled(self.locked())
                            .on_click(cx.listener(|this, _, w, cx| this.add_root(w, cx))),
                    ),
            );
        }
        result
    }
    fn cache_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut result = column()
            .id("cache-results")
            .role(Role::List)
            .aria_label("缓存清理类型")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if self.category == 6 {
            for tool in TOOLS {
                result = result.child(self.developer_tool_row(tool, cx));
            }
            if !self.demo {
                let p = palette();
                result = result.child(
                    Button::new("other-tool-caches")
                        .ghost()
                        .w_full()
                        .h(px(48.))
                        .accessibility_label("其他开发工具缓存")
                        .child(
                            row()
                                .w_full()
                                .gap(px(10.))
                                .child(icon(if self.other_tools_open { UP } else { DOWN }, 18.))
                                .child(text("其他开发工具缓存", 14.).text_color(p.muted)),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.other_tools_open = !this.other_tools_open;
                            cx.notify();
                        })),
                );
                if self.other_tools_open {
                    for cache in self
                        .caches
                        .iter()
                        .filter(|c| self.cache_in_scope(c) && !primary_cache(c.entry.rule.id))
                    {
                        result = result.child(self.other_cache_row(cache, cx));
                    }
                }
            }
        } else {
            for cache in self.caches.iter().filter(|c| self.cache_in_scope(c)) {
                result = result.child(self.other_cache_row(cache, cx));
            }
        }
        result.into_any_element()
    }
    fn developer_tool_row(
        &self,
        tool: &'static developer_tools::Tool,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette();
        let cache = tool
            .cache_id
            .and_then(|id| self.caches.iter().find(|c| c.entry.rule.id == id));
        let available = tool.cache_id.is_some();
        let checked = if self.demo {
            self.demo_tool_selection.contains(tool.id)
        } else {
            cache.is_some_and(|c| c.checked)
        };
        let value = if self.demo {
            tool.demo_label.to_owned()
        } else if available {
            cache.map_or("测量中…".into(), |c| tools_bytes(c.bytes))
        } else {
            "—".into()
        };
        let tag = if !self.demo && !available {
            "暂未实现"
        } else if tool.low_risk {
            "低风险"
        } else {
            "需确认"
        };
        let low_risk = tag == "低风险";
        let description = if !self.demo && tool.id == "gradle-build" {
            "Gradle 全局缓存，包含依赖与构建缓存"
        } else {
            tool.description
        };
        let expanded = self.expanded.contains(tool.id);
        let weak = cx.entity().downgrade();
        let id = tool.id;
        let cache_id = tool.cache_id;
        let name = tool.name;
        let check = selection_box(
            SharedString::from(format!("tool-check-{id}")),
            format!("选择 {name}"),
            checked,
            false,
            self.locked() || (!self.demo && available && cache.is_none_or(|c| c.bytes == 0)),
        )
        .on_change(move |state, _, window, cx| {
            let _ = weak.update(cx, |this, cx| {
                let value = state == gpui_kit::base::CheckboxState::Checked;
                if this.demo {
                    if value {
                        this.demo_tool_selection.insert(id.into());
                    } else {
                        this.demo_tool_selection.remove(id);
                    }
                } else if let Some(cache_id) = cache_id {
                    if let Some(cache) =
                        this.caches.iter_mut().find(|c| c.entry.rule.id == cache_id)
                    {
                        cache.checked = value;
                    }
                } else {
                    Self::unavailable(name, window, cx);
                }
                cx.notify();
            });
        });
        let open = cx.listener(move |this: &mut Self, _, window, cx| {
            if !available {
                Self::unavailable(name, window, cx);
            } else {
                if !this.expanded.remove(id) {
                    this.expanded.insert(id.into());
                }
                cx.notify();
            }
        });
        let mut group = column().child(
            row()
                .h(px(ROW))
                .flex_shrink_0()
                .pl(px(20.))
                .pr(px(34.))
                .child(check)
                .child(div().w(px(26.)).flex_shrink_0())
                .child(
                    row()
                        .w(px(24.))
                        .justify_center()
                        .flex_shrink_0()
                        .child(brand(tool.brand)),
                )
                .child(div().w(px(16.)).flex_shrink_0())
                .child(
                    Button::new(SharedString::from(format!("tool-open-{id}")))
                        .accessibility_label(format!("展开 {name}"))
                        .ghost()
                        .p_0()
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .child(
                            column()
                                .w_full()
                                .min_w_0()
                                .items_start()
                                .gap(px(3.))
                                .child(
                                    row()
                                        .gap(px(7.))
                                        .child(text(name, 15.).font_weight(FontWeight::SEMIBOLD))
                                        .child(
                                            text(tag, 10.)
                                                .px(px(7.))
                                                .py(px(3.))
                                                .rounded(px(11.))
                                                .bg(if low_risk {
                                                    p.low_risk_background
                                                } else {
                                                    p.category
                                                })
                                                .text_color(if low_risk {
                                                    p.low_risk_foreground
                                                } else {
                                                    p.muted
                                                }),
                                        ),
                                )
                                .child(
                                    text(description, 12.)
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(p.muted)
                                        .truncate(),
                                ),
                        )
                        .on_click(open),
                )
                .child(
                    column()
                        .w(px(100.))
                        .flex_shrink_0()
                        .items_end()
                        .gap(px(10.))
                        .child(text(value, 14.).font_weight(FontWeight::SEMIBOLD))
                        .child(
                            text(
                                if checked {
                                    "已选"
                                } else if !self.demo && !available {
                                    "暂未实现"
                                } else {
                                    "发现可清理"
                                },
                                10.,
                            )
                            .text_color(p.muted),
                        ),
                )
                .child(div().w(px(8.)).flex_shrink_0())
                .child(
                    Button::new(SharedString::from(format!("tool-disclose-{id}")))
                        .accessibility_label(format!(
                            "{} {name}",
                            if expanded { "收起" } else { "展开" }
                        ))
                        .ghost()
                        .p_0()
                        .w(px(20.))
                        .h(px(32.))
                        .child(icon(if expanded { UP } else { DOWN }, 17.).text_color(p.muted))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if !available {
                                Self::unavailable(name, window, cx);
                            } else {
                                if !this.expanded.remove(id) {
                                    this.expanded.insert(id.into());
                                }
                                cx.notify();
                            }
                        })),
                ),
        );
        if expanded && available {
            let path = if self.demo {
                format!(
                    "/Users/demo/{}",
                    if id == "gradle-build" {
                        ".gradle/caches"
                    } else {
                        ".npm/_cacache"
                    }
                )
            } else {
                cache
                    .and_then(|c| c.entry.path.as_ref())
                    .map_or("未发现缓存路径".into(), |p| p.display().to_string())
            };
            group = group.child(
                column()
                    .id(SharedString::from(format!("tool-details-{id}")))
                    .test_support()
                    .pl(px(106.))
                    .pr(px(40.))
                    .pb(px(14.))
                    .gap(px(5.))
                    .child(
                        row()
                            .gap(px(10.))
                            .child(icon(FOLDER, 18.).text_color(p.muted))
                            .child(text(path, 12.).truncate())
                            .child(div().flex_1())
                            .child(text(
                                if self.demo {
                                    tool.demo_label.to_owned()
                                } else {
                                    cache.map_or("—".into(), |c| tools_bytes(c.bytes))
                                },
                                12.,
                            )),
                    )
                    .child(
                        text("清理后由工具重新构建或下载；不会删除项目源码。", 11.)
                            .text_color(p.muted),
                    ),
            );
        }
        group
    }
    fn other_cache_row(&self, cache: &CacheRow, cx: &mut Context<Self>) -> Div {
        let p = palette();
        let id = cache.entry.rule.id;
        let name = if id == "cargo.registry" {
            "Cargo registry（完整缓存）"
        } else {
            cache.entry.rule.name
        };
        let expanded = self.expanded.contains(id);
        let weak = cx.entity().downgrade();
        let mut result = column().child(
            row()
                .h(px(ROW))
                .pl(px(20.))
                .pr(px(34.))
                .gap(px(16.))
                .child(
                    selection_box(
                        SharedString::from(format!("cache-{id}")),
                        format!("选择 {name}"),
                        cache.checked,
                        false,
                        self.locked() || cache.bytes == 0,
                    )
                    .on_change(move |state, _, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(c) = this.caches.iter_mut().find(|c| c.entry.rule.id == id)
                            {
                                c.checked = state == gpui_kit::base::CheckboxState::Checked;
                            }
                            cx.notify();
                        });
                    }),
                )
                .child(icon(egui_icons::icons::ICON_DATABASE.codepoint, 20.).text_color(p.muted))
                .child(
                    Button::new(SharedString::from(format!("cache-open-{id}")))
                        .accessibility_label(format!("展开 {name}"))
                        .ghost()
                        .p_0()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(
                            column()
                                .w_full()
                                .items_start()
                                .gap(px(4.))
                                .child(text(name, 15.).font_weight(FontWeight::SEMIBOLD))
                                .child(
                                    text(
                                        if cache.entry.rule.dangerous {
                                            "需确认 · 可能需要重新下载依赖"
                                        } else {
                                            "可重新生成的全局缓存"
                                        },
                                        12.,
                                    )
                                    .text_color(p.muted),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.expanded.remove(id) {
                                this.expanded.insert(id.into());
                            }
                            cx.notify();
                        })),
                )
                .child(
                    column()
                        .items_end()
                        .gap(px(8.))
                        .child(text(tools_bytes(cache.bytes), 14.))
                        .child(
                            text(
                                if cache.checked {
                                    "已选"
                                } else {
                                    "发现可清理"
                                },
                                10.,
                            )
                            .text_color(p.muted),
                        ),
                )
                .child(icon(if expanded { UP } else { DOWN }, 17.).text_color(p.muted)),
        );
        if expanded {
            result = result.child(
                column()
                    .id(SharedString::from(format!("cache-details-{id}")))
                    .pl(px(90.))
                    .pr(px(30.))
                    .pb(px(14.))
                    .gap(px(6.))
                    .child(
                        text(
                            cache
                                .entry
                                .path
                                .as_ref()
                                .map_or("工具链未返回可用路径".into(), |p| {
                                    p.display().to_string()
                                }),
                            12.,
                        )
                        .truncate(),
                    )
                    .child(
                        text(
                            format!("{} · 清理后需要重新构建或下载", tools_bytes(cache.bytes)),
                            11.,
                        )
                        .text_color(p.muted),
                    ),
            );
        }
        result
    }
    fn settings_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = palette();
        let mut view = column()
            .id("settings-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(24.))
            .gap(px(18.))
            .child(text("扫描根", 20.))
            .child(text("只在你添加的目录下发现项目。", 14.).text_color(p.muted));
        for path in &self.config.roots {
            let root = path.clone();
            view = view.child(
                row()
                    .gap(px(12.))
                    .child(text(path.display().to_string(), 14.).flex_1())
                    .child(
                        Button::new(SharedString::from(format!("remove-{}", path.display())))
                            .label("移除")
                            .disabled(self.locked() || self.demo)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.config.roots.retain(|p| p != &root);
                                this.projects.retain_roots(&this.config.roots);
                                config::save(&this.config);
                                this.refresh(cx);
                            })),
                    ),
            );
        }
        view.child(
            Button::new("add-root")
                .label("添加扫描根")
                .disabled(self.locked() || self.demo)
                .on_click(cx.listener(|this, _, w, cx| this.add_root(w, cx))),
        )
        .child(text("删除方式", 20.))
        .child(
            Checkbox::new("direct-delete")
                .label("直接删除（不可恢复）；不勾选时移入废纸篓")
                .checked(self.config.delete_mode == delete::DeleteMode::Direct)
                .disabled(self.locked() || self.demo)
                .on_change(cx.listener(|this, v, _, cx| {
                    this.config.delete_mode = if *v {
                        delete::DeleteMode::Direct
                    } else {
                        delete::DeleteMode::Trash
                    };
                    config::save(&this.config);
                    cx.notify();
                })),
        )
        .child(
            Button::new("open-rules")
                .label("打开规则文件所在目录")
                .on_click(|_, _, _| {
                    if let Some(path) = config::dir() {
                        let _ = std::fs::create_dir_all(&path);
                        let _ = opener::open(path);
                    }
                }),
        )
        .child(
            Button::new("close-settings")
                .label("返回深度清理")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings = false;
                    cx.notify();
                })),
        )
    }
}
fn disk_space() -> Option<(u64, u64)> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("df")
            .args(["-k", "/"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let columns: Vec<_> = text.lines().nth(1)?.split_whitespace().collect();
        Some((
            columns.get(1)?.parse::<u64>().ok()?.checked_mul(1024)?,
            columns.get(3)?.parse::<u64>().ok()?.checked_mul(1024)?,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
fn tools_bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!(
            "{} GB",
            format!("{:.2}", value as f64 / 1_000_000_000.)
                .trim_end_matches('0')
                .trim_end_matches('.')
        )
    } else {
        bytes(value)
    }
}
fn bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        let n = value as f64 / 1_000_000_000.;
        if n.fract() < 0.05 {
            format!("{n:.0} GB")
        } else {
            format!("{n:.1} GB")
        }
    } else if value >= 1_000_000 {
        let n = value as f64 / 1_000_000.;
        if n < 100. && n.fract() >= 0.05 {
            format!("{n:.1} MB")
        } else {
            format!("{n:.0} MB")
        }
    } else if value >= 1000 {
        format!("{} KB", value / 1000)
    } else {
        format!("{value} B")
    }
}
impl Render for SweepWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette();
        let (n, selected) = if self.demo && self.category == 6 {
            self.demo_tools_selected()
        } else {
            self.selected()
        };
        let (global_count, global_bytes) = if self.demo {
            let (count, bytes) = self.demo_tools_selected();
            (11 + count, 50_810_000_000 + bytes)
        } else {
            self.global_selected()
        };
        let selectable = if self.demo && self.category == 6 {
            TOOLS.len()
        } else if self.category == 8 {
            self.projects
                .entries()
                .iter()
                .map(|e| e.targets.len())
                .sum()
        } else {
            self.caches
                .iter()
                .filter(|c| self.cache_in_scope(c) && c.bytes > 0)
                .count()
        };
        let total = if self.demo {
            if self.category == 6 {
                15_200_000_000
            } else {
                63_000_000_000
            }
        } else if self.category != 8 {
            self.caches
                .iter()
                .filter(|c| self.cache_in_scope(c))
                .map(|c| c.bytes)
                .sum()
        } else {
            self.total()
        };
        let weak = cx.entity().downgrade();
        let scan_controls = row()
            .child(
                Button::new("refresh")
                    .outline()
                    .rounded_r(px(0.))
                    .border_r_0()
                    .h(px(44.))
                    .accessibility_label("重新扫描")
                    .child(
                        row()
                            .gap(px(8.))
                            .child(icon(egui_icons::icons::ICON_REFRESH.codepoint, 19.))
                            .child(text(
                                if self.scan.is_some() {
                                    "正在扫描…"
                                } else {
                                    "重新扫描"
                                },
                                15.,
                            )),
                    )
                    .disabled(self.locked())
                    .on_click(cx.listener(|this, _, w, cx| {
                        if this.demo {
                            Self::notice(w, cx, "样例模式", "退出 --demo 后可扫描真实项目。");
                        } else {
                            this.refresh(cx);
                        }
                    })),
            )
            .child(
                Button::new("scan-options")
                    .outline()
                    .rounded_l(px(0.))
                    .h(px(44.))
                    .w(px(40.))
                    .accessibility_label("扫描选项")
                    .child(icon(DOWN, 18.))
                    .dropdown_menu(move |menu, _, _| {
                        let search = weak.clone();
                        let settings = weak.clone();
                        menu.item(PopupMenuItem::new("搜索项目…").on_click(move |_, _, cx| {
                            let _ = search.update(cx, |this, cx| {
                                this.show_search = true;
                                cx.notify();
                            });
                        }))
                        .item(
                            PopupMenuItem::new("管理扫描根…").on_click(move |_, _, cx| {
                                let _ = settings.update(cx, |this, cx| {
                                    this.settings = true;
                                    cx.notify();
                                });
                            }),
                        )
                    }),
            );
        let header = row()
            .h(px(66.))
            .flex_shrink_0()
            .gap(px(12.))
            .child(
                text(
                    if self.settings {
                        "设置"
                    } else {
                        "深度清理"
                    },
                    28.,
                )
                .font_weight(FontWeight::MEDIUM)
                .flex_1()
                .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move()),
            )
            .children(self.demo.then(|| text("样例模式", 11.).text_color(p.muted)))
            .child(
                Button::new("disk")
                    .outline()
                    .h(px(44.))
                    .w(px(304.))
                    .child(
                        column()
                            .w_full()
                            .gap(px(3.))
                            .child(
                                row()
                                    .w_full()
                                    .gap(px(10.))
                                    .child(icon(egui_icons::icons::ICON_HARD_DRIVE.codepoint, 20.))
                                    .child(text(
                                        if self.demo {
                                            "Macintosh HD"
                                        } else {
                                            "启动磁盘"
                                        },
                                        15.,
                                    ))
                                    .child(div().flex_1())
                                    .child(
                                        text(
                                            if self.demo {
                                                "32.73 GB 可用".into()
                                            } else {
                                                self.disk.map_or(
                                                    "容量读取中…".into(),
                                                    |(_, free)| {
                                                        format!(
                                                            "{:.2} GB 可用",
                                                            free as f64 / 1_000_000_000.
                                                        )
                                                    },
                                                )
                                            },
                                            14.,
                                        )
                                        .text_color(p.muted),
                                    ),
                            )
                            .child(
                                div()
                                    .ml(px(30.))
                                    .h(px(3.))
                                    .rounded(px(2.))
                                    .bg(p.category)
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(if self.demo {
                                                0.97
                                            } else {
                                                self.disk.map_or(0., |(total, free)| {
                                                    if total == 0 {
                                                        0.
                                                    } else {
                                                        1. - free as f32 / total as f32
                                                    }
                                                })
                                            }))
                                            .bg(p.accent)
                                            .rounded(px(2.)),
                                    ),
                            ),
                    )
                    .on_click(|_, w, cx| Self::unavailable("磁盘选择", w, cx)),
            )
            .child(scan_controls);
        let summary = row()
            .h(px(53.))
            .flex_shrink_0()
            .px(px(18.))
            .gap(px(10.))
            .border_b_1()
            .border_color(p.border)
            .child(
                text(
                    if self.demo {
                        "发现 43 项可清理内容".into()
                    } else {
                        format!(
                            "发现 {} 项可清理内容",
                            self.projects
                                .entries()
                                .iter()
                                .map(|e| e.targets.len())
                                .sum::<usize>()
                        )
                    },
                    16.,
                )
                .font_weight(FontWeight::SEMIBOLD),
            )
            .child(text("最多可释放", 12.).text_color(p.muted))
            .child(
                text(
                    if self.demo {
                        "171.7 GB".into()
                    } else {
                        bytes(self.total() + self.caches.iter().map(|c| c.bytes).sum::<u64>())
                    },
                    23.,
                )
                .font_weight(FontWeight::BOLD)
                .text_color(p.accent),
            );
        let category_summary = format!(
            "{} / {}",
            if self.category == 6 {
                tools_bytes(selected)
            } else {
                bytes(selected)
            },
            if self.category == 6 {
                tools_bytes(total)
            } else {
                bytes(total)
            }
        );
        let result = column()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                row()
                    .h(px(51.))
                    .flex_shrink_0()
                    .px(px(15.))
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        text(CATEGORIES_DATA[self.category].0, 16.)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(div().flex_1())
                    .child(text("已选 / 发现可清理", 11.).text_color(p.muted))
                    .child(
                        row()
                            .gap(px(7.))
                            .child(
                                text(
                                    if self.category == 6 {
                                        tools_bytes(selected)
                                    } else {
                                        bytes(selected)
                                    },
                                    16.,
                                )
                                .text_color(p.accent),
                            )
                            .child(
                                text(
                                    format!(
                                        "/ {}",
                                        if self.category == 6 {
                                            tools_bytes(total)
                                        } else {
                                            bytes(total)
                                        }
                                    ),
                                    12.,
                                )
                                .text_color(p.muted),
                            )
                            .id("category-selection-summary")
                            .role(Role::Status)
                            .aria_label(category_summary.clone())
                            .test_support()
                            .text_color(p.accent),
                    )
                    .child(div().w(px(14.)))
                    .child(
                        selection_box(
                            "select-category",
                            "选择当前分类全部清理项",
                            n > 0 && n == selectable,
                            n > 0 && n < selectable,
                            self.locked(),
                        )
                        .on_change({
                            let weak = cx.entity().downgrade();
                            move |state, _, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.set_category_selection(
                                        state == gpui_kit::base::CheckboxState::Checked,
                                    );
                                    cx.notify();
                                });
                            }
                        }),
                    ),
            )
            .children((self.show_search && self.category == 8).then(|| {
                row()
                    .px(px(16.))
                    .py(px(8.))
                    .gap(px(10.))
                    .child(Input::new(&self.search))
                    .child(
                        Button::new("close-search")
                            .label("收起")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_search = false;
                                this.search
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                cx.notify();
                            })),
                    )
            }))
            .child(if self.category == 8 {
                self.project_results(cx).into_any_element()
            } else {
                self.cache_results(cx)
            });
        let workspace = column()
            .flex_1()
            .min_h_0()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .rounded(px(15.))
            .overflow_hidden()
            .child(summary)
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .child(self.categories(cx))
                    .child(result),
            );
        let footer = row()
            .id("window-selection-summary")
            .role(Role::Status)
            .aria_label(format!("已选 {global_count} 项 · {}", bytes(global_bytes)))
            .test_support()
            .h(px(54.))
            .flex_shrink_0()
            .px(px(15.))
            .gap(px(12.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .rounded(px(15.))
            .child(
                text(
                    format!("已选择项目  {} 项  │  可释放空间", global_count),
                    12.,
                )
                .text_color(p.muted),
            )
            .child(
                text(bytes(global_bytes), 17.)
                    .text_color(p.accent)
                    .font_weight(FontWeight::BOLD),
            )
            .child(div().flex_1())
            .child(
                Button::new("recommend")
                    .outline()
                    .accessibility_label("选择清理策略")
                    .child(
                        row()
                            .w_full()
                            .child(text("智能推荐", 14.))
                            .child(div().flex_1())
                            .child(icon(DOWN, 18.)),
                    )
                    .w(px(234.))
                    .h(px(40.))
                    .disabled(self.locked())
                    .dropdown_menu({
                        let weak = cx.entity().downgrade();
                        move |mut menu, _, _| {
                            for (mode, label) in [(0, "智能推荐"), (1, "全部选择"), (2, "取消选择")]
                            {
                                let weak = weak.clone();
                                menu = menu.item(PopupMenuItem::new(label).on_click(
                                    move |_, _window, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            if this.demo {
                                                if this.category == 6 {
                                                    this.demo_tool_selection = match mode {
                                                        0 => developer_tools::initial_selection(),
                                                        1 => TOOLS
                                                            .iter()
                                                            .map(|t| t.id.to_owned())
                                                            .collect(),
                                                        _ => HashSet::new(),
                                                    };
                                                    cx.notify();
                                                }
                                                return;
                                            }
                                            if this.locked() {
                                                return;
                                            }
                                            if this.category != 8 {
                                                let category = this.category;
                                                for row in &mut this.caches {
                                                    if (category == 7)
                                                        != (row.entry.rule.id
                                                            == "xcode.derived_data")
                                                    {
                                                        continue;
                                                    }
                                                    row.checked = row.bytes > 0
                                                        && match mode {
                                                            0 => !row.entry.rule.dangerous,
                                                            1 => true,
                                                            _ => false,
                                                        };
                                                }
                                            } else {
                                                let paths: Vec<_> = this
                                                    .projects
                                                    .entries()
                                                    .iter()
                                                    .map(|e| e.project.path.clone())
                                                    .collect();
                                                for path in paths {
                                                    if let Some(targets) =
                                                        this.projects.targets_mut(&path)
                                                    {
                                                        for t in targets {
                                                            t.checked = match mode {
                                                                0 => t.default_checked,
                                                                1 => true,
                                                                _ => false,
                                                            };
                                                        }
                                                    }
                                                }
                                            }
                                            cx.notify();
                                        });
                                    },
                                ));
                            }
                            menu
                        }
                    }),
            )
            .child(if self.execution.is_some() {
                Button::new("abort")
                    .label("中止")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.abort.store(true, Ordering::Relaxed);
                        this.status = "正在中止，等待当前项目完成…".into();
                        cx.notify();
                    }))
            } else {
                Button::new("clean")
                    .primary()
                    .label("清理")
                    .w(px(108.))
                    .h(px(40.))
                    .disabled(self.locked())
                    .on_click(cx.listener(|this, _, w, cx| this.clean(w, cx)))
            });
        let content = column()
            .bg(p.background)
            .rounded_tl(px(22.))
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(px(23.))
            .pr(px(if self.category == 6 { 13. } else { 23. }))
            .pb(px(12.))
            .child(header)
            .child(if self.settings {
                self.settings_view(cx).into_any_element()
            } else {
                column()
                    .flex_1()
                    .min_h_0()
                    .gap(px(20.))
                    .child(workspace)
                    .children(
                        (!self.status.is_empty())
                            .then(|| text(self.status.clone(), 11.).text_color(p.muted)),
                    )
                    .child(footer)
                    .into_any_element()
            });
        row()
            .size_full()
            .bg(p.sidebar)
            .text_color(p.foreground)
            .font_family(".SystemUIFont")
            .child(self.nav(cx))
            .child(content)
    }
}
impl Drop for SweepWindow {
    fn drop(&mut self) {
        if !self.demo {
            config::save(&self.config);
        }
    }
}

#[cfg(test)]
mod root_picker_tests {
    #[gpui_kit::test]
    fn root_picker_cancels_selects_and_ignores_duplicates(cx: &mut gpui_kit::TestAppContext) {
        use super::SweepWindow;
        use gpui_kit::{AppContext, WindowOptions};
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::gpui_theme::install(cx);
        });
        let (handle, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| SweepWindow::new_with_mode(Default::default(), true, window, cx))
                })
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |this, cx| this.prompt_for_root(window, cx))
        })
        .unwrap();
        assert!(
            cx.did_prompt_for_paths(),
            "the picker must be asynchronous and owned by GPUI"
        );
        cx.simulate_path_prompt_response(|options| {
            assert!(!options.files);
            assert!(options.directories);
            assert!(!options.multiple);
            None
        });
        cx.run_until_parked();
        cx.update(|cx| assert!(view.read(cx).config.roots.is_empty()));
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |this, cx| this.prompt_for_root(window, cx))
        })
        .unwrap();
        assert!(cx.did_prompt_for_paths());
        // Repeated activation must not replace the pending platform response.
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |this, cx| this.prompt_for_root(window, cx))
        })
        .unwrap();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        cx.run_until_parked();
        cx.update(|cx| assert!(view.read(cx).root_picker_pending));
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_path_buf();
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(view.read(cx).config.roots, vec![path.clone()]);
            assert!(!view.read(cx).root_picker_pending);
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |this, cx| this.prompt_for_root(window, cx))
        })
        .unwrap();
        cx.simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(view.read(cx).config.roots, vec![path]));
    }
}

#[cfg(test)]
mod cleanup_selection_tests {
    #[gpui_kit::test]
    fn category_selection_preserves_other_categories_and_global_project_plan(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use super::{CacheRow, SweepWindow};
        use gpui_kit::{AppContext, WindowOptions};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("app");
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'",
        )
        .unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::gpui_theme::install(cx);
        });
        let (_, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| SweepWindow::new_with_mode(Default::default(), true, window, cx))
                })
            })
            .unwrap();
        view.update(cx, |this, _| {
            this.rules = devsweep::rules::builtin_rules();
            this.config.roots = vec![root.clone()];
            this.projects.refresh(&this.config.roots, &this.rules);
            let project = this.projects.entries()[0].project.path.clone();
            for target in this.projects.targets_mut(&project).unwrap() {
                target.checked = true;
                target.size = Some(10);
            }
            for (id, bytes) in [("npm.cacache", 20), ("xcode.derived_data", 30)] {
                let rule = devsweep::cache::builtin_caches()
                    .iter()
                    .find(|r| r.id == id)
                    .unwrap();
                this.caches.push(CacheRow {
                    entry: devsweep::cache::CacheEntry { rule, path: None },
                    bytes,
                    checked: true,
                });
            }
            this.category = 6;
            assert_eq!(this.selected(), (1, 20));
            assert_eq!(this.global_selected(), (3, 60));
            // Use the real selection path; demo avoids persistence and traversal only.
            this.demo = false;
            this.set_category_selection(false);
            this.demo = true;
            assert_eq!(this.global_selected(), (2, 40));
            this.category = 7;
            assert_eq!(this.selected(), (1, 30));
            this.demo = false;
            this.set_category_selection(false);
            this.demo = true;
            assert_eq!(this.global_selected(), (1, 10));
            this.category = 6;
            let plan = this.build_plan();
            assert_eq!(plan.items.len(), 1);
            assert_eq!(plan.items[0].target_path, project.join("target"));
            assert_eq!(plan.total_bytes, 10);
            this.category = 8;
            assert_eq!(this.build_plan(), plan);
        });
    }
}
