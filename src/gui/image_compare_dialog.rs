use crate::image_compare::{
    DifferenceClass, ImageBudgetLease, ImageCompareSession, ImageCompareSnapshot, ImageInput,
    ImageLoadStatus, ImageSessionError, ImageSide,
};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

const WINDOW_TITLE: &str = "Image Comparison";
const PRESENTATION_TEXTURE_OPTIONS: egui::TextureOptions = egui::TextureOptions::LINEAR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareView {
    A,
    B,
    Wipe,
    Difference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PairKey {
    ids: [u64; 2],
    revisions: [u64; 2],
}

impl PairKey {
    fn from_snapshot(snapshot: &ImageCompareSnapshot) -> Option<Self> {
        let a = snapshot.input_a.as_ref()?;
        let b = snapshot.input_b.as_ref()?;
        let comparison = snapshot.comparison.as_ref()?;
        let ids = [a.id().get(), b.id().get()];
        let revisions = [a.revision(), b.revision()];
        (comparison.input_ids().0.get() == ids[0]
            && comparison.input_ids().1.get() == ids[1]
            && comparison.input_revisions() == (revisions[0], revisions[1]))
            .then_some(Self { ids, revisions })
    }
}

struct PreparedPresentation {
    key: PairKey,
    a: Arc<egui::ColorImage>,
    b: Arc<egui::ColorImage>,
    difference: Arc<egui::ColorImage>,
    counts: [usize; 5],
    _reservation: ImageBudgetLease,
}

struct PresentedTextures {
    prepared: Arc<PreparedPresentation>,
    a: egui::TextureHandle,
    b: egui::TextureHandle,
    difference: egui::TextureHandle,
}

struct RetiredUpload {
    prepared: Arc<PreparedPresentation>,
    /// egui_glow applies texture `free` updates after painting the current
    /// frame's callbacks. Requiring two actual root paints after dropping the
    /// handles proves that a later paint ran after the free-bearing paint.
    after_paint_serial: Option<u64>,
}

impl RetiredUpload {
    fn ready(&self, paint_serial: u64) -> bool {
        let cpu_references_drained = [
            &self.prepared.a,
            &self.prepared.b,
            &self.prepared.difference,
        ]
        .iter()
        .all(|image| Arc::strong_count(image) == 1);
        retired_upload_ready(
            cpu_references_drained,
            self.after_paint_serial,
            paint_serial,
        )
    }
}

impl PresentedTextures {
    fn key(&self) -> PairKey {
        self.prepared.key
    }

    fn retire(self) -> Arc<PreparedPresentation> {
        let Self {
            prepared,
            a,
            b,
            difference,
        } = self;
        drop((a, b, difference));
        prepared
    }
}

struct PresentationJob {
    generation: u64,
    key: PairKey,
    a: ImageInput,
    b: ImageInput,
    comparison: crate::image_compare::ImageComparison,
    session: Arc<ImageCompareSession>,
    cancelled: Arc<AtomicBool>,
}

struct PresentationState {
    shutdown: bool,
    generation: u64,
    desired: Option<PairKey>,
    pending: Option<PresentationJob>,
    active_generation: Option<u64>,
    active_cancel: Option<Arc<AtomicBool>>,
    completed: Option<(PairKey, Result<Arc<PreparedPresentation>, String>)>,
}

struct PresentationShared {
    state: Mutex<PresentationState>,
    changed: Condvar,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
}

struct PresentationWorker {
    shared: Arc<PresentationShared>,
    owner: Mutex<Option<JoinHandle<()>>>,
    reap_permit: Mutex<Option<crate::thread_reaper::ReapPermit>>,
    retired_uploads: Mutex<Vec<RetiredUpload>>,
    paint_serial: Arc<AtomicU64>,
}

impl PresentationWorker {
    fn new(wake: Arc<dyn Fn() + Send + Sync + 'static>) -> Self {
        Self {
            shared: Arc::new(PresentationShared {
                state: Mutex::new(PresentationState {
                    shutdown: false,
                    generation: 0,
                    desired: None,
                    pending: None,
                    active_generation: None,
                    active_cancel: None,
                    completed: None,
                }),
                changed: Condvar::new(),
                wake,
            }),
            owner: Mutex::new(None),
            reap_permit: Mutex::new(None),
            retired_uploads: Mutex::new(Vec::new()),
            paint_serial: Arc::new(AtomicU64::new(0)),
        }
    }

    fn ensure_worker(&self) -> Result<(), String> {
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| "image presentation worker lock was poisoned".to_owned())?;
        if owner.is_some() {
            return Ok(());
        }
        let permit = crate::thread_reaper::reserve().map_err(|error| {
            format!("image presentation worker capacity is unavailable: {error}")
        })?;
        let completion = permit.completion_notifier();
        let shared = Arc::clone(&self.shared);
        let join = thread::Builder::new()
            .name("image-compare-presentation".into())
            .spawn(move || {
                let _completion = completion;
                presentation_worker_loop(shared);
            })
            .map_err(|error| error.to_string())?;
        *self
            .reap_permit
            .lock()
            .map_err(|_| "image presentation reaper lock was poisoned".to_owned())? = Some(permit);
        *owner = Some(join);
        Ok(())
    }

    fn request(
        &self,
        session: Arc<ImageCompareSession>,
        snapshot: &ImageCompareSnapshot,
    ) -> Result<(), String> {
        let key = PairKey::from_snapshot(snapshot)
            .ok_or_else(|| "the current image pair is not ready for presentation".to_owned())?;
        {
            let state = self
                .shared
                .state
                .lock()
                .map_err(|_| "image presentation state lock was poisoned".to_owned())?;
            if state.shutdown {
                return Err("image presentation is shutting down".into());
            }
            if state.desired == Some(key)
                || state
                    .completed
                    .as_ref()
                    .is_some_and(|(completed_key, _)| *completed_key == key)
            {
                return Ok(());
            }
        }
        self.ensure_worker()?;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "image presentation state lock was poisoned".to_owned())?;
        if state.shutdown {
            return Err("image presentation is shutting down".into());
        }
        if state.desired == Some(key) {
            return Ok(());
        }
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        if let Some(cancelled) = state.active_cancel.as_ref() {
            cancelled.store(true, Ordering::Release);
        }
        let a = snapshot
            .input_a
            .clone()
            .ok_or_else(|| "image A is not ready".to_owned())?;
        let b = snapshot
            .input_b
            .clone()
            .ok_or_else(|| "image B is not ready".to_owned())?;
        let comparison = snapshot
            .comparison
            .clone()
            .ok_or_else(|| "the image comparison is not ready".to_owned())?;
        let cancelled = Arc::new(AtomicBool::new(false));
        state.desired = Some(key);
        state.completed = None;
        state.pending = Some(PresentationJob {
            generation,
            key,
            a,
            b,
            comparison,
            session,
            cancelled,
        });
        drop(state);
        self.shared.changed.notify_one();
        Ok(())
    }

    fn invalidate(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.generation = state.generation.wrapping_add(1);
            state.desired = None;
            state.pending = None;
            state.completed = None;
            if let Some(cancelled) = state.active_cancel.as_ref() {
                cancelled.store(true, Ordering::Release);
            }
        }
        self.shared.changed.notify_one();
    }

    fn take_completed(&self) -> Option<(PairKey, Result<Arc<PreparedPresentation>, String>)> {
        self.shared.state.lock().ok()?.completed.take()
    }

    fn has_request(&self, key: PairKey) -> bool {
        self.shared.state.lock().is_ok_and(|state| {
            state.desired == Some(key)
                && (state.pending.is_some()
                    || state.active_generation.is_some()
                    || state.completed.is_some())
        })
    }

    fn generation(&self) -> u64 {
        self.shared
            .state
            .lock()
            .map(|state| state.generation)
            .unwrap_or(u64::MAX)
    }

    fn retire(&self, presentation: PresentedTextures) {
        let prepared = presentation.retire();
        self.retire_prepared(prepared);
    }

    fn retire_prepared(&self, prepared: Arc<PreparedPresentation>) {
        let after_paint_serial = Some(self.paint_serial.load(Ordering::Acquire).saturating_add(2));
        if let Ok(mut retired) = self.retired_uploads.lock() {
            retired.push(RetiredUpload {
                prepared,
                after_paint_serial,
            });
        }
    }

    fn poll_retired(&self) -> bool {
        let Ok(mut retired) = self.retired_uploads.lock() else {
            return true;
        };
        let paint_serial = self.paint_serial.load(Ordering::Acquire);
        retired.retain(|upload| !upload.ready(paint_serial));
        !retired.is_empty()
    }

    fn needs_paint_fence(&self) -> bool {
        let paint_serial = self.paint_serial.load(Ordering::Acquire);
        self.retired_uploads.lock().is_ok_and(|retired| {
            retired.iter().any(|upload| {
                upload
                    .after_paint_serial
                    .is_some_and(|required| paint_serial < required)
            })
        })
    }

    fn add_paint_fence(&self, painter: &egui::Painter) {
        if !self.needs_paint_fence() {
            return;
        }
        let paint_serial = Arc::clone(&self.paint_serial);
        let callback = Arc::new(eframe::egui_glow::CallbackFn::new(move |_, _| {
            paint_serial.fetch_add(1, Ordering::AcqRel);
        }));
        painter.add(egui::Shape::Callback(egui::PaintCallback {
            rect: painter.clip_rect(),
            callback,
        }));
    }

    fn shutdown(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.shutdown = true;
            state.generation = state.generation.wrapping_add(1);
            state.pending = None;
            state.completed = None;
            if let Some(cancelled) = state.active_cancel.as_ref() {
                cancelled.store(true, Ordering::Release);
            }
        }
        self.shared.changed.notify_all();
        let join = self.owner.lock().ok().and_then(|mut owner| owner.take());
        let permit = self
            .reap_permit
            .lock()
            .ok()
            .and_then(|mut permit| permit.take());
        if let (Some(join), Some(permit)) = (join, permit)
            && let Err(error) = permit.reap(join)
        {
            tracing::error!(%error, "failed to transfer image presentation worker to join reaper");
        }
    }
}

