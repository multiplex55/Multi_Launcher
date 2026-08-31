use super::MkMacroDialog;
use crate::mkmacro::{MkMacro, MkMacroDocument, MkMacroFolder};
use std::collections::HashSet;

struct MacroGroup<'a> {
    /// None identifies the synthetic Unfiled group, never a persisted folder.
    folder: Option<&'a MkMacroFolder>,
    macros: Vec<&'a MkMacro>,
    expanded: bool,
}

/// Preserve document order, including empty folder headings. If malformed data
/// repeats a folder or macro ID, only its first matching group emits that macro.
fn grouped_macros(document: &MkMacroDocument) -> Vec<MacroGroup<'_>> {
    let folder_ids: HashSet<_> = document.folders.iter().map(|folder| folder.id).collect();
    let mut emitted = HashSet::new();
    let mut groups = Vec::with_capacity(document.folders.len() + 1);
    for folder in &document.folders {
        groups.push(MacroGroup {
            folder: Some(folder),
            expanded: true,
            macros: document
                .macros
                .iter()
                .filter(|m| m.folder_id == Some(folder.id) && emitted.insert(m.id))
                .collect(),
        });
    }
    groups.push(MacroGroup {
        folder: None,
        expanded: true,
        macros: document
            .macros
            .iter()
            .filter(|m| {
                !m.folder_id.is_some_and(|id| folder_ids.contains(&id)) && emitted.insert(m.id)
            })
            .collect(),
    });
    groups
}
/// Search only macro names and descriptions, using case-insensitive substrings.
/// Whitespace around the query is ignored; an empty query matches every macro.
fn macro_matches_search(macro_: &MkMacro, search: &str) -> bool {
    let search = search.trim().to_lowercase();
    search.is_empty()
        || macro_.name.to_lowercase().contains(&search)
        || macro_.description.to_lowercase().contains(&search)
}

/// Compute visible rows without changing saved collapse state or document order.
/// Search temporarily expands matching groups and omits all other groups.
fn visible_macro_groups<'a>(
    document: &'a MkMacroDocument,
    search: &str,
    collapsed_folders: &HashSet<u64>,
) -> Vec<MacroGroup<'a>> {
    let searching = !search.trim().is_empty();
    grouped_macros(document)
        .into_iter()
        .filter_map(|mut group| {
            if searching {
                group.macros.retain(|m| macro_matches_search(m, search));
                if group.macros.is_empty() {
                    return None;
                }
            } else {
                group.expanded = !group
                    .folder
                    .is_some_and(|folder| collapsed_folders.contains(&folder.id));
                if !group.expanded {
                    group.macros.clear();
                }
            }
            Some(group)
        })
        .collect()
}

pub const SIDEBAR_WIDTH: f32 = 220.0;
pub(super) fn show_empty(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    let size = ui.available_size();
    ui.allocate_ui_with_layout(
        size,
        eframe::egui::Layout::centered_and_justified(eframe::egui::Direction::TopDown),
        |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Mouse/Keyboard Macros");
                ui.label("Create reusable keyboard, mouse, window, and automation workflows.");
                if ui
                    .add_sized([180.0, 36.0], eframe::egui::Button::new("New Macro"))
                    .clicked()
                {
                    d.create_macro();
                }
                if ui.button("New Folder").clicked() {
                    d.create_folder();
                }
                ui.add_enabled(false, eframe::egui::Button::new("Record New Macro"))
                    .on_disabled_hover_text("Macro recording integration is coming soon.");
            });
        },
    );
}
#[derive(Clone, Copy)]
enum Command {
    NewMacro,
    NewFolder,
    NewMacroHere(u64),
    RenameFolder(u64),
    CommitFolderRename(u64),
    CancelFolderRename,
    DeleteFolder(u64),
    DuplicateMacro(u64),
    MoveMacro(u64, Option<u64>),
    DeleteMacro(u64),
}

