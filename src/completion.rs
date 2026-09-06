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
        if key != query {
            suggestions.push(key.to_string());
        }
        if suggestions.len() >= limit {
            break;
        }
    }
    suggestions
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
}
