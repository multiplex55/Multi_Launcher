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
    current_request: Option<(u64, HashRequestId)>,
    job: Option<HashJob>,
    result: Option<crate::hash_generator::HashResult>,
    error: Option<String>,
    clipboard_feedback: Option<String>,
    focus_text: bool,
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
            current_request: None,
            job: None,
            result: None,
            error: None,
            clipboard_feedback: None,
            focus_text: false,
        }
    }
}

impl HashGeneratorDialogState {
    pub fn ensure_open(&mut self) {
        if !self.open {
            self.begin_session(None);
        }
    }

    pub fn open_with_file(&mut self, path: PathBuf) {
        self.begin_session(Some(path));
    }

    fn begin_session(&mut self, path: Option<PathBuf>) {
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
        self.open = true;
    }

    /// Close or cancel any panel path. The job remains stored until `poll`
    /// receives its terminal event, while its generation is invalidated now.
    pub fn end_session(&mut self) {
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
        self.open = false;
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
        self.job.as_ref().is_some_and(|job| {
            self.current_request == Some((self.generation, job.request_id()))
        })
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
                let trimmed = self.expected.trim_matches(|ch: char| ch.is_ascii_whitespace());
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
                if self.mode != HashInputMode::Clipboard || self.clipboard_text.as_deref() != Some(&text) {
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
        if self.job.is_some() {
            return Err("Wait for the cancelled hash worker to finish before starting another".into());
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
        let request = HashRequest::new(self.algorithm, input, None).map_err(|error| error.to_string())?;
        let request_id = request.request_id();
        let job = HashJob::start(request, repaint).map_err(|error| error.to_string())?;
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

    pub fn show(&mut self, ctx: &egui::Context) {
        self.poll();
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(35));
        }
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new("Hash Generator")
            .id(egui::Id::new("hash_generator_dialog"))
            .collapsible(false)
            .resizable(true)
            .default_size((620.0, 440.0))
            .min_size((420.0, 320.0))
            .open(&mut open)
            .show(ctx, |ui| self.contents(ui, ctx));
        if self.open && !open {
            self.end_session();
        } else {
            self.open = open;
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            let mut mode = self.mode;
            egui::ComboBox::from_id_salt("hash_source_mode")
                .selected_text(mode_label(mode))
                .show_ui(ui, |ui| {
                    for candidate in [HashInputMode::Text, HashInputMode::Clipboard, HashInputMode::File] {
                        ui.selectable_value(&mut mode, candidate, mode_label(candidate));
                    }
                });
            if mode != self.mode {
                self.set_mode(mode);
            }

            let mut algorithm = self.algorithm;
            egui::ComboBox::from_id_salt("hash_algorithm")
                .selected_text(algorithm.name())
                .show_ui(ui, |ui| {
                    for candidate in [HashAlgorithm::Sha256, HashAlgorithm::Sha512, HashAlgorithm::Sha1] {
                        ui.selectable_value(&mut algorithm, candidate, candidate.name());
                    }
                });
            if algorithm != self.algorithm {
                self.set_algorithm(algorithm);
            }

            if ui.button("Load Clipboard").clicked() {
                self.load_clipboard(&ArboardClipboardBackend);
            }
            if self.mode == HashInputMode::File && ui.button("Choose File…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.set_file_path(Some(path));
            }
        });

        match self.mode {
            HashInputMode::Text => {
                ui.label("Text input · hashed as exact UTF-8 bytes");
                let editor_size = egui::vec2(ui.available_width(), (ui.available_height() * 0.36).clamp(72.0, 150.0));
                let response = ui.add_sized(
                    editor_size,
                    egui::TextEdit::multiline(&mut self.text)
                        .id(egui::Id::new("hash_generator_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter text to hash"),
                );
                if self.focus_text {
                    response.request_focus();
                    self.focus_text = false;
                }
                if response.changed() {
                    self.invalidate_current();
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
                egui::TextEdit::singleline(&mut self.expected)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Paste expected hexadecimal digest"),
            );
            if response.changed() {
                self.error = None;
                self.clipboard_feedback = None;
            }
        });

        if let Some(notice) = self.algorithm.security_notice() {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }

        ui.horizontal_wrapped(|ui| {
            let calculate = ui.add_enabled(
                self.job.is_none(),
                egui::Button::new("Calculate"),
            );
            if calculate.clicked() {
                let repaint_context = ctx.clone();
                if let Err(error) = self.calculate(Arc::new(move || repaint_context.request_repaint())) {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(self.is_running(), egui::Button::new("Cancel"))
                .clicked()
            {
                self.cancel();
            }
            if ui
                .add_enabled(self.can_copy_digest(), egui::Button::new("Copy Digest"))
                .clicked()
            {
                self.copy_digest(&ArboardClipboardBackend);
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
                    ui.colored_label(ui.visuals().error_fg_color, format!("Invalid expected digest: {error}"));
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
        writes: Mutex<Vec<String>>,
    }

    impl Default for FakeClipboard {
        fn default() -> Self {
            Self {
                read: Ok(String::new()),
                writes: Mutex::new(Vec::new()),
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.read.clone()
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
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
}
