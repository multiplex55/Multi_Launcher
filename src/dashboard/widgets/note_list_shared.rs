use crate::dashboard::dashboard::DashboardContext;
use crate::plugins::note::Note;
use eframe::egui;
use std::time::SystemTime;

#[derive(Clone)]
pub struct CachedNoteEntry {
    pub title: String,
    pub slug: String,
    pub tags: Vec<String>,
    pub snippet: String,
}

#[derive(Default)]
pub struct CachedRecentNotes {
    pub entries: Vec<CachedNoteEntry>,
    pub last_notes_version: u64,
}

impl CachedRecentNotes {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            last_notes_version: u64::MAX,
        }
    }

    pub fn refresh(&mut self, ctx: &DashboardContext<'_>, count: usize, filter_tag: Option<&str>) {
        if self.last_notes_version == ctx.notes_version {
            return;
        }

        let snapshot = ctx.data_cache.snapshot();
        let mut notes: Vec<Note> = snapshot.notes.as_ref().clone();
        if let Some(tag) = filter_tag.filter(|tag| !tag.trim().is_empty()) {
            notes.retain(|note| note.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)));
        }

        notes.sort_by(|a, b| modified_ts(b).cmp(&modified_ts(a)));
        notes.truncate(count);

        self.entries = notes
            .iter()
            .map(|note| CachedNoteEntry {
                title: note.alias.as_ref().unwrap_or(&note.title).clone(),
                slug: note.slug.clone(),
                tags: note.tags.clone(),
                snippet: note_snippet(note),
            })
            .collect();
        self.last_notes_version = ctx.notes_version;
    }
}

pub fn note_snippet(note: &Note) -> String {
    let first_line = note
        .content
        .lines()
        .skip_while(|line| line.starts_with("# ") || line.starts_with("Alias:"))
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let clean = first_line.trim();
    if clean.len() > 120 {
        format!("{}…", &clean[..120])
    } else {
        clean.to_string()
    }
}

pub fn render_note_rows(
    ui: &mut egui::Ui,
    scroll_id: impl std::hash::Hash,
    entries: &[CachedNoteEntry],
    show_snippet: bool,
    show_tags: bool,
    no_notes_message: &str,
    mut build_action: impl FnMut(&CachedNoteEntry) -> super::WidgetAction,
) -> Option<super::WidgetAction> {
    if entries.is_empty() {
        ui.label(no_notes_message);
        return None;
    }

    let body_height = ui.text_style_height(&egui::TextStyle::Body);
    let small_height = ui.text_style_height(&egui::TextStyle::Small);
    let mut row_height = body_height + ui.spacing().item_spacing.y + 8.0;
    if show_snippet {
        row_height += small_height + 2.0;
    }
    if show_tags {
        row_height += small_height + 2.0;
    }

    let mut clicked = None;
    egui::ScrollArea::both()
        .id_source(ui.id().with(scroll_id))
        .auto_shrink([false; 2])
        .show_rows(ui, row_height, entries.len(), |ui, range| {
            for note in &entries[range] {
                let mut clicked_row = false;
                ui.vertical(|ui| {
                    clicked_row |= ui.add(egui::Button::new(&note.title).wrap(false)).clicked();
                    if show_snippet {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&note.snippet).small())
                                .wrap(false),
                        );
                    }
                    if show_tags && !note.tags.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("#{}", note.tags.join(" #"))).small(),
                            )
                            .wrap(false),
                        );
                    }
                    ui.add_space(4.0);
                });
                if clicked_row {
                    clicked = Some(build_action(note));
                }
            }
        });

    clicked
}

fn modified_ts(note: &Note) -> u64 {
    note.path
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
