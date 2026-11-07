use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;
use eframe::egui;

#[derive(Clone)]
pub(super) struct CalendarPopupAnchor {
    popup: egui::Id,
    layer: egui::LayerId,
    viewport: egui::ViewportId,
    epoch: u64,
}
impl CalendarPopupAnchor {
    pub fn new(ui: &egui::Ui, response: &egui::Response, epoch: u64) -> Self {
        Self {
            popup: response.id.with("popup"),
            layer: response.layer_id,
            viewport: ui.ctx().viewport_id(),
            epoch,
        }
    }
}
fn open_popup_id(
    anchors: &[CalendarPopupAnchor],
    ctx: &egui::Context,
    parent: egui::LayerId,
    epoch: u64,
) -> Option<egui::Id> {
    anchors.iter().find_map(|anchor| {
        (anchor.epoch == epoch
            && anchor.viewport == ctx.viewport_id()
            && anchor.layer == parent
            && egui::Popup::is_id_open(ctx, anchor.popup))
        .then_some(anchor.popup)
    })
}

pub(super) fn owned_popup_layer(
    anchors: &[CalendarPopupAnchor],
    ctx: &egui::Context,
    parent: egui::LayerId,
    epoch: u64,
) -> Option<egui::LayerId> {
    let popup = open_popup_id(anchors, ctx, parent, epoch)?;
    let rect = ctx.memory(|memory| memory.area_rect(popup))?;
    let layer = egui::LayerId::new(egui::Order::Foreground, popup);
    let top = ctx.memory(|memory| {
        memory
            .layer_ids()
            .filter(|layer| {
                layer.order == egui::Order::Foreground && memory.areas().visible_last_frame(layer)
            })
            .last()
    });
    let sizing =
        egui::containers::AreaState::load(ctx, popup).is_some_and(|state| !state.interactable);
    (rect.is_positive()
        && top == Some(layer)
        && (ctx.layer_id_at(rect.center()) == Some(layer) || sizing))
        .then_some(layer)
}

#[cfg(test)]
type CalendarControl = (egui::Rect, egui::Rect, bool);

pub(super) trait CalendarHostActions {
    fn request(&self) -> Option<UtilityPresentation>;
    fn set_close(&mut self);
    fn clear_close(&mut self);
    fn is_closing(&self) -> bool;
    #[cfg(test)]
    fn test_controls(&self) -> [Option<CalendarControl>; 3];
}
macro_rules! calendar_actions {
    ($($ty:ty => $primary:ident $(, $secondary:ident)?);+ $(;)?) => { $(impl CalendarHostActions for $ty {
        fn request(&self) -> Option<UtilityPresentation> { if self.close { None } else { self.host_request } }
        fn set_close(&mut self) { self.close = true; }
        fn clear_close(&mut self) { self.close = false; }
        fn is_closing(&self) -> bool { self.close }
        #[cfg(test)]
        fn test_controls(&self) -> [Option<CalendarControl>; 3] {
            [self.host_button, self.$primary, None $(.or(self.$secondary))?]
        }
    })+ };
}
calendar_actions!(
    super::calendar_popover::CalendarPopoverHostActions => agenda_button;
    super::calendar_event_details::CalendarDetailsHostActions => edit_button, delete_button;
    super::calendar_event_editor::CalendarEditorHostActions => save_button, title_field;
);

pub(super) struct CalendarHostSpec {
    pub title: &'static str,
    pub key: &'static str,
    pub viewport: egui::ViewportId,
    pub layer: egui::LayerId,
    pub size: egui::Vec2,
    pub minimum: egui::Vec2,
}
struct CalendarFrame<D, A> {
    draft: D,
    actions: A,
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    host: HostIdentity,
    epoch: u64,
    info: Option<egui::ViewportInfo>,
    copies: Vec<String>,
}
pub(super) struct CalendarWindowHost<D, A> {
    host: UtilityHostState,
    pending: Option<CalendarFrame<D, A>>,
    close_pending: bool,
    focus_pending: bool,
    epoch: Option<u64>,
    owned_popup: Option<(egui::ViewportId, egui::Id)>,
    #[cfg(test)]
    test_controls: [Option<CalendarControl>; 3],
}
impl<D, A> Default for CalendarWindowHost<D, A> {
    fn default() -> Self {
        Self {
            host: UtilityHostState::docked(),
            pending: None,
            close_pending: false,
            focus_pending: false,
            epoch: None,
            owned_popup: None,
            #[cfg(test)]
            test_controls: [None; 3],
        }
    }
}
impl<D: Clone, A: CalendarHostActions> CalendarWindowHost<D, A> {
    #[cfg(test)]
    pub fn test_controls(&self) -> [Option<CalendarControl>; 3] {
        self.test_controls
    }

