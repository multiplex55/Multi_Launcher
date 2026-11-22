use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::hash_generator::{
    HashAlgorithm, HashComparison, HashInputMode, HashProgress, HashRequestId, HashSession,
};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(super) struct HashRuntime {
    pub(super) host: super::utility_host::HostIdentity,
}
#[derive(Clone)]
struct HashDraft {
    mode: HashInputMode,
    algorithm: HashAlgorithm,
    text: String,
    expected: String,
}
enum HashIntent {
    Clipboard,
    PickFile,
    Calculate,
    Cancel,
    Copy,
}
pub(super) struct HashHostActions {
    epoch: u64,
    generation: u64,
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    pub(crate) close: bool,
    draft: HashDraft,
    intents: Vec<HashIntent>,
    focus: bool,
    focus_delivered: bool,
    host: super::utility_host::HostIdentity,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
}

/// State for one retained generator panel. A cancelled worker stays owned here
/// until its terminal event is polled, so closing and reopening cannot create
/// an unbounded set of detached workers.
pub struct HashGeneratorDialogState {
    pub open: bool,
    session: HashSession,
    close_requested: bool,
    focus_text: bool,
    deferred_focus: Option<(egui::ViewportId, u64, super::utility_host::HostIdentity)>,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_host: Option<super::utility_host::HostIdentity>,
    popup_viewport: Option<egui::ViewportId>,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
}

impl Default for HashGeneratorDialogState {
    fn default() -> Self {
        Self {
            open: false,
            session: HashSession::new(),
            close_requested: false,
            focus_text: false,
            deferred_focus: None,
            popup_anchors: Vec::new(),
            popup_host: None,
            popup_viewport: None,
            #[cfg(test)]
            test_controls: Vec::new(),
        }
    }
}

impl HashGeneratorDialogState {
    pub(crate) fn attach_activity_center(&mut self, center: crate::activity::ActivityCenter) {
        self.session.attach_activity_center(center);
    }

    pub fn ensure_open(&mut self) {
        if !self.open {
            self.begin_session(None);
        } else {
            self.close_requested = false;
        }
    }

