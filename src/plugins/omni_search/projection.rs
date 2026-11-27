use super::OmniSearchSettings;
use crate::actions::Action;
use crate::plugin::Plugin;
use fst::{IntoStreamer, Map, MapBuilder, Streamer, automaton::Subsequence};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OmniSource {
    Folder,
    Bookmark,
    App { generation: u64, ordinal: usize },
    Note,
    Todo,
    Calendar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmniRow {
    pub action: Action,
    pub source: OmniSource,
}

/// One coherent app catalog, retaining the stock first-key FST and fallback.
pub struct OmniAppCatalog {
    actions: Arc<Vec<Action>>,
    generation: u64,
    pub(super) index: Option<Map<Vec<u8>>>,
}

impl OmniAppCatalog {
    /// Build off the UI thread. Return None when the supplied catalog is superseded.
    pub fn build(
        actions: Arc<Vec<Action>>,
        generation: u64,
        current: impl Fn() -> bool,
    ) -> Option<Self> {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for (ordinal, action) in actions.iter().enumerate() {
            if !current() {
                return None;
            }
            let label = action.label.to_lowercase();
            if seen.insert(label.clone()) {
                entries.push((label.clone(), ordinal as u64));
            } else {
                tracing::warn!(key = %label, "duplicate search key; skipping");
            }
            if !action.desc.is_empty() {
                let desc = action.desc.to_lowercase();
                if desc != label {
                    if seen.insert(desc.clone()) {
                        entries.push((desc, ordinal as u64));
                    } else {
                        tracing::warn!(key = %desc, "duplicate search key; skipping");
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut builder = MapBuilder::memory();
        for (key, ordinal) in entries {
            if !current() {
                return None;
            }
            if let Err(error) = builder.insert(key.as_str(), ordinal) {
                tracing::warn!(%key, value = ordinal, ?error, "failed to insert key into search index");
            }
        }
        let index = match builder
            .into_inner()
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Map::new(bytes).map_err(anyhow::Error::from))
        {
            Ok(index) => Some(index),
            Err(error) => {
                tracing::error!(
                    ?error,
                    "failed to build omni search index; falling back to linear scan"
                );
                None
            }
        };
        if !current() {
            return None;
        }
        Some(Self {
            actions,
            generation,
            index,
        })
    }

    fn search(&self, rest: &str) -> Vec<OmniRow> {
        let mut out = Vec::new();
        let mut append = |ordinal: usize| {
            if let Some(action) = self.actions.get(ordinal) {
                out.push(OmniRow {
                    action: action.clone(),
                    source: OmniSource::App {
                        generation: self.generation,
                        ordinal,
                    },
                });
            }
        };
        let query = rest.trim();
        if query.is_empty() {
            for ordinal in 0..self.actions.len() {
                append(ordinal);
            }
        } else {
            let query = query.to_lowercase();
            if let Some(index) = &self.index {
                let mut stream = index.search(Subsequence::new(&query)).into_stream();
                let mut seen = HashSet::new();
                while let Some((_, ordinal)) = stream.next() {
                    if seen.insert(ordinal) {
                        append(ordinal as usize);
                    }
                }
            } else {
                for (ordinal, action) in self.actions.iter().enumerate() {
                    if action.label.to_lowercase().contains(&query)
                        || action.desc.to_lowercase().contains(&query)
                    {
                        append(ordinal);
                    }
                }
            }
        }
        out
    }
}

/// Preserve stock prefix matching; the outer plugin router owns admission.
pub fn omni_query_term(query: &str) -> Option<&str> {
    if let Some(rest) = crate::common::strip_prefix_ci(query, "o list") {
        return Some(rest.trim_start());
    }
    crate::common::strip_prefix_ci(query, "o").map(str::trim_start)
}

/// Existing child providers remain owned by the caller, not a second runtime.
pub struct OmniSources<'a> {
    pub folders: Option<&'a dyn Plugin>,
    pub bookmarks: Option<&'a dyn Plugin>,
    pub apps: Option<&'a OmniAppCatalog>,
    pub notes: Option<&'a dyn Plugin>,
    pub todos: Option<&'a dyn Plugin>,
    pub calendar: Option<&'a dyn Plugin>,
}

pub fn project_omni(
    query: &str,
    settings: &OmniSearchSettings,
    sources: &OmniSources<'_>,
) -> Vec<OmniRow> {
    let Some(rest) = omni_query_term(query) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if settings.include_folders {
        add_rows(
            &mut rows,
            sources.folders,
            format!("f {rest}"),
            OmniSource::Folder,
        );
    }
    if settings.include_bookmarks {
        add_rows(
            &mut rows,
            sources.bookmarks,
            if rest.trim().is_empty() {
                "bm list".into()
            } else {
                format!("bm {rest}")
            },
            OmniSource::Bookmark,
        );
    }
    if settings.include_apps
        && let Some(apps) = sources.apps
    {
        rows.extend(apps.search(rest));
    }
    if settings.include_notes {
        add_rows(
            &mut rows,
            sources.notes,
            if rest.trim().is_empty() {
                "note list".into()
            } else {
                format!("note list {}", rest.trim())
            },
            OmniSource::Note,
        );
    }
    if settings.include_todos {
        add_rows(
            &mut rows,
            sources.todos,
            if rest.trim().is_empty() {
                "todo list".into()
            } else {
                format!("todo list {}", rest.trim())
            },
            OmniSource::Todo,
        );
    }
    if settings.include_calendar {
        if rest.trim().is_empty() {
            add_rows(
                &mut rows,
                sources.calendar,
                "cal".into(),
                OmniSource::Calendar,
            );
            add_rows(
                &mut rows,
                sources.calendar,
                "cal upcoming".into(),
                OmniSource::Calendar,
            );
        } else {
            add_rows(
                &mut rows,
                sources.calendar,
                format!("cal find {rest}"),
                OmniSource::Calendar,
            );
        }
    }
    dedup_rows(rows)
}

fn add_rows(
    rows: &mut Vec<OmniRow>,
    provider: Option<&dyn Plugin>,
    query: String,
    source: OmniSource,
) {
    if let Some(provider) = provider {
        rows.extend(provider.search(&query).into_iter().map(|action| OmniRow {
            action,
            source: source.clone(),
        }));
    }
}

pub(super) fn dedup_rows(rows: Vec<OmniRow>) -> Vec<OmniRow> {
    let mut seen = HashSet::new();
    let mut normalized = String::new();
    let mut key = String::new();
    rows.into_iter()
        .filter(|row| {
            normalize_label_into(&row.action.label, &mut normalized);
            key.clear();
            key.push_str(&row.action.action);
            key.push('\x1f');
            key.push_str(&normalized);
            seen.insert(key.clone())
        })
        .collect()
}

fn normalize_label_into(label: &str, normalized: &mut String) {
    normalized.clear();
    let mut pending_space = false;
    for ch in label.chars() {
        if ch.is_whitespace() {
            if !normalized.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.extend(ch.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Child(Vec<Action>);
    impl Plugin for Child {
        fn search(&self, _: &str) -> Vec<Action> {
            self.0.clone()
        }
        fn name(&self) -> &str {
            "fixture"
        }
        fn description(&self) -> &str {
            "fixture"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }
    fn action(label: &str, target: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: String::new(),
            action: target.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn typed_projection_preserves_first_source_generation_and_categories() {
        let folder = Child(vec![action("Shared   Name", "same", None)]);
        let bookmark = Child(vec![
            action(" shared name ", "same", None),
            action("Web", "https://example", None),
        ]);
        let apps = OmniAppCatalog::build(
            Arc::new(vec![action("Other", "app", Some("--keep"))]),
            42,
            || true,
        )
        .unwrap();
        let sources = OmniSources {
            folders: Some(&folder),
            bookmarks: Some(&bookmark),
            apps: Some(&apps),
            notes: None,
            todos: None,
            calendar: None,
        };
        let mut settings = OmniSearchSettings::default();
        let rows = project_omni("O LIST", &settings, &sources);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].source, OmniSource::Folder);
        assert_eq!(rows[1].source, OmniSource::Bookmark);
        assert_eq!(
            rows[2].source,
            OmniSource::App {
                generation: 42,
                ordinal: 0
            }
        );
        assert_eq!(rows[2].action.args.as_deref(), Some("--keep"));
        settings.include_folders = false;
        assert_eq!(
            project_omni("o", &settings, &sources)[0].source,
            OmniSource::Bookmark
        );
        settings.include_bookmarks = false;
        assert_eq!(
            project_omni("o", &settings, &sources)[0].source,
            OmniSource::App {
                generation: 42,
                ordinal: 0
            }
        );
        assert!(project_omni("x", &settings, &sources).is_empty());
        let pending = OmniSources {
            folders: Some(&folder),
            bookmarks: Some(&bookmark),
            apps: None,
            notes: None,
            todos: None,
            calendar: None,
        };
        settings.include_folders = true;
        assert_eq!(
            project_omni("o", &settings, &pending)[0].source,
            OmniSource::Folder
        );
    }

    #[test]
    fn superseded_catalog_is_not_published_and_folded_prefix_is_stock() {
        let actions = Arc::new(vec![action("Tool", "path", None)]);
        assert!(OmniAppCatalog::build(actions, 7, || false).is_none());
        assert_eq!(omni_query_term("O LiSt  term"), Some("term"));
        assert_eq!(omni_query_term("other"), Some("ther"));
    }
}
