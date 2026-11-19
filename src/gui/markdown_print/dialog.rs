//! Retained Notes and Studio preview over the shared page scene.

use super::{
    AssetBase, DocumentStamp, PreviewWorker, SceneReceipt, Snapshot, output::MarkdownOutput,
};
use crate::gui::{
    jump_mode::{
        TargetKind,
        ui::{
            JumpModeScope, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
            register_and_apply_target,
        },
    },
    utility_host::{UtilityPresentation, presentation_button},
    utility_window_host::HostedUtilityDialog,
};
#[cfg(windows)]
use crate::notes_markdown::print_layout::printer::PrintScale;
use crate::notes_markdown::print_layout::{
    MAX_TEXT_BYTES, Margins, Orientation, Paper, PrintSettings,
};
use eframe::egui;
use std::sync::Arc;

pub(in crate::gui) struct DraftRef<'a> {
    pub stamp: DocumentStamp,
    pub source: &'a str,
    pub base: AssetBase,
}

impl DocumentStamp {
    pub(in crate::gui) fn same_document(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Note { panel: left, .. }, Self::Note { panel: right, .. }) => left == right,
            (Self::Studio { epoch: left, .. }, Self::Studio { epoch: right, .. }) => left == right,
            _ => false,
        }
    }
}

struct Draft {
    stamp: DocumentStamp,
    source: Option<Arc<str>>,
    base: AssetBase,
}

impl From<DraftRef<'_>> for Draft {
    fn from(draft: DraftRef<'_>) -> Self {
        Self {
            stamp: draft.stamp,
            source: (draft.source.len() <= MAX_TEXT_BYTES).then(|| Arc::from(draft.source)),
            base: draft.base,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum MarginPreset {
    Narrow,
    #[default]
    Standard,
    Wide,
    Custom,
}

#[derive(Clone, PartialEq)]
struct Controls {
    settings: PrintSettings,
    margins: MarginPreset,
    page: usize,
    fit: bool,
    zoom: u16,
    #[cfg(windows)]
    print_scale: PrintScale,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            settings: PrintSettings::default(),
            margins: MarginPreset::Standard,
            page: 0,
            fit: true,
            zoom: 100,
            #[cfg(windows)]
            print_scale: PrintScale::Fit,
        }
    }
}

impl Controls {
    fn set_margin_preset(&mut self, preset: MarginPreset) {
        self.margins = preset;
        let points = match preset {
            MarginPreset::Narrow => 36.0,
            MarginPreset::Standard => 54.0,
            MarginPreset::Wide => 72.0,
            MarginPreset::Custom => return,
        };
        self.settings.margins = Margins {
            left: points,
            right: points,
            top: points,
            bottom: points,
        };
    }