    pub fn open_with_file(&mut self, path: PathBuf) {
        if !self.open {
            self.begin_session(Some(path));
        } else if self.session.mode() != HashInputMode::File
            || self.session.file_path() != Some(path.as_path())
        {
            self.session.set_file_path(Some(path));
        }
        self.close_requested = false;
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.session.epoch()
    }

    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.session.generation()
    }

    pub(super) fn jump_generation(&self) -> u64 {
        self.session.generation()
    }

    #[cfg(test)]
    pub(crate) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_controls
            .iter()
            .find(|(key, _, _, _)| key == name)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }
    pub(super) fn owned_popup_layer(
        &self,
        ctx: &egui::Context,
        base: egui::LayerId,
    ) -> Option<egui::LayerId> {
        if self.popup_viewport != Some(ctx.viewport_id()) {
            return None;
        }
        self.popup_anchors.iter().find_map(|(id, layer)| {
            if *layer != base
                || !ctx
                    .read_response(*id)
                    .is_some_and(|r| r.enabled() && r.layer_id == base)
            {
                return None;
            }
            let popup = id.with("popup");
            egui::Popup::is_id_open(ctx, popup)
                .then_some(egui::LayerId::new(egui::Order::Foreground, popup))
        })
    }

    fn begin_session(&mut self, path: Option<PathBuf>) {
        self.session.begin(path);
        self.focus_text = true;
        self.deferred_focus = None;
        self.open = true;
        self.close_requested = false;
    }

    pub(crate) fn request_close(&mut self) {
        self.close_requested = true;
    }

    /// Close or cancel any panel path. The job remains stored until `poll`
    /// receives its terminal event, while its generation is invalidated now.
    pub fn end_session(&mut self) {
        self.session.end();
        self.focus_text = false;
        self.deferred_focus = None;
        self.open = false;
        self.close_requested = false;
    }

    pub fn shutdown(&mut self) {
        self.session.shutdown();
        self.open = false;
    }

    pub fn mode(&self) -> HashInputMode {
        self.session.mode()
    }

    pub fn algorithm(&self) -> HashAlgorithm {
        self.session.algorithm()
    }

    pub fn text(&self) -> &str {
        self.session.text()
    }

    #[cfg(test)]
    pub(crate) fn expected(&self) -> &str {
        self.session.expected()
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.session.file_path()
    }

    pub fn result(&self) -> Option<&crate::hash_generator::HashResult> {
        self.session.result()
    }

    pub fn current_request_id(&self) -> Option<HashRequestId> {
        self.session.current_request_id()
    }

    pub fn has_job(&self) -> bool {
        self.session.has_job()
    }

    pub fn can_copy_digest(&self) -> bool {
        self.session.can_copy_digest()
    }

    pub fn is_running(&self) -> bool {
        self.session.is_running()
    }

    pub fn is_stopping_stale_job(&self) -> bool {
        self.session.is_stopping_stale_job()
    }

    pub fn progress(&self) -> Option<HashProgress> {
        self.session.progress()
    }

    pub fn comparison(&self) -> Option<Result<HashComparison, String>> {
        self.session.comparison()
    }

    pub fn set_mode(&mut self, mode: HashInputMode) {
        self.session.set_mode(mode);
    }

    pub fn set_algorithm(&mut self, algorithm: HashAlgorithm) {
        self.session.set_algorithm(algorithm);
    }

    pub fn set_text(&mut self, text: String) {
        self.session.set_text(text);
    }

    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        self.session.set_file_path(path);
    }

    pub fn set_expected(&mut self, expected: String) {
        self.session.set_expected(expected);
    }

    pub fn load_clipboard(&mut self, clipboard: &dyn ClipboardBackend) {
        self.session.load_clipboard(clipboard);
    }

    pub fn calculate(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
        self.session.calculate(repaint)
    }

    pub fn cancel(&mut self) {
        self.session.cancel();
    }

    pub fn copy_digest(&mut self, clipboard: &dyn ClipboardBackend) -> bool {
        self.session.copy_digest(clipboard)
    }

    /// Polling is nonblocking. A terminal event always releases the retained
    /// worker slot, but only the matching request and generation may publish.
    pub fn poll(&mut self) -> bool {
        self.session.poll()
    }

    pub(crate) fn poll_host(&mut self, ctx: &egui::Context) {
        self.poll();
        if self.session.has_job() {
            ctx.request_repaint_after(Duration::from_millis(35));
        }
    }

    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        runtime: &HashRuntime,
        allowed: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
        scope: Option<&super::jump_mode::ui::JumpModeScope>,
    ) -> HashHostActions {
        if self.popup_host != Some(runtime.host) {
            self.popup_anchors.clear();
            self.popup_host = Some(runtime.host);
            self.popup_viewport = None;
        }
        let mut draft = HashDraft {
            mode: self.session.mode(),
            algorithm: self.session.algorithm(),
            text: self.session.text().to_owned(),
            expected: self.session.expected().to_owned(),
        };
        let mut intents = Vec::new();
        let mut request = None;
        let mut close_requested = false;
        let mut open = self.open;
        let mut focus = false;
        let focus_delivered = allowed
            && first
            && !ctx.will_discard()
            && self.deferred_focus == Some((ctx.viewport_id(), self.session.epoch(), runtime.host));
        if focus_delivered {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("hash_generator_text")));
        }
        let mut popup_anchors = Vec::new();
        let mut order = 0;
        #[cfg(test)]
        let mut test_controls = Vec::new();
        let mut body = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            let active = interactive && first && !ctx.will_discard();
            let (button, next) =
                super::utility_host::presentation_button(ui, presentation, interactive);
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &button,
                "presentation",
                "Dock or pop out",
                super::jump_mode::TargetKind::Button,
                &mut order,
            );
            let close = ui.add_enabled(interactive, egui::Button::new("Close"));
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &close,
                "close",
                "Close Hash Generator",
                super::jump_mode::TargetKind::Button,
                &mut order,
            );
            close_requested |= active && close.clicked();
            #[cfg(test)]
            test_controls.push(("host".into(), button.rect, ui.clip_rect(), button.enabled()));
            if active && button.clicked() {
                request = Some(next);
            }
            ui.add_enabled_ui(interactive, |ui| {
                self.contents(
                    ui,
                    &mut draft,
                    &mut intents,
                    &mut focus,
                    &mut popup_anchors,
                    scope,
                    &mut order,
                    #[cfg(test)]
                    &mut test_controls,
                )
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
            egui::Window::new("Hash Generator")
                .id(egui::Id::new("hash_generator_dialog"))
                .collapsible(false)
                .resizable(true)
                .default_size([620.0, 440.0])
                .min_size([420.0, 320.0])
                .show_utility(
                    ctx,
                    "hash-generator",
                    egui::Id::new("hash_generator_dialog"),
                    Some(&mut open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            620.0, 440.0,
                        )),
                        egui::vec2(420.0, 320.0),
                        [None, None],
                    ),
                    |ui| body(ui),
                );
        }
        #[cfg(test)]
        {
            self.test_controls = test_controls;
        }
        HashHostActions {
            epoch: self.session.epoch(),
            generation: self.session.generation(),
            host_request: request,
            close: !open || self.close_requested || close_requested,
            draft,
            intents: if first && allowed && !ctx.will_discard() {
                intents
            } else {
                Vec::new()
            },
            focus: focus && first && allowed && !ctx.will_discard(),
            focus_delivered,
            host: runtime.host,
            popup_anchors,
        }
    }

    pub(super) fn commit_host_actions(
        &mut self,
        actions: HashHostActions,
        ctx: &egui::Context,
        hwnd: Option<usize>,
        owner: egui::ViewportId,
    ) {
        if !self.open
            || self.session.epoch() != actions.epoch
            || self.session.generation() != actions.generation
        {
            return;
        }
        let moving_host = actions.host_request.is_some();
        self.set_mode(actions.draft.mode);
        self.set_algorithm(actions.draft.algorithm);
        self.set_text(actions.draft.text);
        self.set_expected(actions.draft.expected);
        if moving_host {
            self.deferred_focus = None;
            return;
        }
        self.popup_anchors = actions.popup_anchors;
        self.popup_host = Some(actions.host);
        self.popup_viewport = Some(owner);
        if actions.focus_delivered {
            self.focus_text = false;
            self.deferred_focus = None;
        } else if actions.focus {
            self.deferred_focus = Some((owner, self.session.epoch(), actions.host));
            ctx.request_repaint_of(owner);
        }
        for intent in actions.intents {
            match intent {
                HashIntent::Clipboard => self.load_clipboard(&ArboardClipboardBackend),
                HashIntent::PickFile => {
                    #[cfg(windows)]
                    let picked = hwnd
                        .and_then(|raw| std::num::NonZeroIsize::new(raw as isize))
                        .and_then(|raw| {
                            rfd::FileDialog::new()
                                .set_parent(&super::add_action_dialog::NativeDialogParent(raw))
                                .pick_file()
                        });
                    #[cfg(not(windows))]
                    let picked = {
                        let _ = hwnd;
                        rfd::FileDialog::new().pick_file()
                    };
                    if let Some(path) = picked {
                        self.set_file_path(Some(path));
                    }
                    #[cfg(windows)]
                    if hwnd.is_none_or(|raw| raw == 0) {
                        self.session.set_error(
                            "The Hash window is unavailable for the file picker.".into(),
                        );
                    }
                }
                HashIntent::Calculate => {
                    let repaint = ctx.clone();
                    if let Err(error) = self.calculate(Arc::new(move || repaint.request_repaint()))
                    {
                        self.session.set_error(error);
                    }
                }
                HashIntent::Cancel => self.cancel(),
                HashIntent::Copy => {
                    self.copy_digest(&ArboardClipboardBackend);
                }
            }
        }
    }

    fn contents(
        &self,
        ui: &mut egui::Ui,
        draft: &mut HashDraft,
        intents: &mut Vec<HashIntent>,
        focus: &mut bool,
        popup_anchors: &mut Vec<(egui::Id, egui::LayerId)>,
        scope: Option<&super::jump_mode::ui::JumpModeScope>,
        order: &mut u64,
        #[cfg(test)] test_controls: &mut Vec<(String, egui::Rect, egui::Rect, bool)>,
    ) {
        ui.horizontal_wrapped(|ui| {
            let mut mode = draft.mode;
            let popup_scope = scope.map(|scope| scope.combo_popup_scope("mode"));
            let mode_response = egui::ComboBox::from_id_salt("hash_source_mode")
                .selected_text(mode_label(mode))
                .show_ui(ui, |ui| {
                    for candidate in [
                        HashInputMode::Text,
                        HashInputMode::Clipboard,
                        HashInputMode::File,
                    ] {
                        let _response =
                            ui.selectable_value(&mut mode, candidate, mode_label(candidate));
                        super::jump_mode::ui::register_and_apply_target(
                            popup_scope.as_ref(),
                            ui,
                            &_response,
                            format!("mode/{}", mode_label(candidate)),
                            mode_label(candidate),
                            super::jump_mode::TargetKind::Selectable,
                            order,
                        );
                        #[cfg(test)]
                        test_controls.push((
                            format!("mode/{}", mode_label(candidate)),
                            _response.rect,
                            ui.clip_rect(),
                            _response.enabled(),
                        ));
                    }
                });
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &mode_response.response,
                "mode",
                "Source mode",
                super::jump_mode::TargetKind::ComboBox,
                order,
            );
            popup_anchors.push((mode_response.response.id, mode_response.response.layer_id));
            #[cfg(test)]
            test_controls.push((
                "mode".into(),
                mode_response.response.rect,
                ui.clip_rect(),
                mode_response.response.enabled(),
            ));
            draft.mode = mode;

            let mut algorithm = draft.algorithm;
            let popup_scope = scope.map(|scope| scope.combo_popup_scope("algorithm"));
            let algorithm_response = egui::ComboBox::from_id_salt("hash_algorithm")
                .selected_text(algorithm.name())
                .show_ui(ui, |ui| {
                    for candidate in [
                        HashAlgorithm::Sha256,
                        HashAlgorithm::Sha512,
                        HashAlgorithm::Sha1,
                    ] {
                        let _response =
                            ui.selectable_value(&mut algorithm, candidate, candidate.name());
                        super::jump_mode::ui::register_and_apply_target(
                            popup_scope.as_ref(),
                            ui,
                            &_response,
                            format!("algorithm/{}", candidate.name()),
                            candidate.name(),
                            super::jump_mode::TargetKind::Selectable,
                            order,
                        );
                        #[cfg(test)]
                        test_controls.push((
                            format!("algorithm/{}", candidate.name()),
                            _response.rect,
                            ui.clip_rect(),
                            _response.enabled(),
                        ));
                    }
                });
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &algorithm_response.response,
                "algorithm",
                "Algorithm",
                super::jump_mode::TargetKind::ComboBox,
                order,
            );
            popup_anchors.push((
                algorithm_response.response.id,
                algorithm_response.response.layer_id,
            ));
            #[cfg(test)]
            test_controls.push((
                "algorithm".into(),
                algorithm_response.response.rect,
                ui.clip_rect(),
                algorithm_response.response.enabled(),
            ));
            draft.algorithm = algorithm;

            let load = ui.button("Load Clipboard");
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &load,
                "clipboard",
                "Load clipboard",
                super::jump_mode::TargetKind::Button,
                order,
            );
            if load.clicked() {
                intents.push(HashIntent::Clipboard);
            }
            if draft.mode == HashInputMode::File {
                let choose = ui.button("Choose File…");
                super::jump_mode::ui::register_and_apply_target(
                    scope,
                    ui,
                    &choose,
                    "file",
                    "Choose file",
                    super::jump_mode::TargetKind::Button,
                    order,
                );
                if choose.clicked() {
                    intents.push(HashIntent::PickFile);
                }
            }
        });

        match draft.mode {
            HashInputMode::Text => {
                ui.label("Text input · hashed as exact UTF-8 bytes");
                let editor_size = egui::vec2(
                    ui.available_width(),
                    (ui.available_height() * 0.36).clamp(72.0, 150.0),
                );
                let _response = ui.add_sized(
                    editor_size,
                    egui::TextEdit::multiline(&mut draft.text)
                        .id(egui::Id::new("hash_generator_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter text to hash"),
                );
                super::jump_mode::ui::register_and_apply_target(
                    scope,
                    ui,
                    &_response,
                    "text",
                    "Text input",
                    super::jump_mode::TargetKind::TextEntry,
                    order,
                );
                #[cfg(test)]
                test_controls.push((
                    "text".into(),
                    _response.rect,
                    ui.clip_rect(),
                    _response.enabled(),
                ));
                if self.focus_text {
                    *focus = true;
                }
            }
            HashInputMode::Clipboard => {
                if let Some(byte_len) = self.session.clipboard_byte_len() {
                    ui.label(format!("Clipboard snapshot loaded · {byte_len} bytes"));
                } else {
                    ui.weak("Load a text clipboard snapshot before calculating.");
                }
            }
            HashInputMode::File => {
                let mut displayed_path = self
                    .session
                    .file_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "Choose a local file".into());
                ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::TextEdit::singleline(&mut displayed_path)
                        .interactive(false)
                        .font(egui::TextStyle::Monospace),
                );
            }
        }

        ui.horizontal(|ui| {
            ui.label("Expected digest (optional)");
            let response = ui.add_sized(
                [ui.available_width().max(120.0), 24.0],
                egui::TextEdit::singleline(&mut draft.expected)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Paste expected hexadecimal digest"),
            );
            let _ = response;
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &response,
                "expected",
                "Expected digest",
                super::jump_mode::TargetKind::TextEntry,
                order,
            );
        });

        if let Some(notice) = draft.algorithm.security_notice() {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }

        ui.horizontal_wrapped(|ui| {
            let calculate = ui.add_enabled(!self.session.has_job(), egui::Button::new("Calculate"));
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &calculate,
                "calculate",
                "Calculate",
                super::jump_mode::TargetKind::Button,
                order,
            );
            if calculate.clicked() {
                intents.push(HashIntent::Calculate);
            }
            let cancel = ui.add_enabled(self.is_running(), egui::Button::new("Cancel"));
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &cancel,
                "cancel",
                "Cancel calculation",
                super::jump_mode::TargetKind::Button,
                order,
            );
            if cancel.clicked() {
                intents.push(HashIntent::Cancel);
            }
            let copy = ui.add_enabled(self.can_copy_digest(), egui::Button::new("Copy Digest"));
            super::jump_mode::ui::register_and_apply_target(
                scope,
                ui,
                &copy,
                "copy",
                "Copy digest",
                super::jump_mode::TargetKind::Button,
                order,
            );
            if copy.clicked() {
                intents.push(HashIntent::Copy);
            }
        });

        if let Some(progress) = self.progress() {
            show_progress(ui, progress);
        } else if self.is_stopping_stale_job() {
            ui.weak("Stopping the previous calculation…");
        }

        if let Some(result) = self.session.result() {
            ui.label("Digest");
            ui.code(&result.digest);
            match self.comparison() {
                Some(Ok(HashComparison::NotRequested)) => {}
                Some(Ok(HashComparison::Match)) => {
                    ui.colored_label(ui.visuals().strong_text_color(), "Match");
                }
                Some(Ok(HashComparison::Mismatch)) => {
                    ui.colored_label(ui.visuals().error_fg_color, "Mismatch");
                }
                Some(Err(error)) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Invalid expected digest: {error}"),
                    );
                }
                None => {}
            }
        }
        if let Some(feedback) = self.session.clipboard_feedback() {
            ui.weak(feedback);
        }
        if let Some(error) = self.session.error() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
}

