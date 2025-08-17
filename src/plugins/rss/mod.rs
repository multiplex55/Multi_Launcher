use crate::actions::Action;
use crate::plugin::Plugin;

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

        // Bare `rss` opens the feed management UI.
        if trimmed.eq_ignore_ascii_case(PREFIX) {
            return vec![Action {
                label: "rss: manage feeds".into(),
                desc: "RSS".into(),
                action: "rss:dialog".into(),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(query, "rss ") {
            let cmd = rest.trim();
            if cmd.is_empty() {
                return vec![Action {
                    label: "rss: manage feeds".into(),
                    desc: "RSS".into(),
                    action: "rss:dialog".into(),
                    args: None,
                }];
            }
            return vec![Action {
                label: format!("rss {cmd}"),
                desc: "RSS".into(),
                action: format!("rss:{cmd}"),
                args: None,
            }];
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
        vec![Action {
            label: "rss".into(),
            desc: "RSS feeds".into(),
            action: "query:rss ".into(),
            args: None,
        }]
    }
}