    fn display_scale(&self, available: egui::Vec2) -> f32 {
        if !self.fit {
            return f32::from(self.zoom) / 100.0;
        }
        let Ok(page) = self.settings.geometry() else {
            return 0.5;
        };
        let fit = (available.x / page.width).min(available.y / page.height);
        let fit = fit.clamp(f32::MIN_POSITIVE, 2.0);
        ((fit * 128.0).floor() / 128.0).max(fit.min(1.0 / 128.0))
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct DisplayMetrics {
    pixels_per_point: f32,
    texture_side: usize,
}

impl DisplayMetrics {
    fn for_viewport(ctx: &egui::Context) -> Self {
        Self {
            pixels_per_point: ctx.pixels_per_point(),
            texture_side: ctx.input(|input| input.max_texture_side).max(1),
        }
    }

    fn raster_scale(self, controls: &Controls, display_scale: f32) -> f32 {
        let requested = display_scale * self.pixels_per_point;
        let Ok(geometry) = controls.settings.geometry() else {
            return requested;
        };
        // Allow for rounding to an integer bitmap and keep display zoom independent.
        let safe_side = (self.texture_side as f32 - 0.5).max(0.5);
        requested.min(safe_side / geometry.width.max(geometry.height))
    }
}

#[derive(Default)]
pub(in crate::gui) struct PrintDialog {
    epoch: u64,
    draft: Option<Draft>,
    controls: Controls,
    worker: Option<PreviewWorker>,
    serial: Option<u64>,
    receipt: Option<Arc<SceneReceipt>>,
    image: Option<egui::TextureHandle>,
    error: Option<String>,
    date: String,
    display_scale: f32,
    raster_scale: f32,
    display_metrics: DisplayMetrics,
    focus_pending: bool,
    pub(in crate::gui) output: MarkdownOutput,
}

impl PrintDialog {
    pub fn current_stamp(&self) -> Option<&DocumentStamp> {
        self.draft.as_ref().map(|draft| &draft.stamp)
    }

    pub fn open(&mut self, draft: DraftRef<'_>, ctx: &egui::Context) {
        if self
            .current_stamp()
            .is_some_and(|stamp| stamp.same_document(&draft.stamp))
        {
            self.refresh(draft, ctx, egui::ViewportId::ROOT);
            self.focus_pending = true;
            return;
        }
        self.close();
        self.epoch = self.epoch.wrapping_add(1);
        self.draft = Some(draft.into());
        self.date = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.controls = Controls::default();
        self.display_scale = 0.5;
        self.display_metrics = DisplayMetrics::for_viewport(ctx);
        self.focus_pending = true;
        self.schedule(ctx, egui::ViewportId::ROOT);
    }

    /// Keep a preview tied to the same editor document, including Save As
    /// path changes. Opening a different document requires a fresh receipt.
    pub fn refresh(
        &mut self,
        draft: DraftRef<'_>,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        let Some(previous) = &self.draft else { return };
        if !previous.stamp.same_document(&draft.stamp) {
            self.close();
        } else if previous.stamp != draft.stamp || previous.base != draft.base {
            self.output.retire();
            self.draft = Some(draft.into());
            self.invalidate_scene(ctx, viewport);
        }
    }

    pub fn close(&mut self) {
        self.output.retire();
        if let Some(worker) = &self.worker {
            worker.retire();
        }
        self.draft = None;
        self.serial = None;
        self.receipt = None;
        self.image = None;
        self.error = None;
        self.focus_pending = false;
    }

    fn matches_receipt(&self, receipt: &SceneReceipt) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            draft.stamp == receipt.stamp
                && draft.base == receipt.base
                && draft.source.as_deref() == Some(receipt.source.as_ref())
                && self.controls.settings == receipt.settings
                && self.date == receipt.date
        })
    }

    pub(in crate::gui) fn output_scene(&self) -> Option<Arc<SceneReceipt>> {
        self.receipt
            .as_ref()
            .filter(|receipt| self.matches_receipt(receipt))
            .cloned()
    }

    fn invalidate_scene(&mut self, ctx: &egui::Context, viewport: egui::ViewportId) {
        self.receipt = None;
        self.controls.page = 0;
        self.schedule(ctx, viewport);
    }

    fn schedule(&mut self, ctx: &egui::Context, viewport: egui::ViewportId) {
        self.image = None;
        self.error = None;
        let Some(draft) = &self.draft else { return };
        let Some(source) = draft.source.as_ref() else {
            if let Some(worker) = &self.worker {
                worker.retire();
            }
            self.serial = None;
            self.error = Some("Print preview is limited to 1 MiB of Markdown.".into());
            return;
        };
        self.raster_scale = self
            .display_metrics
            .raster_scale(&self.controls, self.display_scale);
        let scene = self
            .receipt
            .as_ref()
            .filter(|receipt| self.matches_receipt(receipt))
            .cloned();
        let worker = self.worker.get_or_insert_with(PreviewWorker::default);
        self.serial = Some(if let Some(scene) = scene {
            worker.raster(scene, self.controls.page, self.raster_scale, ctx, viewport)
        } else {
            let snapshot = Snapshot {
                stamp: draft.stamp.clone(),
                source: source.clone(),
                base: draft.base.clone(),
                settings: self.controls.settings.clone(),
                date: self.date.clone(),
            };
            worker.prepare(
                snapshot,
                self.controls.page,
                self.raster_scale,
                ctx,
                viewport,
            )
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(reply) = self.worker.as_ref().and_then(PreviewWorker::take) else {
            return;
        };
        if self.serial != Some(reply.serial) || self.draft.is_none() {
            return;
        }
        self.serial = None;
        match reply.result {
            Ok(ready)
                if ready.serial == reply.serial
                    && self.matches_receipt(&ready.receipt)
                    && ready.scale == self.raster_scale =>
            {
                let limit = self
                    .display_metrics
                    .texture_side
                    .min(ctx.input(|input| input.max_texture_side));
                if ready.image.size.iter().any(|side| *side > limit) {
                    self.error = Some(
                        "Preview image exceeds the texture limit. Reduce zoom or use Fit page."
                            .into(),
                    );
                } else {
                    self.image = Some(ctx.load_texture(
                        "markdown-print-page",
                        ready.image,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.controls.page = ready.page;
                    self.receipt = Some(ready.receipt);
                }
            }
            Ok(_) => {}
            Err(error) => self.error = Some(error),
        }
        ctx.request_repaint();
    }
}

pub(in crate::gui) struct Actions {
    controls: Controls,
    display_scale: f32,
    host_request: Option<UtilityPresentation>,
    close: bool,
    ctx: egui::Context,
    viewport: egui::ViewportId,
    display_metrics: DisplayMetrics,
    focus_control: Option<(egui::LayerId, egui::Id)>,
    confirmation_focus: Option<(egui::LayerId, egui::Id, egui::Id)>,
    output_intent: Option<OutputIntent>,
}

pub(in crate::gui) enum Command {
    Closed,
    Output {
        intent: OutputIntent,
        scene: Option<Arc<SceneReceipt>>,
        hwnd: Option<usize>,
    },
}

#[derive(Clone, Copy)]
pub(in crate::gui) enum OutputIntent {
    ChoosePdf,
    #[cfg(windows)]
    ChoosePrinter {
        current_page: usize,
        mode: PrintScale,
    },
    Confirm,
    Cancel,
}

impl HostedUtilityDialog for PrintDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::MarkdownPrint);
    const KEY: &'static str = "markdown-print";
    const TITLE: &'static str = "Export / Print";
    const SIZE: egui::Vec2 = egui::vec2(850.0, 660.0);
    const MINIMUM: egui::Vec2 = egui::vec2(560.0, 380.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = Actions;
    type Checkpoint = ();
    type Command = Command;

    fn is_open(&self) -> bool {
        self.draft.is_some()
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        _: Option<egui::LayerId>,
    ) -> bool {
        use crate::gui::jump_mode::ui::{focused_response_layer, visible_top_middle_layer};
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && visible_top_middle_layer(ctx).is_none_or(|layer| layer == binding.base_layer)
            && focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        _: NavigationOwner,
        _: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Actions {
        let mut actions = Actions {
            controls: self.controls.clone(),
            display_scale: self.display_scale,
            host_request: None,
            close: false,
            ctx: ctx.clone(),
            viewport: ctx.viewport_id(),
            display_metrics: DisplayMetrics::for_viewport(ctx),
            focus_control: None,
            confirmation_focus: None,
            output_intent: None,
        };
        let mut open = true;
        let mut show = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(parent_input, |ui| {
                self.controls_ui(ui, &mut actions, scopes.0, presentation)
            });
        };
        if let Some(ui) = child {
            show(ui);
        } else {
            egui::Window::new(Self::TITLE)
                .id(Self::layer().id)
                .open(&mut open)
                .default_size(Self::SIZE)
                .min_size(Self::MINIMUM)
                .show(ctx, show);
        }
        actions.close |= !open;
        actions
    }
    fn host_request(actions: &Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Command {
        Command::Closed
    }
    fn command_closes(command: &Command) -> bool {
        matches!(command, Command::Closed)
    }
    fn accept_close(&mut self, keyboard: bool) -> bool {
        if keyboard && self.output.confirming() {
            self.output.cancel();
            return false;
        }
        self.output.retire();
        true
    }
    fn complete_actions(&mut self, actions: Actions, hwnd: Option<usize>) -> Vec<Command> {
        if actions.close {
            self.output.retire();
            return vec![Command::Closed];
        }
        if let Some((layer, cancel, replace)) = actions.confirmation_focus {
            if actions.viewport != egui::ViewportId::ROOT {
                self.output.finish_confirmation_focus();
            } else if crate::gui::jump_mode::ui::visible_top_middle_layer(&actions.ctx)
                == Some(layer)
            {
                actions.ctx.memory_mut(|memory| {
                    if !memory.has_focus(cancel) && !memory.has_focus(replace) {
                        memory.request_focus(cancel);
                    }
                });
                self.output.finish_confirmation_focus();
            }
        }
        if self.focus_pending
            && let Some((layer, target)) = actions.focus_control
            && crate::gui::jump_mode::ui::visible_top_middle_layer(&actions.ctx) == Some(layer)
        {
            if crate::gui::jump_mode::ui::focused_response_layer(&actions.ctx) != Some(layer) {
                actions
                    .ctx
                    .memory_mut(|memory| memory.request_focus(target));
            }
            self.focus_pending = false;
        }
        let layout_changed = self.controls.settings != actions.controls.settings;
        let raster_changed = self.controls.page != actions.controls.page
            || self.display_scale != actions.display_scale
            || actions
                .display_metrics
                .raster_scale(&actions.controls, actions.display_scale)
                != self.raster_scale;
        if layout_changed || actions.host_request.is_some() {
            self.output.retire();
        }
        self.controls = actions.controls;
        self.display_scale = actions.display_scale;
        self.display_metrics = actions.display_metrics;
        if layout_changed {
            self.invalidate_scene(&actions.ctx, actions.viewport);
        } else if raster_changed {
            self.schedule(&actions.ctx, actions.viewport);
        }
        if let Some(intent) = actions.output_intent
            && actions.host_request.is_none()
        {
            let scene = self.output_scene();
            if matches!(intent, OutputIntent::Cancel) || scene.is_some() {
                return vec![Command::Output {
                    intent,
                    scene,
                    hwnd,
                }];
            }
        }
        Vec::new()
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::utility_host::{HostIdentity, UtilityPresentation};
    use crate::notes_markdown::print_layout::Drawable;
    use std::time::{Duration, Instant};

    fn studio(source: &str, revision: u64, epoch: u64) -> DraftRef<'_> {
        DraftRef {
            stamp: DocumentStamp::Studio {
                epoch,
                host: HostIdentity {
                    host: UtilityPresentation::Docked,
                    generation: 1,
                },
                path: None,
                revision: Some(revision),
            },
            source,
            base: AssetBase::Studio(None),
        }
    }

    fn wait_for_receipt(dialog: &mut PrintDialog, ctx: &egui::Context) -> Arc<SceneReceipt> {
        let deadline = Instant::now() + Duration::from_secs(4);
        while dialog.serial.is_some() {
            dialog.poll(ctx);
            assert!(Instant::now() < deadline, "preview did not finish");
            std::thread::sleep(Duration::from_millis(4));
        }
        dialog.receipt.clone().expect("accepted current scene")
    }

    #[test]
    fn markdown_print_fit_stabilizes_across_worker_completion() {
        let ctx = egui::Context::default();
        let mut dialog = PrintDialog::default();
        dialog.open(studio("# Draft", 1, 1), &ctx);
        dialog.controls.settings.paper = Paper::Custom {
            width: 2880.0,
            height: 2880.0,
        };
        dialog.invalidate_scene(&ctx, egui::ViewportId::ROOT);

        let render = |dialog: &mut PrintDialog| {
            let mut actions = None;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(660.0, 380.0),
                    )),
                    max_texture_side: Some(2048),
                    ..Default::default()
                },
                |ui| {
                    actions = Some(dialog.render_host(
                        &ui.ctx().clone(),
                        (None, None),
                        NavigationOwner::Blocked,
                        true,
                        NavigationOwner::Blocked,
                        true,
                        0,
                        &(),
                        Some(ui),
                        UtilityPresentation::Docked,
                    ));
                },
            )
            .drop_without_applying_deltas();
            actions.unwrap()
        };
        let actions = render(&mut dialog);
        let fit = actions.display_scale;
        assert!(fit < 0.1);
        dialog.complete_actions(actions, None);
        wait_for_receipt(&mut dialog, &ctx);
        let after = render(&mut dialog);
        assert_eq!(after.display_scale, fit);
        dialog.complete_actions(after, None);
        assert!(
            dialog.serial.is_none(),
            "Fit must remain idle after the raster is visible"
        );
        let sample = DisplayMetrics {
            pixels_per_point: 2.0,
            texture_side: 2048,
        };
        assert!((sample.raster_scale(&dialog.controls, 3.0) * 2880.0).ceil() <= 2048.0);
    }

    #[test]
    fn markdown_print_host_retires_draft_and_reuses_current_scene_for_page_and_zoom() {
        let ctx = egui::Context::default();
        let mut dialog = PrintDialog::default();
        dialog.open(studio("# Old draft", 1, 1), &ctx);
        dialog.refresh(
            studio("# New draft\n\n<!-- pagebreak -->\n\nSecond", 2, 1),
            &ctx,
            egui::ViewportId::ROOT,
        );
        let ready = wait_for_receipt(&mut dialog, &ctx);
        assert_eq!(ready.scene.pages.len(), 2);
        assert!(ready.scene.pages[0].drawables.iter().any(|item| {
            matches!(item, Drawable::Text { text, .. } if text.contains("New draft"))
        }));
        let controls = Controls {
            page: 1,
            fit: false,
            zoom: 80,
            ..dialog.controls.clone()
        };
        let actions = Actions {
            controls,
            display_scale: 0.8,
            host_request: None,
            close: false,
            ctx: ctx.clone(),
            viewport: egui::ViewportId::ROOT,
            display_metrics: DisplayMetrics::for_viewport(&ctx),
            focus_control: None,
            confirmation_focus: None,
            output_intent: None,
        };
        assert!(dialog.complete_actions(actions, None).is_empty());
        assert!(Arc::ptr_eq(&wait_for_receipt(&mut dialog, &ctx), &ready));
        assert_eq!(dialog.controls.page, 1);
        dialog.refresh(studio("# Replacement", 0, 2), &ctx, egui::ViewportId::ROOT);
        assert!(dialog.current_stamp().is_none() && dialog.receipt.is_none());
        assert!(
            dialog.worker.is_some(),
            "the worker is retained across reopen"
        );
    }
}

