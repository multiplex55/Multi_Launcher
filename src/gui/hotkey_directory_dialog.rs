use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::hotkey::directory::{EditDestination, Entry, EntryId, Snapshot, Source, Status};
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DirectorySort {
    #[default]
    Owner,
    Status,
    Conflicts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HotkeyDirectoryEditRequest {
    /// Global inventory serial at the time the user selected the row.
    pub(crate) publication: u64,
    pub(crate) source: Source,
    pub(crate) entry_id: EntryId,
    /// Revision of the owning source snapshot, distinct from both the global
    /// inventory serial and the candidate's owner-specific source revision.
    pub(crate) source_publication_revision: u64,
    /// Revision identifying the source owner's current row data.
    pub(crate) source_revision: u64,
    pub(crate) owner: crate::hotkey::directory::Owner,
    pub(crate) destination: EditDestination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedEntry {
    source: Source,
    id: EntryId,
}

#[derive(Clone, Default)]
pub(crate) struct HotkeyDirectoryDialog {
    pub(crate) open: bool,
    epoch: u64,
    sort_anchor: Option<egui::Id>,
    filter: String,
    sort: DirectorySort,
    selected: Option<SelectedEntry>,
    message: Option<String>,
    pending_edit: Option<HotkeyDirectoryEditRequest>,
    #[cfg(test)]
    test_sort_response: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_status_response: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    last_edit_response: Option<(Source, EntryId, egui::Id, egui::Rect)>,
    #[cfg(test)]
    last_select_responses: Vec<(Source, EntryId, egui::Id, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    last_rows_scroll: Option<(egui::Id, egui::Vec2)>,
}

impl HotkeyDirectoryDialog {
    #[cfg(test)]
    pub(crate) fn take_edit_request(&mut self) -> Option<HotkeyDirectoryEditRequest> {
        self.pending_edit.take()
    }

    #[cfg(test)]
    pub(super) fn test_sort_response(&self) -> Option<(egui::Rect, egui::Rect, bool)> { self.test_sort_response }
    #[cfg(test)]
    pub(super) fn test_status_response(&self) -> Option<(egui::Rect, egui::Rect, bool)> { self.test_status_response }
    #[cfg(test)]
    pub(super) fn test_epoch(&self) -> u64 { self.epoch }
    #[cfg(test)]
    pub(super) fn test_status_sort_selected(&self) -> bool { self.sort == DirectorySort::Status }

    #[cfg(test)]
    pub(crate) fn test_last_edit_response(
        &self,
    ) -> Option<(Source, EntryId, egui::Id, egui::Rect)> {
        self.last_edit_response.clone()
    }

    #[cfg(test)]
    pub(crate) fn test_select_response(
        &self,
        source: Source,
        id: &EntryId,
    ) -> Option<(egui::Id, egui::Rect, egui::Rect)> {
        self.last_select_responses
            .iter()
            .find(|(candidate_source, candidate_id, _, _, _)| {
                *candidate_source == source && candidate_id == id
            })
            .map(|(_, _, id, rect, clip_rect)| (*id, *rect, *clip_rect))
    }

    #[cfg(test)]
    pub(crate) fn test_selected_identity(&self) -> Option<(Source, EntryId)> {
        self.selected
            .as_ref()
            .map(|selected| (selected.source, selected.id.clone()))
    }

    #[cfg(test)]
    pub(crate) fn test_set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
    }

    #[cfg(test)]
    pub(crate) fn test_filter(&self) -> &str {
        &self.filter
    }

    #[cfg(test)]
    pub(crate) fn test_last_rows_scroll(&self) -> Option<(egui::Id, egui::Vec2)> {
        self.last_rows_scroll
    }

    #[cfg(test)]
    pub(crate) fn show(
        &mut self, ctx: &egui::Context, snapshot: Option<&Snapshot>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>, interaction_allowed: bool,
    ) {
        let actions = self.show_host(ctx, snapshot, scope, interaction_allowed, true, None,
            super::utility_host::UtilityPresentation::Docked, true);
        if actions.close { self.open = false; }
    }

    pub(super) fn start_session(&mut self) {
        if !self.open {
            self.epoch = self.epoch.wrapping_add(1).max(1);
            self.open = true;
        }
    }

    fn show_host(
        &mut self, ctx: &egui::Context, snapshot: Option<&Snapshot>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>, interaction_allowed: bool,
        first: bool, child_ui: Option<&mut egui::Ui>, presentation: super::utility_host::UtilityPresentation,
        can_close: bool,
    ) -> DirectoryHostActions {
        #[cfg(test)]
        { self.last_edit_response = None; self.last_select_responses.clear(); self.last_rows_scroll = None; self.test_sort_response = None; self.test_status_response = None; }
        let mut open = self.open;
        let mut visual_order = 0;
        let mut host_request = None;
        #[cfg(test)]
        let mut test_button = None;
        let mut body = |ui: &mut egui::Ui| {
            let (button, next) = super::utility_host::presentation_button(ui, presentation, interaction_allowed);
            #[cfg(test)] { test_button = Some((button.rect, ui.clip_rect(), button.enabled())); }
            if first && !ctx.will_discard() && button.clicked() { host_request = Some(next); }
                ui.label("Browse configured shortcuts and their reported runtime status. Editing opens the owning settings editor; it does not run a shortcut.");
                let filter = ui.add_enabled(
                    interaction_allowed,
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Search End, paste, macro, owner, status…"),
                );
                if let Some(scope) = scope {
                    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
                        Some(scope),
                        ui,
                        &filter,
                        "hotkey-directory/filter",
                        "Search hotkeys",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        &mut visual_order,
                    );
                } else {
                    visual_order = visual_order.saturating_add(1);
                }

                ui.horizontal(|ui| {
                    ui.label("Sort by");
                    let mut sort = self.sort;
                    ui.add_enabled_ui(interaction_allowed, |ui| {
                        let response = egui::ComboBox::from_id_salt("hotkey-directory/sort")
                            .selected_text(match sort {
                                DirectorySort::Owner => "Owner",
                                DirectorySort::Status => "Status",
                                DirectorySort::Conflicts => "Conflicts first",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut sort, DirectorySort::Owner, "Owner");
                                let status = ui.selectable_value(&mut sort, DirectorySort::Status, "Status");
                                #[cfg(test)] { self.test_status_response = Some((status.rect, ui.clip_rect(), status.enabled())); }
                                ui.selectable_value(
                                    &mut sort,
                                    DirectorySort::Conflicts,
                                    "Conflicts first",
                                );
                            });
                        self.sort_anchor = Some(response.response.id);
                        #[cfg(test)] { self.test_sort_response = Some((response.response.rect, ui.clip_rect(), response.response.enabled())); }
                    });
                    if interaction_allowed {
                        self.sort = sort;
                    }
                    if let Some(snapshot) = snapshot {
                        ui.label(format!("{} configured shortcuts", snapshot.entries().len()));
                    }
                });

                if let Some(message) = self.message.as_ref() {
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                }
                let Some(snapshot) = snapshot else {
                    ui.weak("Shortcut status is unavailable until the inventory is published.");
                    return;
                };
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|selected| !snapshot.entries().iter().any(|entry| {
                        entry.source == selected.source && entry.id == selected.id
                    }))
                {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "The selected shortcut is no longer present. Choose a current row before editing.",
                    );
                }

                let mut rows = snapshot
                    .entries()
                    .iter()
                    .filter(|entry| matches_filter(entry, snapshot, &self.filter))
                    .collect::<Vec<_>>();
                rows.sort_by(|left, right| compare_entries(left, right, self.sort));

                let no_matches = rows.is_empty();
                let _rows_output = egui::ScrollArea::both()
                    .id_salt("hotkey-directory/rows")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            header_cell(ui, 125.0, "Shortcut");
                            header_cell(ui, 260.0, "Action");
                            header_cell(ui, 135.0, "Owner");
                            header_cell(ui, 145.0, "Status");
                            header_cell(ui, 255.0, "Conflict");
                            header_cell(ui, 70.0, "Edit");
                        });
                        ui.separator();
                        for entry in rows {
                            ui.push_id(("hotkey-directory-row", entry.source, &entry.id), |ui| {
                                ui.horizontal(|ui| {
                                    let shortcut = entry.chord.as_deref().unwrap_or("—");
                                    cell(ui, 125.0, shortcut);
                                    let is_selected = self.selected.as_ref().is_some_and(|selected| {
                                        selected.source == entry.source && selected.id == entry.id
                                    });
                                    let action = ui.add_sized(
                                        [260.0, ui.spacing().interact_size.y],
                                        egui::Button::selectable(is_selected, &entry.action_label),
                                    );
                                    #[cfg(test)]
                                    self.last_select_responses.push((
                                        entry.source,
                                        entry.id.clone(),
                                        action.id,
                                        action.rect,
                                        ui.clip_rect(),
                                    ));
                                    if action.clicked() && interaction_allowed {
                                        self.selected = Some(SelectedEntry {
                                            source: entry.source,
                                            id: entry.id.clone(),
                                        });
                                        self.message = None;
                                    }
                                    if let Some(scope) = scope {
                                        let identity = row_target_identity(entry, "select");
                                        let _ = crate::gui::jump_mode::ui::register_and_apply_target(
                                            Some(scope),
                                            ui,
                                            &action,
                                            identity,
                                            entry.action_label.clone(),
                                            crate::gui::jump_mode::TargetKind::Button,
                                            &mut visual_order,
                                        );
                                    } else {
                                        visual_order = visual_order.saturating_add(1);
                                    }
                                    cell(ui, 135.0, &entry.owner_label);
                                    let status = status_label(entry);
                                    let status_response = ui.add_sized(
                                        [145.0, ui.spacing().interact_size.y],
                                        egui::Label::new(status),
                                    );
                                    if let Some(reason) = reason_text(entry) {
                                        status_response.on_hover_text(reason);
                                    }
                                    let conflict = conflict_text(entry, snapshot);
                                    cell(ui, 255.0, &conflict);
                                    let editable = entry_is_editable(entry);
                                    let edit = ui.add_enabled(
                                        interaction_allowed && editable,
                                        egui::Button::new("Edit"),
                                    );
                                    #[cfg(test)]
                                    if editable {
                                        self.last_edit_response = Some((
                                            entry.source,
                                            entry.id.clone(),
                                            edit.id,
                                            edit.rect,
                                        ));
                                    }
                                    if let Some(scope) = scope {
                                        let identity = row_target_identity(entry, "edit");
                                        let _ = crate::gui::jump_mode::ui::register_and_apply_target(
                                            Some(scope),
                                            ui,
                                            &edit,
                                            identity,
                                            format!("Edit {}", entry.action_label),
                                            crate::gui::jump_mode::TargetKind::Button,
                                            &mut visual_order,
                                        );
                                    } else {
                                        visual_order = visual_order.saturating_add(1);
                                    }
                                    let edit = if !editable {
                                        edit.on_hover_text(if matches!(
                                            entry.edit.as_ref(),
                                            Some(EditDestination::ScreenDraw(_))
                                        ) {
                                            "Not configurable — no current editor."
                                        } else {
                                            "No current editable owner is available."
                                        })
                                    } else {
                                        edit
                                    };
                                    if edit.clicked() && interaction_allowed && first && !ctx.will_discard() {
                                        let Some(source_publication_revision) = snapshot
                                            .source_revision(entry.source)
                                        else {
                                            self.message = Some(
                                                "The shortcut owner is no longer present.".into(),
                                            );
                                            return;
                                        };
                                        let Some(destination) = entry.edit.clone() else {
                                            self.message = Some(
                                                "No current editable owner is available.".into(),
                                            );
                                            return;
                                        };
                                        self.pending_edit = Some(HotkeyDirectoryEditRequest {
                                            publication: snapshot.publication(),
                                            source: entry.source,
                                            entry_id: entry.id.clone(),
                                            source_publication_revision,
                                            source_revision: entry.source_revision,
                                            owner: entry.owner,
                                            destination,
                                        });
                                        self.message = None;
                                    }
                                });
                            });
                        }
                        if no_matches {
                            ui.weak("No shortcuts match this search.");
                        }
                    });
                #[cfg(test)]
                {
                    self.last_rows_scroll = Some((_rows_output.id, _rows_output.state.offset));
                }
        };
        if let Some(ui) = child_ui { body(ui); } else {
            let id = egui::Id::new(Some("Hotkey Directory"));
            egui::Window::new("Hotkey Directory").id(id).default_size((1060.0, 640.0))
                .min_size((700.0, 380.0)).resizable(true)
                .show_utility(ctx, "hotkey-directory", id, Some(&mut open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::outer(egui::vec2(1060.0, 640.0)), body);
        }
        DirectoryHostActions { host_request, close: can_close && !open, #[cfg(test)] test_button }
    }

    pub(crate) fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub(crate) fn clear_message(&mut self) {
        self.message = None;
    }
}