fn mode_label(mode: HashInputMode) -> &'static str {
    match mode {
        HashInputMode::Text => "Text",
        HashInputMode::Clipboard => "Clipboard",
        HashInputMode::File => "File",
    }
}

fn show_progress(ui: &mut egui::Ui, progress: HashProgress) {
    ui.horizontal(|ui| {
        if let Some(total) = progress.total_bytes {
            let fraction = if total == 0 {
                1.0
            } else {
                (progress.bytes_processed as f64 / total as f64).clamp(0.0, 1.0) as f32
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .show_percentage()
                    .text(format!("{} / {} bytes", progress.bytes_processed, total)),
            );
        } else {
            ui.spinner();
            ui.label(format!("{} bytes", progress.bytes_processed));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_generator::HashJob;
    use std::sync::Mutex;

    fn calculate_deferred(dialog: &mut HashGeneratorDialogState) -> Box<dyn FnOnce() + Send> {
        let worker_slot = Arc::new(Mutex::new(None));
        let worker_slot_for_start = worker_slot.clone();
        dialog
            .session
            .calculate_with_start(Arc::new(|| {}), None, move |request, repaint, _activity| {
                let (job, run_worker) = HashJob::start_deferred_for_test(request, repaint)?;
                *worker_slot_for_start
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(run_worker);
                Ok(job)
            })
            .expect("deferred hash calculation starts");

        Box::new(move || {
            let run_worker = worker_slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .expect("deferred worker is available");
            run_worker();
        })
    }

    #[test]
    fn hash_host_same_file_keeps_worker_and_stale_actions_cannot_mutate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, "hash input").unwrap();
        let mut dialog = HashGeneratorDialogState::default();
        dialog.open_with_file(path.clone());
        dialog.set_algorithm(HashAlgorithm::Sha512);
        dialog.set_text("private draft".into());
        dialog.set_expected("expected".into());
        let finish = calculate_deferred(&mut dialog);
        let before = (
            dialog.epoch(),
            dialog.generation(),
            dialog.current_request_id(),
            dialog.algorithm(),
            dialog.text().to_owned(),
            dialog.expected().to_owned(),
        );
        dialog.open_with_file(path);
        assert_eq!(
            (
                dialog.epoch(),
                dialog.generation(),
                dialog.current_request_id(),
                dialog.algorithm(),
                dialog.text().to_owned(),
                dialog.expected().to_owned()
            ),
            before
        );
        assert!(dialog.has_job() && dialog.is_running());
        let stale = super::HashHostActions {
            epoch: dialog.epoch(),
            generation: dialog.generation().wrapping_sub(1),
            host_request: None,
            close: false,
            draft: super::HashDraft {
                mode: HashInputMode::Text,
                algorithm: HashAlgorithm::Sha1,
                text: "stale".into(),
                expected: "stale".into(),
            },
            intents: vec![super::HashIntent::Cancel],
            focus: false,
            focus_delivered: false,
            host: crate::gui::utility_host::UtilityHostState::docked().identity(),
            popup_anchors: vec![],
        };
        dialog.commit_host_actions(
            stale,
            &egui::Context::default(),
            None,
            egui::ViewportId::ROOT,
        );
        assert_eq!(
            (
                dialog.epoch(),
                dialog.generation(),
                dialog.current_request_id(),
                dialog.algorithm(),
                dialog.text().to_owned(),
                dialog.expected().to_owned()
            ),
            before
        );
        assert!(dialog.has_job() && dialog.is_running());
        let moving = super::HashHostActions {
            epoch: dialog.epoch(),
            generation: dialog.generation(),
            host_request: Some(crate::gui::utility_host::UtilityPresentation::Popout),
            close: false,
            draft: super::HashDraft {
                mode: dialog.mode(),
                algorithm: dialog.algorithm(),
                text: dialog.text().to_owned(),
                expected: "accepted edit".into(),
            },
            intents: vec![super::HashIntent::Cancel],
            focus: false,
            focus_delivered: false,
            host: crate::gui::utility_host::UtilityHostState::docked().identity(),
            popup_anchors: vec![],
        };
        dialog.commit_host_actions(
            moving,
            &egui::Context::default(),
            None,
            egui::ViewportId::ROOT,
        );
        assert_eq!(dialog.expected(), "accepted edit");
        assert_eq!(dialog.current_request_id(), before.2);
        assert!(dialog.has_job() && dialog.is_running());

        let replacement_path = dir.path().join("replacement.txt");
        std::fs::write(&replacement_path, "replacement input").unwrap();
        let generation_before_replacement = dialog.generation();
        dialog.open_with_file(replacement_path.clone());
        assert!(dialog.generation() > generation_before_replacement);
        assert_eq!(dialog.file_path(), Some(replacement_path.as_path()));
        assert_eq!(dialog.mode(), HashInputMode::File);
        assert_eq!(dialog.algorithm(), HashAlgorithm::Sha512);
        assert_eq!(dialog.text(), "private draft");
        assert_eq!(dialog.expected(), "accepted edit");
        assert!(dialog.has_job() && dialog.is_stopping_stale_job());

        dialog.end_session();
        finish();
        dialog.poll_host(&egui::Context::default());
        assert!(!dialog.has_job());
        assert!(dialog.result().is_none());
    }

    #[test]
    fn hash_generator_lifecycle_cancel_close_reopen_hidden_poll_and_shutdown_are_nonblocking() {
        let context = egui::Context::default();
        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("cancelled".into());

        let finish_cancelled = calculate_deferred(&mut dialog);
        dialog.cancel();
        assert!(dialog.is_stopping_stale_job());
        assert!(dialog.calculate(Arc::new(|| {})).is_err());
        dialog.open = false;
        dialog.poll_host(&context);
        assert!(
            dialog.has_job(),
            "polling must not run or join the deferred worker"
        );
        finish_cancelled();
        dialog.poll_host(&context);
        assert!(!dialog.has_job());
        assert!(dialog.result().is_none());

        let finish_closed = calculate_deferred(&mut dialog);
        dialog.end_session();
        dialog.ensure_open();
        assert!(dialog.is_stopping_stale_job());
        assert!(dialog.calculate(Arc::new(|| {})).is_err());
        dialog.open = false;
        dialog.poll_host(&context);
        assert!(dialog.has_job(), "hidden polling must remain nonblocking");
        finish_closed();
        dialog.poll_host(&context);
        assert!(!dialog.has_job());
        assert!(dialog.result().is_none());

        dialog.ensure_open();
        let finish_shutdown = calculate_deferred(&mut dialog);
        dialog.shutdown();
        assert!(!dialog.open);
        assert!(dialog.is_stopping_stale_job());
        finish_shutdown();
        dialog.poll_host(&context);
        assert!(!dialog.has_job());
        assert!(dialog.result().is_none());
    }

    #[test]
    fn utility_size_integration_hash_minimum_does_not_rewrite_smaller_preference() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut sizes = crate::settings::UtilityDialogSizes::default();
        let key = crate::settings::UtilityToolKey::parse("hash-generator").unwrap();
        let preferred = crate::settings::PreferredWindowSize::new(300.0, 250.0).unwrap();
        sizes.set(key, preferred).unwrap();
        let manager = crate::gui::utility_dialog_size::UtilityDialogSizeManager::new(
            path.to_string_lossy().into_owned(),
            crate::settings_save::SettingsSaveWorkerRegistry::default(),
            sizes,
        );
        let ctx = egui::Context::default();
        manager.install_in_context(&ctx);
        let mut dialog = HashGeneratorDialogState::default();
        dialog.open = true;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    manager.begin_frame(ui.ctx()).unwrap();
                    let _ = dialog.render_host(
                        ui.ctx(),
                        &super::HashRuntime {
                            host: crate::gui::utility_host::UtilityHostState::docked().identity(),
                        },
                        true,
                        true,
                        None,
                        crate::gui::utility_host::UtilityPresentation::Docked,
                        None,
                    );
                },
            );
            output.drop_without_applying_deltas();
        }

        let rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("hash_generator_dialog")))
            .expect("Hash Generator uses its stable production window id");
        assert!(rect.width() >= 420.0, "{rect:?}");
        assert!(rect.height() >= 320.0, "{rect:?}");
        assert_eq!(manager.snapshot().preferred(key), Some(preferred));
    }
}