impl Drop for PresentationWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct ImageCompareDialogState {
    pub open: bool,
    session: Arc<ImageCompareSession>,
    presentation_worker: PresentationWorker,
    presented: Option<PresentedTextures>,
    retired_uploads_pending: bool,
    view: CompareView,
    wipe_fraction: f32,
    zoom: f32,
    pan: egui::Vec2,
    status: Option<String>,
    last_pair_error: Option<(PairKey, String)>,
    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    test_responses: std::collections::HashMap<&'static str, (egui::Id, egui::Rect, bool)>,
    #[cfg(test)]
    test_file_picks: std::collections::VecDeque<Option<PathBuf>>,
    #[cfg(test)]
    test_clipboard_images: std::collections::VecDeque<(u32, u32, Vec<u8>)>,
}

impl ImageCompareDialogState {
    pub fn new(ctx: &egui::Context) -> Self {
        let session_ctx = ctx.clone();
        let session = Arc::new(ImageCompareSession::with_wake(Arc::new(move || {
            session_ctx.request_repaint();
        })));
        let presentation_ctx = ctx.clone();
        Self {
            open: false,
            session,
            presentation_worker: PresentationWorker::new(Arc::new(move || {
                presentation_ctx.request_repaint();
            })),
            presented: None,
            retired_uploads_pending: false,
            view: CompareView::Wipe,
            wipe_fraction: 0.5,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            status: None,
            last_pair_error: None,
            #[cfg(any(test, feature = "dev-smoke-binaries"))]
            test_responses: std::collections::HashMap::new(),
            #[cfg(test)]
            test_file_picks: std::collections::VecDeque::new(),
            #[cfg(test)]
            test_clipboard_images: std::collections::VecDeque::new(),
        }
    }

    pub fn open(&mut self) {
        if self.open {
            return;
        }
        let snapshot = self.session.snapshot();
        if snapshot.closed {
            match self.session.reopen() {
                Ok(()) => self.status = None,
                Err(error) => self.status = Some(image_session_message(&error)),
            }
        }
        self.open = true;
    }

    pub fn close(&mut self) {
        if !self.open && self.session.snapshot().closed {
            return;
        }
        self.open = false;
        self.session.close();
        self.presentation_worker.invalidate();
        if let Some(presented) = self.presented.take() {
            self.presentation_worker.retire(presented);
        }
        self.status = None;
        self.last_pair_error = None;
        self.view = CompareView::Wipe;
        self.wipe_fraction = 0.5;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    pub fn shutdown(&mut self) {
        self.close();
        self.session.close();
        self.presentation_worker.shutdown();
    }

    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        owner: crate::gui::jump_mode::ui::NavigationOwner,
        application_input_allowed: bool,
        native_owner_current: bool,
        first_ui_pass: bool,
    ) {
        self.reap_retired_uploads(ctx);
        if !self.open {
            return;
        }
        if self.session.snapshot().closed {
            match self.session.reopen() {
                Ok(()) => self.status = None,
                Err(error) => self.status = Some(image_session_message(&error)),
            }
        }
        let snapshot = self.session.snapshot();
        self.collect_presentation(ctx, &snapshot);

        let current_instance = instance.filter(|instance| {
            matches!(
                owner,
                crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                    kind: crate::gui::jump_mode::ui::NavigationDialogKind::ImageCompare,
                    instance: owner_instance,
                } if owner_instance == *instance
            )
        });
        let current_scope = scope.zip(current_instance).map(|(scope, instance)| {
            scope.child_namespace("image-compare").with_surface(
                crate::gui::jump_mode::SurfaceId::dialog("image-compare", instance),
                crate::gui::jump_mode::TargetGeneration(image_compare_generation(&snapshot)),
            )
        });
        let parent_layer = crate::gui::jump_mode::ui::NavigationDialogKind::ImageCompare.layer_id();
        let parent_is_current = parent_layer.is_some_and(|layer| {
            crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(layer)
        });
        let owner_current = self.open
            && native_owner_current
            && application_input_allowed
            && parent_is_current
            && !egui::Popup::is_any_open(ctx);
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        self.test_responses.clear();