    pub fn presentation(&self) -> UtilityPresentation {
        self.host.presentation()
    }
    pub fn root_layer(&self, open: bool, spec: &CalendarHostSpec) -> Option<egui::LayerId> {
        (open
            && (self.host.presentation() == UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(spec.layer)
    }
    pub fn needs_root_ui_when_hidden(&self, open: bool) -> bool {
        open && self.host.presentation() == UtilityPresentation::Popout
    }
    pub fn request_focus(&mut self) {
        self.focus_pending = true;
        self.host.request_focus();
    }
    pub fn initialize(&mut self, presentation: UtilityPresentation) {
        self.host.transition(presentation);
        self.pending = None;
        self.close_pending = false;
        self.focus_pending = true;
    }
    fn identity(spec: &CalendarHostSpec, host: HostIdentity, epoch: u64) -> NativeViewportIdentity {
        NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(spec.key).expect("stable calendar key"),
            viewport_id: spec.viewport,
            owner_generation: epoch,
            host_generation: host.generation,
        }
    }
    pub fn retire(&mut self, ctx: &egui::Context, spec: &CalendarHostSpec, epoch: u64) {
        close_bound_popup(ctx, self.owned_popup.take());
        if self.host.presentation() == UtilityPresentation::Popout {
            let identity = Self::identity(spec, self.host.identity(), self.epoch.unwrap_or(epoch));
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(spec.viewport, egui::ViewportCommand::Close);
        }
        self.pending = None;
        self.close_pending = false;
        self.focus_pending = false;
        self.epoch = None;
        #[cfg(test)]
        {
            self.test_controls = [None; 3];
        }
        self.host.retire_to_docked();
    }
    pub fn show<F>(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut D,
        spec: &CalendarHostSpec,
        open: bool,
        epoch: u64,
        root_visible: bool,
        first: bool,
        pinned: bool,
        root_owner: bool,
        input_allowed: bool,
        root_shortcut: bool,
        popup_anchors: fn(&D) -> &[CalendarPopupAnchor],
        mut render: F,
    ) where
        F: FnMut(
            &mut D,
            &egui::Context,
            bool,
            bool,
            Option<&mut egui::Ui>,
            UtilityPresentation,
        ) -> A,
    {
        if let Some(previous) = self.pending.take() {
            if open && previous.epoch == epoch {
                *dialog = previous.draft;
            }
        }
        if !open {
            return;
        }
        let stale_popup = if let Some(old_epoch) = self.epoch.filter(|old| *old != epoch) {
            if self.host.presentation() == UtilityPresentation::Popout {
                super::utility_dialog_size::close_native_utility_geometry_viewport(
                    ctx,
                    Self::identity(spec, self.host.identity(), old_epoch),
                );
            }
            self.close_pending = false;
            self.owned_popup.take()
        } else {
            None
        };
        close_bound_popup(ctx, stale_popup);
        self.epoch = Some(epoch);
        if pinned {
            self.close_pending = false;
        }
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.host.identity();
        let draft = dialog.clone();
        let mut staged = None;
        let mut owner = None;
        let mut info = None;
        let mut copies = Vec::new();
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_owner && input_allowed;
                let shortcut = first
                    && root_shortcut
                    && allowed
                    && !pinned
                    && !egui::Popup::is_any_open(ctx)
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let start = ctx.output(|o| o.commands.len());
                let mut actions = render(dialog, ctx, allowed, first, None, host.host);
                self.owned_popup = open_popup_id(popup_anchors(dialog), ctx, spec.layer, epoch)
                    .map(|id| (ctx.viewport_id(), id));
                let captured = take_copies(ctx, start);
                if allowed && first {
                    copies = captured;
                }
                if shortcut {
                    actions.set_close();
                }
                owner = Some(root);
                staged = Some(actions);
            }
        } else {
            let builder = egui::ViewportBuilder::default()
                .with_title(spec.title)
                .with_inner_size(spec.size)
                .with_min_inner_size(spec.minimum)
                .with_resizable(true);
            ctx.show_viewport_immediate(spec.viewport, builder, |child, class| {
                let c = child.ctx().clone();
                close_bound_popup(&c, stale_popup);
                let native =
                    class == egui::ViewportClass::Immediate && c.viewport_id() == spec.viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner = Some(ViewportPassProvenance::current(&c, c.viewport_id()));
                    if native {
                        info = Some(child.input(|i| i.viewport().clone()));
                    }
                } else {
                    owner = None;
                    info = None;
                    staged = None;
                    *dialog = draft.clone();
                }
                let parent = if native {
                    egui::LayerId::background()
                } else {
                    spec.layer
                };
                let owned_popup = owned_popup_layer(popup_anchors(dialog), &c, parent, epoch);
                let allowed = if native {
                    input_allowed
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && (!egui::Popup::is_any_open(&c) || owned_popup.is_some())
                        && super::jump_mode::ui::focused_response_layer(&c).is_none_or(|l| {
                            l == egui::LayerId::background() || Some(l) == owned_popup
                        })
                } else {
                    root_owner && input_allowed
                };
                if native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.close_pending = true;
                    }
                }
                let shortcut = child_first
                    && (native || root_shortcut)
                    && allowed
                    && !pinned
                    && !egui::Popup::is_any_open(&c)
                    && c.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let start = c.output(|o| o.commands.len());
                let mut result = None;
                if native {
                    egui::CentralPanel::default().show(child, |ui| {
                        result = Some(render(
                            dialog,
                            &c,
                            allowed,
                            child_first,
                            Some(ui),
                            host.host,
                        ));
                    });
                } else {
                    result = Some(render(dialog, &c, allowed, child_first, None, host.host));
                }
                let captured = take_copies(&c, start);
                self.owned_popup = open_popup_id(popup_anchors(dialog), &c, parent, epoch)
                    .map(|id| (c.viewport_id(), id));
                if allowed && child_first {
                    copies = captured;
                }
                if let Some(actions) = &mut result {
                    if !pinned && (shortcut || self.close_pending) {
                        actions.set_close();
                    }
                }
                staged = result;
            });
        }
        #[cfg(test)]
        {
            self.test_controls = staged
                .as_ref()
                .map_or([None; 3], CalendarHostActions::test_controls);
        }
        if let (Some(owner), Some(mut actions)) = (owner, staged) {
            if pinned {
                actions.clear_close();
            }
            if first && !ctx.will_discard() {
                self.pending = Some(CalendarFrame {
                    draft,
                    actions,
                    root,
                    owner,
                    host,
                    epoch,
                    info,
                    copies,
                });
                return;
            }
        }
        *dialog = draft;
    }
    pub fn complete(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut D,
        spec: &CalendarHostSpec,
        open: bool,
        epoch: u64,
    ) -> Option<(A, UtilityPresentation, bool)> {
        let frame = self.pending.take()?;
        if !open
            || frame.epoch != epoch
            || frame.host != self.host.identity()
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if open && frame.epoch == epoch {
                *dialog = frame.draft;
            }
            return None;
        }
        let identity = Self::identity(spec, frame.host, epoch);
        if let Some(info) = &frame.info {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                spec.title,
                identity,
                info,
                spec.size,
                Some(spec.minimum),
                None,
            );
            if self.focus_pending {
                ctx.send_viewport_cmd_to(spec.viewport, egui::ViewportCommand::Focus);
            }
        } else if self.focus_pending {
            ctx.move_to_top(spec.layer);
        }
        self.focus_pending = false;
        let request = frame.actions.request();
        if request.is_none() && !frame.actions.is_closing() {
            for copy in frame.copies {
                let _ = crate::actions::clipboard::set_text(&copy);
            }
        }
        let show_root = request == Some(UtilityPresentation::Docked);
        if let Some(next) = request {
            if self.host.transition(next) {
                close_bound_popup(ctx, self.owned_popup.take());
                if next == UtilityPresentation::Docked {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(spec.viewport, egui::ViewportCommand::Close);
                }
                self.focus_pending = true;
                ctx.request_repaint();
            }
        }
        if frame.host.host == UtilityPresentation::Popout {
            self.host.complete_bootstrap(
                frame.host,
                frame.info.is_none().then_some(spec.layer),
                None,
            );
        }
        Some((frame.actions, frame.host.host, show_root))
    }
}
fn close_bound_popup(ctx: &egui::Context, bound: Option<(egui::ViewportId, egui::Id)>) {
    if let Some((viewport, id)) = bound
        && ctx.viewport_id() == viewport
    {
        egui::Popup::close_id(ctx, id);
    }
}

fn take_copies(ctx: &egui::Context, start: usize) -> Vec<String> {
    ctx.output_mut(|output| {
        let mut copies = Vec::new();
        for command in output.commands.split_off(start) {
            match command {
                egui::OutputCommand::CopyText(text) => copies.push(text),
                other => output.commands.push(other),
            }
        }
        copies
    })
}
