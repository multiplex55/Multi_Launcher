use eframe::egui;

/// The presentation used by an ordinary tool's single live session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UtilityPresentation {
    Docked,
    Popout,
}

/// Identifies a particular lifetime of a tool presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HostIdentity {
    pub(super) host: UtilityPresentation,
    pub(super) generation: u64,
}

/// Provenance for effects staged during one viewport pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ViewportPassProvenance {
    pub(super) viewport_id: egui::ViewportId,
    pub(super) frame_nr: u64,
    pub(super) pass_nr: u64,
}

impl ViewportPassProvenance {
    pub(super) fn current(ctx: &egui::Context, viewport_id: egui::ViewportId) -> Self {
        Self {
            viewport_id,
            frame_nr: ctx.cumulative_frame_nr_for(viewport_id),
            pass_nr: ctx.cumulative_pass_nr_for(viewport_id),
        }
    }

    /// True only after the originating viewport completed the exact pass.
    pub(super) fn was_retained(self, ctx: &egui::Context) -> bool {
        ctx.cumulative_frame_nr_for(self.viewport_id) == self.frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(self.viewport_id) == self.pass_nr.saturating_add(1)
    }
}

/// Session-independent lifecycle state shared by ordinary utility hosts.
/// Tool documents, close decisions, workers, and accepted effects stay with
/// their existing owners.
pub(super) struct UtilityHostState {
    identity: HostIdentity,
    bootstrap_pending: bool,
    focus_pending: bool,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
}

impl Default for UtilityHostState {
    fn default() -> Self {
        Self::docked()
    }
}

impl UtilityHostState {
    pub(super) fn docked() -> Self {
        Self {
            identity: HostIdentity {
                host: UtilityPresentation::Docked,
                generation: 0,
            },
            bootstrap_pending: false,
            focus_pending: false,
            embedded_root_layer: None,
            embedded_root_focused_id: None,
        }
    }

    pub(super) fn popped_out() -> Self {
        Self {
            identity: HostIdentity {
                host: UtilityPresentation::Popout,
                generation: 0,
            },
            bootstrap_pending: false,
            focus_pending: false,
            embedded_root_layer: None,
            embedded_root_focused_id: None,
        }
    }

    pub(super) fn identity(&self) -> HostIdentity {
        self.identity
    }

    pub(super) fn presentation(&self) -> UtilityPresentation {
        self.identity.host
    }

    pub(super) fn bootstrap_pending(&self) -> bool {
        self.bootstrap_pending
    }

    pub(super) fn focus_pending(&self) -> bool {
        self.focus_pending
    }

    pub(super) fn embedded_root_layer(&self) -> Option<egui::LayerId> {
        self.embedded_root_layer
    }

    pub(super) fn embedded_root_focused_id(&self) -> Option<egui::Id> {
        self.embedded_root_focused_id
    }

    /// Begin a new presentation generation. Repeated requests are harmless.
    pub(super) fn transition(&mut self, presentation: UtilityPresentation) -> bool {
        if self.identity.host == presentation {
            return false;
        }

        self.identity.generation = self.identity.generation.wrapping_add(1);
        self.identity.host = presentation;
        self.embedded_root_layer = None;
        self.embedded_root_focused_id = None;
        self.focus_pending = presentation == UtilityPresentation::Popout;
        self.bootstrap_pending = presentation == UtilityPresentation::Popout;
        true
    }

    /// Retire a popped out presentation after its owning tool closes.
    pub(super) fn retire_to_docked(&mut self) {
        self.transition(UtilityPresentation::Docked);
        self.bootstrap_pending = false;
        self.focus_pending = false;
        self.embedded_root_layer = None;
        self.embedded_root_focused_id = None;
    }

    pub(super) fn request_focus(&mut self) {
        if self.identity.host == UtilityPresentation::Popout {
            self.focus_pending = true;
        }
    }

    pub(super) fn complete_bootstrap(
        &mut self,
        identity: HostIdentity,
        embedded_root_layer: Option<egui::LayerId>,
        embedded_root_focused_id: Option<egui::Id>,
    ) {
        if identity == self.identity {
            self.bootstrap_pending = false;
            self.embedded_root_layer = embedded_root_layer;
            self.embedded_root_focused_id = embedded_root_focused_id;
        }
    }

    pub(super) fn complete_focus(&mut self, identity: HostIdentity) {
        if identity == self.identity {
            self.focus_pending = false;
        }
    }
}