        let mut window_open = self.open;
        egui::Window::new(WINDOW_TITLE)
            .id(egui::Id::new(Some(WINDOW_TITLE)))
            .collapsible(false)
            .resizable(true)
            .default_size((1100.0, 760.0))
            .min_size((620.0, 430.0))
            .open(&mut window_open)
            .show(ctx, |ui| {
                let visible_owner = owner_current && ui.is_visible() && !ui.is_sizing_pass();
                let controls_enabled = visible_owner && !snapshot.closed;
                let actions_enabled = controls_enabled && first_ui_pass;
                let mut order = 0_u64;
                self.show_controls(
                    ui,
                    &snapshot,
                    controls_enabled,
                    actions_enabled,
                    current_scope.as_ref(),
                    &mut order,
                );
                ui.separator();
                self.show_status(ui, &snapshot);
                ui.separator();
                self.show_canvas(ui, &snapshot, actions_enabled);
            });
        if !window_open {
            self.close();
        }
        let snapshot = self.session.snapshot();
        self.collect_presentation(ctx, &snapshot);
    }

    fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &ImageCompareSnapshot,
        controls_enabled: bool,
        actions_enabled: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
    ) {
        ui.horizontal_wrapped(|ui| {
            let a_file = ui.add_enabled(controls_enabled, egui::Button::new("Open A…"));
            self.record_response("open-a", &a_file);
            register_target(scope, ui, &a_file, "input/open-a", "Open image A", order);
            if a_file.clicked() && actions_enabled {
                self.pick_file(ui.ctx(), ImageSide::A);
            }
            let paste_a = ui.add_enabled(controls_enabled, egui::Button::new("Paste A"));
            self.record_response("paste-a", &paste_a);
            register_target(scope, ui, &paste_a, "input/paste-a", "Paste image A", order);
            if paste_a.clicked() && actions_enabled {
                self.request_paste(ImageSide::A);
            }

            let b_file = ui.add_enabled(controls_enabled, egui::Button::new("Open B…"));
            self.record_response("open-b", &b_file);
            register_target(scope, ui, &b_file, "input/open-b", "Open image B", order);
            if b_file.clicked() && actions_enabled {
                self.pick_file(ui.ctx(), ImageSide::B);
            }
            let paste_b = ui.add_enabled(controls_enabled, egui::Button::new("Paste B"));
            self.record_response("paste-b", &paste_b);
            register_target(scope, ui, &paste_b, "input/paste-b", "Paste image B", order);
            if paste_b.clicked() && actions_enabled {
                self.request_paste(ImageSide::B);
            }
        });

        ui.horizontal_wrapped(|ui| {
            for (label, view, target) in [
                ("A", CompareView::A, "view/a"),
                ("B", CompareView::B, "view/b"),
                ("Wipe", CompareView::Wipe, "view/wipe"),
                ("Difference", CompareView::Difference, "view/difference"),
            ] {
                let response = ui.add_enabled(
                    controls_enabled,
                    egui::Button::selectable(self.view == view, label),
                );
                self.record_response(target, &response);
                register_target(scope, ui, &response, target, label, order);
                if response.clicked() && actions_enabled {
                    self.view = view;
                }
            }

            if self.view == CompareView::Wipe {
                let mut requested_wipe = self.wipe_fraction;
                let wipe = ui.add_enabled(
                    controls_enabled,
                    egui::Slider::new(&mut requested_wipe, 0.0..=1.0).text("A to B wipe"),
                );
                if wipe.changed() && actions_enabled {
                    self.wipe_fraction = requested_wipe;
                }
                self.record_response("view/wipe-fraction", &wipe);
                register_slider_target(
                    scope,
                    ui,
                    &wipe,
                    "view/wipe-fraction",
                    "Set the A/B wipe position",
                    order,
                );
                let a_end = ui.add_enabled(controls_enabled, egui::Button::new("A end"));
                self.record_response("view/wipe-a-end", &a_end);
                register_target(
                    scope,
                    ui,
                    &a_end,
                    "view/wipe-a-end",
                    "Show A at the wipe endpoint",
                    order,
                );
                if a_end.clicked() && actions_enabled {
                    self.wipe_fraction = 0.0;
                }
                let b_end = ui.add_enabled(controls_enabled, egui::Button::new("B end"));
                self.record_response("view/wipe-b-end", &b_end);
                register_target(
                    scope,
                    ui,
                    &b_end,
                    "view/wipe-b-end",
                    "Show B at the wipe endpoint",
                    order,
                );
                if b_end.clicked() && actions_enabled {
                    self.wipe_fraction = 1.0;
                }
            }
            let mut requested_zoom = self.zoom;
            let zoom = ui.add_enabled(
                controls_enabled,
                egui::Slider::new(&mut requested_zoom, 0.1..=8.0).text("Zoom"),
            );
            if zoom.changed() && actions_enabled {
                self.zoom = requested_zoom;
            }
            self.record_response("view/zoom", &zoom);
            register_slider_target(
                scope,
                ui,
                &zoom,
                "view/zoom",
                "Zoom the aligned image canvas",
                order,
            );
        });

        ui.horizontal_wrapped(|ui| {
            for (side, input) in [
                (ImageSide::A, snapshot.input_a.as_ref()),
                (ImageSide::B, snapshot.input_b.as_ref()),
            ] {
                if let Some(input) = input {
                    ui.label(input_caption(side, input));
                }
            }
        });
    }

    fn show_status(&mut self, ui: &mut egui::Ui, snapshot: &ImageCompareSnapshot) {
        if snapshot.closed {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Waiting for the previous image worker to retire…",
            );
            return;
        }
        let mut any_status = false;
        for (label, status) in [("A", &snapshot.status_a), ("B", &snapshot.status_b)] {
            match status {
                ImageLoadStatus::Loading { .. } => {
                    any_status = true;
                    ui.label(format!("{label}: loading image…"));
                }
                ImageLoadStatus::Failed { message, .. } => {
                    any_status = true;
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("{label}: {message}"));
                }
                ImageLoadStatus::Cancelled { .. } => {
                    any_status = true;
                    ui.label(format!(
                        "{label}: replacement cancelled; keeping the last image."
                    ));
                }
                ImageLoadStatus::Empty | ImageLoadStatus::Ready { .. } => {}
            }
        }
        if let Some(status) = self.status.as_ref() {
            any_status = true;
            ui.colored_label(egui::Color32::LIGHT_RED, status);
        }
        let current_key = PairKey::from_snapshot(snapshot);
        let presented_is_current = self
            .presented
            .as_ref()
            .is_some_and(|presented| current_key == Some(presented.key()));
        if self.presented.is_some() && !presented_is_current {
            any_status = true;
            ui.label("The previous comparison remains visible while the current pair is prepared or unavailable.");
        }
        if !any_status
            && presented_is_current
            && let Some(presented) = self.presented.as_ref()
        {
            let counts = presented.prepared.counts;
            ui.label(format!(
                "Exact RGBA comparison · {} changed · {} A-only · {} B-only · {} neither",
                counts[DifferenceClass::Changed as usize],
                counts[DifferenceClass::AOnly as usize],
                counts[DifferenceClass::BOnly as usize],
                counts[DifferenceClass::Neither as usize],
            ));
            ui.horizontal(|ui| {
                legend(ui, egui::Color32::from_rgb(240, 48, 220), "Changed");
                legend(ui, egui::Color32::from_rgb(32, 185, 238), "A only");
                legend(ui, egui::Color32::from_rgb(255, 161, 32), "B only");
                legend(ui, egui::Color32::from_gray(125), "Neither: checkerboard");
            });
        } else if !any_status && (snapshot.input_a.is_some() || snapshot.input_b.is_some()) {
            ui.label("Waiting for both images and their comparison…");
        }
    }

    fn show_canvas(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &ImageCompareSnapshot,
        actions_enabled: bool,
    ) {
        let available = ui.available_size().max(egui::vec2(120.0, 120.0));
        let (canvas_rect, response) = ui.allocate_exact_size(available, egui::Sense::drag());
        self.record_response("canvas", &response);
        if actions_enabled && response.dragged_by(egui::PointerButton::Primary) {
            self.pan += response.drag_delta();
        }
        if actions_enabled && response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(0.1, 8.0);
            }
        }
        paint_checkerboard(ui.painter(), canvas_rect, 14.0);

        let Some(presented) = self.presented.as_ref() else {
            ui.painter().text(
                canvas_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Open or paste one image for A and one for B",
                egui::FontId::proportional(19.0),
                ui.visuals().text_color(),
            );
            return;
        };
        let dimensions = snapshot
            .comparison
            .as_ref()
            .filter(|_| PairKey::from_snapshot(snapshot) == Some(presented.key()))
            .map(|comparison| comparison.dimensions())
            .unwrap_or_else(|| {
                (
                    presented.prepared.difference.size[0] as u32,
                    presented.prepared.difference.size[1] as u32,
                )
            });
        let canvas_size = egui::vec2(dimensions.0 as f32, dimensions.1 as f32);
        let fit = (canvas_rect.width() / canvas_size.x.max(1.0))
            .min(canvas_rect.height() / canvas_size.y.max(1.0))
            .max(0.0001);
        let scale = fit * self.zoom;
        let display_size = canvas_size * scale;
        let display_rect = egui::Rect::from_min_size(
            canvas_rect.center() - display_size * 0.5 + self.pan,
            display_size,
        );
        let clipped = canvas_rect.intersect(display_rect);
        let painter = ui.painter().with_clip_rect(clipped);
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        match self.view {
            CompareView::A => paint_top_left_texture(
                &painter,
                &presented.a,
                display_rect.min,
                scale,
                &presented.prepared.a,
                uv,
            ),
            CompareView::B => paint_top_left_texture(
                &painter,
                &presented.b,
                display_rect.min,
                scale,
                &presented.prepared.b,
                uv,
            ),
            CompareView::Difference => {
                painter.image(
                    presented.difference.id(),
                    display_rect,
                    uv,
                    egui::Color32::WHITE,
                );
            }
            CompareView::Wipe => {
                let (a_clip, b_clip, split) =
                    wipe_clip_regions(canvas_rect, display_rect, self.wipe_fraction);
                let a_display = egui::Rect::from_min_size(
                    display_rect.min,
                    egui::vec2(
                        presented.prepared.a.size[0] as f32 * scale,
                        presented.prepared.a.size[1] as f32 * scale,
                    ),
                );
                let b_display = egui::Rect::from_min_size(
                    display_rect.min,
                    egui::vec2(
                        presented.prepared.b.size[0] as f32 * scale,
                        presented.prepared.b.size[1] as f32 * scale,
                    ),
                );
                ui.painter().with_clip_rect(b_clip).image(
                    presented.b.id(),
                    b_display,
                    uv,
                    egui::Color32::WHITE,
                );
                ui.painter().with_clip_rect(a_clip).image(
                    presented.a.id(),
                    a_display,
                    uv,
                    egui::Color32::WHITE,
                );
                let divider = egui::pos2(split, display_rect.top());
                if self.wipe_fraction > 0.0 && self.wipe_fraction < 1.0 {
                    ui.painter().line_segment(
                        [divider, egui::pos2(split, display_rect.bottom())],
                        egui::Stroke::new(1.5, egui::Color32::WHITE),
                    );
                }
            }
        }
    }

    fn collect_presentation(&mut self, ctx: &egui::Context, snapshot: &ImageCompareSnapshot) {
        if let Some((key, result)) = self.presentation_worker.take_completed() {
            if !self.open || snapshot.closed || PairKey::from_snapshot(snapshot) != Some(key) {
                if let Ok(prepared) = result {
                    self.retain_prepared(prepared);
                }
            } else {
                match result {
                    Ok(prepared) => match make_textures(ctx, Arc::clone(&prepared)) {
                        Ok(new) => {
                            if let Some(old) = self.presented.replace(new) {
                                self.presentation_worker.retire(old);
                            }
                            self.status = None;
                            self.last_pair_error = None;
                        }
                        Err(error) => {
                            self.retain_prepared(prepared);
                            self.status = Some(error.clone());
                            self.last_pair_error = Some((key, error));
                        }
                    },
                    Err(error) => {
                        self.status = Some(error.clone());
                        self.last_pair_error = Some((key, error));
                    }
                }
            }
        }
        let Some(key) = PairKey::from_snapshot(snapshot) else {
            return;
        };
        if self
            .presented
            .as_ref()
            .is_some_and(|presented| presented.key() == key)
            || self.presentation_worker.has_request(key)
            || self
                .last_pair_error
                .as_ref()
                .is_some_and(|(failed_key, _)| *failed_key == key)
        {
            return;
        }
        match self
            .presentation_worker
            .request(Arc::clone(&self.session), snapshot)
        {
            Ok(()) => self.status = Some("Preparing the bounded comparison view…".into()),
            Err(error) => {
                self.status = Some(error.clone());
                self.last_pair_error = Some((key, error));
            }
        }
    }

    fn retain_prepared(&mut self, prepared: Arc<PreparedPresentation>) {
        if let Ok(mut retired) = self.presentation_worker.retired_uploads.lock() {
            retired.push(RetiredUpload {
                prepared,
                after_paint_serial: None,
            });
        }
    }

    fn reap_retired_uploads(&mut self, ctx: &egui::Context) {
        self.retired_uploads_pending = self.presentation_worker.poll_retired();
        if self.retired_uploads_pending {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn pick_file(&mut self, ctx: &egui::Context, side: ImageSide) {
        let generation = self.presentation_worker.generation();
        #[cfg(test)]
        let selected = self.test_file_picks.pop_front().unwrap_or_else(|| {
            rfd::FileDialog::new()
                .add_filter(
                    "Supported images",
                    crate::image_compare::SUPPORTED_IMAGE_FILE_EXTENSIONS,
                )
                .pick_file()
        });
        #[cfg(not(test))]
        let selected = rfd::FileDialog::new()
            .add_filter(
                "Supported images",
                crate::image_compare::SUPPORTED_IMAGE_FILE_EXTENSIONS,
            )
            .pick_file();
        let still_current = self.open
            && generation == self.presentation_worker.generation()
            && crate::gui::jump_mode::ui::NavigationDialogKind::ImageCompare
                .layer_id()
                .is_some_and(|layer| {
                    crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(layer)
                });
        if !still_current {
            return;
        }
        if let Some(path) = selected {
            self.replace_file(side, path);
        }
    }

    fn replace_file(&mut self, side: ImageSide, path: PathBuf) {
        match self.session.replace_file(side, path) {
            Ok(_) => {
                self.status = None;
                self.last_pair_error = None;
            }
            Err(error) => self.status = Some(image_session_message(&error)),
        }
    }

    fn request_paste(&mut self, side: ImageSide) {
        #[cfg(test)]
        let result = if let Some((width, height, pixels)) = self.test_clipboard_images.pop_front() {
            self.session
                .submit_raw_for_test(side, width, height, pixels, "test clipboard image")
        } else {
            self.session.paste_image(side)
        };
        #[cfg(not(test))]
        let result = self.session.paste_image(side);
        match result {
            Ok(_) => {
                self.status = Some(format!(
                    "Loading image {} from the clipboard…",
                    side_label(side)
                ));
                self.last_pair_error = None;
            }
            Err(error) => self.status = Some(image_session_message(&error)),
        }
    }

    fn record_response(&mut self, key: &'static str, response: &egui::Response) {
        #[cfg(any(test, feature = "dev-smoke-binaries"))]
        self.test_responses.insert(
            key,
            (response.id, response.interact_rect, response.enabled()),
        );
        #[cfg(not(any(test, feature = "dev-smoke-binaries")))]
        let _ = (key, response);
    }

    #[cfg(test)]
    pub(crate) fn response_for_test(
        &self,
        key: &'static str,
    ) -> Option<(egui::Id, egui::Rect, bool)> {
        self.test_responses.get(key).copied()
    }

    #[cfg(test)]
    pub(crate) fn test_view_name(&self) -> &'static str {
        match self.view {
            CompareView::A => "a",
            CompareView::B => "b",
            CompareView::Wipe => "wipe",
            CompareView::Difference => "difference",
        }
    }

    #[cfg(test)]
    pub(crate) fn test_wipe_fraction(&self) -> f32 {
        self.wipe_fraction
    }

    #[cfg(test)]
    pub(crate) fn queue_file_pick_for_test(&mut self, selected: Option<PathBuf>) {
        self.test_file_picks.push_back(selected);
    }

    #[cfg(test)]
    pub(crate) fn queue_clipboard_image_for_test(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) {
        self.test_clipboard_images
            .push_back((width, height, pixels));
    }

    #[cfg(feature = "dev-smoke-binaries")]
    pub(crate) fn load_smoke_inputs(&mut self, a: PathBuf, b: PathBuf) -> Result<(), String> {
        self.open();
        self.session
            .replace_file(ImageSide::A, a)
            .map_err(|error| error.to_string())?;
        self.session
            .replace_file(ImageSide::B, b)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(feature = "dev-smoke-binaries")]
    pub(crate) fn smoke_observation(
        &self,
        ctx: &egui::Context,
        fixture_identity: &str,
    ) -> serde_json::Value {
        let snapshot = self.session.snapshot();
        let current_key = PairKey::from_snapshot(&snapshot);
        let presentation_is_current = self
            .presented
            .as_ref()
            .is_some_and(|presented| current_key == Some(presented.key()));
        let controls: serde_json::Map<String, serde_json::Value> = self
            .test_responses
            .iter()
            .map(|(key, (id, rect, enabled))| {
                (
                    (*key).to_owned(),
                    serde_json::json!({
                        "response_id": format!("{id:?}"),
                        "rect_points": [[rect.left(), rect.top()], [rect.right(), rect.bottom()]],
                        "enabled": enabled,
                        "visible": rect.is_positive(),
                    }),
                )
            })
            .collect();
        let input = |input: Option<&ImageInput>| {
            input.map(|input| {
                let (width, height) = input.dimensions();
                serde_json::json!({
                    "id": input.id().get(),
                    "revision": input.revision(),
                    "dimensions": [width, height],
                    "name": input.source().display_name(),
                })
            })
        };
        let counts = self
            .presented
            .as_ref()
            .filter(|_| presentation_is_current)
            .map(|presented| {
                serde_json::json!({
                    "equal": presented.prepared.counts[DifferenceClass::Equal as usize],
                    "changed": presented.prepared.counts[DifferenceClass::Changed as usize],
                    "a_only": presented.prepared.counts[DifferenceClass::AOnly as usize],
                    "b_only": presented.prepared.counts[DifferenceClass::BOnly as usize],
                    "neither": presented.prepared.counts[DifferenceClass::Neither as usize],
                })
            });
        serde_json::json!({
            "schema_version": 1,
            "fixture_identity": fixture_identity,
            "process_id": std::process::id(),
            "frame_id": ctx.cumulative_frame_nr(),
            "pixels_per_point": ctx.pixels_per_point(),
            "dialog_open": self.open,
            "session_closed": snapshot.closed,
            "budget_used_bytes": self.session.budget_snapshot().used_bytes,
            "input_a": input(snapshot.input_a.as_ref()),
            "input_b": input(snapshot.input_b.as_ref()),
            "comparison_ready": snapshot.comparison.is_some(),
            "presentation_current": presentation_is_current,
            "view": match self.view {
                CompareView::A => "a",
                CompareView::B => "b",
                CompareView::Wipe => "wipe",
                CompareView::Difference => "difference",
            },
            "wipe_fraction": self.wipe_fraction,
            "counts": counts,
            "controls": controls,
        })
    }

    #[cfg(feature = "dev-smoke-binaries")]
    pub(crate) fn smoke_retirement_observation(&self) -> serde_json::Value {
        let retired_uploads = self
            .presentation_worker
            .retired_uploads
            .lock()
            .map(|uploads| uploads.len())
            .unwrap_or(usize::MAX);
        serde_json::json!({
            "schema_version": 1,
            "session_closed": self.session.snapshot().closed,
            "budget_used_bytes": self.session.budget_snapshot().used_bytes,
            "retired_uploads": retired_uploads,
            "presented_pair_retained": self.presented.is_some(),
        })
    }

    pub(crate) fn paint_retirement_fence(&self, painter: &egui::Painter) {
        self.presentation_worker.add_paint_fence(painter);
    }
}

