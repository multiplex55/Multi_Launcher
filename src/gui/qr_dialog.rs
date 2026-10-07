use crate::qr::QrErrorCorrection;
use eframe::egui;

/// Fresh invocations reset transient state; panel reopening only changes visibility.
#[derive(Default)]
pub struct QrDialogState {
    pub open: bool,
    pub source: String,
    pub error_correction: QrErrorCorrection,
    pub focus_source: bool,
    pub feedback: Option<String>,
}
impl QrDialogState {
    pub fn open(&mut self, initial_text: Option<&str>) {
        *self = Self {
            open: true,
            source: initial_text.unwrap_or_default().to_owned(),
            focus_source: true,
            ..Self::default()
        };
    }
    pub fn close(&mut self) {
        self.open = false;
        self.focus_source = false;
        self.feedback = None;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("QR Generator")
            .id(egui::Id::new("qr_generator"))
            .default_size([560.0, 480.0])
            .min_size([320.0, 240.0])
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label("Local QR Generator");
                close = ui.button("Close").clicked();
            });
        if close || !self.open {
            self.close();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qr_fresh_invocation_resets_state_and_preserves_exact_source() {
        let mut state = QrDialogState::default();
        let source = "  caf\u{e9}\n\u{1f512} ";
        state.open(Some(source));
        assert_eq!(state.source, source);
        assert!(state.open && state.focus_source);
        assert_eq!(state.error_correction, QrErrorCorrection::Medium);
        state.error_correction = QrErrorCorrection::High;
        state.feedback = Some("Old status".into());
        state.focus_source = false;
        state.open(None);
        assert!(state.source.is_empty());
        assert!(state.focus_source);
        assert!(state.feedback.is_none());
        assert_eq!(state.error_correction, QrErrorCorrection::Medium);
    }
}