#[cfg(windows)]
fn page_list(pages: impl IntoIterator<Item = usize>) -> String {
    let mut pages = pages.into_iter().peekable();
    let mut labels = Vec::new();
    while let Some(first) = pages.next() {
        let mut last = first;
        while pages.next_if(|page| *page == last + 1).is_some() {
            last += 1;
        }
        if first == last {
            labels.push(first.to_string());
        } else {
            labels.push(format!("{first}–{last}"));
        }
    }
    labels.join(", ")
}

#[cfg(windows)]
fn printer_review_ui(ui: &mut egui::Ui, review: &super::output::PrinterReview) {
    let device = review.layout.device;
    let pages = &review.layout.pages;
    let x_inches = |px| f64::from(px) / f64::from(device.dpi_x);
    let y_inches = |px| f64::from(px) / f64::from(device.dpi_y);
    ui.strong("Review print job");
    ui.label(
        review
            .device_label
            .as_deref()
            .unwrap_or("Selected Windows printer"),
    );
    ui.label(format!(
        "Pages {}",
        page_list(pages.iter().map(|page| page.page))
    ));
    ui.label(format!(
        "Device paper {:.2} × {:.2} in, printable area {:.2} × {:.2} in",
        x_inches(device.physical_width_px),
        y_inches(device.physical_height_px),
        x_inches(device.printable_width_px),
        y_inches(device.printable_height_px),
    ));
    let right = device.physical_width_px - device.offset_x_px - device.printable_width_px;
    let bottom = device.physical_height_px - device.offset_y_px - device.printable_height_px;
    ui.label(format!(
        "Hard margins {:.2} in left, {:.2} in right, {:.2} in top, {:.2} in bottom",
        x_inches(device.offset_x_px),
        x_inches(right),
        y_inches(device.offset_y_px),
        y_inches(bottom),
    ));
    match review.mode {
        PrintScale::ActualSize => {
            ui.label("Actual size (100%)");
        }
        PrintScale::Fit => {
            if let Some(first) = pages.first() {
                let (minimum, maximum) = pages.iter().fold(
                    (first.fit_percent, first.fit_percent),
                    |(min, max), page| (min.min(page.fit_percent), max.max(page.fit_percent)),
                );
                let scale = if (minimum - maximum).abs() < 0.05 {
                    format!("{minimum:.1}%")
                } else {
                    format!("{minimum:.1}%–{maximum:.1}%")
                };
                ui.label(format!("Fit to printable area ({scale})"));
            }
        }
    }
    let mismatch = page_list(
        pages
            .iter()
            .filter(|page| page.paper_mismatch)
            .map(|page| page.page),
    );
    if !mismatch.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Preview paper differs from printer paper on pages {mismatch}."),
        );
    }
    let clipped = page_list(
        pages
            .iter()
            .filter(|page| page.painted_clipped)
            .map(|page| page.page),
    );
    if !clipped.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Printer hard margins clip content on pages {clipped}."),
        );
    }
}