pub(super) struct DirectoryHostRuntime {
    pub snapshot: Option<std::sync::Arc<Snapshot>>,
    pub can_close: bool,
}

#[derive(Default)]
pub(super) struct DirectoryHostActions {
    host_request: Option<super::utility_host::UtilityPresentation>,
    close: bool,
    #[cfg(test)]
    test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

pub(super) enum DirectoryHostCommand {
    Close,
    Edit(HotkeyDirectoryEditRequest),
}

impl super::utility_window_host::HostedUtilityDialog for HotkeyDirectoryDialog {
    const KIND: Option<super::jump_mode::ui::NavigationDialogKind> =
        Some(super::jump_mode::ui::NavigationDialogKind::HotkeyDirectory);
    const KEY: &'static str = "hotkey-directory";
    const TITLE: &'static str = "Hotkey Directory";
    const SIZE: egui::Vec2 = egui::vec2(1060.0, 640.0);
    const MINIMUM: egui::Vec2 = egui::vec2(700.0, 380.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = DirectoryHostRuntime;
    type Actions = DirectoryHostActions;
    type Checkpoint = Self;
    type Command = DirectoryHostCommand;
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: super::jump_mode::ui::NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|i| i.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|l| l == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|l| {
                l == binding.base_layer || Some(l) == popup || l == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId> {
        let anchor = self.sort_anchor?;
        (egui::ComboBox::is_open(ctx, anchor)
            && ctx
                .read_response(anchor)
                .is_some_and(|response| response.enabled() && response.layer_id == parent))
        .then(|| egui::LayerId::new(egui::Order::Foreground, anchor.with("popup")))
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<super::jump_mode::ui::NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> Self {
        self.clone()
    }
    fn restore(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (
            Option<&super::jump_mode::ui::JumpModeScope>,
            Option<&super::jump_mode::ui::JumpModeScope>,
        ),
        _: super::jump_mode::ui::NavigationOwner,
        allowed: bool,
        _: super::jump_mode::ui::NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> Self::Actions {
        self.show_host(
            ctx,
            runtime.snapshot.as_deref(),
            scopes.0,
            allowed,
            first,
            child,
            presentation,
            runtime.can_close,
        )
    }
    fn host_request(actions: &Self::Actions) -> Option<super::utility_host::UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        DirectoryHostCommand::Close
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, DirectoryHostCommand::Close)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![DirectoryHostCommand::Close]
        } else {
            self.pending_edit
                .take()
                .map(DirectoryHostCommand::Edit)
                .into_iter()
                .collect()
        }
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.test_button
    }
}

fn header_cell(ui: &mut egui::Ui, width: f32, text: &str) {
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::Label::new(egui::RichText::new(text).strong()),
    );
}

fn cell(ui: &mut egui::Ui, width: f32, text: &str) {
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::Label::new(text),
    );
}

