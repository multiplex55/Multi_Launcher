use crate::dashboard::config::{DashboardConfig, SlotConfig};
use crate::dashboard::widgets::WidgetRegistry;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedSlot {
    pub id: Option<String>,
    pub widget: String,
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
    pub settings: Value,
    pub overflow: crate::dashboard::config::OverflowMode,
}

/// Validate and normalize slot positions to the configured grid size.
pub fn normalize_slots(
    cfg: &DashboardConfig,
    registry: &WidgetRegistry,
) -> (Vec<NormalizedSlot>, Vec<String>) {
    let rows = cfg.grid.rows.max(1) as usize;
    let cols = cfg.grid.cols.max(1) as usize;
    let mut occupied = vec![vec![false; cols]; rows];
    let mut normalized = Vec::new();
    let mut warnings = Vec::new();

    for slot in &cfg.slots {
        if !registry.contains(&slot.widget) {
            warnings.push(format!("dropping unknown widget '{}'", slot.widget));
            continue;
        }
        if let Some(ns) = normalize_slot(slot, rows, cols, &mut occupied) {
            normalized.push(ns);
        } else {
            warnings.push(format!(
                "slot for widget '{}' is outside the grid and was ignored",
                slot.widget
            ));
        }
    }

    (normalized, warnings)
}

fn normalize_slot(
    slot: &SlotConfig,
    rows: usize,
    cols: usize,
    occupied: &mut [Vec<bool>],
) -> Option<NormalizedSlot> {
    if slot.row < 0 || slot.col < 0 {
        return None;
    }
    let row = slot.row as usize;
    let col = slot.col as usize;
    if row >= rows || col >= cols {
        return None;
    }
    let row_span = slot.row_span.max(1).min((rows - row).max(1) as u8) as usize;
    let col_span = slot.col_span.max(1).min((cols - col).max(1) as u8) as usize;

    for r in row..row + row_span {
        for c in col..col + col_span {
            if occupied[r][c] {
                return None;
            }
        }
    }
    for r in row..row + row_span {
        for c in col..col + col_span {
            occupied[r][c] = true;
        }
    }

    Some(NormalizedSlot {
        id: slot.id.clone(),
        widget: slot.widget.clone(),
        row,
        col,
        row_span,
        col_span,
        settings: slot.settings.clone(),
        overflow: slot.overflow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::config::OverflowMode;
    use serde_json::json;

    #[derive(Default)]
    struct DummyWidget;

    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct DummyConfig;

    impl crate::dashboard::widgets::Widget for DummyWidget {
        fn render(
            &mut self,
            _ui: &mut eframe::egui::Ui,
            _ctx: &crate::dashboard::dashboard::DashboardContext<'_>,
            _activation: crate::dashboard::dashboard::WidgetActivation,
        ) -> Option<crate::dashboard::widgets::WidgetAction> {
            None
        }
    }

    fn test_registry() -> WidgetRegistry {
        let mut reg = WidgetRegistry::default();
        reg.register(
            "test",
            crate::dashboard::widgets::WidgetFactory::new(|_: DummyConfig| DummyWidget),
        );
        reg
    }

    #[test]
    fn clamps_out_of_bounds() {
        let cfg = DashboardConfig {
            version: 1,
            grid: crate::dashboard::config::GridConfig { rows: 2, cols: 2 },
            slots: vec![SlotConfig {
                id: None,
                widget: "test".into(),
                row: 0,
                col: 0,
                row_span: 5,
                col_span: 5,
                settings: json!({}),
                overflow: OverflowMode::Scroll,
            }],
        };
        let registry = test_registry();
        let (slots, _) = normalize_slots(&cfg, &registry);
        assert_eq!(slots[0].row_span, 2);
        assert_eq!(slots[0].col_span, 2);
    }

    #[test]
    fn prevents_overlap() {
        let cfg = DashboardConfig {
            version: 1,
            grid: crate::dashboard::config::GridConfig { rows: 2, cols: 2 },
            slots: vec![
                SlotConfig::with_widget("test", 0, 0),
                SlotConfig::with_widget("test", 0, 0),
            ],
        };
        let registry = test_registry();
        let (slots, warnings) = normalize_slots(&cfg, &registry);
        assert_eq!(slots.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn ignores_negative() {
        let cfg = DashboardConfig {
            version: 1,
            grid: crate::dashboard::config::GridConfig { rows: 2, cols: 2 },
            slots: vec![SlotConfig::with_widget("test", -1, 0)],
        };
        let registry = test_registry();
        let (slots, warnings) = normalize_slots(&cfg, &registry);
        assert!(slots.is_empty());
        assert_eq!(warnings.len(), 1);
    }
}
