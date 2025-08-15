use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::bookmarks::{load_bookmarks, BOOKMARKS_FILE};
use crate::plugins::fav::{load_favs, FAV_FILE};
use crate::plugins::folders::{load_folders, FOLDERS_FILE};
use url::Url;

pub struct MissingPlugin;

impl Plugin for MissingPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let prefix = "check missing";
        if crate::common::strip_prefix_ci(trimmed, prefix).is_none()
            && !prefix.starts_with(&trimmed.to_ascii_lowercase())
        {
            return Vec::new();
        }
        let mut out = Vec::new();

        if let Ok(folders) = load_folders(FOLDERS_FILE) {
            for f in folders {
                if !std::path::Path::new(&f.path).exists() {
                    out.push(Action {
                        label: format!("Remove missing folder {}", f.path),
                        desc: "Maintenance".into(),
                        action: format!("folder:remove:{}", f.path),
                        args: None,
                    });
                }
            }
        }

        if let Ok(bookmarks) = load_bookmarks(BOOKMARKS_FILE) {
            for b in bookmarks {
                if Url::parse(&b.url).is_err() {
                    out.push(Action {
                        label: format!("Remove invalid bookmark {}", b.url),
                        desc: "Maintenance".into(),
                        action: format!("bookmark:remove:{}", b.url),
                        args: None,
                    });
                }
            }
        }

        if let Ok(favs) = load_favs(FAV_FILE) {
            for f in favs {
                let mut missing = false;
                if let Some(arg) = f.args.as_ref() {
                    let path = std::path::Path::new(arg);
                    if path.is_absolute() && !path.exists() {
                        missing = true;
                    }
                } else {
                    let path = std::path::Path::new(&f.action);
                    if path.is_absolute() && !path.exists() {
                        missing = true;
                    }
                }
                if missing {
                    out.push(Action {
                        label: format!("Remove missing fav {}", f.label),
                        desc: "Maintenance".into(),
                        action: format!("fav:remove:{}", f.label),
                        args: None,
                    });
                }
            }
        }

        if out.is_empty() {
            out.push(Action {
                label: "No missing entries".into(),
                desc: "Maintenance".into(),
                action: "noop:".into(),
                args: None,
            });
        }

        out
    }

    fn name(&self) -> &str {
        "missing"
    }

    fn description(&self) -> &str {
        "Check and remove entries with missing paths"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "check miss".into(),
            desc: "Maintenance".into(),
            action: "query:check miss".into(),
            args: None,
        }]
    }
}
