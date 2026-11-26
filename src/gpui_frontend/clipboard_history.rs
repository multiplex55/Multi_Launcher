//! Retained read-only Clipboard History browser and cancellable edit draft.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::common::persistence::{LoadState, PersistenceError};
use multi_launcher::plugins::clipboard::{clipboard_version, load_history_typed};
use std::path::{Path, PathBuf};

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|enabled| enabled.contains("clipboard"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "clipboard")
        && multi_launcher::plugins::clipboard::bare_history_opener(query).as_ref() == Some(action)
}
fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let mut result: String = chars
        .by_ref()
        .take(160)
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}
fn load(path: &Path) -> Result<Vec<String>, String> {
    match load_history_typed(path).map_err(|error| match error {
        PersistenceError::MalformedJson { source, .. } => format!(
            "Invalid history JSON at line {}, column {}",
            source.line(),
            source.column()
        ),
        PersistenceError::Read { source, .. } => {
            format!("Could not read history ({:?})", source.kind())
        }
        _ => "Could not load history".to_owned(),
    })? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(rows) => Ok(rows),
    }
}
#[derive(Clone)]
struct EditReceipt {
    version: u64,
    generation: u64,
    index: usize,
    rows: Arc<Vec<String>>,
}
impl EditReceipt {
    fn current(
        &self,
        version: Option<u64>,
        live_version: u64,
        generation: u64,
        rows: &Arc<Vec<String>>,
        loading: bool,
        failed: bool,
    ) -> bool {
        !loading
            && !failed
            && version == Some(self.version)
            && live_version == self.version
            && generation == self.generation
            && Arc::ptr_eq(&self.rows, rows)
            && self.rows.get(self.index).is_some()
    }
}
pub(super) struct ClipboardHistory {
    pub open: bool,
    owner: utility_mount::MountOwner,
    receipt: utility_mount::MountReceipt,
    root: WeakEntity<RootWindow>,
    path: PathBuf,
    focus: FocusHandle,
    editor: Option<Entity<TextareaState>>,
    edit: Option<EditReceipt>,
    selected: Option<(u64, usize)>,
    rows: Arc<Vec<String>>,
    generation: u64,
    version: Option<u64>,
    loading: bool,
    diagnostic: Option<String>,
}
impl ClipboardHistory {
    fn new(
        _window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        path: PathBuf,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let receipt = owner.attach();
        Self {
            open: false,
            owner,
            receipt,
            root,
            path,
            focus: cx.focus_handle(),
            editor: None,
            edit: None,
            selected: None,
            rows: Arc::new(Vec::new()),
            generation: 0,
            version: None,
            loading: false,
            diagnostic: None,
        }
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            if self.edit.is_some() {
                if let Some(editor) = &self.editor {
                    editor.focus_handle(cx).focus(window, cx);
                }
            } else {
                self.focus.focus(window, cx);
            }
            return;
        }
        self.receipt = self.owner.attach();
        self.editor = Some(cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 14)
                .placeholder("Clipboard entry")
        }));
        self.open = true;
        self.edit = None;
        self.selected = None;
        self.loading = false;
        self.version = None;
        self.focus.focus(window, cx);
        self.refresh(cx);
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.loading {
            return;
        }
        self.loading = true;
        let version = clipboard_version();
        let path = self.path.clone();
        let receipt = self.receipt.clone();
        let task = cx.background_executor().spawn(async move { load(&path) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                if !view.open || !receipt.is_current() || !receipt.matches(&view.receipt) {
                    return;
                }
                view.loading = false;
                if version != clipboard_version() {
                    view.refresh(cx);
                    return;
                }
                view.version = Some(version);
                match result {
                    Ok(rows) => {
                        view.rows = Arc::new(rows);
                        view.generation = view.generation.wrapping_add(1);
                        view.selected = None;
                        view.diagnostic = None;
                    }
                    Err(error) => view.diagnostic = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.owner.retire();
        self.edit = None;
        self.editor = None;
        self.loading = false;
        let _ = self.root.update(cx, |root, cx| {
            root.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        cx.notify();
    }
}
impl Render for ClipboardHistory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.open && !self.loading && self.version != Some(clipboard_version()) {
            self.refresh(cx);
        }
        let receipt = self.receipt.clone();
        let close = receipt.clone();
        let cancel = receipt.clone();
        let mut panel = div()
            .id("clipboard-history")
            .track_focus(&self.focus)
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(Label::new("Clipboard History"));
        if let Some(error) = &self.diagnostic {
            panel = panel.child(Label::new(format!(
                "History could not be loaded; showing last available entries: {error}"
            )));
        }
        if self.loading {
            panel = panel.child(Label::new("Loading clipboard history…"));
        }
        if let Some(edit) = &self.edit {
            if !edit.current(
                self.version,
                clipboard_version(),
                self.generation,
                &self.rows,
                self.loading,
                self.diagnostic.is_some(),
            ) {
                panel = panel.child(Label::new(
                    "History changed or could not be loaded; this draft is stale.",
                ));
            }
            let Some(editor) = &self.editor else {
                return panel;
            };
            panel = panel
                .child(Textarea::new(editor).aria_label("Edit clipboard entry"))
                .child(
                    Button::new("clipboard-edit-cancel")
                        .label("Cancel")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if cancel.is_current()
                                && cancel.matches(&view.receipt)
                                && view.open
                                && view.edit.is_some()
                            {
                                view.edit = None;
                                view.focus.focus(window, cx);
                                cx.notify();
                            }
                        })),
                );
        } else {
            let rows = self.rows.clone();
            let generation = self.generation;
            let version = self.version;
            let ready =
                !self.loading && self.diagnostic.is_none() && version == Some(clipboard_version());
            let selected = self.selected;
            let owner = cx.entity().downgrade();
            let row_receipt = receipt.clone();
            let list =
                gpui_kit::uniform_list("clipboard-history-rows", rows.len(), move |range, _, _| {
                    range
                        .filter_map(|index| {
                            let entry = rows.get(index)?;
                            let label = preview(entry);
                            let owner_select = owner.clone();
                            let owner_edit = owner.clone();
                            let select_receipt = row_receipt.clone();
                            let edit_receipt = row_receipt.clone();
                            let selected_rows = rows.clone();
                            let edit_rows = rows.clone();
                            let row_live =
                                move |view: &ClipboardHistory,
                                      receipt: &utility_mount::MountReceipt,
                                      expected: &Arc<Vec<String>>| {
                                    view.open
                                        && receipt.is_current()
                                        && receipt.matches(&view.receipt)
                                        && view.edit.is_none()
                                        && !view.loading
                                        && view.diagnostic.is_none()
                                        && view.generation == generation
                                        && view.version == version
                                        && version == Some(clipboard_version())
                                        && Arc::ptr_eq(&view.rows, expected)
                                };
                            Some(
                                div()
                                    .id(SharedString::from(format!(
                                        "clipboard-row-{generation}-{index}"
                                    )))
                                    .h(px(36.))
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "clipboard-select-{generation}-{index}"
                                        )))
                                        .label(label)
                                        .selected(selected == Some((generation, index)))
                                        .disabled(!ready)
                                        .on_click(
                                            move |_, _, app| {
                                                let _ = owner_select.update(app, |view, cx| {
                                                    if row_live(
                                                        view,
                                                        &select_receipt,
                                                        &selected_rows,
                                                    ) && view.rows.get(index).is_some()
                                                    {
                                                        view.selected = Some((generation, index));
                                                        cx.notify();
                                                    }
                                                });
                                            },
                                        ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "clipboard-edit-{generation}-{index}"
                                        )))
                                        .label("Edit")
                                        .disabled(!ready)
                                        .on_click(
                                            move |_, window, app| {
                                                let _ =
                                                    owner_edit.update(app, |view, cx| {
                                                        if !row_live(
                                                            view,
                                                            &edit_receipt,
                                                            &edit_rows,
                                                        ) {
                                                            return;
                                                        }
                                                        let (
                                                            Some(entry),
                                                            Some(version),
                                                            Some(editor),
                                                        ) = (
                                                            view.rows.get(index).cloned(),
                                                            version,
                                                            view.editor.clone(),
                                                        )
                                                        else {
                                                            return;
                                                        };
                                                        view.selected = Some((generation, index));
                                                        view.edit = Some(EditReceipt {
                                                            version,
                                                            generation,
                                                            index,
                                                            rows: edit_rows.clone(),
                                                        });
                                                        editor.update(cx, |input, cx| {
                                                            input.set_value(entry, window, cx)
                                                        });
                                                        editor.focus_handle(cx).focus(window, cx);
                                                        cx.notify();
                                                    });
                                            },
                                        ),
                                    )
                                    .into_any_element(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .h(px(240.));
            panel = panel.child(list).child(
                Button::new("clipboard-history-close")
                    .label("Close")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if close.is_current() && close.matches(&view.receipt) {
                            view.close(window, cx);
                        }
                    })),
            );
        }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_clipboard_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = {
            #[cfg(windows)]
            {
                self.app_data_root
                    .path()
                    .join(multi_launcher::plugins::clipboard::CLIPBOARD_FILE)
            }
            #[cfg(not(windows))]
            {
                PathBuf::from(multi_launcher::plugins::clipboard::CLIPBOARD_FILE)
            }
        };
        let root = cx.entity().downgrade();
        let view = self
            .clipboard_history
            .clone()
            .unwrap_or_else(|| cx.new(|cx| ClipboardHistory::new(window, cx, root, path)));
        view.update(cx, |view, cx| view.open(window, cx));
        self.clipboard_history = Some(view);
        let host = window.window_handle().window_id();
        let mut index = self.file_search_scroll_item();
        index += usize::from(
            self.file_search
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        #[cfg(windows)]
        {
            index += usize::from(
                self.downloads_triage
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            );
        }
        index += usize::from(
            self.plugin_editor
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        index += usize::from(
            self.activity_view
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        index += usize::from(
            self.json_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        index += usize::from(
            self.xml_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        index += usize::from(
            self.regex_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::{EditReceipt, current_provider, load, preview};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    #[test]
    fn exact_edit_receipt_rejects_stale_version_and_replaced_occurrence() {
        let rows = Arc::new(vec!["same".to_owned(), "same".to_owned()]);
        let edit = EditReceipt {
            version: 4,
            generation: 7,
            index: 1,
            rows: rows.clone(),
        };
        assert!(edit.current(Some(4), 4, 7, &rows, false, false));
        assert!(!edit.current(Some(4), 5, 7, &rows, false, false));
        assert!(!edit.current(Some(4), 4, 7, &rows, true, false));
        assert!(!edit.current(Some(4), 4, 7, &rows, false, true));
        assert!(!edit.current(Some(4), 4, 8, &Arc::new((*rows).clone()), false, false));
    }
    #[test]
    fn opener_requires_exact_action_query_and_live_capability() {
        let action = multi_launcher::plugins::clipboard::bare_history_opener("Cb").unwrap();
        assert!(current_provider("Cb", &action, None, None));
        assert!(!current_provider("cb list", &action, None, None));
        let mut changed = action.clone();
        changed.label.push('!');
        assert!(!current_provider("cb", &changed, None, None));
        assert!(!current_provider(
            "cb",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let caps = HashMap::from([("clipboard".to_owned(), Vec::new())]);
        assert!(!current_provider("cb", &action, None, Some(&caps)));
    }
    #[test]
    fn previews_are_stock_unicode_and_full_data_is_not_changed() {
        let value = format!("{}\nend", "é".repeat(160));
        assert_eq!(preview(&value), format!("{}…", "é".repeat(160)));
        assert!(value.ends_with("\nend"));
    }
    #[test]
    fn typed_load_preserves_duplicates_and_reports_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        assert!(load(&path).unwrap().is_empty());
        std::fs::write(&path, "[\"a\",\"a\",\"猫\"]").unwrap();
        assert_eq!(load(&path).unwrap(), ["a", "a", "猫"]);
        std::fs::write(&path, "{bad").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{bad");
        let secret = "SYNTHETIC_SECRET_4291";
        let bytes = format!("\"{secret}\"");
        std::fs::write(&path, &bytes).unwrap();
        assert!(!load(&path).unwrap_err().contains(secret));
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
    }
}