fn presentation_worker_loop(shared: Arc<PresentationShared>) {
    loop {
        let job = {
            let Ok(state) = shared.state.lock() else {
                return;
            };
            let mut state = match shared
                .changed
                .wait_while(state, |state| !state.shutdown && state.pending.is_none())
            {
                Ok(state) => state,
                Err(_) => return,
            };
            if state.shutdown {
                return;
            }
            let Some(job) = state.pending.take() else {
                continue;
            };
            state.active_generation = Some(job.generation);
            state.active_cancel = Some(Arc::clone(&job.cancelled));
            job
        };
        let result = prepare_presentation(&job);
        if let Ok(mut state) = shared.state.lock() {
            if state.active_generation == Some(job.generation) {
                state.active_generation = None;
                state.active_cancel = None;
            }
            if !state.shutdown
                && state.generation == job.generation
                && state.desired == Some(job.key)
                && !job.cancelled.load(Ordering::Acquire)
            {
                state.completed = Some((job.key, result));
            }
        }
        (shared.wake)();
    }
}

fn prepare_presentation(job: &PresentationJob) -> Result<Arc<PreparedPresentation>, String> {
    let (a_width, a_height) = job.a.dimensions();
    let (b_width, b_height) = job.b.dimensions();
    let (width, height) = job.comparison.dimensions();
    let pixel_count = checked_pixel_count(a_width, a_height)?
        .checked_add(checked_pixel_count(b_width, b_height)?)
        .and_then(|count| count.checked_add(checked_pixel_count(width, height).ok()?))
        .ok_or_else(|| "the display representation is too large".to_owned())?;
    // The lease covers the CPU color images and an equal-sized GPU upload
    // allowance before any presentation buffer is allocated.
    let reserved_bytes = pixel_count
        .checked_mul(8)
        .ok_or_else(|| "the display representation size overflowed".to_owned())?;
    let reservation = job
        .session
        .reserve_retained_representation(reserved_bytes)
        .map_err(|error| error.to_string())?;
    let a = rgba_color_image(a_width, a_height, job.a.pixels(), &job.cancelled)?;
    let b = rgba_color_image(b_width, b_height, job.b.pixels(), &job.cancelled)?;
    let (difference, counts) =
        difference_color_image(width, height, job.comparison.classes(), &job.cancelled)?;
    Ok(Arc::new(PreparedPresentation {
        key: job.key,
        a: Arc::new(a),
        b: Arc::new(b),
        difference: Arc::new(difference),
        counts,
        _reservation: reservation,
    }))
}

