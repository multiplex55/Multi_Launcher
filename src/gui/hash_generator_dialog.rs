use crate::clipboard_modify::clipboard::{
    ArboardClipboardBackend, ClipboardBackend, ClipboardError,
};
use crate::hash_generator::{
    HashAlgorithm, HashComparison, HashInput, HashInputMode, HashJob, HashJobEvent, HashProgress,
    HashRequest, HashRequestId, parse_expected_digest,
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
    mode: HashInputMode,
    algorithm: HashAlgorithm,
    text: String,
    clipboard_text: Option<String>,
    file_path: Option<PathBuf>,
    expected: String,
    generation: u64,
    epoch: u64,
    close_requested: bool,
    current_request: Option<(u64, HashRequestId)>,
    job: Option<HashJob>,
    result: Option<crate::hash_generator::HashResult>,
    error: Option<String>,
    clipboard_feedback: Option<String>,
    focus_text: bool,
    deferred_focus: Option<(egui::ViewportId, u64, super::utility_host::HostIdentity)>,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_host: Option<super::utility_host::HostIdentity>,
    popup_viewport: Option<egui::ViewportId>,
    #[cfg(test)] test_controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
    activity_center: Option<crate::activity::ActivityCenter>,
}

impl Default for HashGeneratorDialogState {
    fn default() -> Self {
        Self {
            open: false,
            mode: HashInputMode::Text,
            algorithm: HashAlgorithm::Sha256,
            text: String::new(),
            clipboard_text: None,
            file_path: None,
            expected: String::new(),
            generation: 0,
            epoch: 0,
            close_requested: false,
            current_request: None,
            job: None,
            result: None,
            error: None,
            clipboard_feedback: None,
            focus_text: false,
            deferred_focus: None,
            popup_anchors: Vec::new(),
            popup_host: None,
            popup_viewport: None,
            #[cfg(test)] test_controls: Vec::new(),
            activity_center: None,
        }
    }
}