fn row_target_identity(entry: &Entry, action: &str) -> String {
    format!(
        "entry/{:?}/{}/{}/{}",
        entry.source,
        entry.id.as_str().len(),
        entry.id.as_str(),
        action
    )
}

fn entry_is_editable(entry: &Entry) -> bool {
    entry.status != Status::Unavailable
        && !matches!(entry.edit.as_ref(), Some(EditDestination::ScreenDraw(_)))
        && entry.edit.is_some()
}

fn status_label(entry: &Entry) -> String {
    match entry.status {
        Status::Active { .. } => format!("Active · {}", route_label(entry)),
        Status::Pending { .. } => format!("Pending · {}", route_label(entry)),
        Status::Disabled => "Disabled".into(),
        Status::Invalid => "Invalid".into(),
        Status::Conflict => "Conflict".into(),
        Status::Unsupported => "Unsupported".into(),
        Status::OmittedByPrecedence => "Omitted by precedence".into(),
        Status::Unavailable => "Unavailable".into(),
        Status::Unknown => format!("Unknown · {}", route_label(entry)),
    }
}

fn route_label(entry: &Entry) -> &'static str {
    match entry.route {
        crate::hotkey::directory::RouteKind::LegacyPolling => "legacy route",
        crate::hotkey::directory::RouteKind::SharedHook => "shared hook",
        crate::hotkey::directory::RouteKind::MacroPolling => "macro route",
        crate::hotkey::directory::RouteKind::Radial => "radial route",
        crate::hotkey::directory::RouteKind::Local => "local route",
        crate::hotkey::directory::RouteKind::Gesture => "gesture route",
    }
}