fn checked_pixel_count(width: u32, height: u32) -> Result<usize, String> {
    usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "image display dimensions overflow addressable memory".to_owned())
}

fn rgba_color_image(
    width: u32,
    height: u32,
    rgba: &[u8],
    cancelled: &AtomicBool,
) -> Result<egui::ColorImage, String> {
    let count = checked_pixel_count(width, height)?;
    let expected = count
        .checked_mul(4)
        .ok_or_else(|| "image display byte length overflowed".to_owned())?;
    if rgba.len() != expected {
        return Err("the decoded image has an invalid pixel buffer".into());
    }
    let mut pixels = Vec::with_capacity(count);
    for (row_index, row) in rgba.chunks_exact(width as usize * 4).enumerate() {
        if cancelled.load(Ordering::Acquire) {
            return Err("image presentation was superseded".into());
        }
        let _ = row_index;
        pixels.extend(row.chunks_exact(4).map(|pixel| {
            egui::Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3])
        }));
    }
    Ok(egui::ColorImage::new(
        [width as usize, height as usize],
        pixels,
    ))
}

fn difference_color_image(
    width: u32,
    height: u32,
    classes: &[DifferenceClass],
    cancelled: &AtomicBool,
) -> Result<(egui::ColorImage, [usize; 5]), String> {
    let count = checked_pixel_count(width, height)?;
    if classes.len() != count {
        return Err("the comparison mask has an invalid pixel buffer".into());
    }
    let mut counts = [0usize; 5];
    let mut pixels = Vec::with_capacity(count);
    for row in classes.chunks_exact(width as usize) {
        if cancelled.load(Ordering::Acquire) {
            return Err("image presentation was superseded".into());
        }
        for class in row {
            counts[*class as usize] += 1;
            pixels.push(match class {
                DifferenceClass::Equal => egui::Color32::from_rgba_unmultiplied(80, 200, 110, 110),
                DifferenceClass::Changed => egui::Color32::from_rgb(240, 48, 220),
                DifferenceClass::AOnly => egui::Color32::from_rgb(32, 185, 238),
                DifferenceClass::BOnly => egui::Color32::from_rgb(255, 161, 32),
                DifferenceClass::Neither => egui::Color32::TRANSPARENT,
            });
        }
    }
    Ok((
        egui::ColorImage::new([width as usize, height as usize], pixels),
        counts,
    ))
}

