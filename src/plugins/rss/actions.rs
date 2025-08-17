use super::source::FeedType;
use super::{source, storage};
use crate::actions::Action;

/// Return the top-level `rss` subcommands.
pub fn root() -> Vec<Action> {
    let cmds = [
        ("add", "Add feed"),
        ("rm", "Remove feed"),
        ("refresh", "Refresh feeds"),
        ("ls", "List feeds/groups"),
        ("items", "Show feed items"),
        ("open", "Open feed items"),
        ("group", "Manage groups"),
        ("mark", "Mark read/unread"),
        ("import", "Import OPML"),
        ("export", "Export OPML"),
    ];
    cmds.iter()
        .map(|(c, d)| Action {
            label: format!("rss {c}"),
            desc: d.to_string(),
            action: format!("query:rss {c} "),
            args: None,
        })
        .collect()
}

/// Handle `rss add`.
pub fn add(args: &str) -> Vec<Action> {
    let src = args.trim();
    if src.is_empty() {
        return vec![Action {
            label: "rss add <url>".into(),
            desc: "Add feed".into(),
            action: "query:rss add ".into(),
            args: None,
        }];
    }

    match source::resolve(src) {
        Ok(resolved) if !resolved.is_empty() => {
            let multiple = resolved.len() > 1;
            resolved
                .into_iter()
                .map(|r| {
                    let label_target = if multiple {
                        r.feed_url.clone()
                    } else {
                        r.site_url.clone().unwrap_or_else(|| r.feed_url.clone())
                    };
                    let label = format!("Add feed {label_target}");
                    let desc = match r.feed_type {
                        FeedType::Atom => "Atom",
                        FeedType::Rss => "RSS",
                        FeedType::Json => "JSON",
                    };
                    Action {
                        label,
                        desc: desc.into(),
                        action: format!("rss:add {}", r.feed_url),
                        args: None,
                    }
                })
                .collect()
        }
        _ => vec![Action {
            label: format!("Add feed {src}"),
            desc: "RSS".into(),
            action: format!("rss:add {src}"),
            args: None,
        }],
    }
}

/// Handle `rss rm` prompting with known feed ids.
pub fn rm(args: &str) -> Vec<Action> {
    let feeds = storage::FeedsFile::load();
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return feeds
            .feeds
            .iter()
            .map(|f| {
                let title = f.title.clone().unwrap_or_else(|| f.id.clone());
                Action {
                    label: format!("Remove {title}"),
                    desc: "RSS".into(),
                    action: format!("rss:rm {}", f.id),
                    args: None,
                }
            })
            .collect();
    }
    vec![Action {
        label: format!("Remove feed {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:rm {trimmed}"),
        args: None,
    }]
}

/// Handle `rss refresh` prompting with feed ids/groups/all.
pub fn refresh(args: &str) -> Vec<Action> {
    let feeds = storage::FeedsFile::load();
    let trimmed = args.trim();
    if trimmed.is_empty() {
        let mut acts = Vec::new();
        acts.push(Action {
            label: "Refresh all feeds".into(),
            desc: "RSS".into(),
            action: "rss:refresh all".into(),
            args: None,
        });
        for g in &feeds.groups {
            acts.push(Action {
                label: format!("Refresh group {g}"),
                desc: "RSS".into(),
                action: format!("rss:refresh {g}"),
                args: None,
            });
        }
        for f in &feeds.feeds {
            let title = f.title.clone().unwrap_or_else(|| f.id.clone());
            acts.push(Action {
                label: format!("Refresh {title}"),
                desc: "RSS".into(),
                action: format!("rss:refresh {}", f.id),
                args: None,
            });
        }
        return acts;
    }
    vec![Action {
        label: format!("Refresh {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:refresh {trimmed}"),
        args: None,
    }]
}

/// Handle `rss ls` subcommand.
pub fn ls(args: &str) -> Vec<Action> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![
            Action {
                label: "rss ls groups".into(),
                desc: "RSS".into(),
                action: "rss:ls groups".into(),
                args: None,
            },
            Action {
                label: "rss ls feeds".into(),
                desc: "RSS".into(),
                action: "rss:ls feeds".into(),
                args: None,
            },
        ];
    }
    vec![Action {
        label: format!("List {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:ls {trimmed}"),
        args: None,
    }]
}

/// Handle `rss items`.
pub fn items(args: &str) -> Vec<Action> {
    let feeds = storage::FeedsFile::load();
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return feeds
            .feeds
            .iter()
            .map(|f| {
                let title = f.title.clone().unwrap_or_else(|| f.id.clone());
                Action {
                    label: format!("Items for {title}"),
                    desc: "RSS".into(),
                    action: format!("rss:items {}", f.id),
                    args: None,
                }
            })
            .collect();
    }
    vec![Action {
        label: format!("Items for {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:items {trimmed}"),
        args: None,
    }]
}

/// Handle `rss open` using the same options as `items`.
pub fn open(args: &str) -> Vec<Action> {
    items(args)
        .into_iter()
        .map(|mut a| {
            a.action = a.action.replacen("rss:items", "rss:open", 1);
            a.label = a.label.replacen("Items", "Open", 1);
            a
        })
        .collect()
}

/// Handle `rss group` showing subgroup operations.
pub fn group(args: &str) -> Vec<Action> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![
            Action {
                label: "rss group add <name>".into(),
                desc: "Add group".into(),
                action: "query:rss group add ".into(),
                args: None,
            },
            Action {
                label: "rss group rm <name>".into(),
                desc: "Remove group".into(),
                action: "query:rss group rm ".into(),
                args: None,
            },
            Action {
                label: "rss group mv <old> <new>".into(),
                desc: "Rename group".into(),
                action: "query:rss group mv ".into(),
                args: None,
            },
        ];
    }
    vec![Action {
        label: format!("rss group {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:group {trimmed}"),
        args: None,
    }]
}

/// Handle `rss mark`.
pub fn mark(args: &str) -> Vec<Action> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![
            Action {
                label: "rss mark read <target>".into(),
                desc: "Mark read".into(),
                action: "query:rss mark read ".into(),
                args: None,
            },
            Action {
                label: "rss mark unread <target>".into(),
                desc: "Mark unread".into(),
                action: "query:rss mark unread ".into(),
                args: None,
            },
        ];
    }
    vec![Action {
        label: format!("rss mark {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:mark {trimmed}"),
        args: None,
    }]
}

/// Handle `rss import`.
pub fn import(args: &str) -> Vec<Action> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![Action {
            label: "rss import <file>".into(),
            desc: "Import OPML".into(),
            action: "query:rss import ".into(),
            args: None,
        }];
    }
    vec![Action {
        label: format!("Import {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:import {trimmed}"),
        args: None,
    }]
}

/// Handle `rss export`.
pub fn export(args: &str) -> Vec<Action> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return vec![Action {
            label: "rss export <file>".into(),
            desc: "Export OPML".into(),
            action: "query:rss export ".into(),
            args: None,
        }];
    }
    vec![Action {
        label: format!("Export to {trimmed}"),
        desc: "RSS".into(),
        action: format!("rss:export {trimmed}"),
        args: None,
    }]
}
