use super::{open_link, LauncherApp};
use crate::actions::rss as rss_actions;
use crate::plugins::rss::storage;
use eframe::egui;

const OPEN_BATCH: usize = 5;

#[derive(Default)]
pub struct RssDialog {
    pub open: bool,
    selected_group: Option<String>,
    selected_feed: Option<String>,
}

impl RssDialog {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("RSS")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                let feeds_file = storage::FeedsFile::load();
                let mut state = storage::StateFile::load();
                ui.columns(3, |cols| {
                    // Column 0: groups
                    for g in &feeds_file.groups {
                        let unread: u32 = feeds_file
                            .feeds
                            .iter()
                            .filter(|f| f.group.as_deref() == Some(g))
                            .map(|f| state.feeds.get(&f.id).map(|s| s.unread).unwrap_or(0))
                            .sum();
                        if cols[0]
                            .selectable_label(self.selected_group.as_deref() == Some(g), format!("{g} ({unread})"))
                            .clicked()
                        {
                            self.selected_group = Some(g.clone());
                            self.selected_feed = None;
                        }
                    }

                    // Column 1: feeds in selected group
                    let feed_iter = feeds_file.feeds.iter().filter(|f| {
                        if let Some(g) = &self.selected_group {
                            f.group.as_deref() == Some(g)
                        } else {
                            true
                        }
                    });
                    for f in feed_iter {
                        let unread = state.feeds.get(&f.id).map(|s| s.unread).unwrap_or(0);
                        let title = f.title.clone().unwrap_or_else(|| f.id.clone());
                        if cols[1]
                            .selectable_label(self.selected_feed.as_deref() == Some(&f.id), format!("{title} ({unread})"))
                            .clicked()
                        {
                            self.selected_feed = Some(f.id.clone());
                        }
                    }

                    // Column 2: items for selected feed
                    if let Some(fid) = &self.selected_feed {
                        let cache = storage::FeedCache::load(fid);
                        let entry = state.feeds.entry(fid.clone()).or_default();
                        let cursor = entry.last_read_published.unwrap_or(0);
                        let mut changed = false;
                        for item in cache.items.iter().rev() {
                            let ts = item.timestamp.unwrap_or(0);
                            let unread = ts > cursor && !entry.read.contains(&item.guid);
                            let label = if unread {
                                format!("* {}", item.title)
                            } else {
                                item.title.clone()
                            };
                            if cols[2].button(label).clicked() {
                                if let Some(link) = &item.link {
                                    let _ = open_link(link);
                                }
                                entry.read.insert(item.guid.clone());
                                let count = cache
                                    .items
                                    .iter()
                                    .filter(|i| {
                                        let ts = i.timestamp.unwrap_or(0);
                                        ts > cursor && !entry.read.contains(&i.guid)
                                    })
                                    .count();
                                entry.unread = count as u32;
                                changed = true;
                            }
                        }
                        if changed {
                            let _ = state.save();
                        }
                    }
                });

                // Keyboard shortcuts operating on the selected feed.
                if let Some(fid) = &self.selected_feed {
                    let modifiers = ctx.input(|i| i.modifiers);
                    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let n = if modifiers.shift { OPEN_BATCH } else { 1 };
                        let cmd = format!("open {fid} --unread --n {n}");
                        let _ = rss_actions::run(&cmd);
                        ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                    }
                    if modifiers.alt && ctx.input(|i| i.key_pressed(egui::Key::R)) {
                        let cmd = format!("mark read {fid} --through newest");
                        let _ = rss_actions::run(&cmd);
                        ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::R));
                    }
                }

                ui.separator();
                ui.label(format!(
                    "Enter: open newest unread\nShift+Enter: open next {OPEN_BATCH}\nAlt+R: mark visible items read"
                ));
            });
        self.open = open;
    }
}
