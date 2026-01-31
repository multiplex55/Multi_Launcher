use crate::mouse_gestures::db::{GestureConflictKind, GestureDb};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GestureStats {
    pub zero_bindings: usize,
    pub duplicate_tokens: usize,
    pub disabled_gestures: usize,
}

pub fn gesture_stats(db: &GestureDb) -> GestureStats {
    let mut stats = GestureStats::default();
    for gesture in &db.gestures {
        if !gesture.enabled {
            stats.disabled_gestures += 1;
        }
        let enabled_bindings = gesture.bindings.iter().filter(|b| b.enabled).count();
        if enabled_bindings == 0 {
            stats.zero_bindings += 1;
        }
    }
    stats.duplicate_tokens = db
        .find_conflicts()
        .iter()
        .filter(|conflict| conflict.kind == GestureConflictKind::DuplicateTokens)
        .count();
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse_gestures::db::{BindingEntry, BindingKind, GestureEntry};
    use crate::mouse_gestures::engine::DirMode;

    fn binding(label: &str) -> BindingEntry {
        BindingEntry {
            label: label.into(),
            kind: BindingKind::Execute,
            action: "action".into(),
            args: None,
            enabled: true,
        }
    }

    #[test]
    fn gesture_stats_detects_unbound_and_duplicates() {
        let db = GestureDb {
            schema_version: 2,
            gestures: vec![
                GestureEntry {
                    label: "One".into(),
                    tokens: "RD".into(),
                    dir_mode: DirMode::Four,
                    stroke: Vec::new(),
                    enabled: true,
                    bindings: vec![],
                },
                GestureEntry {
                    label: "Two".into(),
                    tokens: "RD".into(),
                    dir_mode: DirMode::Four,
                    stroke: Vec::new(),
                    enabled: true,
                    bindings: vec![binding("Action")],
                },
                GestureEntry {
                    label: "Three".into(),
                    tokens: "UL".into(),
                    dir_mode: DirMode::Four,
                    stroke: Vec::new(),
                    enabled: false,
                    bindings: vec![binding("Other")],
                },
            ],
        };

        let stats = gesture_stats(&db);

        assert_eq!(stats.zero_bindings, 1);
        assert_eq!(stats.duplicate_tokens, 1);
        assert_eq!(stats.disabled_gestures, 1);
    }
}