impl PrintDialog {
    fn confirmation_buttons(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        scope: Option<&JumpModeScope>,
        confirm_key: &'static str,
        confirm_label: &'static str,
    ) {
        ui.horizontal_wrapped(|ui| {
            let cancel = ui.button("Cancel");
            let confirm = ui.button(confirm_label);
            if !ui.is_enabled() {
                return;
            }
            let mut order = 0;
            for (response, key, label) in [
                (&cancel, "cancel-output-review", "Cancel output review"),
                (&confirm, confirm_key, confirm_label),
            ] {
                register_and_apply_target(
                    scope,
                    ui,
                    response,
                    key,
                    label,
                    TargetKind::Button,
                    &mut order,
                );
            }
            if self.output.confirmation_needs_focus() && !ui.ctx().will_discard() {
                if actions.viewport != egui::ViewportId::ROOT
                    && !cancel.has_focus()
                    && !confirm.has_focus()
                {
                    cancel.request_focus();
                }
                actions.confirmation_focus = Some((ui.layer_id(), cancel.id, confirm.id));
            }
            if cancel.clicked() {
                actions.output_intent = Some(OutputIntent::Cancel);
            } else if confirm.clicked() {
                actions.output_intent = Some(OutputIntent::Confirm);
            }
        });
    }

