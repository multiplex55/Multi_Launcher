use crate::actions::Action;
use crate::plugin::Plugin;

pub struct TextCasePlugin;

impl Plugin for TextCasePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "case ";
        if let Some(rest) = crate::common::strip_prefix_ci(query.trim_start(), PREFIX) {
            let text = rest.trim();
            if !text.is_empty() {
                let upper = text.to_uppercase();
                let lower = text.to_lowercase();
                let title = text
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            Some(first) => {
                                let mut s = first.to_uppercase().to_string();
                                s.push_str(&c.as_str().to_lowercase());
                                s
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let snake = text
                    .split_whitespace()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("_");
                return vec![
                    Action {
                        label: upper.clone(),
                        desc: "Text Case".into(),
                        action: format!("clipboard:{}", upper),
                        args: None,
                    },
                    Action {
                        label: lower.clone(),
                        desc: "Text Case".into(),
                        action: format!("clipboard:{}", lower),
                        args: None,
                    },
                    Action {
                        label: title.clone(),
                        desc: "Text Case".into(),
                        action: format!("clipboard:{}", title),
                        args: None,
                    },
                    Action {
                        label: snake.clone(),
                        desc: "Text Case".into(),
                        action: format!("clipboard:{}", snake),
                        args: None,
                    },
                ];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "text_case"
    }

    fn description(&self) -> &str {
        "Convert text cases (prefix: `case`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "case <text>".into(),
            desc: "Text Case".into(),
            action: "query:case ".into(),
            args: None,
        }]
    }
}
