use crate::actions::Action;
use crate::plugin::Plugin;
use urlencoding::encode;

pub struct WebSearchPlugin;

impl Plugin for WebSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("g ") && query.len() > 2 {
            let q = &query[2..];
            vec![Action {
                label: format!("Search Google for {q}"),
                desc: "Web search".into(),
                action: format!("https://www.google.com/search?q={}", encode(q)),
                args: None,
            }]
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Perform web searches using Google (prefix: `g`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "g".into(), desc: "Web search".into(), action: "query:g ".into(), args: None }]
    }
}

pub struct CalculatorPlugin;

impl Plugin for CalculatorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.starts_with("=") {
            let expr = &query[1..];
            match exmex::eval_str::<f64>(expr) {
                Ok(v) => vec![Action {
                    label: format!("{} = {}", expr, v),
                    desc: "Calculator".into(),
                    action: format!("calc:{}", v),
                    args: None,
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

    fn description(&self) -> &str {
        "Evaluate mathematical expressions (prefix: `=`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "=".into(), desc: "Calculator".into(), action: "query:= ".into(), args: None }]
    }
}
