use crate::actions::Action;
use crate::plugin::Plugin;

pub mod actions;
pub mod poller;
pub mod source;
pub mod storage;

/// RSS plugin registering the `rss` prefix.
///
/// Commands are routed to the handlers in `crate::actions::rss` via
/// colon-separated action strings such as `rss:refresh:all`.
///
/// Example daily usage:
///   rss refresh all
///   rss open feed-name
pub struct RssPlugin;

impl RssPlugin {
    /// Create a new instance of the RSS plugin.
    ///
    /// Ensures the configuration directory exists on initialization.
    pub fn new() -> Self {
        storage::ensure_config_dir();
        Self
    }
}

impl Default for RssPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for RssPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "rss";
        let trimmed = query.trim();

        // Bare `rss` discovers available subcommands and offers to manage feeds.
        if trimmed.eq_ignore_ascii_case(PREFIX) {
            let mut acts = actions::root();
            acts.insert(
                0,
                Action {
                    label: "rss: manage feeds".into(),
                    desc: "RSS".into(),
                    action: "rss:dialog".into(),
                    args: None,
                },
            );
            return acts;
        }

        // `rss ` (with space) should list subcommands handled below.
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "rss ") {
            let rest = rest.trim_start();
            if rest.is_empty() {
                return actions::root();
            }
            // Allow using colon-separated subcommands like `group:add`.
            let rest = rest.replacen(':', " ", 1);
            let mut parts = rest.splitn(2, ' ');
            let sub = parts.next().unwrap_or("");
            let args = parts.next().unwrap_or("");
            return match sub {
                "add" => actions::add(args),
                "rm" => actions::rm(args),
                "refresh" => actions::refresh(args),
                "list" => actions::list(args),
                "items" => actions::items(args),
                "open" => actions::open(args),
                "group" => actions::group(args),
                "mark" => actions::mark(args),
                "import" => actions::import(args),
                "export" => actions::export(args),
                _ => Vec::new(),
            };
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "rss"
    }

    fn description(&self) -> &str {
        "Manage RSS feeds (prefix: `rss`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "rss".into(),
                desc: "RSS feeds".into(),
                action: "query:rss ".into(),
                args: None,
            },
            Action {
                label: "rss add".into(),
                desc: "RSS feeds".into(),
                action: "query:rss add ".into(),
                args: None,
            },
            Action {
                label: "rss list".into(),
                desc: "RSS feeds".into(),
                action: "query:rss list".into(),
                args: None,
            },
            Action {
                label: "rss rm".into(),
                desc: "RSS feeds".into(),
                action: "query:rss rm ".into(),
                args: None,
            },
        ]
    }
}
