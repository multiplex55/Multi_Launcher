use crate::dashboard::dashboard::DashboardContext;
use crate::plugins::note::Note;
use eframe::egui;
use std::time::SystemTime;

#[derive(Clone)]
pub struct CachedRecentNote {
    pub title: String,
    pub slug: String,
    pub tags: Vec<String>,
    pub snippet: String,
}

pub fn note_snippet(note: &Note) -> String {
    let first_line = note
        .content
        .lines()
        .skip_while(|l| l.starts_with("# ") || l.starts_with("Alias:"))
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    let clean = first_line.trim();
    if clean.len() > 120 {
        format!("{}…", &clean[..120])
    } else {
        clean.to_string()
    }
}

fn modified_ts(note: &Note) -> u64 {
    note.path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn refresh_cached_notes(
    cache: &mut Vec<CachedRecentNote>,
    last_notes_version: &mut u64,
    ctx: &DashboardContext<'_>,
    count: usize,
    filter_tag: Option<&str>,
) {
    if *last_notes_version == ctx.notes_version {
        return;
    }

    let mut notes_with_ts: Vec<(u64, Note)> = ctx
        .data_cache
        .snapshot()
        .notes
        .as_ref()
        .iter()
        .filter(|note| {
            filter_tag.is_none_or(|tag| note.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
        })
        .map(|note| (modified_ts(note), note.clone()))
        .collect();

    notes_with_ts.sort_by(|a, b| b.0.cmp(&a.0));
    notes_with_ts.truncate(count);

    *cache = notes_with_ts
        .into_iter()
        .map(|(_, note)| CachedRecentNote {
            title: note.alias.as_ref().unwrap_or(&note.title).clone(),
            slug: note.slug,
            tags: note.tags,
            snippet: note_snippet(&note),
        })
        .collect();

    *last_notes_version = ctx.notes_version;
}

pub fn render_note_rows(
    ui: &mut egui::Ui,
    scroll_id: &str,
    notes: &[CachedRecentNote],
    show_snippet: bool,
    show_tags: bool,
    mut on_click: impl FnMut(&CachedRecentNote),
) {
    let body_height = ui.text_style_height(&egui::TextStyle::Body);
    let small_height = ui.text_style_height(&egui::TextStyle::Small);
    let mut row_height = body_height + ui.spacing().item_spacing.y + 8.0;
    if show_snippet {
        row_height += small_height + 2.0;
    }
    if show_tags {
        row_height += small_height + 2.0;
    }

    let scroll_id = ui.id().with(scroll_id);
    egui::ScrollArea::both()
        .id_source(scroll_id)
        .auto_shrink([false; 2])
        .show_rows(ui, row_height, notes.len(), |ui, range| {
            for note in &notes[range] {
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
                    on_click(note);
                }
            }
        });
}
