use crate::actions::Action;
use crate::plugin::Plugin;

/// Launcher entries for the GUI-owned crop workflows.
pub struct CropPlugin;

impl CropPlugin {
    fn screenshot_action() -> Action {
        Action {
            label: "Screenshot and Clip".into(),
            desc: "Crop screenshot / clip image".into(),
            action: "crop:screenshot".into(),
            args: None,
        }
    }

    fn image_action() -> Action {
        Action {
            label: "Crop Image".into(),
            desc: "Crop image file".into(),
            action: "crop:image".into(),
            args: None,
        }
    }
}

impl Plugin for CropPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(rest) = crate::common::strip_prefix_ci(query.trim(), "crop") else {
            return Vec::new();
        };
        match rest.trim().to_ascii_lowercase().as_str() {
            "" => vec![Self::screenshot_action(), Self::image_action()],
            "screenshot" | "screen" | "clip" => vec![Self::screenshot_action()],
            "image" | "file" => vec![Self::image_action()],
            _ => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "crop"
    }

    fn description(&self) -> &str {
        "Screenshot and crop image files (prefix: `crop`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["crop"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_queries_return_stable_gui_action_ids() {
        let plugin = CropPlugin;
        assert_eq!(
            plugin
                .search("crop")
                .iter()
                .map(|a| a.action.as_str())
                .collect::<Vec<_>>(),
            ["crop:screenshot", "crop:image"]
        );
        assert_eq!(
            plugin.search("crop screenshot")[0].action,
            "crop:screenshot"
        );
        assert_eq!(plugin.search("crop clip")[0].action, "crop:screenshot");
        assert_eq!(plugin.search("crop image")[0].action, "crop:image");
        assert_eq!(plugin.search("crop file")[0].action, "crop:image");
        assert!(plugin.search("crop other").is_empty());
    }
}
