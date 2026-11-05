//! Presentation adapter for the shared Project Clean lifecycle.
use super::*;
use std::sync::atomic::Ordering;

impl ProjectDiscovery {
    pub(crate) fn set_delete_mode(&mut self, mode: DeleteMode) {
        self.default_mode = mode;
    }
    pub(crate) fn clean_locked(&self) -> bool {
        self.clean_requested || self.clean_refresh.is_clean_locked()
    }
    pub(super) fn open_clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clean_requested || window.has_active_dialog(cx) {
            self.clean_requested = false;
            self.run_queued_refresh(cx);
            cx.notify();
            return;
        }
        self.clean_epoch += 1;
        let epoch = self.clean_epoch;
        self.clean_mode = self.default_mode;
        self.clean_large_ack = false;
        self.execution = None;
        let mut clean = ProjectClean::batch(
            self.model
                .entries()
                .iter()
                .filter(|entry| self.selected.contains(&entry.project.path))
                .cloned()
                .collect(),
        );
        let task = clean.task(Stack::Node, self.roots.clone(), self.rules.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        clean.preview(rx);
        self.clean = Some(clean);
        let work = cx.background_spawn(async move {
            let _ = tx.send(task());
        });
        self.clean_task = Some(cx.spawn(async move |this, cx| {
            work.await;
            let _ = this.update(cx, |this, cx| {
                if this.clean_epoch != epoch {
                    return;
                }
                if let Some(clean) = &mut this.clean {
                    clean.poll();
                    clean.confirm(); // Lock the reviewed snapshot; only Execute below authorizes deletion.
                }
                this.clean_refresh
                    .coordinate(this.clean.as_ref(), this.execution.as_ref());
                cx.notify();
            });
        }));
        let owner = cx.entity();
        let close_owner = owner.clone();
        let en = Lang::from_choice(self.language) == Lang::En;
        window.open_dialog(cx, move |dialog, window, cx| {
            dialog
                .title(if en {
                    "Clean selected projects?"
                } else {
                    "清理所选项目？"
                })
                .w(window.rem_size() * 36.)
                .overlay_closable(false)
                .on_ok(|_, _, _| false)
                .on_close({
                    let owner = close_owner.clone();
                    move |_, _, cx| {
                        owner.update(cx, |this, cx| this.cancel_clean(cx));
                    }
                })
                .child(owner.update(cx, |this, cx| this.render_confirmation(window, cx)))
                .footer(owner.update(cx, |this, cx| this.render_confirmation_actions(cx)))
        });
    }
    fn cancel_clean(&mut self, cx: &mut Context<Self>) {
        self.clean_epoch += 1;
        self.clean = None;
        self.clean_requested = false;
        self.clean_task = None;
        self.clean_refresh.coordinate(None, self.execution.as_ref());
        self.run_queued_refresh(cx);
        cx.notify();
    }
    fn clean_ready(&self) -> bool {
        self.clean.as_ref().is_some_and(|clean| {
            clean.locked()
                && (self.clean_large_ack || !clean.plan.items.iter().any(|item| item.large))
        })
    }
    fn execute_clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .execution
            .as_ref()
            .is_some_and(ProjectExecution::running)
        {
            return;
        }
        if !self.clean_ready() {
            return;
        }
        let clean = self.clean.take().expect("reviewed draft");
        let execution = clean.execute(
            Stack::Node,
            self.roots.clone(),
            self.rules.clone(),
            self.clean_mode,
            |work| {
                cx.background_spawn(async move {
                    work();
                })
                .detach()
            },
        );
        window.close_dialog(cx);
        self.clean_requested = false;
        if let Some(execution) = execution {
            self.track_execution(execution, cx);
        }
    }
    /// Attach the shared Execute lifecycle at the worker/event boundary.
    pub(super) fn track_execution(&mut self, execution: ProjectExecution, cx: &mut Context<Self>) {
        self.execution = Some(execution);
        self.clean_refresh.coordinate(None, self.execution.as_ref());
        self.clean_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let running = this
                    .update(cx, |this, cx| {
                        let Some(execution) = &mut this.execution else {
                            return false;
                        };
                        execution.poll_projects(&mut this.model);
                        let running = execution.running();
                        this.clean_refresh.coordinate(None, this.execution.as_ref());
                        if !running {
                            this.update_summary(cx);
                            this.measure_missing(cx);
                            this.run_queued_refresh(cx);
                        }
                        cx.notify();
                        running
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
        cx.notify();
    }
    fn render_confirmation(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let en = Lang::from_choice(self.language) == Lang::En;
        let strings = Lang::from_choice(self.language).strings();
        let ready = self.clean.as_ref().is_some_and(ProjectClean::locked);
        let mut body = div().flex().flex_col().gap_3().min_w_0();
        if let Some(clean) = &self.clean {
            if clean.query.is_some() {
                body = body.child(if en {
                    "Checking Delete plan…"
                } else {
                    "正在检查清理计划…"
                });
            } else if clean.failed() {
                body = body.child(strings.clean_preview_failed);
            } else {
                body = body
                    .child(format!(
                        "{}: {} · {} {}",
                        if en { "Chosen size" } else { "已选体积" },
                        format_bytes(clean.plan.total_bytes),
                        clean.plan.items.len(),
                        if en { "Targets" } else { "个清理项" }
                    ))
                    .child(
                        div()
                            .id("clean-plan")
                            .max_h(window.bounds().size.height * 0.3)
                            .overflow_y_scroll()
                            .children(clean.plan.items.iter().map(|item| {
                                div()
                                    .id(SharedString::from(
                                        item.target_path.to_string_lossy().into_owned(),
                                    ))
                                    .text_sm()
                                    .child(format!(
                                        "{} · {}{}",
                                        item.target_path.display(),
                                        format_bytes(item.bytes),
                                        if item.large {
                                            if en {
                                                " · Large Target"
                                            } else {
                                                " · 大体积清理项"
                                            }
                                        } else {
                                            ""
                                        }
                                    ))
                            }))
                            .children(clean.plan.skipped.iter().map(|skip| {
                                div().text_sm().child(format!(
                                    "{}: {} — {}",
                                    if en { "Skip" } else { "跳过" },
                                    skip.path.display(),
                                    strings.skip_reason(skip.reason)
                                ))
                            })),
                    );
                if !ready {
                    body = body.child(if en {
                        "No executable measured Targets. Cancel and review the selection."
                    } else {
                        "没有可执行且已测量的清理项。请取消后重新选择。"
                    });
                }
            }
        }
        if self
            .clean
            .as_ref()
            .is_some_and(|clean| clean.plan.items.iter().any(|item| item.large))
        {
            body = body.child(
                Checkbox::new("clean-large-ack")
                    .label(strings.large_ack)
                    .checked(self.clean_large_ack)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.clean_large_ack = *checked;
                        cx.notify();
                    })),
            );
        }
        let consequence = match (en, self.clean_mode) {
            (true, DeleteMode::Trash) => "Targets will move to Trash and can be recovered.",
            (false, DeleteMode::Trash) => "清理项将移入废纸篓或回收站，可恢复。",
            (true, DeleteMode::Direct) => {
                "Direct deletion is permanent. These Targets cannot be recovered."
            }
            (false, DeleteMode::Direct) => "直接删除不可恢复，将永久删除这些清理项。",
        };
        body.child(
            div()
                .flex()
                .gap_3()
                .child(
                    Radio::new("clean-mode-trash")
                        .label(if en {
                            "Trash"
                        } else {
                            "移入废纸篓 / 回收站"
                        })
                        .checked(self.clean_mode == DeleteMode::Trash)
                        .on_change(cx.listener(|this, _, _, cx| {
                            this.clean_mode = DeleteMode::Trash;
                            cx.notify();
                        })),
                )
                .child(
                    Radio::new("clean-mode-direct")
                        .label(if en { "Direct" } else { "直接删除" })
                        .checked(self.clean_mode == DeleteMode::Direct)
                        .on_change(cx.listener(|this, _, _, cx| {
                            this.clean_mode = DeleteMode::Direct;
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("clean-consequence")
                .test_support()
                .aria_label(consequence)
                .text_color(if self.clean_mode == DeleteMode::Direct {
                    cx.theme().danger
                } else {
                    cx.theme().muted_foreground
                })
                .child(consequence),
        )
        .into_any_element()
    }
    fn render_confirmation_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let en = Lang::from_choice(self.language) == Lang::En;
        let ready = self.clean_ready();
        div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("cancel-clean")
                    .label(if en { "Cancel" } else { "取消" })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel_clean(cx);
                        window.close_dialog(cx);
                    })),
            )
            .child(
                Button::new("execute-clean")
                    .danger()
                    .label(if en { "Clean" } else { "清理" })
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, window, cx| this.execute_clean(window, cx))),
            )
            .into_any_element()
    }
    fn failure_details(&self) -> String {
        let strings = Lang::from_choice(self.language).strings();
        self.execution
            .as_ref()
            .and_then(|execution| execution.report.as_ref())
            .map(|report| {
                report
                    .failures
                    .iter()
                    .map(|failure| {
                        format!(
                            "{}: {}",
                            failure.path.display(),
                            strings.failure_reason(failure.reason)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
    pub(super) fn render_execution(&self, cx: &mut Context<Self>) -> AnyElement {
        let execution = self.execution.as_ref().expect("execution presentation");
        let en = Lang::from_choice(self.language) == Lang::En;
        let strings = Lang::from_choice(self.language).strings();
        let running = execution.running();
        let aborting = execution.abort.load(Ordering::Relaxed);
        let state = if running {
            if aborting {
                if en {
                    "Abort requested · waiting for active projects to finish"
                } else {
                    "已请求中止 · 等待当前项目完成"
                }
            } else if en {
                "Cleaning · abort takes effect between projects"
            } else {
                "正在清理 · 中止仅在项目之间生效"
            }
        } else if execution.interrupted {
            strings.clean_execution_interrupted
        } else if execution
            .report
            .as_ref()
            .is_some_and(|report| report.aborted)
        {
            if en { "Stopped" } else { "已停止" }
        } else if en {
            "Clean finished"
        } else {
            "清理完成"
        };
        let bytes = execution
            .report
            .as_ref()
            .map_or(execution.freed_bytes, |report| report.freed_bytes);
        let failures = execution
            .report
            .as_ref()
            .map_or(0, |report| report.failures.len());
        let (done, total) = execution
            .report
            .as_ref()
            .map_or((execution.done, execution.total), |report| {
                (report.projects_done, report.projects_total)
            });
        let summary = if en {
            format!(
                "{state} · {}/{} projects · Freed bytes (estimate): {} · Skip: {} · Failure: {failures}",
                done,
                total,
                format_bytes(bytes),
                execution.skipped.len()
            )
        } else {
            format!(
                "{state} · {}/{} 个项目 · 已释放（估算）：{} · 跳过：{} · 失败：{failures}",
                done,
                total,
                format_bytes(bytes),
                execution.skipped.len()
            )
        };
        div()
            .id("clean-result")
            .test_support()
            .aria_label(summary.clone())
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .max_h_48()
            .child(summary)
            .children(
                execution
                    .current
                    .as_ref()
                    .filter(|_| running)
                    .map(|path| div().truncate().child(path.display().to_string())),
            )
            .children(running.then(|| {
                Button::new("abort-clean")
                    .label(if aborting {
                        if en {
                            "Abort requested"
                        } else {
                            "已请求中止"
                        }
                    } else if en {
                        "Abort"
                    } else {
                        "中止"
                    })
                    .disabled(aborting)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(execution) = &this.execution {
                            execution.request_abort();
                        }
                        cx.notify();
                    }))
            }))
            .child(
                div()
                    .id("execution-skips")
                    .max_h_16()
                    .overflow_y_scroll()
                    .children(execution.skipped.iter().map(|skip| {
                        div().text_sm().child(format!(
                            "{}: {} — {}",
                            if en { "Skip" } else { "跳过" },
                            skip.path.display(),
                            strings.skip_reason(skip.reason)
                        ))
                    })),
            )
            .children((failures > 0).then(|| {
                Button::new("show-clean-failures")
                    .label(if en {
                        "Failure details…"
                    } else {
                        "失败详情…"
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        let details = this.failure_details();
                        let en = Lang::from_choice(this.language) == Lang::En;
                        window.open_dialog(cx, move |dialog, _, _| {
                            let copy = details.clone();
                            dialog
                                .title(if en { "Failures" } else { "失败详情" })
                                .child(
                                    div()
                                        .id("clean-failure-details")
                                        .test_support()
                                        .aria_label(details.clone())
                                        .max_h_64()
                                        .overflow_y_scroll()
                                        .child(details.clone()),
                                )
                                .footer(
                                    Button::new("copy-clean-failures")
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

impl Drop for ProjectDiscovery {
    fn drop(&mut self) {
        if let Some(execution) = &self.execution {
            execution.request_abort();
        }
    }
}
