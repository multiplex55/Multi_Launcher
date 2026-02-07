use crate::actions::Action;
use crate::plugin::Plugin;
use figlet_rs::FIGfont;

pub struct AsciiArtPlugin {
    font: Option<FIGfont>,
}

impl AsciiArtPlugin {
    /// Create a new plugin instance with the bundled standard font.
    pub fn new() -> Self {
        let font = match FIGfont::standard() {
            Ok(f) => Some(f),
            Err(e) => {
                tracing::error!("failed to load standard FIGfont: {e}");
                None
            }
        };
        Self { font }
    }
}

impl Default for AsciiArtPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AsciiArtPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "ascii ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, PREFIX) {
            let text = rest.trim();
            if !text.is_empty() {
                if let Some(font) = &self.font {
                    if let Some(fig) = font.convert(text) {
                        let art = fig.to_string();
                        return vec![Action {
                            label: art.clone(),
                            desc: "AsciiArt".into(),
                            action: format!("clipboard:{}", art),
                            args: None,
                            preview_text: None,
                            risk_level: None,
                            icon: None,
                        }];
                    }
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "asciiart"
    }

    fn description(&self) -> &str {
        "Generate ASCII art (prefix: `ascii`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "ascii <text>".into(),
            desc: "AsciiArt".into(),
            action: "query:ascii ".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