fn make_textures(
    ctx: &egui::Context,
    prepared: Arc<PreparedPresentation>,
) -> Result<PresentedTextures, String> {
    let max_texture_side = ctx.input(|input| input.max_texture_side);
    validate_texture_dimensions(
        max_texture_side,
        [prepared.a.size, prepared.b.size, prepared.difference.size],
    )?;
    let a = ctx.load_texture(
        format!("image-compare-a-{}", prepared.key.ids[0]),
        egui::ImageData::Color(Arc::clone(&prepared.a)),
        PRESENTATION_TEXTURE_OPTIONS,
    );
    let b = ctx.load_texture(
        format!("image-compare-b-{}", prepared.key.ids[1]),
        egui::ImageData::Color(Arc::clone(&prepared.b)),
        PRESENTATION_TEXTURE_OPTIONS,
    );
    let difference = ctx.load_texture(
        format!(
            "image-compare-difference-{}-{}",
            prepared.key.ids[0], prepared.key.ids[1]
        ),
        egui::ImageData::Color(Arc::clone(&prepared.difference)),
        PRESENTATION_TEXTURE_OPTIONS,
    );
    Ok(PresentedTextures {
        prepared,
        a,
        b,
        difference,
    })
}

fn validate_texture_dimensions(
    max_texture_side: usize,
    dimensions: [[usize; 2]; 3],
) -> Result<(), String> {
    if !texture_dimensions_fit(max_texture_side, dimensions) {
        if max_texture_side == 0 {
            return Err("The renderer reports no usable texture capacity.".into());
        }
        let [width, height] = dimensions
            .into_iter()
            .find(|[width, height]| {
                *width == 0
                    || *height == 0
                    || *width > max_texture_side
                    || *height > max_texture_side
            })
            .unwrap_or([0, 0]);
        return Err(format!(
            "An image or comparison layer ({width} × {height}) exceeds the renderer's maximum texture side ({max_texture_side})."
        ));
    }
    Ok(())
}

fn texture_dimensions_fit(max_texture_side: usize, dimensions: [[usize; 2]; 3]) -> bool {
    max_texture_side > 0
        && dimensions.into_iter().all(|[width, height]| {
            width > 0 && height > 0 && width <= max_texture_side && height <= max_texture_side
        })
}

fn retired_upload_ready(
    cpu_references_drained: bool,
    after_paint_serial: Option<u64>,
    paint_serial: u64,
) -> bool {
    cpu_references_drained && after_paint_serial.is_none_or(|required| paint_serial >= required)
}

fn wipe_clip_regions(
    canvas_rect: egui::Rect,
    display_rect: egui::Rect,
    wipe_fraction: f32,
) -> (egui::Rect, egui::Rect, f32) {
    let visible_display = canvas_rect.intersect(display_rect);
    let split = display_rect.left() + display_rect.width() * (1.0 - wipe_fraction.clamp(0.0, 1.0));
    let a_clip = visible_display.intersect(egui::Rect::from_min_max(
        visible_display.min,
        egui::pos2(split, visible_display.bottom()),
    ));
    let b_clip = visible_display.intersect(egui::Rect::from_min_max(
        egui::pos2(split, visible_display.top()),
        visible_display.max,
    ));
    (a_clip, b_clip, split)
}

fn paint_top_left_texture(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    canvas_top_left: egui::Pos2,
    scale: f32,
    image: &egui::ColorImage,
    uv: egui::Rect,
) {
    let size = egui::vec2(image.size[0] as f32 * scale, image.size[1] as f32 * scale);
    painter.image(
        texture.id(),
        egui::Rect::from_min_size(canvas_top_left, size),
        uv,
        egui::Color32::WHITE,
    );
}

fn paint_checkerboard(painter: &egui::Painter, rect: egui::Rect, cell: f32) {
    let light = egui::Color32::from_gray(72);
    let dark = egui::Color32::from_gray(48);
    let x_count = (rect.width() / cell).ceil().max(1.0) as usize;
    let y_count = (rect.height() / cell).ceil().max(1.0) as usize;
    for y in 0..y_count {
        for x in 0..x_count {
            let min = rect.min + egui::vec2(x as f32 * cell, y as f32 * cell);
            let tile = egui::Rect::from_min_size(min, egui::vec2(cell, cell)).intersect(rect);
            let color = if (x + y) % 2 == 0 { light } else { dark };
            painter.rect_filled(tile, 0.0, color);
        }
    }
}

fn legend(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, color);
        ui.label(text);
    });
}

fn register_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    id: impl AsRef<str>,
    label: impl Into<String>,
    order: &mut u64,
) {
    register_target_kind(
        scope,
        ui,
        response,
        id,
        label,
        crate::gui::jump_mode::TargetKind::Button,
        order,
    );
}

fn register_slider_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    id: impl AsRef<str>,
    label: impl Into<String>,
    order: &mut u64,
) {
    register_target_kind(
        scope,
        ui,
        response,
        id,
        label,
        crate::gui::jump_mode::TargetKind::Slider,
        order,
    );
}

fn register_target_kind(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    id: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    order: &mut u64,
) {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope, ui, response, id, label, kind, order,
    );
}

fn image_session_message(error: &ImageSessionError) -> String {
    match error {
        ImageSessionError::RetiringWorker => {
            "The previous image operation is still retiring; try again in a moment.".into()
        }
        other => other.to_string(),
    }
}

fn side_label(side: ImageSide) -> &'static str {
    match side {
        ImageSide::A => "A",
        ImageSide::B => "B",
    }
}