pub(super) fn presentation_button(
    ui: &mut egui::Ui,
    presentation: UtilityPresentation,
    allowed: bool,
) -> (egui::Response, UtilityPresentation) {
    let (label, requested) = match presentation {
        UtilityPresentation::Docked => ("Pop Out", UtilityPresentation::Popout),
        UtilityPresentation::Popout => ("Dock", UtilityPresentation::Docked),
    };
    (ui.add_enabled(allowed, egui::Button::new(label)), requested)
}

#[cfg(test)]
mod tests {
    use super::{UtilityHostState, UtilityPresentation};
    use eframe::egui;

    #[test]
    fn transition_is_idempotent_and_retired_generations_are_stale() {
        let mut host = UtilityHostState::docked();
        let docked = host.identity();

        assert!(!host.transition(UtilityPresentation::Docked));
        assert_eq!(host.identity(), docked);

        assert!(host.transition(UtilityPresentation::Popout));
        let popout = host.identity();
        assert_eq!(popout.generation, docked.generation.wrapping_add(1));
        assert!(host.bootstrap_pending());
        assert!(host.focus_pending());
        assert!(!host.transition(UtilityPresentation::Popout));
        assert_eq!(host.identity(), popout);

        assert!(host.transition(UtilityPresentation::Docked));
        let docked_again = host.identity();
        assert_ne!(docked_again, popout);
        host.complete_focus(popout);
        host.complete_bootstrap(popout, None, None);
        assert!(!host.bootstrap_pending());
        assert!(!host.focus_pending());
        assert_eq!(host.presentation(), UtilityPresentation::Docked);

        host.transition(UtilityPresentation::Popout);
        let new_popout = host.identity();
        assert_ne!(new_popout, popout);
        host.complete_focus(popout);
        host.complete_bootstrap(popout, None, None);
        assert!(host.focus_pending());
        assert!(host.bootstrap_pending());
        host.complete_focus(new_popout);
        host.complete_bootstrap(new_popout, None, None);
        assert!(!host.focus_pending());
        assert!(!host.bootstrap_pending());
    }

    #[test]
    fn retiring_a_popout_closes_only_host_lifecycle_state() {
        let mut host = UtilityHostState::docked();
        host.transition(UtilityPresentation::Popout);
        let popout = host.identity();

        host.retire_to_docked();

        assert_ne!(host.identity(), popout);
        assert_eq!(host.presentation(), UtilityPresentation::Docked);
        assert!(!host.bootstrap_pending());
        assert!(!host.focus_pending());
        host.retire_to_docked();
        assert_eq!(
            host.identity().generation,
            popout.generation.wrapping_add(1)
        );
    }

    #[test]
    fn viewport_provenance_requires_the_originating_pass_to_complete() {
        let ctx = egui::Context::default();
        let root = egui::ViewportId::ROOT;
        let provenance = super::ViewportPassProvenance::current(&ctx, root);
        assert!(!provenance.was_retained(&ctx));

        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        assert!(provenance.was_retained(&ctx));
        output.drop_without_applying_deltas();
    }
}

#[derive(Clone, Copy)]
pub(super) struct HostedCopyReceipt {
    pub(super) root: ViewportPassProvenance,
    pub(super) owner: ViewportPassProvenance,
    pub(super) host: HostIdentity,
    pub(super) epoch: u64,
    pub(super) eligible: bool,
}
#[derive(Default)]
pub(super) struct HostedCopyCapture {
    pub(super) armed: std::collections::HashMap<egui::ViewportId, HostedCopyReceipt>,
    pub(super) captured: Vec<(HostedCopyReceipt, Vec<String>)>,
}
impl egui::Plugin for HostedCopyCapture {
    fn debug_name(&self) -> &'static str {
        "HostedNativeCopyCapture"
    }
    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        // egui restores the parent viewport before invoking output hooks. Identify
        // the just-ended child from its counters rather than ctx.viewport_id().
        let ended = self.armed.iter().find_map(|(viewport, receipt)| {
            (ctx.cumulative_pass_nr_for(*viewport) == receipt.owner.pass_nr.saturating_add(1)
                && ctx.cumulative_frame_nr_for(*viewport) == receipt.owner.frame_nr)
                .then_some(*viewport)
        });
        let Some(receipt) = ended.and_then(|viewport| self.armed.remove(&viewport)) else {
            return;
        };
        let mut copies = Vec::new();
        output
            .platform_output
            .commands
            .retain(|command| match command {
                egui::OutputCommand::CopyText(text) => {
                    copies.push(text.clone());
                    false
                }
                _ => true,
            });
        if receipt.eligible && !copies.is_empty() {
            self.captured.push((receipt, copies));
        }
    }
}