fn reason_text(entry: &Entry) -> Option<&'static str> {
    entry.reason.map(|reason| match reason {
        crate::hotkey::directory::Reason::InvalidChord => "The configured shortcut is invalid.",
        crate::hotkey::directory::Reason::InvalidScope => "The configured scope is invalid.",
        crate::hotkey::directory::Reason::RouteNotAcknowledged => {
            "The route is not acknowledged by its runtime owner."
        }
        crate::hotkey::directory::Reason::RuntimeRestorationUncertain => {
            "Runtime restoration is uncertain; no active route is claimed."
        }
        crate::hotkey::directory::Reason::OwnerDisabled => "The owning feature is disabled.",
        crate::hotkey::directory::Reason::OwnerUnavailable => {
            "The owning editor or provider is unavailable."
        }
        crate::hotkey::directory::Reason::UnsupportedByRoute => {
            "This shortcut is not supported by the current route."
        }
        crate::hotkey::directory::Reason::OmittedByPrecedence => {
            "A higher-precedence route owns this shortcut."
        }
        crate::hotkey::directory::Reason::ConflictsWithConfiguredRoute => {
            "Another configured shortcut can fire with this one."
        }
        crate::hotkey::directory::Reason::LocalRouteHasNoGlobalReceipt => {
            "This local route has no global runtime receipt."
        }
    })
}

