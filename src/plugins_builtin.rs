use crate::actions::Action;
use crate::plugin::Plugin;

pub struct WebSearchPlugin;

impl Plugin for WebSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("g ") && query.len() > 2 {
            let q = &query[2..];
            vec![Action {
                label: format!("Search Google for {q}"),
                desc: "Web search".into(),
                action: format!("https://www.google.com/search?q={}", q),
            }]
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "web_search"
    }
}

pub struct CalculatorPlugin;

impl Plugin for CalculatorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("=") {
            let expr = &query[1..];
            match meval::eval_str(expr) {
                Ok(v) => vec![Action {
                    label: format!("{} = {}", expr, v),
                    desc: "Calculator".into(),
                    action: format!("{}", v),
                }],
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "calculator"
    }
}
