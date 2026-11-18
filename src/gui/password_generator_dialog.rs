//! Retained password generator controls. Rendering captures intents, never secrets.
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
use crate::password_generator::{
    self, Capitalization, GeneratorMode, GeneratorPreferences, PASSWORD_LENGTH_RANGE, SecretResult,
    SecretResultId, Separator,
};
use crate::platform::protected_clipboard::{ClipboardError, CopyStatus};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CopyTicket {
    pub id: SecretResultId,
    pub policy_revision: u64,
    pub session: u64,
}
#[derive(Debug)]
pub(super) enum PasswordGeneratorCommand {
    PreferencesChanged(GeneratorPreferences),
    Copy(CopyTicket),
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyFailure {
    Stale,
    Clipboard(ClipboardError),
}

pub(super) struct PasswordGeneratorDialog {
    pub(super) open: bool,
    session: u64,
    revision: u64,
    preferences: GeneratorPreferences,
    result: Option<SecretResult>,
    revealed: bool,
    notice: Option<&'static str>,
}
impl Default for PasswordGeneratorDialog {
    fn default() -> Self {
        Self::new(GeneratorPreferences::default())
    }
}
impl PasswordGeneratorDialog {
    pub fn new(preferences: GeneratorPreferences) -> Self {
        Self {
            open: false,
            session: 0,
            revision: 0,
            preferences,
            result: None,
            revealed: false,
            notice: None,
        }
    }
    pub fn open(&mut self) {
        if !self.open {
            let Some(session) = self.session.checked_add(1) else {
                return;
            };
            self.session = session;
            self.open = true;
            self.result = None;
            self.revealed = false;
            self.notice = None;
        }
    }
    pub fn ensure_open(&mut self) {
        self.open();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.result = None;
        self.revealed = false;
        self.notice = None;
    }
    #[cfg(test)]
    pub fn preferences(&self) -> &GeneratorPreferences {
        &self.preferences
    }
    #[cfg(test)]
    pub(super) fn test_result_id(&self) -> Option<SecretResultId> {
        self.ticket().map(|ticket| ticket.id)
    }