    fn controls_ui(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        scope: Option<&JumpModeScope>,
        presentation: UtilityPresentation,
    ) {
        let mut order = 0;
        macro_rules! target {
            ($ui:expr, $response:expr, $key:expr, $label:expr, $kind:expr) => {
                if $response.enabled() {
                    register_and_apply_target(
                        scope, $ui, &$response, $key, $label, $kind, &mut order,
                    );
                }
            };
        }
        if let Some(path) = self.output.target_path() {
            ui.strong("Replace the existing PDF?");
            ui.label(path.display().to_string());
            self.confirmation_buttons(ui, actions, scope, "replace-pdf", "Replace PDF");
            return;
        }
        #[cfg(windows)]
        if let Some(review) = self.output.printer_review() {
            printer_review_ui(ui, review);
            self.confirmation_buttons(ui, actions, scope, "submit-print-job", "Submit print job");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let enabled = ui.is_enabled();
            let (control, requested) = presentation_button(ui, presentation, enabled);
            target!(
                ui,
                control,
                "presentation",
                "Dock or pop out preview",
                TargetKind::Button
            );
            if control.clicked() {
                actions.host_request = Some(requested);
            }
            let close = ui.button("Close");
            target!(ui, close, "close", "Close preview", TargetKind::Button);
            actions.close |= close.clicked();
            let can_choose = !self.output.busy() && self.output_scene().is_some();
            let save = ui.add_enabled(can_choose, egui::Button::new("Save PDF…"));
            target!(ui, save, "save-pdf", "Save PDF", TargetKind::Button);
            if save.clicked() {
                actions.output_intent = Some(OutputIntent::ChoosePdf);
            }
            #[cfg(windows)]
            {
                let print = ui.add_enabled(can_choose, egui::Button::new("Print…"));
                target!(ui, print, "choose-printer", "Print", TargetKind::Button);
                if print.clicked() {
                    actions.output_intent = Some(OutputIntent::ChoosePrinter {
                        current_page: actions.controls.page + 1,
                        mode: actions.controls.print_scale,
                    });
                }
            }
            if let Some(label) = self.output.cancel_job_label() {
                let cancel = ui.add_enabled(self.output.can_cancel_job(), egui::Button::new(label));
                target!(ui, cancel, "cancel-output-job", label, TargetKind::Button);
                if cancel.clicked() {
                    actions.output_intent = Some(OutputIntent::Cancel);
                }
            }
        });
        #[cfg(windows)]
        ui.add_enabled_ui(!self.output.busy(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Printer scaling");
                for (mode, key, label) in [
                    (PrintScale::Fit, "printer-fit", "Fit to printable area"),
                    (PrintScale::ActualSize, "printer-actual", "Actual size"),
                ] {
                    let response = ui.radio_value(&mut actions.controls.print_scale, mode, label);
                    target!(ui, response, key, label, TargetKind::Selectable);
                }
            });
        });
        if let Some((error, feedback)) = self.output.feedback() {
            if error {
                ui.colored_label(ui.visuals().error_fg_color, feedback);
            } else {
                ui.label(feedback);
            }
        }
        ui.separator();
        let half = ui.available_width() / 2.0;
        let settings_width = half.clamp(210.0, 310.0);
        let page_width = (ui.available_width() - settings_width - 20.0).max(180.0);
        let height = ui.available_height().clamp(220.0, 660.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(settings_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("print-settings")
                        .max_height(height)
                        .show(ui, |ui| {
                            let controls = &mut actions.controls;
                            ui.strong("Paper");
                            ui.horizontal_wrapped(|ui| {
                                for (paper, label, key) in
                                    [(Paper::Letter, "Letter", "letter"), (Paper::A4, "A4", "a4")]
                                {
                                    let response =
                                        ui.radio_value(&mut controls.settings.paper, paper, label);
                                    target!(ui, response, key, label, TargetKind::Selectable);
                                    if self.focus_pending
                                        && key == "letter"
                                        && ui.is_enabled()
                                        && ui.ctx().viewport_id() == egui::ViewportId::ROOT
                                    {
                                        actions.focus_control = Some((ui.layer_id(), response.id));
                                    }
                                }
                                let custom =
                                    matches!(controls.settings.paper, Paper::Custom { .. });
                                let response = ui.radio(custom, "Custom");
                                target!(
                                    ui,
                                    response,
                                    "custom-paper",
                                    "Custom paper",
                                    TargetKind::Selectable
                                );
                                if response.clicked() && !custom {
                                    controls.settings.paper = Paper::Custom {
                                        width: 612.0,
                                        height: 792.0,
                                    };
                                }
                            });
                            if let Paper::Custom { width, height } = &mut controls.settings.paper {
                                ui.horizontal_wrapped(|ui| {
                                    for (value, label, key) in [
                                        (width, "Width", "paper-width"),
                                        (height, "Height", "paper-height"),
                                    ] {
                                        ui.label(label);
                                        let response = ui.add(
                                            egui::DragValue::new(value)
                                                .range(72.0..=2880.0)
                                                .suffix(" pt"),
                                        );
                                        target!(ui, response, key, label, TargetKind::TextEntry);
                                    }
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                for (orientation, label, key) in [
                                    (Orientation::Portrait, "Portrait", "portrait"),
                                    (Orientation::Landscape, "Landscape", "landscape"),
                                ] {
                                    let response = ui.radio_value(
                                        &mut controls.settings.orientation,
                                        orientation,
                                        label,
                                    );
                                    target!(ui, response, key, label, TargetKind::Selectable);
                                }
                            });
                            ui.add_space(6.0);
                            ui.strong("Margins");
                            ui.horizontal_wrapped(|ui| {
                                for (preset, label, key) in [
                                    (MarginPreset::Narrow, "Narrow", "narrow-margins"),
                                    (MarginPreset::Standard, "Standard", "standard-margins"),
                                    (MarginPreset::Wide, "Wide", "wide-margins"),
                                    (MarginPreset::Custom, "Custom", "custom-margins"),
                                ] {
                                    let response = ui.radio(controls.margins == preset, label);
                                    target!(
                                        ui,
                                        response,
                                        key,
                                        format!("{label} margins"),
                                        TargetKind::Selectable
                                    );
                                    if response.clicked() {
                                        controls.set_margin_preset(preset);
                                    }
                                }
                            });
                            if controls.margins == MarginPreset::Custom {
                                let margins = &mut controls.settings.margins;
                                egui::Grid::new("print-custom-margins").show(ui, |ui| {
                                    for (value, label, key) in [
                                        (&mut margins.top, "Top", "margin-top"),
                                        (&mut margins.right, "Right", "margin-right"),
                                        (&mut margins.bottom, "Bottom", "margin-bottom"),
                                        (&mut margins.left, "Left", "margin-left"),
                                    ] {
                                        ui.label(label);
                                        let response = ui.add(
                                            egui::DragValue::new(value)
                                                .range(0.0..=1440.0)
                                                .suffix(" pt"),
                                        );
                                        target!(
                                            ui,
                                            response,
                                            key,
                                            format!("{label} margin"),
                                            TargetKind::TextEntry
                                        );
                                        ui.end_row();
                                    }
                                });
                            }
                            ui.add_space(6.0);
                            ui.strong("Text");
                            ui.horizontal_wrapped(|ui| {
                                for (value, label, key) in [
                                    (&mut controls.settings.body_size, "Body", "body-size"),
                                    (&mut controls.settings.code_size, "Code", "code-size"),
                                ] {
                                    ui.label(label);
                                    let response = ui.add(
                                        egui::DragValue::new(value).range(6.0..=48.0).suffix(" pt"),
                                    );
                                    target!(
                                        ui,
                                        response,
                                        key,
                                        format!("{label} font size"),
                                        TargetKind::TextEntry
                                    );
                                }
                            });
                            for (text, label, key) in [
                                (&mut controls.settings.title, "Title (optional)", "title"),
                                (&mut controls.settings.header, "Header", "header"),
                                (&mut controls.settings.footer, "Footer", "footer"),
                            ] {
                                let label_response = ui.label(label);
                                let response = ui.add(
                                    egui::TextEdit::singleline(text)
                                        .char_limit(512)
                                        .desired_width(settings_width - 20.0),
                                );
                                response.clone().labelled_by(label_response.id);
                                target!(ui, response, key, label, TargetKind::TextEntry);
                            }
                            ui.weak("Header and footer tokens: {title}, {date}, {page}, {pages}");
                        });
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(page_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let count = self
                        .receipt
                        .as_ref()
                        .map_or(0, |receipt| receipt.scene.pages.len());
                    ui.horizontal_wrapped(|ui| {
                        let previous = ui
                            .add_enabled(actions.controls.page > 0, egui::Button::new("Previous"));
                        target!(
                            ui,
                            previous,
                            "previous-page",
                            "Previous page",
                            TargetKind::Button
                        );
                        if previous.clicked() {
                            actions.controls.page -= 1;
                        }
                        if count == 0 {
                            ui.label("Page preview");
                        } else {
                            ui.label(format!("Page {} of {count}", actions.controls.page + 1));
                        }
                        let next = ui.add_enabled(
                            actions.controls.page + 1 < count,
                            egui::Button::new("Next"),
                        );
                        target!(ui, next, "next-page", "Next page", TargetKind::Button);
                        if next.clicked() {
                            actions.controls.page += 1;
                        }
                        let fit = ui.checkbox(&mut actions.controls.fit, "Fit page");
                        target!(ui, fit, "fit-page", "Fit page", TargetKind::Checkbox);
                        let zoom = ui.add_enabled(
                            !actions.controls.fit,
                            egui::DragValue::new(&mut actions.controls.zoom)
                                .range(20..=300)
                                .suffix("%"),
                        );
                        target!(ui, zoom, "zoom", "Zoom percent", TargetKind::TextEntry);
                    });
                    if let Some(receipt) = &self.receipt
                        && !receipt.scene.warnings.is_empty()
                    {
                        let warnings = ui.collapsing(
                            format!("Warnings ({})", receipt.scene.warnings.len()),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(98.0)
                                    .show(ui, |ui| {
                                        for warning in &receipt.scene.warnings {
                                            ui.label(&warning.message);
                                        }
                                    });
                            },
                        );
                        let header = warnings.header_response;
                        target!(ui, header, "warnings", "Preview warnings", TargetKind::Button);
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    } else if self.serial.is_some() {
                        ui.label("Preparing preview…");
                    }
                    // Worker status and warning rows do not participate in Fit sizing.
                    let page_height = (height - 112.0).max(120.0);
                    actions.display_scale = actions
                        .controls
                        .display_scale(egui::vec2(page_width - 12.0, page_height - 12.0));
                    if let Some(texture) = &self.image
                        && let Some(geometry) = self
                            .receipt
                            .as_ref()
                            .and_then(|receipt| receipt.scene.pages.get(self.controls.page))
                            .map(|page| page.geometry)
                    {
                        let size = self.display_scale * egui::vec2(geometry.width, geometry.height);
                        egui::ScrollArea::both()
                            .id_salt("print-page")
                            .max_width(page_width)
                            .max_height(page_height)
                            .show(ui, |ui| {
                                ui.add(egui::Image::new((texture.id(), size)));
                            });
                    }
                },
            );
        });
    }
}
