use super::Action;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;

/// Lowercase text cached for a single complete action during launcher search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CachedSearchEntry {
    pub(crate) label_lc: String,
    pub(crate) desc_lc: String,
    pub(crate) action_lc: String,
}

impl CachedSearchEntry {
    pub(crate) fn from_action(action: &Action) -> Self {
        Self {
            label_lc: action.label.to_lowercase(),
            desc_lc: action.desc.to_lowercase(),
            action_lc: action.action.to_lowercase(),
        }
    }

    pub(crate) fn matches_exact_display_text(&self, query_lc: &str) -> bool {
        let query_lc = query_lc.trim();
        query_lc.is_empty() || self.label_lc.contains(query_lc)
    }

    /// Score one already selected candidate while borrowing its full action.
    /// Callers retain their own iteration, filters, fallback rules and cloning.
    pub(crate) fn score_candidate<M: FuzzyMatcher>(
        &self,
        action: &Action,
        query: &str,
        query_lc: &str,
        exact_match_mode: bool,
        alias_match: bool,
        matcher: &M,
        fuzzy_weight: f32,
    ) -> Option<f32> {
        if exact_match_mode {
            let matches = self.matches_exact_display_text(query_lc)
                || self.desc_lc.contains(query_lc)
                || self.action_lc.contains(query_lc)
                || alias_match;
            return matches.then_some(if alias_match { 1.0 } else { 0.0 });
        }

        matcher
            .fuzzy_match(&action.label, query)
            .max(matcher.fuzzy_match(&action.desc, query))
            .map(|score| score as f32 * fuzzy_weight)
    }
}

/// Folder entries shadow bookmark aliases with the same action ID, including
/// an explicit folder entry whose alias is absent.
pub(crate) fn alias_matches_lc(
    action: &str,
    query_lc: &str,
    folder_aliases_lc: &HashMap<String, Option<String>>,
    bookmark_aliases_lc: &HashMap<String, Option<String>>,
) -> bool {
    folder_aliases_lc
        .get(action)
        .or_else(|| bookmark_aliases_lc.get(action))
        .and_then(Option::as_ref)
        .is_some_and(|alias| alias.contains(query_lc))
}

#[cfg(test)]
mod tests {
    use super::alias_matches_lc;
    use std::collections::HashMap;

    #[test]
    fn folder_without_alias_shadows_bookmark_alias_for_the_same_action() {
        let folder_aliases = HashMap::from([("shared-action".to_owned(), None)]);
        let bookmark_aliases = HashMap::from([(
            "shared-action".to_owned(),
            Some("useful bookmark alias".to_owned()),
        )]);

        assert!(alias_matches_lc(
            "shared-action",
            "useful",
            &HashMap::new(),
            &bookmark_aliases,
        ));
        assert!(!alias_matches_lc(
            "shared-action",
            "useful",
            &folder_aliases,
            &bookmark_aliases,
        ));
    }
}
