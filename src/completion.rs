use crate::actions::Action;
use fst::{IntoStreamer, Map, MapBuilder, Streamer};

/// Builds the launcher's production prefix-completion index.
pub fn build_index(commands: &[Action], actions: &[Action]) -> Map<Vec<u8>> {
    let mut entries: Vec<String> = commands
        .iter()
        .map(|action| action.label.to_lowercase())
        .collect();
    entries.extend(
        actions
            .iter()
            .map(|action| format!("app {}", action.label.to_lowercase())),
    );
    entries.sort();
    entries.dedup();

    let mut builder = MapBuilder::memory();
    for (index, key) in entries.iter().enumerate() {
        if let Err(error) = builder.insert(key, index as u64) {
            tracing::warn!(%key, ?error, "failed to insert key into completion index");
        }
    }
    Map::new(builder.into_inner().expect("in-memory FST construction"))
        .expect("valid in-memory completion FST")
}

/// Returns at most `limit` prefix completions, excluding the query itself.
pub fn suggestions(index: &Map<Vec<u8>>, query: &str, limit: usize) -> Vec<String> {
    suggestions_matching(index, query, limit, |_| true)
}

/// Returns the launcher's prefix completions that GPUI's pinned single-line
/// Search editor can represent without changing their text.
pub fn single_line_suggestions(index: &Map<Vec<u8>>, query: &str, limit: usize) -> Vec<String> {
    suggestions_matching(index, query, limit, is_single_line)
}

fn suggestions_matching(
    index: &Map<Vec<u8>>,
    query: &str,
    limit: usize,
    accepts: impl Fn(&str) -> bool,
) -> Vec<String> {
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let query = query.to_lowercase();
    let mut stream = index.range().ge(query.as_str()).into_stream();
    let mut suggestions = Vec::with_capacity(limit.min(5));
    while let Some((key, _)) = stream.next() {
        let Ok(key) = std::str::from_utf8(key) else {
            continue;
        };
        if !key.starts_with(&query) {
            break;
        }
        if key != query && accepts(key) {
            suggestions.push(key.to_string());
        }
        if suggestions.len() >= limit {
            break;
        }
    }
    suggestions
}

/// Returns the candidate at the current cycle position unless it is already
/// the case-normalized current query.
pub fn checked_tab_candidate<'a>(
    suggestions: &'a [String],
    index: usize,
    query: &str,
) -> Option<&'a str> {
    let candidate = suggestions.get(index)?;
    let normalized_query = query.to_lowercase();
    if candidate.as_str() == normalized_query.as_str() {
        return None;
    }
    Some(candidate)
}

/// Advances one Tab-cycle position, wrapping once through the current list.
pub fn next_tab_index(index: usize, suggestion_count: usize) -> Option<usize> {
    (suggestion_count > 0).then(|| (index % suggestion_count + 1) % suggestion_count)
}

fn is_single_line(candidate: &str) -> bool {
    !candidate.contains(['\r', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(label: &str) -> Action {
        Action {
            label: label.into(),
            desc: String::new(),
            action: label.into(),
            args: None,
        }
    }

    #[test]
    fn index_deduplicates_and_suggestions_match_production_prefix_rules() {
        let commands = vec![action("Help"), action("help")];
        let actions = vec![action("Editor"), action("Email"), action("Emulator")];
        let index = build_index(&commands, &actions);

        assert_eq!(
            suggestions(&index, "APP E", 2),
            vec!["app editor", "app email"]
        );
        assert!(suggestions(&index, "help", 5).is_empty());
        assert!(suggestions(&index, "", 5).is_empty());
    }

    #[test]
    fn tab_candidates_preserve_single_line_text_and_cycle_order() {
        let commands = vec![
            action("Alpha\nBeta"),
            action("Alpha\rBeta"),
            action("Alpha Gamma"),
            action("Alpha Delta"),
        ];
        let index = build_index(&commands, &[]);

        assert_eq!(
            suggestions(&index, "alpha", 5),
            vec!["alpha\nbeta", "alpha\rbeta", "alpha delta", "alpha gamma"]
        );
        assert_eq!(
            single_line_suggestions(&index, "alpha", 5),
            vec!["alpha delta", "alpha gamma"]
        );
        assert_eq!(
            checked_tab_candidate(
                &single_line_suggestions(&index, "alpha", 5),
                0,
                "ALPHA",
            ),
            Some("alpha delta")
        );
        assert_eq!(
            checked_tab_candidate(&["alpha".into()], 0, "ALPHA"),
            None,
            "the exact current query cannot be accepted again"
        );
        assert_eq!(
            checked_tab_candidate(&["alpha\nbeta".into()], 0, "alpha"),
            Some("alpha\nbeta"),
            "the shared legacy rule keeps egui's existing stored-label behavior"
        );
        assert!(
            single_line_suggestions(&index, "alpha", 5)
                .iter()
                .all(|candidate| !candidate.contains(['\r', '\n'])),
            "GPUI candidates cannot contain text its pinned single-line editor would rewrite"
        );
        assert_eq!(next_tab_index(0, 3), Some(1));
        assert_eq!(next_tab_index(2, 3), Some(0));
        assert_eq!(next_tab_index(0, 0), None);
    }
}
