use crate::actions::Action;
use crate::plugin::Plugin;

pub struct HotFolderPlugin;

impl Plugin for HotFolderPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("hotfolder") {
            return Vec::new();
        }
        vec![action(
            "Open Hot Folder Manager",
            "Review disabled-by-default file rules, select exact registered actions, and inspect terminal execution history.",
            "hotfolder:open",
        )]
    }

    fn name(&self) -> &str {
        "hotfolder"
    }

    fn description(&self) -> &str {
        "Watch explicit directories with disabled-by-default rules, exact action confirmation, and truthful execution history (prefix: `hotfolder`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["file_watch", "automation"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["hotfolder"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            action(
                "hotfolder",
                "Open the Hot Folder rule manager. Choose explicit watched roots; rules start disabled. Previously enabled intent returns only after fresh validation and startup never scans or backfills existing files. Direct children are the default scope; .part and .crdownload files are skipped unless included. Files settle for 2–10 stable observations at 50–5,000 ms intervals before execution, with bounded queues and per-rule/global run limits. Test Rule reads sample metadata only and has zero execution effects. Saving a changed rule disables it. Closing the manager leaves watching active; Pause or disabling the plugin stops new admission. History distinguishes terminal Success, Failure, Partial, and Cancelled receipts and retains recovery records.",
                "query:hotfolder",
            ),
            action("Pause Hot Folder", "Stop new Hot Folder admissions and request cancellation of owned active work.", "hotfolder:pause"),
            action("Resume Hot Folder", "Revalidate enabled rules and resume watching without backfilling existing files.", "hotfolder:resume"),
            action(
                "Open Hot Folder Manager",
                "Edit and test rules without side effects. Saving a changed rule disables it; close keeps background watching active, while Pause or disabling the plugin stops admission.",
                "hotfolder:open",
            ),
        ]
    }
}

fn action(label: &str, desc: &str, command: &str) -> Action {
    Action {
        label: label.into(),
        desc: desc.into(),
        action: command.into(),
        args: None,
    }
}