fn apply_command(d: &mut MkMacroDialog, command: Command) {
    match command {
        Command::NewMacro => d.create_macro(),
        Command::NewFolder => {
            d.create_folder();
        }
        Command::NewMacroHere(folder_id) => {
            if d.create_macro_in_folder(Some(folder_id)) {
                d.collapsed_folders.remove(&folder_id);
            }
        }
        Command::RenameFolder(id) => d.begin_folder_rename(id),
        Command::CommitFolderRename(id) => {
            let name = d.folder_rename_text.clone();
            if let Err(error) = d.rename_folder(id, &name) {
                d.folder_error = Some(error.to_string());
                d.folder_rename_needs_focus = true;
            }
        }
        Command::CancelFolderRename => d.cancel_folder_rename(),
        Command::DeleteFolder(id) => d.request_delete_folder(id),
        Command::DuplicateMacro(id) | Command::DeleteMacro(id) => {
            d.set_selected_macro(Some(id));
            d.selection.clear();
            if matches!(command, Command::DuplicateMacro(_)) {
                d.duplicate_selected_macro();
            } else {
                d.request_delete_selected_macro();
            }
        }
        Command::MoveMacro(id, folder_id) => {
            d.move_macro_to_folder(id, folder_id);
        }
    }
}

/// Commit explicitly; losing focus leaves the edit and any error visible.
fn show_folder_rename(
    ui: &mut eframe::egui::Ui,
    folder_id: u64,
    text: &mut String,
    error: Option<&str>,
    needs_focus: bool,
) -> Option<Command> {
    use eframe::egui::{Key, Modifiers, TextEdit};
    let id = ui.make_persistent_id(("folder_rename", folder_id));
    if needs_focus {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    // Consume Enter before TextEdit handles it and surrenders focus.
    let mut command = None;
    let has_focus = ui.memory(|memory| memory.has_focus(id));
    if has_focus && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
        command = Some(Command::CommitFolderRename(folder_id));
    }
    let response = ui.add(
        TextEdit::singleline(text)
            .id(id)
            .hint_text("Folder name")
            .desired_width(ui.available_width()),
    );
    // egui clears focus for Escape at frame start, before this widget runs.
    if (has_focus || response.lost_focus())
        && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        command = Some(Command::CancelFolderRename);
    }
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.horizontal(|ui| {
        if ui.button("Rename").clicked() {
            command = Some(Command::CommitFolderRename(folder_id));
        }
        if ui.button("Cancel").clicked() {
            command = Some(Command::CancelFolderRename);
        }
    });
    command
}

pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    let mut command = None;
    ui.heading("Macros");
    ui.horizontal(|ui| {
        if ui.button("New Macro").clicked() {
            command = Some(Command::NewMacro);
        }
        if ui.button("New Folder").clicked() {
            command = Some(Command::NewFolder);
        }
    });
    ui.horizontal(|ui| {
        let selected = d.selected_macro().map(|m| m.id);
        if ui
            .add_enabled(selected.is_some(), eframe::egui::Button::new("Duplicate"))
            .clicked()
        {
            command = selected.map(Command::DuplicateMacro);
        }
        if ui
            .add_enabled(selected.is_some(), eframe::egui::Button::new("Delete"))
            .clicked()
        {
            command = selected.map(Command::DeleteMacro);
        }
    });
    ui.add(
        eframe::egui::TextEdit::singleline(&mut d.search)
            .hint_text("Search names and descriptions"),
    );
    let mut rename_text = d.folder_rename_text.clone();
    let rename_needs_focus = std::mem::take(&mut d.folder_rename_needs_focus);
    let searching = !d.search.trim().is_empty();
    let groups = visible_macro_groups(&d.draft, &d.search, &d.collapsed_folders);
    let mut clicked = None;
    let mut toggled_folders = Vec::new();
    eframe::egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if searching && groups.is_empty() {
                ui.label("No macros match this search.");
            }
            for (index, group) in groups.iter().enumerate() {
                let identity = match group.folder {
                    Some(folder) => eframe::egui::Id::new(("mkmacro_folder", folder.id, index)),
                    None => eframe::egui::Id::new("mkmacro_unfiled"),
                };
                ui.push_id(identity, |ui| {
                    if let Some(folder) = group.folder {
                        if d.pending_folder_rename == Some(folder.id) {
                            if let Some(action) = show_folder_rename(
                                ui,
                                folder.id,
                                &mut rename_text,
                                d.folder_error.as_deref(),
                                rename_needs_focus,
                            ) {
                                command = Some(action);
                            }
                        } else {
                            let arrow = if group.expanded { "▼" } else { "▶" };
                            let response = ui.add(
                                eframe::egui::Button::new(format!("{arrow} {}", folder.name))
                                    .frame(false),
                            );
                            if response.clicked() && !searching {
                                toggled_folders.push(folder.id);
                            }
                            response.context_menu(|ui| {
                                for (label, action) in [
                                    ("New Macro Here", Command::NewMacroHere(folder.id)),
                                    ("Rename Folder", Command::RenameFolder(folder.id)),
                                    ("Delete Folder", Command::DeleteFolder(folder.id)),
                                ] {
                                    if ui.button(label).clicked() {
                                        command = Some(action);
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                    } else {
                        ui.strong("Unfiled");
                    }
                    if group.expanded {
                        ui.indent("members", |ui| {
                            for m in &group.macros {
                                ui.push_id(m.id, |ui| {
                                    let response = ui.selectable_label(
                                        d.selected_macro_id == Some(m.id),
                                        &m.name,
                                    );
                                    if response.clicked() {
                                        clicked = Some(m.id);
                                    }
                                    response.context_menu(|ui| {
                                        if ui.button("Duplicate").clicked() {
                                            command = Some(Command::DuplicateMacro(m.id));
                                            ui.close_menu();
                                        }
                                        ui.menu_button("Move to Folder", |ui| {
                                            let destinations = d
                                                .draft
                                                .folders
                                                .iter()
                                                .map(|folder| {
                                                    (Some(folder.id), folder.name.as_str())
                                                })
                                                .chain(std::iter::once((None, "Unfiled")));
                                            for (folder_id, label) in destinations {
                                                if ui
                                                    .add_enabled(
                                                        m.folder_id != folder_id,
                                                        eframe::egui::Button::new(label),
                                                    )
                                                    .clicked()
                                                {
                                                    command =
                                                        Some(Command::MoveMacro(m.id, folder_id));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        if ui.button("Delete").clicked() {
                                            command = Some(Command::DeleteMacro(m.id));
                                            ui.close_menu();
                                        }
                                        // A submenu command closes the parent context menu too.
                                        if command.is_some() {
                                            ui.close_menu();
                                        }
                                    });
                                });
                            }
                        });
                    }
                });
            }
        });
    d.folder_rename_text = rename_text;
    // Apply commands only after rendering has released its document borrows.
    for id in toggled_folders {
        d.toggle_folder_collapsed(id);
    }
    if let Some(id) = clicked {
        d.set_selected_macro(Some(id));
        d.selection.clear();
    }
    if let Some(command) = command {
        apply_command(d, command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_names_case_insensitively_and_trims_query() {
        let m = macro_entry(1, "Open Browser", None);
        for query in ["open", "BROWSER", "oPeN bRoWsEr", "  browser \t"] {
            assert!(macro_matches_search(&m, query), "{query:?}");
        }
        assert!(!macro_matches_search(&m, "terminal"));
        assert!(macro_matches_search(&m, " \t\n"));
    }

    #[test]
    fn search_matches_descriptions_case_insensitively() {
        let mut m = macro_entry(1, "Daily workflow", None);
        m.description = "Launch the Browser and open mail".into();
        assert!(macro_matches_search(&m, "BROWSER"));
        assert!(macro_matches_search(&m, "  Open Mail  "));
        assert!(!macro_matches_search(&m, "terminal"));
    }

    #[test]
    fn search_expands_collapsed_folder_without_changing_collapsed_bytes() {
        let draft = document();
        let collapsed = HashSet::from([20, 30, 99]);
        let before = serde_json::to_vec(&collapsed).unwrap();
        let capacity = collapsed.capacity();

        let groups = visible_macro_groups(&draft, "  zULu  ", &collapsed);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].folder.unwrap().id, 20);
        assert!(groups[0].expanded);
        assert_eq!(ids(&groups[0]), vec![7]);
        // Compare the same set's serialized bytes, including iteration order.
        assert_eq!(serde_json::to_vec(&collapsed).unwrap(), before);
        assert_eq!(collapsed.capacity(), capacity);
    }

    #[test]
    fn clearing_search_restores_collapsed_rendering_including_whitespace_query() {
        let draft = document();
        let collapsed = HashSet::from([20, 30]);
        let before = serde_json::to_vec(&collapsed).unwrap();
        let matches = visible_macro_groups(&draft, "Zulu", &collapsed);
        assert!(matches[0].expanded);
        assert_eq!(ids(&matches[0]), vec![7]);

        for query in ["", " \t\n"] {
            let groups = visible_macro_groups(&draft, query, &collapsed);
            assert_eq!(groups.len(), 4);
            assert_eq!(groups[0].folder.unwrap().id, 20);
            assert!(!groups[0].expanded);
            assert!(groups[0].macros.is_empty());
            assert!(groups[1].expanded);
            assert_eq!(ids(&groups[1]), vec![6]);
            assert!(!groups[2].expanded);
            assert_eq!(serde_json::to_vec(&collapsed).unwrap(), before);
        }
    }

    #[test]
    fn search_shows_matching_unfiled_and_dangling_macros() {
        let draft = document();
        for (query, expected) in [("unfiled", vec![8, 3]), ("dangling", vec![4])] {
            let groups = visible_macro_groups(&draft, query, &HashSet::from([20, 10, 30]));
            assert_eq!(groups.len(), 1);
            assert!(groups[0].folder.is_none());
            assert!(groups[0].expanded);
            assert_eq!(ids(&groups[0]), expected);
        }
    }

    #[test]
    fn search_omits_unmatched_groups_and_does_not_match_folder_names() {
        let mut draft = document();
        draft.folders[1].name = "Zulu folder".into();
        let groups = visible_macro_groups(&draft, "Zulu", &HashSet::new());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].folder.unwrap().id, 20);
        assert_eq!(ids(&groups[0]), vec![7]);

        assert!(visible_macro_groups(&draft, "Zulu folder", &HashSet::new()).is_empty());
        assert!(visible_macro_groups(&draft, "no match", &HashSet::new()).is_empty());
    }

    #[test]
    fn search_preserves_group_and_member_order() {
        let mut draft = document();
        for m in &mut draft.macros {
            if m.id != 4 {
                m.description = "Match this description".into();
            }
        }
        let groups = visible_macro_groups(&draft, "MATCH", &HashSet::from([20, 10]));
        assert_eq!(
            groups
                .iter()
                .map(|g| g.folder.map(|f| f.id))
                .collect::<Vec<_>>(),
            vec![Some(20), Some(10), None]
        );
        assert_eq!(
            groups.iter().flat_map(ids).collect::<Vec<_>>(),
            vec![7, 5, 6, 8, 3]
        );
        assert!(groups.iter().all(|g| g.expanded));
    }

    #[test]
    fn rendering_search_preserves_filtered_selection_and_collapsed_state() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(dir.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.draft = document();
        d.selected_macro_id = Some(6);
        d.collapsed_folders = HashSet::from([20, 30]);
        let before = serde_json::to_vec(&d.collapsed_folders).unwrap();
        let ctx = eframe::egui::Context::default();

        for query in ["Zulu", "no match", "", "  "] {
            d.search = query.into();
            rename_frame(&ctx, &mut d, None);
            assert_eq!(d.selected_macro_id, Some(6));
            assert_eq!(serde_json::to_vec(&d.collapsed_folders).unwrap(), before);
        }
    }

    fn rename_frame(
        ctx: &eframe::egui::Context,
        d: &mut MkMacroDialog,
        key: Option<eframe::egui::Key>,
    ) {
        use eframe::egui::{CentralPanel, Event, Modifiers, RawInput};
        let events = key
            .map(|key| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .into_iter()
            .collect();
        let _ = ctx.run(
            RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| show(ui, d));
            },
        );
    }

    #[test]
    fn inline_rename_enter_validates_retains_errors_on_blur_and_commits_correction() {
        use eframe::egui::{Context, Key};
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(dir.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.draft = document();
        d.selected_macro_id = Some(7);
        d.collapsed_folders.insert(20);
        let before = d.draft.clone();
        let ctx = Context::default();
        d.begin_folder_rename(20);
        rename_frame(&ctx, &mut d, None);
        let focus = ctx
            .memory(|memory| memory.focused())
            .expect("rename should gain focus");
        for (text, error) in [
            ("  ", "A macro folder name cannot be empty."),
            ("alpha", "A macro folder named \"Alpha\" already exists."),
        ] {
            d.folder_rename_text = text.into();
            rename_frame(&ctx, &mut d, Some(Key::Enter));
            assert_eq!(d.pending_folder_rename, Some(20));
            assert_eq!(d.folder_error.as_deref(), Some(error));
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
            rename_frame(&ctx, &mut d, None);
            ctx.memory_mut(|memory| memory.surrender_focus(focus));
            rename_frame(&ctx, &mut d, None);
            assert_eq!(d.pending_folder_rename, Some(20));
            assert_eq!(d.folder_rename_text, text);
            assert_eq!(d.folder_error.as_deref(), Some(error));
            assert_eq!(d.draft, before);
            ctx.memory_mut(|memory| memory.request_focus(focus));
        }
        d.folder_rename_text = "  Work  ".into();
        rename_frame(&ctx, &mut d, Some(Key::Enter));
        assert_eq!(d.draft.folders[0].name, "Work");
        assert_eq!(d.draft.macros, before.macros);
        assert_eq!(d.selected_macro_id, Some(7));
        assert!(d.collapsed_folders.contains(&20));
        assert!(d.dirty);
        assert_eq!(d.pending_folder_rename, None);
        assert_eq!(d.folder_error, None);
        assert!(d.folder_rename_text.is_empty());
    }

    #[test]
    fn inline_rename_escape_cancels_without_mutating_draft() {
        use eframe::egui::{Context, Key};
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(dir.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.draft = document();
        let before = d.draft.clone();
        let ctx = Context::default();
        d.begin_folder_rename(20);
        rename_frame(&ctx, &mut d, None);
        d.folder_rename_text = "Uncommitted".into();
        d.folder_error = Some("Previous error".into());
        rename_frame(&ctx, &mut d, Some(Key::Escape));
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        assert_eq!(d.pending_folder_rename, None);
        assert_eq!(d.folder_error, None);
        assert!(d.folder_rename_text.is_empty());
    }

    fn document() -> MkMacroDocument {
        MkMacroDocument {
            folders: vec![
                MkMacroFolder {
                    id: 20,
                    name: "Zulu".into(),
                },
                MkMacroFolder {
                    id: 10,
                    name: "Alpha".into(),
                },
                MkMacroFolder {
                    id: 30,
                    name: "Empty".into(),
                },
            ],
            macros: vec![
                macro_entry(8, "Unfiled Z", None),
                macro_entry(7, "Zulu", Some(20)),
                macro_entry(6, "Other folder", Some(10)),
                macro_entry(5, "Alpha", Some(20)),
                macro_entry(4, "Dangling", Some(99)),
                macro_entry(3, "Unfiled A", None),
            ],
            ..Default::default()
        }
    }

    fn macro_entry(id: u64, name: &str, folder_id: Option<u64>) -> MkMacro {
        MkMacro {
            id,
            name: name.into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id,
            playback: Default::default(),
            steps: vec![],
            image_assets: vec![],
        }
    }

    fn ids(group: &MacroGroup<'_>) -> Vec<u64> {
        group.macros.iter().map(|m| m.id).collect()
    }

    #[test]
    fn new_macro_here_creates_in_folder_and_rejects_invalid_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(dir.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.draft.folders = document().folders;
        d.collapsed_folders.insert(20);

        apply_command(&mut d, Command::NewMacroHere(20));

        let created = d.selected_macro().unwrap();
        assert_eq!(created.folder_id, Some(20));
        assert_ne!(created.id, 0);
        assert!(!d.collapsed_folders.contains(&20));
        let before = d.draft.clone();
        d.dirty = false;
        for id in [0, 99] {
            apply_command(&mut d, Command::NewMacroHere(id));
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
        }
    }

    #[test]
    fn canonical_folder_order_follows_document() {
        let draft = document();
        let groups = grouped_macros(&draft);
        assert_eq!(
            groups
                .iter()
                .filter_map(|g| g.folder.map(|f| f.id))
                .collect::<Vec<_>>(),
            vec![20, 10, 30]
        );
    }

    #[test]
    fn macro_order_follows_document_not_names_or_ids() {
        let draft = document();
        let groups = grouped_macros(&draft);
        assert_eq!(ids(&groups[0]), vec![7, 5]);
        assert_eq!(ids(&groups[1]), vec![6]);
        assert_eq!(ids(&groups[3]), vec![8, 4, 3]);
    }

    #[test]
    fn unfiled_is_always_last_even_when_empty() {
        let mut draft = document();
        for folder_id in [None, Some(20)] {
            for m in &mut draft.macros {
                m.folder_id = folder_id;
            }
            let groups = grouped_macros(&draft);
            assert_eq!(groups.len(), draft.folders.len() + 1);
            assert!(groups.last().unwrap().folder.is_none());
            assert!(
                groups[..groups.len() - 1]
                    .iter()
                    .all(|g| g.folder.is_some())
            );
            assert_eq!(
                groups.last().unwrap().macros.is_empty(),
                folder_id.is_some()
            );
        }
    }

    #[test]
    fn dangling_references_are_unfiled() {
        let mut draft = document();
        assert_eq!(ids(grouped_macros(&draft).last().unwrap()), vec![8, 4, 3]);
        draft.folders.clear();
        let groups = grouped_macros(&draft);
        assert_eq!(groups.len(), 1);
        assert_eq!(ids(&groups[0]), vec![8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn empty_folders_remain_represented() {
        let mut draft = document();
        assert!(grouped_macros(&draft)[2].macros.is_empty());
        draft.macros.clear();
        let groups = grouped_macros(&draft);
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|g| g.macros.is_empty()));
        assert_eq!(groups[2].folder.unwrap().name, "Empty");
    }

    #[test]
    fn every_macro_id_appears_once_despite_duplicate_folders_and_macro_ids() {
        let mut draft = document();
        draft.folders.insert(
            1,
            MkMacroFolder {
                id: 20,
                name: "Duplicate folder".into(),
            },
        );
        draft
            .macros
            .push(macro_entry(7, "Duplicate macro", Some(10)));
        draft
            .macros
            .push(macro_entry(8, "Duplicate unfiled", Some(99)));
        let groups = grouped_macros(&draft);
        assert_eq!(groups.len(), 5);
        assert!(groups[1].macros.is_empty());
        let rendered: Vec<_> = groups.iter().flat_map(ids).collect();
        let unique: HashSet<_> = rendered.iter().copied().collect();
        let expected: HashSet<_> = draft.macros.iter().map(|m| m.id).collect();
        assert_eq!(unique, expected);
        assert_eq!(rendered.len(), unique.len());
    }
}
