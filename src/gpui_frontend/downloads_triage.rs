//! Retained, read-only Downloads scan pane.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::{
    downloads_triage::{Scan, scan_profile},
    file_operations::ScanCancellation,
    file_transfer::{DownloadsScanCompletion, EntryKind, EntrySnapshot, recheck_source_metadata},
    platform::app_data::AppDataRoot,
};
use std::time::SystemTime;

const PAGE_SIZE: usize = 100;

fn current_row(
    open: bool,
    generation: u64,
    row_generation: u64,
    index: usize,
    visible: std::ops::Range<usize>,
) -> bool {
    open && generation == row_generation && visible.contains(&index)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InspectReceipt {
    pub owner: EntityId,
    pub generation: u64,
    pub revision: u64,
    pub index: usize,
    pub entry: EntrySnapshot,
}

fn inspect_selection_current(
    open: bool,
    generation: u64,
    revision: u64,
    selected: Option<usize>,
    expected_generation: u64,
    expected_revision: u64,
    index: usize,
    visible: std::ops::Range<usize>,
) -> bool {
    current_row(open, generation, expected_generation, index, visible)
        && revision == expected_revision
        && selected == Some(index)
}

pub(super) struct DownloadsTriageView {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    profile: AppDataRoot,
    admission: Arc<RestartAdmission>,
    focus: FocusHandle,
    refresh_focus: FocusHandle,
    inspect_focus: FocusHandle,
    selection_revision: u64,
    inspect_cancel: Option<ScanCancellation>,
    inspect_error: Option<String>,
    generation: u64,
    cancellation: Option<ScanCancellation>,
    scan: Option<Arc<Scan>>,
    status: String,
    selected: Option<usize>,
    page: usize,
    skipped_page: usize,
    scroll: ScrollHandle,
}

impl DownloadsTriageView {
    pub fn new(
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        admission: Arc<RestartAdmission>,
        profile: AppDataRoot,
    ) -> Self {
        Self {
            open: false,
            root,
            profile,
            admission,
            focus: cx.focus_handle(),
            refresh_focus: cx.focus_handle(),
            inspect_focus: cx.focus_handle(),
            selection_revision: 0,
            inspect_cancel: None,
            inspect_error: None,
            generation: 0,
            cancellation: None,
            scan: None,
            status: String::new(),
            selected: None,
            page: 0,
            skipped_page: 0,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.open = true;
            self.refresh(cx);
        }
        if self.cancellation.is_some() {
            self.focus.focus(window, cx);
        } else {
            self.refresh_focus.focus(window, cx);
        }
        cx.notify();
    }

    pub fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.open && self.focus.contains_focused(window, cx)
    }

    fn retire_inspect(&mut self) {
        self.selection_revision = self.selection_revision.wrapping_add(1).max(1);
        self.inspect_error = None;
        if let Some(cancel) = self.inspect_cancel.take() {
            cancel.cancel();
        }
    }

    fn receipt(&self, owner: EntityId) -> Option<InspectReceipt> {
        let index = self.selected?;
        if !current_row(
            self.open,
            self.generation,
            self.generation,
            index,
            self.visible(),
        ) {
            return None;
        }
        let entry = self.scan.as_ref()?.rows().get(index)?.entry();
        (entry.metadata().kind == EntryKind::File).then(|| InspectReceipt {
            owner,
            generation: self.generation,
            revision: self.selection_revision,
            index,
            entry: entry.clone(),
        })
    }

    pub fn receipt_is_current(&self, receipt: &InspectReceipt, owner: EntityId) -> bool {
        receipt.owner == owner
            && inspect_selection_current(
                self.open,
                self.generation,
                self.selection_revision,
                self.selected,
                receipt.generation,
                receipt.revision,
                receipt.index,
                self.visible(),
            )
            && self
                .scan
                .as_ref()
                .and_then(|scan| scan.rows().get(receipt.index))
                .is_some_and(|row| {
                    row.entry() == &receipt.entry && row.entry().metadata().kind == EntryKind::File
                })
    }

    pub fn inspect_is_focused(&self, window: &Window) -> bool {
        self.inspect_focus.is_focused(window)
    }

    pub fn restore_receipt_focus(
        &self,
        receipt: &InspectReceipt,
        owner: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.receipt_is_current(receipt, owner) {
            return false;
        }
        self.inspect_focus.focus(window, cx);
        true
    }

    pub fn finish_inspect(
        &mut self,
        receipt: &InspectReceipt,
        owner: EntityId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.receipt_is_current(receipt, owner) || self.inspect_cancel.is_none() {
            return false;
        }
        self.inspect_cancel = None;
        cx.notify();
        true
    }

    pub fn report_inspect_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.inspect_error = Some(format!("Could not inspect file: {error}"));
        cx.notify();
    }

    fn inspect(&mut self, receipt: InspectReceipt, window: &mut Window, cx: &mut Context<Self>) {
        if self.inspect_cancel.is_some()
            || !window.is_window_active()
            || !self.receipt_is_current(&receipt, cx.entity_id())
        {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.inspect_error = None;
        self.inspect_focus.focus(window, cx);
        let cancellation = ScanCancellation::new();
        let worker_cancel = cancellation.clone();
        self.inspect_cancel = Some(cancellation);
        let entry = receipt.entry.clone();
        let task = cx.background_executor().spawn(async move {
            let result = recheck_source_metadata(&entry, &worker_cancel);
            (result, permit)
        });
        let root = self.root.clone();
        let source = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (result, permit) = task.await;
            let Ok(Some(handle)) = root.update(cx, |root, _| root.root_window) else {
                return;
            };
            let Some(root_view) = root.upgrade() else {
                return;
            };
            let _ = update_kit_window_content(handle, &root_view, cx, |root, window, cx| {
                root.open_hash_from_downloads(source, receipt, result, permit, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn retire(&mut self) {
        self.retire_inspect();
        self.generation = self.generation.wrapping_add(1).max(1);
        if let Some(cancel) = self.cancellation.take() {
            cancel.cancel();
        }
        self.scan = None;
        self.selected = None;
        self.page = 0;
        self.skipped_page = 0;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }

    pub fn shutdown(&mut self) {
        self.open = false;
        self.retire();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shutdown();
        let _ = self.root.update(cx, |root, cx| {
            root.root_scroll
                .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            root.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.cancellation.is_some() {
            return;
        }
        self.retire();
        let Some(permit) = self.admission.admit() else {
            self.status = "Application is closing.".into();
            cx.notify();
            return;
        };
        let cancellation = ScanCancellation::new();
        let worker_cancel = cancellation.clone();
        self.cancellation = Some(cancellation);
        self.status = "Scanning Downloadsâ€¦".into();
        let generation = self.generation;
        let profile = self.profile.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            scan_profile(&profile, &worker_cancel, SystemTime::now())
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |this, cx| {
                if !this.open || this.generation != generation || this.admission.is_fenced() {
                    return;
                }
                this.cancellation = None;
                match result {
                    Ok(scan) => {
                        let completion = match scan.completion() {
                            DownloadsScanCompletion::Complete => "Complete".to_owned(),
                            DownloadsScanCompletion::Limited => "Limited".to_owned(),
                            DownloadsScanCompletion::Cancelled => "Cancelled".to_owned(),
                            DownloadsScanCompletion::Io(error) => format!("I/O error: {error}"),
                        };
                        this.status = format!(
                            "{completion} Â· {} entries Â· {} skipped",
                            scan.rows().len(),
                            scan.skipped().len()
                        );
                        this.scan = Some(Arc::new(scan));
                    }
                    Err(error) => this.status = format!("Scan failed: {error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.cancellation.is_some() {
            self.retire();
            self.status = "Downloads scan cancelled.".into();
            cx.notify();
        }
    }

    fn accepts_generation(&self, generation: u64) -> bool {
        self.open && self.generation == generation
    }

    fn visible(&self) -> std::ops::Range<usize> {
        let start = self.page.saturating_mul(PAGE_SIZE);
        let len = self.scan.as_ref().map_or(0, |scan| scan.rows().len());
        start..start.saturating_add(PAGE_SIZE).min(len)
    }

    fn set_page(&mut self, page: usize) {
        self.retire_inspect();
        self.page = page;
        self.selected = None;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }
}

impl Drop for DownloadsTriageView {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancellation.take() {
            cancel.cancel();
        }
        if let Some(cancel) = self.inspect_cancel.take() {
            cancel.cancel();
        }
    }
}

impl Render for DownloadsTriageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let generation = self.generation;
        let inspect_receipt = self
            .inspect_cancel
            .is_none()
            .then(|| self.receipt(cx.entity_id()))
            .flatten();
        let range = self.visible();
        let count = self.scan.as_ref().map_or(0, |scan| scan.rows().len());
        let root = self
            .scan
            .as_ref()
            .map(|scan| scan.root().path().display().to_string());
        let selected = self
            .scan
            .as_ref()
            .and_then(|scan| self.selected.and_then(|i| scan.rows().get(i)))
            .map(|row| row.entry().path().display().to_string());
        let skipped_count = self.scan.as_ref().map_or(0, |scan| scan.skipped().len());
        let skipped_start = self.skipped_page.saturating_mul(PAGE_SIZE);
        let skipped_end = skipped_start.saturating_add(PAGE_SIZE).min(skipped_count);
        let skipped = self
            .scan
            .as_ref()
            .map(|scan| {
                scan.skipped()
                    .iter()
                    .skip(skipped_start)
                    .take(PAGE_SIZE)
                    .map(|item| {
                        format!(
                            "Skipped {}: {}",
                            item.name().to_string_lossy(),
                            item.reason()
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let rows = self
            .scan
            .as_ref()
            .map(|scan| {
                scan.rows()
                    .iter()
                    .enumerate()
                    .skip(range.start)
                    .take(PAGE_SIZE)
                    .map(|(index, row)| {
                        let label = row.entry().path().display().to_string();
                        let button = Button::new(SharedString::from(format!(
                            "downloads-triage-row-{generation}-{index}"
                        )))
                        .accessibility_label(format!("Downloads entry: {label}"))
                        .label(label)
                        .tab_stop(true)
                        .selected(self.selected == Some(index))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if current_row(
                                this.open,
                                this.generation,
                                generation,
                                index,
                                this.visible(),
                            ) && this
                                .scan
                                .as_ref()
                                .is_some_and(|scan| index < scan.rows().len())
                            {
                                if this.selected != Some(index) {
                                    this.retire_inspect();
                                    this.selected = Some(index);
                                    cx.notify();
                                }
                            }
                        }));
                        div().flex_shrink_0().child(button)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        div()
            .id("downloads-triage-pane")
            .key_context("DownloadsTriagePane")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .child(Label::new("Downloads Triage"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("downloads-triage-refresh")
                            .track_focus(&self.refresh_focus)
                            .tab_stop(true)
                            .label("Refresh")
                            .disabled(self.cancellation.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Button::new("downloads-triage-cancel")
                            .tab_stop(true)
                            .label("Cancel scan")
                            .disabled(self.cancellation.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        Button::new("downloads-triage-inspect")
                            .track_focus(&self.inspect_focus)
                            .tab_stop(true)
                            .label("Inspect")
                            .disabled(inspect_receipt.is_none())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(receipt) = inspect_receipt.clone() {
                                    this.inspect(receipt, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("downloads-triage-close")
                            .tab_stop(true)
                            .label("Close")
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
            .child(Label::new(self.status.clone()).id("downloads-triage-status"))
            .children(
                self.inspect_error
                    .as_ref()
                    .map(|error| Label::new(error.clone()).id("downloads-triage-inspect-error")),
            )
            .when(self.inspect_cancel.is_some(), |pane| {
                pane.child(
                    Label::new("Checking selected fileâ€¦").id("downloads-triage-inspect-status"),
                )
            })
            .when_some(root, |pane, path| {
                pane.child(Label::new(format!("Root: {path}")))
            })
            .when_some(selected, |pane, path| {
                pane.child(Label::new(format!("Selected: {path}")))
            })
            .when(count > PAGE_SIZE, |pane| {
                pane.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("downloads-triage-previous")
                                .tab_stop(true)
                                .label("Previous page")
                                .disabled(self.page == 0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.accepts_generation(generation) && this.page > 0 {
                                        this.set_page(this.page - 1);
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(Label::new(format!(
                            "Entries {}â€“{} of {count}",
                            range.start + 1,
                            range.end
                        )))
                        .child(
                            Button::new("downloads-triage-next")
                                .tab_stop(true)
                                .label("Next page")
                                .disabled(range.end >= count)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.accepts_generation(generation)
                                        && this.visible().end
                                            < this.scan.as_ref().map_or(0, |scan| scan.rows().len())
                                    {
                                        this.set_page(this.page + 1);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
            })
            .child(
                div()
                    .id("downloads-triage-results")
                    .flex()
                    .flex_col()
                    .max_h(px(300.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows)
                    .vertical_scrollbar(&self.scroll),
            )
            .when(skipped_count > PAGE_SIZE, |pane| {
                pane.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("downloads-triage-skipped-previous")
                                .tab_stop(true)
                                .label("Previous skipped page")
                                .disabled(self.skipped_page == 0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.accepts_generation(generation) && this.skipped_page > 0
                                    {
                                        this.skipped_page -= 1;
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(Label::new(format!(
                            "Skipped {}â€“{} of {skipped_count}",
                            skipped_start + 1,
                            skipped_end
                        )))
                        .child(
                            Button::new("downloads-triage-skipped-next")
                                .tab_stop(true)
                                .label("Next skipped page")
                                .disabled(skipped_end >= skipped_count)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let count =
                                        this.scan.as_ref().map_or(0, |scan| scan.skipped().len());
                                    if this.accepts_generation(generation)
                                        && (this.skipped_page + 1).saturating_mul(PAGE_SIZE) < count
                                    {
                                        this.skipped_page += 1;
                                        cx.notify();
                                    }
                                })),
                        ),
                )
            })
            .children(skipped.into_iter().enumerate().map(|(index, text)| {
                Label::new(text).id(SharedString::from(format!(
                    "downloads-triage-skipped-{}",
                    skipped_start + index
                )))
            }))
    }
}

impl RootWindow {
    fn open_hash_from_downloads(
        &mut self,
        source: WeakEntity<DownloadsTriageView>,
        receipt: InspectReceipt,
        result: Result<(), multi_launcher::file_transfer::AdmissionError>,
        operation: EffectPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = source.upgrade() else {
            return;
        };
        if !self
            .downloads_triage
            .as_ref()
            .is_some_and(|mounted| mounted.entity_id() == receipt.owner)
            || view.entity_id() != receipt.owner
            || !view.update(cx, |view, cx| {
                view.finish_inspect(&receipt, cx.entity_id(), cx)
            })
        {
            return;
        }
        let current = self
            .root_window
            .is_some_and(|root| root.window_id() == window.window_handle().window_id())
            && self.root_visibility.active.load(Ordering::Acquire)
            && self.root_visibility.visible.load(Ordering::Acquire)
            && window.is_window_active()
            && self.action_sheet.is_none()
            && self.context_favorite_popup.is_none()
            && self.native_window.is_some_and(|native| {
                native_window_is_valid(native)
                    && current_foreground_identity() == Some(native.identity)
            })
            && !self.restart_admission.is_fenced()
            && view.read(cx).inspect_is_focused(window)
            && view.read(cx).receipt_is_current(&receipt, view.entity_id());
        if !current {
            return;
        }
        if let Err(error) = result {
            view.update(cx, |view, cx| {
                view.report_inspect_error(error.to_string(), cx)
            });
            return;
        }
        let outcome = self.open_hash_generator_window(
            HashOpenIntent::File(receipt.entry.path()),
            operation,
            window,
            cx,
        );
        match outcome {
            Ok(()) => {
                self.hash_return = Some(HashReturnReceipt::Downloads(receipt));
                if self.hash_window.mode() == Some(HashPresentationMode::Docked) {
                    self.root_scroll
                        .scroll_to_top_of_item(self.docked_hash_scroll_item());
                }
            }
            Err(error) => view.update(cx, |view, cx| view.report_inspect_error(error, cx)),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{current_row, inspect_selection_current};

    #[test]
    fn inspect_receipt_requires_exact_active_selection() {
        assert!(inspect_selection_current(
            true,
            7,
            4,
            Some(3),
            7,
            4,
            3,
            0..100
        ));
        assert!(!inspect_selection_current(
            false,
            7,
            4,
            Some(3),
            7,
            4,
            3,
            0..100
        ));
        assert!(!inspect_selection_current(
            true,
            8,
            4,
            Some(3),
            7,
            4,
            3,
            0..100
        ));
        assert!(!inspect_selection_current(
            true,
            7,
            5,
            Some(3),
            7,
            4,
            3,
            0..100
        ));
        assert!(!inspect_selection_current(
            true,
            7,
            4,
            Some(4),
            7,
            4,
            3,
            0..100
        ));
        assert!(!inspect_selection_current(
            true,
            7,
            4,
            Some(3),
            7,
            4,
            3,
            100..200
        ));
    }

    #[test]
    fn selection_requires_current_open_generation_and_page() {
        assert!(current_row(true, 3, 3, 101, 100..200));
        assert!(!current_row(false, 3, 3, 101, 100..200));
        assert!(!current_row(true, 4, 3, 101, 100..200));
        assert!(!current_row(true, 3, 3, 99, 100..200));
    }
}
