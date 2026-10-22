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

#[derive(Default)]
pub(crate) struct HotkeyDirectoryDialog {
    pub(crate) open: bool,
    pending_open: bool,
    filter: String,
    sort: DirectorySort,
    selected: Option<SelectedEntry>,
    message: Option<String>,
    pending_edit: Option<HotkeyDirectoryEditRequest>,
    #[cfg(test)]
    last_edit_response: Option<(Source, EntryId, egui::Id, egui::Rect)>,
}

impl HotkeyDirectoryDialog {
    pub(crate) fn request_open(&mut self) {
        self.pending_open = true;
    }

    pub(crate) fn take_open_request(&mut self) -> bool {
        std::mem::take(&mut self.pending_open)
    }

    pub(crate) fn take_edit_request(&mut self) -> Option<HotkeyDirectoryEditRequest> {
        self.pending_edit.take()
    }

    #[cfg(test)]
    pub(crate) fn test_last_edit_response(
        &self,
    ) -> Option<(Source, EntryId, egui::Id, egui::Rect)> {
        self.last_edit_response.clone()
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        snapshot: Option<&Snapshot>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        interaction_allowed: bool,
    ) {
        if !self.open {
            return;
        }

        #[cfg(test)]
        {
            self.last_edit_response = None;
        }

        let mut open = self.open;
        let mut visual_order = 0;
        egui::Window::new("Hotkey Directory")
            .id(egui::Id::new(Some("Hotkey Directory")))
            .open(&mut open)
            .default_size((1060.0, 640.0))
            .min_size((700.0, 380.0))
            .resizable(true)
            .show(ctx, |ui| {
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
                        egui::ComboBox::from_id_salt("hotkey-directory/sort")
                            .selected_text(match sort {
                                DirectorySort::Owner => "Owner",
                                DirectorySort::Status => "Status",
                                DirectorySort::Conflicts => "Conflicts first",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut sort, DirectorySort::Owner, "Owner");
                                ui.selectable_value(&mut sort, DirectorySort::Status, "Status");
                                ui.selectable_value(
                                    &mut sort,
                                    DirectorySort::Conflicts,
                                    "Conflicts first",
                                );
                            });
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
                egui::ScrollArea::both()
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
                                    if edit.clicked() && interaction_allowed {
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
            });
        self.open = open;
    }

    pub(crate) fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub(crate) fn clear_message(&mut self) {
        self.message = None;
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
