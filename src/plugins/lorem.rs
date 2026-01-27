use crate::actions::Action;
use crate::plugin::Plugin;

pub struct LoremPlugin;

fn gen_sentences(n: usize) -> String {
    let mut sentences = Vec::new();
    for _ in 0..n {
        sentences.push(lipsum::lipsum_words(12));
    }
    sentences.join(" ")
}

fn gen_paragraphs(n: usize) -> String {
    let mut paras = Vec::new();
    for _ in 0..n {
        paras.push(gen_sentences(5));
    }
    paras.join("\n\n")
}

impl Plugin for LoremPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "lorem ";
        if let Some(rest) = crate::common::strip_prefix_ci(query.trim_start(), PREFIX) {
            let mut parts = rest.split_whitespace();
            if let (Some(kind), Some(num)) = (parts.next(), parts.next()) {
                if parts.next().is_some() {
                    return Vec::new();
                }
                if let Ok(n) = num.parse::<usize>() {
                    let text = match kind {
                        "w" => lipsum::lipsum_words(n),
                        "s" => gen_sentences(n),
                        "p" => gen_paragraphs(n),
                        _ => return Vec::new(),
                    };
                    return vec![Action {
                        label: text.clone(),
                        desc: "Lorem".into(),
                        action: format!("clipboard:{text}"),
                        args: None,
                    }];
                }
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "lorem"
    }

    fn description(&self) -> &str {
        "Generate lorem ipsum text (prefix: `lorem`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "lorem w <n>".into(),
                desc: "Lorem words".into(),
                action: "query:lorem w ".into(),
                args: None,
            },
            Action {
                label: "lorem s <n>".into(),
                desc: "Lorem sentences".into(),
                action: "query:lorem s ".into(),
                args: None,
            },
            Action {
                label: "lorem p <n>".into(),
                desc: "Lorem paragraphs".into(),
                action: "query:lorem p ".into(),
                args: None,
            },
        ]
    }
}