    fn ticket(&self) -> Option<CopyTicket> {
        self.result
            .as_ref()
            .filter(|result| self.open && result.policy_revision() == self.revision)
            .map(|result| CopyTicket {
                id: result.id(),
                policy_revision: self.revision,
                session: self.session,
            })
    }
    /// The caller must first verify the accepted host completion still owns the action.
    pub fn copy_with(
        &mut self,
        ticket: CopyTicket,
        writer: impl FnOnce(&SecretResult) -> Result<CopyStatus, ClipboardError>,
    ) -> Result<CopyStatus, CopyFailure> {
        if self.ticket() != Some(ticket) {
            return Err(CopyFailure::Stale);
        }
        let result = self.result.as_ref().ok_or(CopyFailure::Stale)?;
        match writer(result) {
            Ok(status) => {
                self.notice = Some(match status {
                    CopyStatus::AcceptedAndPersisted => "Copied privately.",
                    CopyStatus::AcceptedButNotPersisted => {
                        "Windows accepted the copy. Clipboard persistence could not be confirmed."
                    }
                });
                Ok(status)
            }
            Err(error) => {
                self.notice = Some(if error.is_busy() {
                    "The clipboard is busy. Try copying again."
                } else {
                    "Copy failed. The result remains available."
                });
                Err(CopyFailure::Clipboard(error))
            }
        }
    }
    fn generate(&mut self) {
        self.revealed = false;
        self.notice = None;
        let result = match self.preferences.mode {
            GeneratorMode::Password => {
                password_generator::generate_password(&self.preferences.password, self.revision)
            }
            GeneratorMode::Passphrase => {
                password_generator::generate_passphrase(&self.preferences.passphrase, self.revision)
            }
        };
        match result {
            Ok(value) => self.result = Some(value),
            Err(_) => {
                self.notice = Some("Could not generate a result. Check the settings and try again.")
            }
        }
    }
}

pub(super) struct Actions {
    preferences: GeneratorPreferences,
    generate: bool,
    reveal: Option<CopyTicket>,
    copy: Option<CopyTicket>,
    close: bool,
    host_request: Option<UtilityPresentation>,
}
impl HostedUtilityDialog for PasswordGeneratorDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::PasswordGenerator);
    const KEY: &'static str = "password-generator";
    const TITLE: &'static str = "Password Generator";
    const SIZE: egui::Vec2 = egui::vec2(620.0, 580.0);
    const MINIMUM: egui::Vec2 = egui::vec2(400.0, 360.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = Actions;
    type Checkpoint = ();
    type Command = PasswordGeneratorCommand;
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
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
            preferences: self.preferences.clone(),
            generate: false,
            reveal: None,
            copy: None,
            close: false,
            host_request: None,
        };
        let mut open = true;
        let mut show = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(parent_input, |ui| {
                self.contents(ui, &mut actions, scopes.0, presentation)
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
    fn closed_command() -> Self::Command {
        PasswordGeneratorCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, PasswordGeneratorCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            self.close();
            return vec![PasswordGeneratorCommand::Closed];
        }
        let mut output = Vec::new();
        if self.preferences != actions.preferences {
            self.preferences = actions.preferences;
            self.result = None;
            self.revealed = false;
            self.notice = None;
            if let Some(next) = self.revision.checked_add(1) {
                self.revision = next;
            } else {
                self.close();
                return vec![PasswordGeneratorCommand::Closed];
            }
            output.push(PasswordGeneratorCommand::PreferencesChanged(
                self.preferences.clone(),
            ));
        }
        if actions.generate {
            self.generate();
        }
        if actions.host_request.is_some() {
            self.revealed = false;
        } else if actions.reveal.is_some() && actions.reveal == self.ticket() {
            self.revealed = !self.revealed;
        }
        if let Some(ticket) = actions.copy
            && self.ticket() == Some(ticket)
        {
            output.push(PasswordGeneratorCommand::Copy(ticket));
        }
        output
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
impl PasswordGeneratorDialog {
    fn contents(
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
        ui.horizontal_wrapped(|ui| {
            let (button, requested) = presentation_button(ui, presentation, ui.is_enabled());
            let label = match presentation {
                UtilityPresentation::Docked => "Pop Out",
                UtilityPresentation::Popout => "Dock",
            };
            target!(ui, button, "presentation", label, TargetKind::Button);
            if button.clicked() {
                actions.host_request = Some(requested);
            }
            let close = ui.button("Close");
            target!(ui, close, "close", "Close", TargetKind::Button);
            actions.close |= close.clicked();
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("password-generator-content")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (mode, label, key) in [
                        (GeneratorMode::Password, "Password", "mode-password"),
                        (GeneratorMode::Passphrase, "Passphrase", "mode-passphrase"),
                    ] {
                        let response = ui.radio_value(&mut actions.preferences.mode, mode, label);
                        target!(ui, response, key, label, TargetKind::Selectable);
                    }
                });

                let valid = match actions.preferences.mode {
                    GeneratorMode::Password => {
                        let settings = &mut actions.preferences.password;
                        ui.horizontal(|ui| {
                            ui.label("Length");
                            let length = ui.add(
                                egui::DragValue::new(&mut settings.length)
                                    .range(PASSWORD_LENGTH_RANGE)
                                    .clamp_existing_to_range(false),
                            );
                            target!(ui, length, "password-length", "Password length", TargetKind::TextEntry);
                        });
                        for (value, label, key) in [
                            (&mut settings.uppercase, "Uppercase", "uppercase"),
                            (&mut settings.lowercase, "Lowercase", "lowercase"),
                            (&mut settings.digits, "Digits", "digits"),
                            (&mut settings.symbols, "Symbols", "symbols"),
                            (
                                &mut settings.exclude_ambiguous,
                                "Exclude ambiguous characters",
                                "exclude-ambiguous",
                            ),
                        ] {
                            let response = ui.checkbox(value, label);
                            target!(ui, response, key, label, TargetKind::Checkbox);
                        }
                        match password_generator::validate_password_settings(settings) {
                            Ok(_) => true,
                            Err(error) => {
                                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                                false
                            }
                        }
                    }
                    GeneratorMode::Passphrase => {
                        let settings = &mut actions.preferences.passphrase;
                        ui.horizontal(|ui| {
                            ui.label("Words");
                            let count = ui.add(
                                egui::DragValue::new(&mut settings.words)
                                    .range(password_generator::PASSPHRASE_WORD_RANGE)
                                    .clamp_existing_to_range(false),
                            );
                            target!(ui, count, "word-count", "Word count", TargetKind::TextEntry);
                        });
                        ui.label("Separator");
                        ui.horizontal_wrapped(|ui| {
                            for (value, label, key) in [
                                (Separator::Hyphen, "Hyphen", "separator-hyphen"),
                                (Separator::Space, "Space", "separator-space"),
                                (Separator::Underscore, "Underscore", "separator-underscore"),
                                (Separator::Dot, "Dot", "separator-dot"),
                            ] {
                                let response = ui.radio_value(&mut settings.separator, value, label);
                                target!(ui, response, key, label, TargetKind::Selectable);
                            }
                        });
                        ui.label("Capitalization");
                        ui.horizontal_wrapped(|ui| {
                            for (value, label, key) in [
                                (Capitalization::Lowercase, "Lowercase", "case-lower"),
                                (Capitalization::InitialUppercase, "Initial capitals", "case-initial"),
                                (Capitalization::Uppercase, "Uppercase", "case-upper"),
                            ] {
                                let response = ui.radio_value(&mut settings.capitalization, value, label);
                                target!(ui, response, key, label, TargetKind::Selectable);
                            }
                        });
                        let digit = ui.checkbox(&mut settings.suffix_digit, "Add random digit");
                        target!(ui, digit, "suffix-digit", "Add random digit", TargetKind::Checkbox);
                        let symbol = ui.checkbox(&mut settings.suffix_symbol, "Add random symbol");
                        target!(ui, symbol, "suffix-symbol", "Add random symbol", TargetKind::Checkbox);
                        ui.small(
                            "EFF Large Wordlist by Joseph Bonneau for the Electronic Frontier Foundation. Sampling uses its 7,772 alphabetic entries.",
                        );
                        ui.small("https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt");
                        ui.small("Creative Commons Attribution 4.0 · https://creativecommons.org/licenses/by/4.0/");
                        match password_generator::validate_passphrase_settings(settings) {
                            Ok(_) => true,
                            Err(error) => {
                                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                                false
                            }
                        }
                    }
                };

                ui.separator();
                let settings_current = actions.preferences == self.preferences;
                let ticket = if settings_current { self.ticket() } else { None };
                ui.horizontal_wrapped(|ui| {
                    let label = if ticket.is_some() { "Regenerate" } else { "Generate" };
                    let generate = ui.add_enabled(valid, egui::Button::new(label));
                    target!(ui, generate, "generate", label, TargetKind::Button);
                    actions.generate |= generate.clicked();

                    let label = if self.revealed { "Conceal" } else { "Reveal" };
                    let reveal = ui.add_enabled(ticket.is_some(), egui::Button::new(label));
                    target!(ui, reveal, "reveal", label, TargetKind::Button);
                    if reveal.clicked() {
                        actions.reveal = ticket;
                    }

                    let copy = ui.add_enabled(ticket.is_some(), egui::Button::new("Copy"));
                    target!(ui, copy, "copy", "Copy result", TargetKind::Button);
                    if copy.clicked() {
                        actions.copy = ticket;
                    }
                });
                if ticket.is_some() && let Some(result) = &self.result {
                    if self.revealed {
                        // Label::ui publishes its text to AccessKit and widget events.
                        let (position, galley, response) = egui::Label::new(result.expose())
                            .sense(egui::Sense::hover())
                            .selectable(false)
                            .wrap()
                            .layout_in_ui(ui);
                        if ui.is_rect_visible(response.rect) {
                            ui.painter()
                                .galley(position, galley, ui.visuals().text_color());
                        }
                        ui.small("Result revealed. Use Copy to transfer it.");
                    } else {
                        ui.add(egui::Label::new("••••••••••••").selectable(false));
                    }
                    ui.small(format!("Estimated entropy {} bits", result.entropy_bits()));
                    if self.preferences.mode == GeneratorMode::Passphrase {
                        ui.small("Fixed capitalization and separators contribute zero bits.");
                    }
                }
                if settings_current && let Some(notice) = self.notice {
                    ui.label(notice);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actions(dialog: &PasswordGeneratorDialog) -> Actions {
        Actions {
            preferences: dialog.preferences.clone(),
            generate: false,
            reveal: None,
            copy: None,
            close: false,
            host_request: None,
        }
    }
    #[test]
    fn password_dialog_revision_and_copy_retry_are_bound() {
        let mut dialog = PasswordGeneratorDialog::default();
        dialog.open();
        let mut a = actions(&dialog);
        a.generate = true;
        dialog.complete_actions(a, None);
        let ticket = dialog.ticket().unwrap();
        assert_eq!(
            dialog.copy_with(ticket, |_| Err(ClipboardError::Busy)),
            Err(CopyFailure::Clipboard(ClipboardError::Busy))
        );
        assert_eq!(dialog.ticket(), Some(ticket));
        assert_eq!(
            dialog.copy_with(ticket, |_| Ok(CopyStatus::AcceptedAndPersisted)),
            Ok(CopyStatus::AcceptedAndPersisted)
        );
        let mut regenerate = actions(&dialog);
        regenerate.generate = true;
        regenerate.reveal = Some(ticket);
        dialog.complete_actions(regenerate, None);
        assert!(!dialog.revealed);
        let next = dialog.ticket().unwrap();
        assert_ne!(next.id, ticket.id);

        let mut edit = actions(&dialog);
        edit.preferences.password.length += 1;
        dialog.complete_actions(edit, None);
        assert_eq!(
            dialog.copy_with(next, |_| panic!("stale copy")),
            Err(CopyFailure::Stale)
        );
        dialog.close();
        assert!(dialog.result.is_none());
        assert!(!dialog.revealed);
    }
    #[test]
    fn password_dialog_reveal_omits_secret_from_accessibility_tree() {
        let mut dialog = PasswordGeneratorDialog::default();
        dialog.open();

        let mut generate = actions(&dialog);
        generate.generate = true;
        dialog.complete_actions(generate, None);
        let mut reveal = actions(&dialog);
        reveal.reveal = dialog.ticket();
        dialog.complete_actions(reveal, None);
        assert!(dialog.revealed);

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            dialog.contents(ui, &mut actions(&dialog), None, UtilityPresentation::Docked);
        });
        let tree = output.platform_output.accesskit_update.take();
        output.drop_without_applying_deltas();
        let tree = tree.unwrap();
        assert!(tree.nodes.iter().any(|(_, node)| {
            node.value() == Some("Result revealed. Use Copy to transfer it.")
        }));

        let secret = dialog.result.as_ref().unwrap().expose();
        assert!(tree.nodes.iter().all(|(_, node)| {
            [node.label(), node.value(), node.description()]
                .into_iter()
                .flatten()
                .all(|text| !text.contains(secret))
        }));
    }

    #[test]
    fn password_preferences_default_partial_and_secret_free() {
        let default: GeneratorPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(default, GeneratorPreferences::default());
        let partial: GeneratorPreferences =
            serde_json::from_str(r#"{"password":{"length":24}}"#).unwrap();
        assert_eq!(partial.password.length, 24);
        assert!(partial.password.uppercase);
        let serialized = serde_json::to_string(&partial).unwrap();
        assert!(!serialized.contains("result"));
        let mut dialog = PasswordGeneratorDialog::new(partial.clone());
        dialog.open();
        dialog.close();
        dialog.open();
        assert_eq!(dialog.preferences(), &partial);
    }
}
