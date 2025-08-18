use eframe::egui;
use crate::plugins::rss::storage;
use super::{LauncherApp, open_link};

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
            });
        self.open = open;
    }
}
