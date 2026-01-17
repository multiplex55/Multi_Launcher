use crate::actions::Action;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct QueryFilters {
    pub remaining_tokens: Vec<String>,
    pub negate_text: bool,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub include_kinds: Vec<String>,
    pub exclude_kinds: Vec<String>,
    pub include_ids: Vec<String>,
    pub exclude_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawToken {
    pub raw: String,
    pub value: String,
}

pub fn tokenize_query(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' {
            escape = true;
            continue;
        }

        if let Some(active) = quote {
            if ch == active {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }

    if escape {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

pub fn tokenize_query_with_raw(input: &str) -> Vec<RawToken> {
    let mut tokens = Vec::new();
    let mut raw = String::new();
    let mut value = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            raw.push(ch);
            value.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' {
            raw.push(ch);
            escape = true;
            continue;
        }

        if let Some(active) = quote {
            raw.push(ch);
            if ch == active {
                quote = None;
            } else {
                value.push(ch);
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            raw.push(ch);
            quote = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !raw.is_empty() {
                tokens.push(RawToken {
                    raw: std::mem::take(&mut raw),
                    value: std::mem::take(&mut value),
                });
            }
        } else {
            raw.push(ch);
            value.push(ch);
        }
    }

    if escape {
        raw.push('\\');
        value.push('\\');
    }

    if !raw.is_empty() {
        tokens.push(RawToken { raw, value });
    }

    tokens
}

pub fn parse_query_filters(input: &str, tag_prefixes: &[&str]) -> QueryFilters {
    let tokens = tokenize_query(input);
    let mut filters = QueryFilters::default();

    for token in tokens {
        let (token, negated) = split_negation(&token);
        if token.is_empty() {
            continue;
        }

        if let Some(tag) = strip_tag_prefix(token, tag_prefixes) {
            if !tag.is_empty() {
                push_filter_value(tag, negated, &mut filters.include_tags, &mut filters.exclude_tags);
            }
            continue;
        }

        if let Some(kind) = strip_named_filter(token, "kind") {
            if !kind.is_empty() {
                push_filter_value(kind, negated, &mut filters.include_kinds, &mut filters.exclude_kinds);
            }
            continue;
        }

        if let Some(id) = strip_named_filter(token, "id") {
            if !id.is_empty() {
                push_filter_value(id, negated, &mut filters.include_ids, &mut filters.exclude_ids);
            }
            continue;
        }

        if negated && !filters.negate_text && filters.remaining_tokens.is_empty() {
            filters.negate_text = true;
        }
        filters.remaining_tokens.push(token.to_string());
    }

    filters
}

pub fn split_action_filters(input: &str) -> (String, QueryFilters) {
    let tokens = tokenize_query_with_raw(input);
    let mut filters = QueryFilters::default();
    let mut remaining = Vec::new();

    for token in tokens {
        let (token_value, negated) = split_negation(&token.value);
        if let Some(kind) = strip_named_filter(token_value, "kind") {
            if !kind.is_empty() {
                push_filter_value(kind, negated, &mut filters.include_kinds, &mut filters.exclude_kinds);
            }
            continue;
        }
        if let Some(id) = strip_named_filter(token_value, "id") {
            if !id.is_empty() {
                push_filter_value(id, negated, &mut filters.include_ids, &mut filters.exclude_ids);
            }
            continue;
        }
        remaining.push(token.raw);
    }

    (remaining.join(" "), filters)
}

pub fn rebuild_query(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| {
            if token.chars().any(char::is_whitespace) || token.contains('"') || token.contains('\\') {
                let mut escaped = String::new();
                for ch in token.chars() {
                    if ch == '\\' || ch == '"' {
                        escaped.push('\\');
                    }
                    escaped.push(ch);
                }
                format!("\"{}\"", escaped)
            } else {
                token.clone()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn apply_action_filters(actions: Vec<Action>, filters: &QueryFilters) -> Vec<Action> {
    actions
        .into_iter()
        .filter(|action| action_matches_filters(action, filters))
        .collect()
}

fn action_matches_filters(action: &Action, filters: &QueryFilters) -> bool {
    let action_id = action.action.to_lowercase();
    let kind_candidates = action_kind_candidates(action);

    if !filters.include_kinds.is_empty()
        && !filters
            .include_kinds
            .iter()
            .any(|kind| kind_candidates.iter().any(|candidate| candidate == kind))
    {
        return false;
    }

    if filters
        .exclude_kinds
        .iter()
        .any(|kind| kind_candidates.iter().any(|candidate| candidate == kind))
    {
        return false;
    }

    if !filters.include_ids.is_empty()
        && !filters
            .include_ids
            .iter()
            .any(|id| action_id == *id)
    {
        return false;
    }

    if filters.exclude_ids.iter().any(|id| action_id == *id) {
        return false;
    }

    true
}

fn action_kind_candidates(action: &Action) -> Vec<String> {
    let mut kinds = Vec::new();
    if !action.desc.trim().is_empty() {
        kinds.push(action.desc.trim().to_lowercase());
    }
    if let Some(prefix) = action.action.split(':').next() {
        if !prefix.trim().is_empty() {
            kinds.push(prefix.trim().to_lowercase());
        }
    }
    kinds.sort();
    kinds.dedup();
    kinds
}

fn split_negation(token: &str) -> (&str, bool) {
    token
        .strip_prefix('!')
        .map(|stripped| (stripped.trim_start(), true))
        .unwrap_or((token, false))
}

fn strip_tag_prefix<'a>(token: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for prefix in prefixes {
        if *prefix == "tag:" {
            let lower = token.to_lowercase();
            if lower.starts_with("tag:") {
                return token.get(4..).map(|rest| rest.trim());
            }
            continue;
        }
        if let Some(rest) = token.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }
    None
}

fn strip_named_filter<'a>(token: &'a str, name: &str) -> Option<&'a str> {
    let lower = token.to_lowercase();
    if lower.starts_with(name) && lower.as_bytes().get(name.len()) == Some(&b':') {
        return token.get(name.len() + 1..).map(|rest| rest.trim());
    }
    None
}

fn push_filter_value(value: &str, negated: bool, include: &mut Vec<String>, exclude: &mut Vec<String>) {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return;
    }
    if negated {
        exclude.push(normalized);
    } else {
        include.push(normalized);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_handles_quotes_and_escapes() {
        let tokens = tokenize_query(r#"tag:\"high priority\" kind:todo"#);
        assert_eq!(tokens, vec!["tag:\"high priority\"", "kind:todo"]);
    }

    #[test]
    fn parse_query_filters_handles_negated_tags_and_kinds() {
        let filters = parse_query_filters("todo kind:work !tag:chore", &["tag:", "@", "#"]);
        assert_eq!(filters.remaining_tokens, vec!["todo"]);
        assert_eq!(filters.include_kinds, vec!["work"]);
        assert_eq!(filters.exclude_tags, vec!["chore"]);
    }

    #[test]
    fn parse_query_filters_handles_quoted_tags_and_mixed_ops() {
        let filters = parse_query_filters(
            r#"note list tag:\"high priority\" !#chore "done soon""#,
            &["tag:", "#"],
        );
        assert_eq!(filters.include_tags, vec!["high priority"]);
        assert_eq!(filters.exclude_tags, vec!["chore"]);
        assert_eq!(filters.remaining_tokens, vec!["note", "list", "done soon"]);
    }

    #[test]
    fn split_action_filters_preserves_query_tokens() {
        let (query, filters) = split_action_filters(
            r#"todo list tag:"high priority" kind:todo !id:todo:done:1"#,
        );
        assert_eq!(query, r#"todo list tag:"high priority""#);
        assert_eq!(filters.include_kinds, vec!["todo"]);
        assert_eq!(filters.exclude_ids, vec!["todo:done:1"]);
    }
}