fn input_caption(side: ImageSide, input: &ImageInput) -> String {
    let (width, height) = input.dimensions();
    format!(
        "Current input {}: {} ({} × {})",
        side_label(side),
        input.source().display_name(),
        width,
        height,
    )
}

fn image_compare_generation(snapshot: &ImageCompareSnapshot) -> u64 {
    snapshot.revisions[0].rotate_left(17) ^ snapshot.revisions[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imgdiff_texture_admission_rejects_long_thin_layers_before_upload() {
        assert!(texture_dimensions_fit(
            16_384,
            [[320, 240], [360, 220], [360, 240]],
        ));
        assert!(!texture_dimensions_fit(
            16_384,
            [[100_000, 1], [1, 1], [100_000, 1]],
        ));
        assert!(!texture_dimensions_fit(0, [[1, 1]; 3]));
        assert!(
            validate_texture_dimensions(16_384, [[100_000, 1], [1, 1], [3, 1]])
                .unwrap_err()
                .contains("maximum texture side")
        );
    }

    #[test]
    fn imgdiff_retired_presentation_waits_for_cpu_and_glow_free_fence() {
        assert!(!retired_upload_ready(false, Some(2), 2));
        assert!(!retired_upload_ready(true, Some(2), 1));
        assert!(retired_upload_ready(true, Some(2), 2));
        assert!(retired_upload_ready(true, None, 0));
    }

    #[test]
    fn imgdiff_retired_upload_keeps_budget_until_cpu_and_actual_paint_acks() {
        let budget = crate::image_compare::ImageCompareBudget::with_limit(256);
        let reservation = budget.reserve_retained_representation(128).unwrap();
        let a = Arc::new(egui::ColorImage::new(
            [1, 1],
            vec![egui::Color32::from_rgb(10, 20, 30)],
        ));
        let b = Arc::new(egui::ColorImage::new(
            [1, 1],
            vec![egui::Color32::from_rgb(40, 50, 60)],
        ));
        let difference = Arc::new(egui::ColorImage::new(
            [1, 1],
            vec![egui::Color32::from_rgb(240, 48, 220)],
        ));
        let retained_cpu = Arc::clone(&a);
        let prepared = Arc::new(PreparedPresentation {
            key: PairKey {
                ids: [1, 2],
                revisions: [1, 1],
            },
            a,
            b,
            difference,
            counts: [0, 1, 0, 0, 0],
            _reservation: reservation,
        });
        let worker = PresentationWorker::new(Arc::new(|| {}));

        // This is the same owner queue used after dropping the actual
        // TextureHandles. The test drives its paint serial exactly as the
        // no-op Glow callback does, without needing a native renderer.
        worker.retire_prepared(Arc::clone(&prepared));
        drop(prepared);
        assert_eq!(budget.snapshot().used_bytes, 128);
        assert!(worker.poll_retired());

        worker.paint_serial.fetch_add(1, Ordering::AcqRel);
        assert!(worker.poll_retired(), "one paint is before the free fence");
        worker.paint_serial.fetch_add(1, Ordering::AcqRel);
        assert!(
            worker.poll_retired(),
            "an external CPU image reference is live"
        );
        assert_eq!(budget.snapshot().used_bytes, 128);

        drop(retained_cpu);
        assert!(!worker.poll_retired());
        assert_eq!(budget.snapshot().used_bytes, 0);
    }

    #[test]
    fn imgdiff_wipe_clips_are_complementary_and_endpoints_show_one_source() {
        let canvas = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(210.0, 120.0));
        // The comparison canvas uses the larger top-left-aligned extent. A is
        // narrower than B, so its transparent/missing area must remain the
        // checkerboard under the complementary B clip.
        let display = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(310.0, 170.0));
        let visible = canvas.intersect(display);

        let (a_only, b_empty, _) = wipe_clip_regions(canvas, display, 0.0);
        assert_eq!(a_only, visible);
        assert!(!b_empty.is_positive());

        let (a_empty, b_only, _) = wipe_clip_regions(canvas, display, 1.0);
        assert!(!a_empty.is_positive());
        assert_eq!(b_only, visible);

        let (a_left, b_right, split) = wipe_clip_regions(canvas, display, 0.4);
        assert_eq!(a_left.left(), visible.left());
        assert_eq!(b_right.right(), visible.right());
        assert_eq!(a_left.right(), split);
        assert_eq!(b_right.left(), split);
        assert_eq!(a_left.top(), visible.top());
        assert_eq!(a_left.bottom(), visible.bottom());
        assert_eq!(b_right.top(), visible.top());
        assert_eq!(b_right.bottom(), visible.bottom());
    }

    fn image_compare_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 900.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn render_image_compare_frame(
        ctx: &egui::Context,
        dialog: &mut ImageCompareDialogState,
        first_ui_pass: bool,
        events: Vec<egui::Event>,
    ) {
        let output = ctx.run_ui(image_compare_input(events), |ctx| {
            dialog.show_with_jump_mode(
                ctx,
                None,
                Some(77),
                crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                    kind: crate::gui::jump_mode::ui::NavigationDialogKind::ImageCompare,
                    instance: 77,
                },
                true,
                true,
                first_ui_pass,
            );
        });
        output.drop_without_applying_deltas();
    }

    fn click_image_compare_response(
        ctx: &egui::Context,
        dialog: &mut ImageCompareDialogState,
        key: &'static str,
    ) {
        let (_, rect, enabled) = dialog
            .response_for_test(key)
            .unwrap_or_else(|| panic!("the current {key} response was rendered"));
        assert!(enabled, "the current {key} response is enabled");
        let position = rect.center();
        render_image_compare_frame(
            ctx,
            dialog,
            true,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        render_image_compare_frame(
            ctx,
            dialog,
            true,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
    }

    fn wait_for_image_pair(dialog: &ImageCompareDialogState) -> ImageCompareSnapshot {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let snapshot = dialog.session.snapshot();
            if snapshot.input_a.is_some()
                && snapshot.input_b.is_some()
                && snapshot.comparison.is_some()
            {
                return snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the dialog's real input responses should reach a bounded comparison"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn wait_for_image_presentation(
        ctx: &egui::Context,
        dialog: &mut ImageCompareDialogState,
    ) -> PairKey {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            render_image_compare_frame(ctx, dialog, true, Vec::new());
            let snapshot = dialog.session.snapshot();
            if let Some(key) = PairKey::from_snapshot(&snapshot)
                && dialog
                    .presented
                    .as_ref()
                    .is_some_and(|presented| presented.key() == key)
            {
                return key;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the actual presentation worker publishes the current pair"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn imgdiff_actual_file_and_paste_responses_reach_the_shared_domain() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owned-a.png");
        image::RgbaImage::from_raw(2, 1, vec![12, 34, 56, 255, 78, 90, 123, 255])
            .unwrap()
            .save(&path)
            .unwrap();

        let ctx = egui::Context::default();
        let mut dialog = ImageCompareDialogState::new(&ctx);
        dialog.open();
        render_image_compare_frame(&ctx, &mut dialog, true, Vec::new());
        render_image_compare_frame(&ctx, &mut dialog, true, Vec::new());

        // A cancelled chooser is inert; the following owned test path uses the
        // same current Open response and the production file-loading path.
        dialog.queue_file_pick_for_test(None);
        click_image_compare_response(&ctx, &mut dialog, "open-a");
        assert!(dialog.session.snapshot().input_a.is_none());

        dialog.queue_file_pick_for_test(Some(path.clone()));
        click_image_compare_response(&ctx, &mut dialog, "open-a");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let snapshot = dialog.session.snapshot();
            if snapshot.input_a.is_some() {
                let caption = input_caption(ImageSide::A, snapshot.input_a.as_ref().unwrap());
                assert!(caption.contains("Current input A: owned-a.png (2 × 1)"));
                assert!(
                    snapshot.input_b.is_none(),
                    "input A's accepted source is independently available before B loads"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the selected owned file loads"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        dialog.queue_clipboard_image_for_test(1, 2, vec![1, 2, 3, 255, 4, 5, 6, 128]);
        click_image_compare_response(&ctx, &mut dialog, "paste-b");
        let snapshot = wait_for_image_pair(&dialog);
        assert!(matches!(
            snapshot.input_a.as_ref().unwrap().source(),
            crate::image_compare::ImageInputSource::File { path: input_path, .. }
                if input_path == &path
        ));
        assert!(matches!(
            snapshot.input_b.as_ref().unwrap().source(),
            crate::image_compare::ImageInputSource::Test { display_name }
                if display_name == "test clipboard image"
        ));
        assert_eq!(snapshot.input_a.as_ref().unwrap().dimensions(), (2, 1));
        assert_eq!(snapshot.input_b.as_ref().unwrap().dimensions(), (1, 2));
        dialog.shutdown();
    }

    #[test]
    fn imgdiff_retained_pass_keeps_real_controls_enabled_without_committing_mutations() {
        let ctx = egui::Context::default();
        let mut dialog = ImageCompareDialogState::new(&ctx);
        dialog.open();
        render_image_compare_frame(&ctx, &mut dialog, true, Vec::new());
        // The second real egui pass is retained by the root frame owner. Its
        // controls remain observable/enabled, while action handlers still
        // honor the first-pass mutation grant.
        render_image_compare_frame(&ctx, &mut dialog, false, Vec::new());
        let (_, rect, enabled) = dialog
            .response_for_test("view/wipe-a-end")
            .expect("the current A-end response was rendered");
        assert!(enabled);
        assert_eq!(dialog.wipe_fraction, 0.5);

        let position = rect.center();
        render_image_compare_frame(
            &ctx,
            &mut dialog,
            false,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        render_image_compare_frame(
            &ctx,
            &mut dialog,
            false,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        assert!(dialog.response_for_test("view/wipe-a-end").unwrap().2);
        assert_eq!(dialog.wipe_fraction, 0.5);
        dialog.shutdown();
    }

    #[test]
    fn imgdiff_dialog_reentry_reuses_idle_session_and_retires_old_upload_budget() {
        let ctx = egui::Context::default();
        let mut dialog = ImageCompareDialogState::new(&ctx);
        dialog.open();
        dialog
            .session
            .submit_raw_for_test(
                ImageSide::A,
                2,
                1,
                vec![10, 20, 30, 255, 40, 50, 60, 255],
                "owned A",
            )
            .unwrap();
        dialog
            .session
            .submit_raw_for_test(
                ImageSide::B,
                2,
                1,
                vec![10, 20, 30, 255, 41, 50, 60, 255],
                "owned B",
            )
            .unwrap();
        let accepted = wait_for_image_pair(&dialog);
        let accepted_ids = [
            accepted.input_a.as_ref().unwrap().id(),
            accepted.input_b.as_ref().unwrap().id(),
        ];
        drop(accepted);
        let pair = wait_for_image_presentation(&ctx, &mut dialog);

        let response_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !dialog
            .response_for_test("view/b")
            .is_some_and(|(_, _, enabled)| enabled)
        {
            assert!(
                std::time::Instant::now() < response_deadline,
                "the current B-view response becomes enabled after window sizing"
            );
            render_image_compare_frame(&ctx, &mut dialog, true, Vec::new());
        }
        click_image_compare_response(&ctx, &mut dialog, "view/b");
        assert_eq!(dialog.test_view_name(), "b");
        let session_address = Arc::as_ptr(&dialog.session);
        let budget_before_focus = dialog.session.budget_snapshot();
        let generation_before_focus = dialog.presentation_worker.generation();

        // Reopening an already-open panel is only a focus/reentry request.
        dialog.open();
        render_image_compare_frame(&ctx, &mut dialog, true, Vec::new());
        let focused_pair = dialog.session.snapshot();
        assert_eq!(
            [
                focused_pair.input_a.as_ref().unwrap().id(),
                focused_pair.input_b.as_ref().unwrap().id(),
            ],
            accepted_ids
        );
        assert_eq!(dialog.test_view_name(), "b");
        assert_eq!(dialog.session.budget_snapshot(), budget_before_focus);
        assert_eq!(
            dialog.presentation_worker.generation(),
            generation_before_focus
        );
        {
            let worker = dialog.presentation_worker.shared.state.lock().unwrap();
            assert!(worker.pending.is_none());
            assert!(worker.active_generation.is_none());
            assert!(worker.completed.is_none());
            assert_eq!(worker.desired, Some(pair));
        }
        assert!(
            dialog
                .presentation_worker
                .retired_uploads
                .lock()
                .unwrap()
                .is_empty()
        );
        drop(focused_pair);

        let externally_retained_cpu = Arc::clone(
            &dialog
                .presented
                .as_ref()
                .expect("the accepted pair has a retained presentation")
                .prepared
                .a,
        );
        let generation_before_close = dialog.presentation_worker.generation();
        let worker_reaped = dialog
            .session
            .reaper_probe_for_test()
            .expect("the image loader has an owned reaper probe before close");
        dialog.close();
        assert!(!dialog.open);
        assert!(dialog.session.snapshot().closed);
        assert!(dialog.session.snapshot().input_a.is_none());
        assert!(dialog.session.snapshot().input_b.is_none());
        assert!(dialog.presentation_worker.generation() > generation_before_close);
        assert_eq!(dialog.test_view_name(), "wipe");
        let charged_after_close = dialog.session.budget_snapshot().used_bytes;
        assert!(
            charged_after_close > 0,
            "retired presentation remains charged"
        );

        // Wait for the same session's loader to retire, then use the real GUI
        // reopen path. No new session or fresh budget can replace this owner.
        let retirement_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !worker_reaped.is_reaped() {
            assert!(std::time::Instant::now() < retirement_deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        dialog.open();
        assert!(dialog.open);
        assert_eq!(Arc::as_ptr(&dialog.session), session_address);
        let reopened = dialog.session.snapshot();
        assert!(!reopened.closed);
        assert!(reopened.input_a.is_none() && reopened.input_b.is_none());
        assert!(reopened.comparison.is_none());
        assert_eq!(dialog.test_view_name(), "wipe");
        assert_eq!(
            dialog.session.budget_snapshot().used_bytes,
            charged_after_close
        );
        {
            let worker = dialog.presentation_worker.shared.state.lock().unwrap();
            assert!(worker.pending.is_none());
            assert!(worker.active_generation.is_none());
            assert!(worker.completed.is_none());
            assert!(worker.desired.is_none());
        }
        drop(reopened);

        dialog.close();
        dialog
            .presentation_worker
            .paint_serial
            .fetch_add(1, Ordering::AcqRel);
        dialog.reap_retired_uploads(&ctx);
        assert!(dialog.session.budget_snapshot().used_bytes > 0);
        dialog
            .presentation_worker
            .paint_serial
            .fetch_add(1, Ordering::AcqRel);
        dialog.reap_retired_uploads(&ctx);
        assert!(dialog.session.budget_snapshot().used_bytes > 0);
        drop(externally_retained_cpu);
        dialog.reap_retired_uploads(&ctx);
        assert_eq!(dialog.session.budget_snapshot().used_bytes, 0);
        dialog.shutdown();
    }
}
