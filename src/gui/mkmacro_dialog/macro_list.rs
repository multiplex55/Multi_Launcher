use super::MkMacroDialog;
use crate::mkmacro::{MkMacro, MkMacroDocument, MkMacroFolder};
use std::collections::HashSet;

struct MacroGroup<'a> {
    /// None identifies the synthetic Unfiled group, never a persisted folder.
    folder: Option<&'a MkMacroFolder>,
    macros: Vec<&'a MkMacro>,
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
            macros: document
                .macros
                .iter()
                .filter(|m| m.folder_id == Some(folder.id) && emitted.insert(m.id))
                .collect(),
        });
    }
    groups.push(MacroGroup {
        folder: None,
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
    ui.add(eframe::egui::TextEdit::singleline(&mut d.search).hint_text("Search"));
    let groups = grouped_macros(&d.draft);
    let search = d.search.to_lowercase();
    let mut clicked = None;
    let mut toggled_folders = Vec::new();
    eframe::egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, group) in groups.iter().enumerate() {
                let identity = match group.folder {
                    Some(folder) => eframe::egui::Id::new(("mkmacro_folder", folder.id, index)),
                    None => eframe::egui::Id::new("mkmacro_unfiled"),
                };
                ui.push_id(identity, |ui| {
                    let expanded = if let Some(folder) = group.folder {
                        let collapsed = d.is_folder_collapsed(folder.id);
                        let arrow = if collapsed { "▶" } else { "▼" };
                        let response = ui.add(
                            eframe::egui::Button::new(format!("{arrow} {}", folder.name))
                                .frame(false),
                        );
                        if response.clicked() {
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
                        !collapsed
                    } else {
                        ui.strong("Unfiled");
                        true
                    };
                    if expanded {
                        ui.indent("members", |ui| {
                            for m in &group.macros {
                                if !search.is_empty() && !m.name.to_lowercase().contains(&search) {
                                    continue;
                                }
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