impl HashGeneratorDialogState {
    pub(crate) fn attach_activity_center(&mut self, center: crate::activity::ActivityCenter) {
        self.activity_center = Some(center);
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
        } else if self.mode != HashInputMode::File || self.file_path.as_ref() != Some(&path) {
            self.set_file_path(Some(path));
        }
        self.close_requested = false;
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
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
        self.epoch = self.epoch.wrapping_add(1);
        self.invalidate_current();
        self.mode = if path.is_some() {
            HashInputMode::File
        } else {
            HashInputMode::Text
        };
        self.algorithm = HashAlgorithm::Sha256;
        self.text.clear();
        self.clipboard_text = None;
        self.file_path = path;
        self.expected.clear();
        self.error = None;
        self.clipboard_feedback = None;
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
        self.epoch = self.epoch.wrapping_add(1);
        self.invalidate_current();
        self.mode = HashInputMode::Text;
        self.algorithm = HashAlgorithm::Sha256;
        self.text.clear();
        self.clipboard_text = None;
        self.file_path = None;
        self.expected.clear();
        self.error = None;
        self.clipboard_feedback = None;
        self.focus_text = false;
        self.deferred_focus = None;
        self.open = false;
        self.close_requested = false;
    }

    pub fn shutdown(&mut self) {
        self.invalidate_current();
        self.open = false;
    }

    pub fn mode(&self) -> HashInputMode {
        self.mode
    }

    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.file_path.as_deref()
    }

    pub fn result(&self) -> Option<&crate::hash_generator::HashResult> {
        self.result.as_ref()
    }

    pub fn current_request_id(&self) -> Option<HashRequestId> {
        self.current_request.map(|(_, request_id)| request_id)
    }

    pub fn has_job(&self) -> bool {
        self.job.is_some()
    }

    pub fn can_copy_digest(&self) -> bool {
        self.result.is_some() && !self.is_running()
    }

    pub fn is_running(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| self.current_request == Some((self.generation, job.request_id())))
    }

    pub fn is_stopping_stale_job(&self) -> bool {
        self.job.is_some() && !self.is_running()
    }

    pub fn progress(&self) -> Option<HashProgress> {
        self.is_running()
            .then(|| self.job.as_ref().map(HashJob::progress))
            .flatten()
    }

    pub fn comparison(&self) -> Option<Result<HashComparison, String>> {
        let result = self.result.as_ref()?;
        match parse_expected_digest(result.algorithm, &self.expected) {
            Ok(None) => Some(Ok(HashComparison::NotRequested)),
            Ok(Some(_)) => {
                let trimmed = self
                    .expected
                    .trim_matches(|ch: char| ch.is_ascii_whitespace());
                Some(Ok(if result.digest.eq_ignore_ascii_case(trimmed) {
                    HashComparison::Match
                } else {
                    HashComparison::Mismatch
                }))
            }
            Err(error) => Some(Err(error.to_string())),
        }
    }

    pub fn set_mode(&mut self, mode: HashInputMode) {
        if self.mode != mode {
            self.invalidate_current();
            self.mode = mode;
        }
    }

    pub fn set_algorithm(&mut self, algorithm: HashAlgorithm) {
        if self.algorithm != algorithm {
            self.invalidate_current();
            self.algorithm = algorithm;
        }
    }

    pub fn set_text(&mut self, text: String) {
        if self.text != text {
            self.invalidate_current();
            self.text = text;
        }
    }

    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        if self.file_path != path || self.mode != HashInputMode::File {
            self.invalidate_current();
            self.mode = HashInputMode::File;
            self.file_path = path;
        }
    }

    pub fn set_expected(&mut self, expected: String) {
        if self.expected != expected {
            self.expected = expected;
            self.error = None;
            self.clipboard_feedback = None;
        }
    }

    pub fn load_clipboard(&mut self, clipboard: &dyn ClipboardBackend) {
        match clipboard.read_text() {
            Ok(text) => {
                if self.mode != HashInputMode::Clipboard
                    || self.clipboard_text.as_deref() != Some(&text)
                {
                    self.invalidate_current();
                    self.mode = HashInputMode::Clipboard;
                    self.clipboard_text = Some(text);
                }
                self.error = None;
                self.clipboard_feedback = Some("Clipboard snapshot loaded for hashing".into());
            }
            Err(error) => {
                self.error = Some(format!("Could not load clipboard: {error}"));
                self.clipboard_feedback = None;
            }
        }
    }

    pub fn calculate(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let activity_center = self.activity_center.clone();
        self.calculate_with_start(repaint, activity_center, |request, repaint, activity| {
            HashJob::start_with_activity(request, repaint, activity)
        })
    }

    fn calculate_with_start(
        &mut self,
        repaint: Arc<dyn Fn() + Send + Sync>,
        activity_center: Option<crate::activity::ActivityCenter>,
        start_job: impl FnOnce(
            HashRequest,
            Arc<dyn Fn() + Send + Sync>,
            Option<crate::activity::ActivityTask>,
        ) -> Result<HashJob, crate::hash_generator::HashError>,
    ) -> Result<(), String> {
        if self.job.is_some() {
            return Err(
                "Wait for the cancelled hash worker to finish before starting another".into(),
            );
        }
        let input = match self.mode {
            HashInputMode::Text => HashInput::Text(self.text.clone()),
            HashInputMode::Clipboard => HashInput::Clipboard(
                self.clipboard_text
                    .clone()
                    .ok_or_else(|| "Load text from the clipboard before calculating".to_owned())?,
            ),
            HashInputMode::File => HashInput::File(
                self.file_path
                    .clone()
                    .ok_or_else(|| "Choose a file before calculating".to_owned())?,
            ),
        };
        let request =
            HashRequest::new(self.algorithm, input, None).map_err(|error| error.to_string())?;
        let request_id = request.request_id();
        let activity = activity_center.as_ref().and_then(|center| {
            center.start(
                crate::activity::ActivityOwner::Hash,
                crate::activity::ActivitySlot::Primary,
                crate::activity::ActivityIncarnation::new(request_id.get()),
                crate::activity::ActivityOperation::CalculateHash,
            )
        });
        let job = match start_job(request, repaint, activity.clone()) {
            Ok(job) => job,
            Err(error) => {
                if let Some(activity) = activity {
                    activity.finish(
                        crate::activity::ActivityOutcome::Failed,
                        Some(crate::activity::ActivityDiagnostic::WorkerUnavailable),
                        crate::activity::ActivityRetirement::Confirmed,
                    );
                }
                return Err(error.to_string());
            }
        };
        self.result = None;
        self.error = None;
        self.clipboard_feedback = None;
        self.current_request = Some((self.generation, request_id));
        self.job = Some(job);
        Ok(())
    }

    pub fn cancel(&mut self) {
        if self.is_running() {
            self.invalidate_current();
        }
    }

    pub fn copy_digest(&mut self, clipboard: &dyn ClipboardBackend) -> bool {
        let Some(result) = self.result.as_ref().filter(|_| self.can_copy_digest()) else {
            return false;
        };
        match clipboard.write_text(&result.digest) {
            Ok(()) => {
                self.clipboard_feedback = Some("Digest copied".into());
                self.error = None;
                true
            }
            Err(error) => {
                self.clipboard_feedback = None;
                self.error = Some(format!("Could not copy digest: {error}"));
                false
            }
        }
    }

    /// Polling is nonblocking. A terminal event always releases the retained
    /// worker slot, but only the matching request and generation may publish.
    pub fn poll(&mut self) -> bool {
        let Some(event) = self.job.as_mut().and_then(HashJob::poll) else {
            return false;
        };
        self.job = None;
        if self.current_request != Some((self.generation, event.request_id)) {
            return true;
        }
        self.current_request = None;
        self.accept_event(event);
        true
    }

    fn accept_event(&mut self, event: HashJobEvent) {
        match event.result {
            Ok(result) if result.request_id == event.request_id => {
                self.result = Some(result);
                self.error = None;
            }
            Ok(_) => {
                self.result = None;
                self.error = Some("Hash worker returned a mismatched request".into());
            }
            Err(crate::hash_generator::HashError::Cancelled) => {
                self.result = None;
                self.error = Some("Hash calculation was cancelled".into());
            }
            Err(error) => {
                self.result = None;
                self.error = Some(error.to_string());
            }
        }
    }

    fn invalidate_current(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.current_request = None;
        if let Some(job) = self.job.as_ref() {
            job.cancel();
        }
        self.result = None;
        self.error = None;
        self.clipboard_feedback = None;
    }

    pub(crate) fn poll_host(&mut self, ctx: &egui::Context) {
        self.poll();
        if self.job.is_some() {
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
    ) -> HashHostActions {
        if self.popup_host != Some(runtime.host) {
            self.popup_anchors.clear();
            self.popup_host = Some(runtime.host);
            self.popup_viewport = None;
        }
        let mut draft = HashDraft {
            mode: self.mode,
            algorithm: self.algorithm,
            text: self.text.clone(),
            expected: self.expected.clone(),
        };
        let mut intents = Vec::new();
        let mut request = None;
        let mut open = self.open;
        let mut focus = false;
        let focus_delivered = allowed
            && first
            && !ctx.will_discard()
            && self.deferred_focus == Some((ctx.viewport_id(), self.epoch, runtime.host));
        if focus_delivered {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("hash_generator_text")));
        }
        let mut popup_anchors = Vec::new();
        #[cfg(test)]
        let mut test_controls = Vec::new();
        let mut body = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            let active = interactive && first && !ctx.will_discard();
            let (button, next) =
                super::utility_host::presentation_button(ui, presentation, interactive);
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
            epoch: self.epoch,
            generation: self.generation,
            host_request: request,
            close: !open || self.close_requested,
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
        if !self.open || self.epoch != actions.epoch || self.generation != actions.generation {
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
            self.deferred_focus = Some((owner, self.epoch, actions.host));
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
                        self.error =
                            Some("The Hash window is unavailable for the file picker.".into());
                    }
                }
                HashIntent::Calculate => {
                    let repaint = ctx.clone();
                    if let Err(error) = self.calculate(Arc::new(move || repaint.request_repaint()))
                    {
                        self.error = Some(error);
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
        #[cfg(test)] test_controls: &mut Vec<(String, egui::Rect, egui::Rect, bool)>,
    ) {
        ui.horizontal_wrapped(|ui| {
            let mut mode = draft.mode;
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
                        #[cfg(test)]
                        test_controls.push((
                            format!("mode/{}", mode_label(candidate)),
                            _response.rect,
                            ui.clip_rect(),
                            _response.enabled(),
                        ));
                    }
                });
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
                        #[cfg(test)]
                        test_controls.push((
                            format!("algorithm/{}", candidate.name()),
                            _response.rect,
                            ui.clip_rect(),
                            _response.enabled(),
                        ));
                    }
                });
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

            if ui.button("Load Clipboard").clicked() {
                intents.push(HashIntent::Clipboard);
            }
            if draft.mode == HashInputMode::File && ui.button("Choose File…").clicked() {
                intents.push(HashIntent::PickFile);
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
                if let Some(text) = &self.clipboard_text {
                    ui.label(format!("Clipboard snapshot loaded · {} bytes", text.len()));
                } else {
                    ui.weak("Load a text clipboard snapshot before calculating.");
                }
            }
            HashInputMode::File => {
                let mut displayed_path = self
                    .file_path
                    .as_ref()
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
        });

        if let Some(notice) = draft.algorithm.security_notice() {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }

        ui.horizontal_wrapped(|ui| {
            let calculate = ui.add_enabled(self.job.is_none(), egui::Button::new("Calculate"));
            if calculate.clicked() {
                intents.push(HashIntent::Calculate);
            }
            if ui
                .add_enabled(self.is_running(), egui::Button::new("Cancel"))
                .clicked()
            {
                intents.push(HashIntent::Cancel);
            }
            if ui
                .add_enabled(self.can_copy_digest(), egui::Button::new("Copy Digest"))
                .clicked()
            {
                intents.push(HashIntent::Copy);
            }
        });

        if let Some(progress) = self.progress() {
            show_progress(ui, progress);
        } else if self.is_stopping_stale_job() {
            ui.weak("Stopping the previous calculation…");
        }

        if let Some(result) = &self.result {
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
        if let Some(feedback) = &self.clipboard_feedback {
            ui.weak(feedback);
        }
        if let Some(error) = &self.error {
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
    use std::sync::Mutex;
    use std::thread;
    use std::time::Instant;

    struct FakeClipboard {
        read: Result<String, ClipboardError>,
        write_error: Option<String>,
        writes: Mutex<Vec<String>>,
    }

    impl Default for FakeClipboard {
        fn default() -> Self {
            Self {
                read: Ok(String::new()),
                write_error: None,
                writes: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.read.clone()
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            if let Some(error) = &self.write_error {
                return Err(ClipboardError::Permanent(error.clone()));
            }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    fn calculate_deferred(dialog: &mut HashGeneratorDialogState) -> Box<dyn FnOnce() + Send> {
        let worker_slot = Arc::new(Mutex::new(None));
        let worker_slot_for_start = worker_slot.clone();
        dialog
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

    fn wait_for_terminal(dialog: &mut HashGeneratorDialogState) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while dialog.has_job() {
            dialog.poll();
            assert!(Instant::now() < deadline, "hash worker did not finish");
            thread::yield_now();
        }
    }

    #[test]
    fn expected_digest_edits_recompare_without_rehashing_and_copy_only_current_digest() {
        let clipboard = FakeClipboard {
            read: Ok("source".into()),
            ..Default::default()
        };
        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("abc".into());
        dialog
            .calculate(Arc::new(|| {}))
            .expect("text request starts");
        wait_for_terminal(&mut dialog);

        let result_id = dialog.result().unwrap().request_id;
        assert!(dialog.can_copy_digest());
        dialog.set_expected(
            " BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD\n".into(),
        );
        assert_eq!(dialog.result().unwrap().request_id, result_id);
        assert_eq!(dialog.comparison(), Some(Ok(HashComparison::Match)));
        assert!(dialog.copy_digest(&clipboard));
        assert_eq!(
            *clipboard.writes.lock().unwrap(),
            ["ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"]
        );

        dialog.set_expected("not a digest".into());
        assert_eq!(dialog.result().unwrap().request_id, result_id);
        assert!(matches!(dialog.comparison(), Some(Err(_))));
        dialog.set_text("edited".into());
        assert!(dialog.result().is_none());
        assert!(!dialog.can_copy_digest());
    }

    #[test]
    fn source_edits_cancel_and_reject_a_stale_completion() {
        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("original".into());
        dialog
            .calculate(Arc::new(|| {}))
            .expect("text request starts");
        assert!(dialog.is_running());

        dialog.set_text("edited before completion".into());
        assert!(dialog.is_stopping_stale_job());
        assert!(dialog.result().is_none());
        assert!(!dialog.can_copy_digest());
        wait_for_terminal(&mut dialog);
        assert!(dialog.result().is_none());
    }

    #[test]
    fn clipboard_input_requires_explicit_load_and_errors_do_not_write() {
        let clipboard = FakeClipboard {
            read: Err(ClipboardError::Permanent("unavailable".into())),
            ..Default::default()
        };
        let mut dialog = HashGeneratorDialogState::default();
        dialog.set_mode(HashInputMode::Clipboard);
        assert!(dialog.calculate(Arc::new(|| {})).is_err());
        dialog.load_clipboard(&clipboard);
        assert!(dialog.error.as_deref().unwrap().contains("unavailable"));
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn closing_cancels_and_reopening_keeps_the_retiring_worker_owned() {
        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("closing".into());
        dialog.calculate(Arc::new(|| {})).unwrap();
        dialog.end_session();
        assert!(!dialog.open);
        assert!(dialog.is_stopping_stale_job());

        dialog.ensure_open();
        assert!(dialog.open);
        assert!(dialog.is_stopping_stale_job());
        assert!(dialog.calculate(Arc::new(|| {})).is_err());
        wait_for_terminal(&mut dialog);
        assert!(dialog.result().is_none());
    }

    #[test]
    fn hash_generator_lifecycle_rejects_deferred_results_after_edits_and_allows_reruns() {
        for edit in ["source", "algorithm", "mode"] {
            let mut dialog = HashGeneratorDialogState::default();
            dialog.ensure_open();
            dialog.set_text("before edit".into());

            let finish_old_worker = calculate_deferred(&mut dialog);
            let old_request_id = dialog.current_request_id().unwrap();
            finish_old_worker();

            match edit {
                "source" => dialog.set_text("after edit".into()),
                "algorithm" => dialog.set_algorithm(HashAlgorithm::Sha512),
                "mode" => dialog.load_clipboard(&FakeClipboard {
                    read: Ok("clipboard snapshot".into()),
                    ..Default::default()
                }),
                _ => unreachable!(),
            }
            assert!(dialog.poll());
            assert!(
                dialog.result().is_none(),
                "stale {edit} result was published"
            );
            assert!(dialog.error.is_none(), "stale {edit} error was published");
            assert!(!dialog.can_copy_digest());

            let finish_current_worker = calculate_deferred(&mut dialog);
            let current_request_id = dialog.current_request_id().unwrap();
            assert_ne!(current_request_id, old_request_id);
            finish_current_worker();
            assert!(dialog.poll());
            let result = dialog.result().expect("current result is published");
            assert_eq!(result.request_id, current_request_id);
            assert_eq!(result.input_mode, dialog.mode());
            assert_eq!(result.algorithm, dialog.algorithm());
            assert!(dialog.can_copy_digest());
        }
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
            dialog.epoch,
            dialog.generation,
            dialog.current_request,
            dialog.algorithm,
            dialog.text.clone(),
            dialog.expected.clone(),
        );
        dialog.open_with_file(path);
        assert_eq!(
            (
                dialog.epoch,
                dialog.generation,
                dialog.current_request,
                dialog.algorithm,
                dialog.text.clone(),
                dialog.expected.clone()
            ),
            before
        );
        assert!(dialog.has_job() && dialog.is_running());
        let stale = super::HashHostActions {
            epoch: dialog.epoch,
            generation: dialog.generation.wrapping_sub(1),
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
                dialog.epoch,
                dialog.generation,
                dialog.current_request,
                dialog.algorithm,
                dialog.text.clone(),
                dialog.expected.clone()
            ),
            before
        );
        assert!(dialog.has_job() && dialog.is_running());
        let moving = super::HashHostActions {
            epoch: dialog.epoch,
            generation: dialog.generation,
            host_request: Some(crate::gui::utility_host::UtilityPresentation::Popout),
            close: false,
            draft: super::HashDraft {
                mode: dialog.mode,
                algorithm: dialog.algorithm,
                text: dialog.text.clone(),
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
        assert_eq!(dialog.expected, "accepted edit");
        assert_eq!(dialog.current_request, before.2);
        assert!(dialog.has_job() && dialog.is_running());
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
    fn hash_generator_lifecycle_file_invocation_keeps_path_mode_and_generation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("selected-report.csv");
        std::fs::write(&path, "id,value\r\n1,λ\n").unwrap();

        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("old text invocation".into());
        let finish_old_worker = calculate_deferred(&mut dialog);
        let old_generation = dialog.generation;
        finish_old_worker();

        dialog.open_with_file(path.clone());
        assert!(dialog.generation > old_generation);
        assert_eq!(dialog.mode(), HashInputMode::File);
        assert_eq!(dialog.file_path(), Some(path.as_path()));
        assert!(dialog.poll(), "the prior invocation is retired");
        assert!(dialog.result().is_none());

        dialog
            .calculate(Arc::new(|| {}))
            .expect("selected file calculation starts");
        wait_for_terminal(&mut dialog);
        let result = dialog.result().expect("file digest is available");
        assert_eq!(result.input_mode, HashInputMode::File);
        assert_eq!(
            result.bytes_processed,
            std::fs::metadata(&path).unwrap().len()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "id,value\r\n1,λ\n");
    }

    #[test]
    fn hash_generator_lifecycle_copy_failure_keeps_digest_and_reports_error() {
        let mut dialog = HashGeneratorDialogState::default();
        dialog.ensure_open();
        dialog.set_text("abc".into());
        dialog.calculate(Arc::new(|| {})).unwrap();
        wait_for_terminal(&mut dialog);
        let digest = dialog.result().unwrap().digest.clone();
        let clipboard = FakeClipboard {
            write_error: Some("clipboard busy".into()),
            ..Default::default()
        };

        assert!(!dialog.copy_digest(&clipboard));
        assert!(dialog.error.as_deref().unwrap().contains("clipboard busy"));
        assert_eq!(dialog.result().unwrap().digest, digest);
        assert!(clipboard.writes.lock().unwrap().is_empty());
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
                    let _ = dialog.render_host(ui.ctx(), &super::HashRuntime { host: crate::gui::utility_host::UtilityHostState::docked().identity() }, true, true, None, crate::gui::utility_host::UtilityPresentation::Docked);
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