fn conflict_text(entry: &Entry, snapshot: &Snapshot) -> String {
    if entry.conflicts.is_empty() {
        return "—".into();
    }
    entry
        .conflicts
        .iter()
        .filter_map(|conflict| {
            snapshot
                .entries()
                .iter()
                .find(|other| other.source == conflict.source && other.id == conflict.id)
                .map(|other| other.action_label.as_str())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn matches_filter(entry: &Entry, snapshot: &Snapshot, filter: &str) -> bool {
    let query = filter.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let status = status_label(entry);
    let conflict = conflict_text(entry, snapshot);
    [
        entry.chord.as_deref().unwrap_or(""),
        entry.action_label.as_str(),
        entry.owner_label.as_str(),
        status.as_str(),
        conflict.as_str(),
        reason_text(entry).unwrap_or(""),
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&query))
}

fn compare_entries(left: &Entry, right: &Entry, sort: DirectorySort) -> std::cmp::Ordering {
    let stable = || {
        left.owner_label
            .to_lowercase()
            .cmp(&right.owner_label.to_lowercase())
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.id.cmp(&right.id))
    };
    match sort {
        DirectorySort::Owner => stable(),
        DirectorySort::Status => status_rank(left.status)
            .cmp(&status_rank(right.status))
            .then_with(stable),
        DirectorySort::Conflicts => right
            .conflicts
            .len()
            .cmp(&left.conflicts.len())
            .then_with(stable),
    }
}

fn status_rank(status: Status) -> u8 {
    match status {
        Status::Conflict => 0,
        Status::Invalid => 1,
        Status::Pending { .. } => 2,
        Status::Active { .. } => 3,
        Status::Unknown => 4,
        Status::Unsupported => 5,
        Status::OmittedByPrecedence => 6,
        Status::Unavailable => 7,
        Status::Disabled => 8,
    }
}

pub(crate) fn validate_edit_request(
    request: &HotkeyDirectoryEditRequest,
    snapshot: Option<&Snapshot>,
) -> Result<EditDestination, &'static str> {
    let snapshot = snapshot.ok_or("The shortcut inventory is no longer available.")?;
    if snapshot.publication() < request.publication {
        return Err("The shortcut inventory is older than the selected row.");
    }
    let current_revision = snapshot
        .source_revision(request.source)
        .ok_or("The shortcut owner is no longer present.")?;
    if current_revision != request.source_publication_revision {
        return Err(
            "This shortcut changed after the directory was opened. Select the current row and try again.",
        );
    }
    let mut matches = snapshot.entries().iter().filter(|entry| {
        entry.source == request.source
            && entry.id == request.entry_id
            && entry.source_revision == request.source_revision
            && entry.owner == request.owner
            && entry.edit.as_ref() == Some(&request.destination)
    });
    let entry = matches
        .next()
        .ok_or("This shortcut is no longer available for editing.")?;
    if matches.next().is_some() {
        return Err("This shortcut identity is ambiguous and cannot be edited safely.");
    }
    if !entry_is_editable(entry) {
        return Err(
            "The current owner does not provide an editable destination for this shortcut.",
        );
    }
    Ok(request.destination.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::directory::{
        Candidate, CandidateState, MainSetting, Owner, PasteSetting, RouteKind, Scope,
        SourceSnapshot,
    };

    fn candidate(id: &str, chord: &str, action: &str, revision: u64) -> Candidate {
        Candidate {
            id: EntryId::new(id),
            owner: Owner::Launcher,
            owner_label: "Application".into(),
            action_label: action.into(),
            chord: Some(chord.into()),
            route: crate::hotkey::directory::RouteKind::LegacyPolling,
            scope: Scope::ProcessWide,
            source_revision: revision,
            expected_runtime_generation: None,
            state: CandidateState::Unknown,
            reason: Some(crate::hotkey::directory::Reason::RouteNotAcknowledged),
            edit: Some(EditDestination::MainSettings(
                crate::hotkey::directory::MainSetting::Launcher,
            )),
        }
    }

    fn snapshot(revision: u64, candidates: Vec<Candidate>) -> Snapshot {
        Snapshot::from_sources(
            revision,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                revision,
                candidates,
                Vec::new(),
            )],
        )
    }

    #[test]
    fn hotkey_directory_ui_filters_metadata_and_sorts_stably_by_identity() {
        let mut first = candidate("launcher", "End", "Toggle launcher", 1);
        let duplicate_label = Candidate {
            id: EntryId::new("quit"),
            chord: Some("Ctrl+End".into()),
            ..candidate("quit", "Ctrl+End", "Toggle launcher", 1)
        };
        first.reason = Some(crate::hotkey::directory::Reason::RouteNotAcknowledged);
        let inventory = snapshot(1, vec![duplicate_label, first]);
        let filtered = inventory
            .entries()
            .iter()
            .filter(|entry| matches_filter(entry, &inventory, "end"))
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(filtered, vec!["launcher", "quit"]);
        assert!(matches_filter(
            &inventory.entries()[0],
            &inventory,
            "unknown"
        ));
        assert!(matches_filter(
            &inventory.entries()[0],
            &inventory,
            "not acknowledged"
        ));
        let mut sorted = inventory.entries().iter().collect::<Vec<_>>();
        sorted.sort_by(|left, right| compare_entries(left, right, DirectorySort::Owner));
        assert_eq!(
            sorted
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["launcher", "quit"]
        );
        let selection = SelectedEntry {
            source: Source::MainSettings,
            id: EntryId::new("quit"),
        };
        let changed = snapshot(2, vec![candidate("launcher", "F3", "Toggle launcher", 2)]);
        assert!(
            !changed
                .entries()
                .iter()
                .any(|entry| { entry.source == selection.source && entry.id == selection.id })
        );
    }

    #[test]
    fn hotkey_directory_ui_resolves_cross_source_conflicts_by_exact_identity() {
        let mut main = candidate("launcher:end", "End", "Open launcher", 3);
        main.state = CandidateState::Configured;
        main.reason = None;
        let paste = Candidate {
            id: EntryId::new("paste:next"),
            owner: Owner::PasteSequences,
            owner_label: "Paste Sequences".into(),
            action_label: "Start next paste sequence".into(),
            chord: Some("Ctrl+End".into()),
            route: RouteKind::SharedHook,
            scope: Scope::ProcessWide,
            source_revision: 7,
            expected_runtime_generation: None,
            state: CandidateState::Configured,
            reason: None,
            edit: Some(EditDestination::PasteSequence {
                sequence_id: None,
                setting: PasteSetting::Next,
            }),
        };
        let inventory = Snapshot::from_sources(
            9,
            vec![
                SourceSnapshot::new(Source::MainSettings, 42, vec![main], Vec::new()),
                SourceSnapshot::new(Source::PasteSequences, 7, vec![paste], Vec::new()),
            ],
        );
        let main_entry = inventory
            .entries()
            .iter()
            .find(|entry| entry.source == Source::MainSettings)
            .unwrap();
        assert_eq!(main_entry.status, Status::Conflict);
        assert!(main_entry.conflicts.iter().any(|conflict| {
            conflict.source == Source::PasteSequences && conflict.id.as_str() == "paste:next"
        }));
        assert_eq!(
            conflict_text(main_entry, &inventory),
            "Start next paste sequence"
        );
    }

    #[test]
    fn hotkey_directory_ui_revalidates_owner_revision_and_current_edit_target() {
        let inventory = Snapshot::from_sources(
            3,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                50,
                vec![candidate("launcher", "End", "Toggle launcher", 3)],
                Vec::new(),
            )],
        );
        let entry = &inventory.entries()[0];
        let request = HotkeyDirectoryEditRequest {
            publication: inventory.publication(),
            source: entry.source,
            entry_id: entry.id.clone(),
            source_publication_revision: inventory.source_revision(entry.source).unwrap(),
            source_revision: entry.source_revision,
            owner: entry.owner,
            destination: entry.edit.clone().unwrap(),
        };
        assert_eq!(
            validate_edit_request(&request, Some(&inventory)),
            Ok(EditDestination::MainSettings(
                crate::hotkey::directory::MainSetting::Launcher
            ))
        );
        assert!(validate_edit_request(&request, None).is_err());

        let changed_owner_row = Snapshot::from_sources(
            4,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                50,
                vec![candidate("launcher", "F3", "Toggle launcher", 4)],
                Vec::new(),
            )],
        );
        assert!(
            validate_edit_request(&request, Some(&changed_owner_row)).is_err(),
            "the row's owner revision rejects a genuinely changed control"
        );
        let changed_source_publication = Snapshot::from_sources(
            4,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                51,
                vec![candidate("launcher", "End", "Toggle launcher", 3)],
                Vec::new(),
            )],
        );
        assert!(
            validate_edit_request(&request, Some(&changed_source_publication)).is_err(),
            "a changed owning-source publication rejects the old positional destination"
        );
        let changed_destination = Candidate {
            edit: Some(EditDestination::MainSettings(MainSetting::Quit)),
            ..candidate("launcher", "End", "Toggle launcher", 3)
        };
        let changed_destination_snapshot = Snapshot::from_sources(
            4,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                50,
                vec![changed_destination],
                Vec::new(),
            )],
        );
        assert!(
            validate_edit_request(&request, Some(&changed_destination_snapshot)).is_err(),
            "reusing an identity with a different exact owner destination is refused"
        );

        let unrelated_republication = Snapshot::from_sources(
            4,
            vec![
                SourceSnapshot::new(
                    Source::MainSettings,
                    50,
                    vec![candidate("launcher", "End", "Toggle launcher", 3)],
                    Vec::new(),
                ),
                SourceSnapshot::new(Source::Radial, 2, Vec::new(), Vec::new()),
            ],
        );
        assert_eq!(
            validate_edit_request(&request, Some(&unrelated_republication)),
            Ok(EditDestination::MainSettings(MainSetting::Launcher)),
            "an unrelated global inventory publication does not stale the owning row"
        );

        let wrong_owner = Candidate {
            owner: Owner::Radial,
            ..candidate("launcher", "End", "Toggle launcher", 3)
        };
        assert!(validate_edit_request(&request, Some(&snapshot(3, vec![wrong_owner]))).is_err());

        let unavailable = Candidate {
            state: CandidateState::Unavailable,
            ..candidate("launcher", "End", "Toggle launcher", 3)
        };
        assert!(validate_edit_request(&request, Some(&snapshot(3, vec![unavailable]))).is_err());

        let fixed = Candidate {
            id: EntryId::new("screen-draw.launch"),
            owner: Owner::ScreenDraw,
            owner_label: "Screen Draw".into(),
            action_label: "Launch Screen Draw".into(),
            chord: Some("Ctrl+F12".into()),
            route: crate::hotkey::directory::RouteKind::Local,
            scope: Scope::LocalSession,
            edit: Some(EditDestination::ScreenDraw(
                crate::hotkey::directory::ScreenDrawSetting::Launch,
            )),
            ..candidate("screen-draw.launch", "Ctrl+F12", "Launch Screen Draw", 3)
        };
        let fixed_inventory = snapshot(3, vec![fixed]);
        assert!(!entry_is_editable(&fixed_inventory.entries()[0]));
        let fixed_row = &fixed_inventory.entries()[0];
        let fixed_request = HotkeyDirectoryEditRequest {
            publication: fixed_inventory.publication(),
            source: fixed_row.source,
            entry_id: fixed_row.id.clone(),
            source_publication_revision: fixed_inventory.source_revision(fixed_row.source).unwrap(),
            source_revision: fixed_row.source_revision,
            owner: fixed_row.owner,
            destination: fixed_row.edit.clone().unwrap(),
        };
        assert!(validate_edit_request(&fixed_request, Some(&fixed_inventory)).is_err());
    }
}
