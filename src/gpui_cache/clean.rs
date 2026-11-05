//! Cache-specific confirmation and execution; paths are always resolved by the fixed catalogue.
use super::*;
use crate::{
    delete::{self, DeleteMode},
    go_cache::GoCacheClean,
    plan::DeletePlan,
};
use gpui_kit::component::{WindowExt, radio::Radio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone)]
struct Choice {
    id: &'static str,
    path: PathBuf,
    bytes: u64,
}
#[derive(Clone)]
struct Reviewed {
    choice: Choice,
    plan: DeletePlan,
}
#[derive(Default)]
pub(super) struct CacheClean {
    confirming: bool,
    checking: bool,
    large_ack: bool,
    epoch: u64,
    mode: DeleteMode,
    reviewed: Vec<Reviewed>,
    skips: Vec<String>,
    failures: Vec<String>,
    running: bool,
    stopped: bool,
    interrupted: bool,
    done: usize,
    total: usize,
    freed: u64,
    abort: Arc<AtomicBool>,
    task: Option<Task<()>>,
}
impl Drop for GlobalCacheView {
    fn drop(&mut self) {
        self.clean.abort.store(true, Ordering::Relaxed);
    }
}
enum Progress {
    Entry {
        id: &'static str,
        deleted: bool,
        completed: bool,
        freed: u64,
        failures: Vec<String>,
        go_update: Option<GoCacheSnapshot>,
        skips: Vec<String>,
    },
    Finished {
        aborted: bool,
    },
}
fn checked_plan(
    choice: &Choice,
    snapshot: Option<&GoCacheSnapshot>,
    context: Option<&GoCacheContext>,
    language: LanguageChoice,
) -> Result<DeletePlan, String> {
    let en = Lang::from_choice(language) == Lang::En;
    let strings = Lang::from_choice(language).strings();
    let plan = if go_cache::is_go_cache(choice.id) {
        let (snapshot, context) = snapshot.zip(context).ok_or_else(|| {
            (if en {
                "Go cache context unavailable"
            } else {
                "无法获取 Go 缓存保护上下文"
            })
            .to_string()
        })?;
        snapshot
            .plan(choice.id, context)
            .map_err(|e| strings.go_cache_error(e).to_string())?
    } else {
        cache::plan(choice.id, choice.bytes).ok_or_else(|| {
            (if en {
                "Fixed cache path is unavailable"
            } else {
                "固定清单中的缓存路径不可用"
            })
            .to_string()
        })?
    };
    if plan.items.len() != 1 || plan.items[0].target_path != choice.path {
        return Err(strings.go_cache_error(GoCacheError::Changed).into());
    }
    Ok(plan)
}
impl GlobalCacheView {
    pub(crate) fn invalidate_clean(&mut self, cx: &mut Context<Self>) {
        self.context_invalidated = true;
        if self.clean.running {
            self.clean.abort.store(true, Ordering::Relaxed);
        }
        if self.clean.confirming {
            self.clean.epoch += 1;
            self.clean.task = None;
            self.clean.checking = false;
            self.clean.reviewed.clear();
            self.clean
                .skips
                .push(if Lang::from_choice(self.language) == Lang::En {
                    "Protected locations changed. Cancel and review the refreshed caches.".into()
                } else {
                    "保护路径已变更，请取消后查看刷新后的缓存。".into()
                });
        }
        cx.notify();
    }
    pub(crate) fn clean_locked(&self) -> bool {
        self.clean.confirming || self.clean.running
    }
    pub(crate) fn set_delete_mode(&mut self, mode: DeleteMode) {
        self.default_mode = mode;
    }
    pub(crate) fn open_clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_invalidated
            || self.clean_locked()
            || window.has_active_dialog(cx)
            || self
                .rows
                .iter()
                .any(|r| matches!(r.measurement, Measurement::Pending))
        {
            return;
        }
        let choices = self
            .rows
            .iter()
            .filter(|r| r.selected)
            .filter_map(|r| {
                let Measurement::Ready(bytes) = r.measurement else {
                    return None;
                };
                Some(Choice {
                    id: r.entry.rule.id,
                    path: r.entry.path.clone()?,
                    bytes,
                })
            })
            .collect::<Vec<_>>();
        if choices.is_empty() {
            return;
        }
        let epoch = self.clean.epoch + 1;
        self.clean = CacheClean {
            confirming: true,
            checking: true,
            epoch,
            mode: self.default_mode,
            ..Default::default()
        };
        let snapshot = self.go_snapshot.clone();
        let context = self.context.clone();
        let language = self.language;
        self.clean.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    choices
                        .into_iter()
                        .map(|choice| {
                            checked_plan(&choice, snapshot.as_ref(), context.as_ref(), language)
                                .map(|plan| Reviewed {
                                    choice: choice.clone(),
                                    plan,
                                })
                                .map_err(|e| {
                                    format!("{}: {} — {e}", choice.id, choice.path.display())
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.clean.epoch != epoch || !this.clean.confirming {
                    return;
                }
                this.clean.checking = false;
                for item in result {
                    match item {
                        Ok(item) => this.clean.reviewed.push(item),
                        Err(e) => this.clean.skips.push(e),
                    }
                }
                cx.notify();
            });
        }));
        let owner = cx.entity();
        let close = owner.clone();
        let en = Lang::from_choice(self.language) == Lang::En;
        window.open_dialog(cx, move |dialog, window, cx| {
            dialog
                .title(if en {
                    "Clean selected global caches?"
                } else {
                    "清理所选全局缓存？"
                })
                .w(window.rem_size() * 36.)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_close({
                    let close = close.clone();
                    move |_, _, cx| {
                        close.update(cx, |this, cx| this.cancel_clean(cx));
                    }
                })
                .child(owner.update(cx, |this, cx| this.confirmation(window, cx)))
                .footer(owner.update(cx, |this, cx| this.confirmation_actions(cx)))
        });
        cx.notify();
    }
    fn cancel_clean(&mut self, cx: &mut Context<Self>) {
        if !self.clean.confirming {
            return;
        }
        self.clean.confirming = false;
        self.clean.epoch += 1;
        self.clean.task = None;
        cx.notify();
    }
    fn confirmation(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let en = Lang::from_choice(self.language) == Lang::En;
        let bytes: u64 = self.clean.reviewed.iter().map(|r| r.plan.total_bytes).sum();
        let consequence = match (en, self.clean.mode) {
            (true, DeleteMode::Trash) => "Caches will move to Trash and can be recovered.",
            (false, DeleteMode::Trash) => "缓存将移入废纸篓或回收站，可恢复。",
            (true, DeleteMode::Direct) => {
                "Direct deletion is permanent. These caches cannot be recovered."
            }
            (false, DeleteMode::Direct) => "直接删除不可恢复，将永久删除这些缓存。",
        };
        let feedback = self
            .clean
            .skips
            .iter()
            .map(|s| format!("{}: {s}", if en { "Skip" } else { "跳过" }))
            .collect::<Vec<_>>()
            .join("\n");
        div()
            .flex()
            .flex_col()
            .gap_3()
            .min_w_0()
            .child(
                div()
                    .id("cache-plan-feedback")
                    .test_support()
                    .aria_label(feedback.clone())
                    .text_sm()
                    .child(feedback),
            )
            .child(if self.clean.checking {
                if en {
                    "Checking cache paths…".into()
                } else {
                    "正在检查缓存路径…".into()
                }
            } else {
                format!(
                    "{} · {} {} · {}",
                    if en {
                        "Global caches"
                    } else {
                        "全局缓存范围"
                    },
                    self.clean.reviewed.len(),
                    if en {
                        "cache entries"
                    } else {
                        "个缓存条目"
                    },
                    bytes_label(bytes)
                )
            })
            .child(
                div()
                    .id("cache-plan")
                    .max_h(window.bounds().size.height * 0.3)
                    .overflow_y_scroll()
                    .children(self.clean.reviewed.iter().map(|r| {
                        let rule = cache::builtin_caches()
                            .iter()
                            .find(|rule| rule.id == r.choice.id)
                            .unwrap();
                        let strings = Lang::from_choice(self.language).strings();
                        let note = match rule.id {
                            "go.module" => strings.go_module_note,
                            "go.build" => strings.go_build_note,
                            "cargo.registry" => strings.cache_cargo_note,
                            "xcode.derived_data" => "",
                            _ => strings.cache_redownload,
                        };
                        let name = match rule.id {
                            "go.build" => strings.go_build_cache,
                            "go.module" => strings.go_module_cache,
                            _ => rule.name,
                        };
                        let note = if rule.id == "cargo.registry" {
                            format!("{} · {note}", strings.cache_redownload)
                        } else {
                            note.to_string()
                        };
                        div().text_sm().child(format!(
                            "{} · {} · {}{} · {} · {}",
                            name,
                            r.choice.path.display(),
                            bytes_label(r.choice.bytes),
                            if r.plan.items.iter().any(|i| i.large) {
                                if en {
                                    " · Large cache"
                                } else {
                                    " · 大体积缓存"
                                }
                            } else {
                                ""
                            },
                            if rule.dangerous {
                                strings.dangerous
                            } else {
                                ""
                            },
                            note
                        ))
                    })),
            )
            .children(
                self.clean
                    .reviewed
                    .iter()
                    .any(|reviewed| reviewed.plan.items.iter().any(|item| item.large))
                    .then(|| {
                        Checkbox::new("cache-large-ack")
                            .label(Lang::from_choice(self.language).strings().large_ack)
                            .checked(self.clean.large_ack)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                this.clean.large_ack = *checked;
                                cx.notify();
                            }))
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        Radio::new("cache-mode-trash")
                            .label(if en {
                                "Trash"
                            } else {
                                "移入废纸篓 / 回收站"
                            })
                            .checked(self.clean.mode == DeleteMode::Trash)
                            .on_change(cx.listener(|this, _, _, cx| {
                                this.clean.mode = DeleteMode::Trash;
                                cx.notify();
                            })),
                    )
                    .child(
                        Radio::new("cache-mode-direct")
                            .label(if en { "Direct" } else { "直接删除" })
                            .checked(self.clean.mode == DeleteMode::Direct)
                            .on_change(cx.listener(|this, _, _, cx| {
                                this.clean.mode = DeleteMode::Direct;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("cache-consequence")
                    .test_support()
                    .aria_label(consequence)
                    .text_color(if self.clean.mode == DeleteMode::Direct {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(consequence),
            )
            .into_any_element()
    }
    fn confirmation_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let en = Lang::from_choice(self.language) == Lang::En;
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("cache-cancel")
                    .label(if en { "Cancel" } else { "取消" })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel_clean(cx);
                        window.close_dialog(cx);
                    })),
            )
            .child(
                Button::new("cache-execute")
                    .danger()
                    .label(if en { "Clean" } else { "清理" })
                    .disabled(!self.clean_ready())
                    .on_click(cx.listener(|this, _, window, cx| this.execute_clean(window, cx))),
            )
            .into_any_element()
    }
    fn clean_ready(&self) -> bool {
        !self.clean.checking
            && !self.clean.reviewed.is_empty()
            && (self.clean.large_ack
                || !self
                    .clean
                    .reviewed
                    .iter()
                    .any(|reviewed| reviewed.plan.items.iter().any(|item| item.large)))
    }
    fn execute_clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clean.confirming
            || self.clean.checking
            || self.clean.running
            || !self.clean_ready()
        {
            return;
        }
        let reviewed = self.clean.reviewed.clone();
        let snapshot = self.go_snapshot.clone();
        let context = self.context.clone();
        let mode = self.clean.mode;
        let language = self.language;
        self.clean.running = true;
        self.clean.confirming = false;
        self.clean.total = reviewed.len();
        let abort = self.clean.abort.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for reviewed in reviewed {
                if abort.load(Ordering::Relaxed) {
                    break;
                }
                let choice = reviewed.choice;
                let mut failures = Vec::new();
                let mut skips = Vec::new();
                let mut go_update = None;
                let strings = Lang::from_choice(language).strings();
                let mut recheck_failed = false;
                let mut freed = 0;
                let mut deleted = false;
                let mut completed = true;
                match checked_plan(&choice, snapshot.as_ref(), context.as_ref(), language) {
                    Err(e) => skips.push(format!("{}: {} — {e}", choice.id, choice.path.display())),
                    Ok(plan) => {
                        let (events, event_rx) = mpsc::channel();
                        let report = if go_cache::is_go_cache(choice.id) {
                            let (checked, checks) = mpsc::channel();
                            let report = GoCacheClean {
                                confirmed: snapshot.as_ref().unwrap(),
                                id: choice.id,
                                context: context.as_ref().unwrap(),
                            }
                            .execute(mode, &abort, events, checked);
                            if let Ok((fresh, error)) = checks.try_recv() {
                                go_update = Some(fresh);
                                if let Some(error) = error {
                                    recheck_failed = true;
                                    failures.push(format!(
                                        "{}: {} — {}",
                                        choice.id,
                                        choice.path.display(),
                                        strings.go_cache_error(error)
                                    ));
                                }
                            }
                            report
                        } else {
                            delete::execute(plan, mode, &abort, events)
                        };
                        completed = report.projects_done != 0;
                        freed = report.freed_bytes;
                        for event in event_rx.try_iter() {
                            if let delete::DeleteEvent::ProjectDone { deleted: paths, .. } = event {
                                deleted |= paths.contains(&choice.path);
                            }
                        }
                        failures.extend(report.failures.iter().filter(|_| !recheck_failed).map(
                            |f| {
                                format!(
                                    "{}: {} — {}",
                                    choice.id,
                                    f.path.display(),
                                    strings.failure_reason(f.reason)
                                )
                            },
                        ));
                    }
                }
                if tx
                    .send(Progress::Entry {
                        id: choice.id,
                        deleted,
                        completed,
                        freed,
                        failures,
                        go_update,
                        skips,
                    })
                    .is_err()
                {
                    abort.store(true, Ordering::Relaxed);
                    return;
                }
            }
            let _ = tx.send(Progress::Finished {
                aborted: abort.load(Ordering::Relaxed),
            });
        });
        self.clean.task = Some(cx.spawn(async move |this, cx| {
            loop {
                let running = this
                    .update(cx, |this, cx| {
                        loop {
                            match rx.try_recv() {
                                Ok(Progress::Entry {
                                    id,
                                    deleted,
                                    completed,
                                    freed,
                                    failures,
                                    go_update,
                                    skips,
                                }) => {
                                    this.clean.done += usize::from(completed);
                                    this.clean.freed = this.clean.freed.saturating_add(freed);
                                    this.clean.failures.extend(failures);
                                    this.clean.skips.extend(skips);
                                    if let Some(snapshot) = go_update {
                                        for ix in 0..this.rows.len() {
                                            let id = this.rows[ix].entry.rule.id;
                                            if let Some(entry) = snapshot.entry(id) {
                                                let same_path =
                                                    this.rows[ix].entry.path == entry.path;
                                                if same_path
                                                    && matches!(
                                                        this.rows[ix].measurement,
                                                        Measurement::Cleaned
                                                    )
                                                {
                                                    continue;
                                                }
                                                let selected = this.rows[ix].selected;
                                                this.rows[ix].entry.path = entry.path.clone();
                                                this.finish(ix, entry.size);
                                                if same_path && entry.size.is_ok() {
                                                    this.rows[ix].selected = selected;
                                                }
                                            }
                                        }
                                        this.go_snapshot = Some(snapshot);
                                    }
                                    if deleted {
                                        for row in &mut this.rows {
                                            if row.entry.rule.id == id {
                                                row.selected = false;
                                                row.measurement = Measurement::Cleaned;
                                            }
                                        }
                                    }
                                }
                                Ok(Progress::Finished { aborted }) => {
                                    this.clean.running = false;
                                    this.clean.stopped = aborted;
                                    break;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    this.clean.running = false;
                                    this.clean.interrupted = true;
                                    break;
                                }
                            }
                        }
                        cx.notify();
                        this.clean.running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(30))
                    .await;
            }
        }));
        window.close_dialog(cx);
        cx.notify();
    }
    pub(super) fn render_clean(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.clean.total == 0 {
            return div().into_any_element();
        }
        let en = Lang::from_choice(self.language) == Lang::En;
        let aborting = self.clean.abort.load(Ordering::Relaxed);
        let state = match (
            self.clean.running,
            aborting,
            self.clean.stopped,
            self.clean.interrupted,
            en,
        ) {
            (true, true, _, _, true) => "Abort requested · waiting for active cache operation",
            (true, true, _, _, false) => "已请求中止 · 等待当前缓存操作结束",
            (true, false, _, _, true) => "Cleaning global caches",
            (true, false, _, _, false) => "正在清理全局缓存",
            (_, _, _, true, true) => "Cache worker disconnected before completion",
            (_, _, _, true, false) => "缓存清理未正常完成",
            (_, _, true, _, true) => "Stopped",
            (_, _, true, _, false) => "已停止",
            (_, _, _, _, true) => "Cache clean finished",
            _ => "缓存清理完成",
        };
        let summary = if en {
            format!(
                "{state} · {}/{} cache entries · Freed bytes (estimate): {} · Skip: {} · Failure: {}",
                self.clean.done,
                self.clean.total,
                bytes_label(self.clean.freed),
                self.clean.skips.len(),
                self.clean.failures.len()
            )
        } else {
            format!(
                "{state} · {}/{} 个缓存条目 · 已释放（估算）：{} · 跳过：{} · 失败：{}",
                self.clean.done,
                self.clean.total,
                bytes_label(self.clean.freed),
                self.clean.skips.len(),
                self.clean.failures.len()
            )
        };
        div()
            .id("cache-clean-result")
            .test_support()
            .aria_label(summary.clone())
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(summary)
            .child(
                div()
                    .id("cache-result-skips")
                    .max_h_16()
                    .overflow_y_scroll()
                    .children(self.clean.skips.iter().map(|skip| {
                        div()
                            .text_sm()
                            .child(format!("{}: {skip}", if en { "Skip" } else { "跳过" }))
                    })),
            )
            .children(self.clean.running.then(|| {
                Button::new("cache-abort")
                    .label(if en { "Abort" } else { "中止" })
                    .disabled(aborting)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clean.abort.store(true, Ordering::Relaxed);
                        cx.notify();
                    }))
            }))
            .children((!self.clean.failures.is_empty()).then(|| {
                Button::new("cache-failures")
                    .label(if en {
                        "Failure details…"
                    } else {
                        "失败详情…"
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let details = this.clean.failures.join("\n");
                        window.open_dialog(cx, move |dialog, window, _| {
                            let copy = details.clone();
                            dialog
                                .title(if en {
                                    "Cache failures"
                                } else {
                                    "缓存失败详情"
                                })
                                .child(
                                    div()
                                        .id("cache-failure-details")
                                        .test_support()
                                        .aria_label(details.clone())
                                        .max_h(window.bounds().size.height * 0.5)
                                        .overflow_y_scroll()
                                        .child(details.clone()),
                                )
                                .footer(
                                    Button::new("cache-copy-failures")
                                        .icon(IconName::Copy)
                                        .label(if en { "Copy details" } else { "复制详情" })
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy.clone(),
                                            ))
                                        }),
                                )
                        });
                    }))
            }))
            .into_any_element()
    }
}
