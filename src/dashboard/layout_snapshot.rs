//! Read-only dashboard configuration and normalized slots shared by frontends.
use crate::dashboard::config::DashboardConfig;
use crate::dashboard::layout::{NormalizedSlot, normalize_slots};
use crate::dashboard::widgets::WidgetRegistry;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardLayout {
    pub config: DashboardConfig,
    pub slots: Vec<NormalizedSlot>,
    pub warnings: Vec<String>,
}

impl DashboardLayout {
    /// Read and normalize without creating or rewriting the document.
    pub fn load(path: &Path, registry: &WidgetRegistry) -> anyhow::Result<Self> {
        let config = DashboardConfig::load(path, registry)?;
        Ok(Self::normalize(config, registry))
    }

    fn normalize(config: DashboardConfig, registry: &WidgetRegistry) -> Self {
        let (slots, mut warnings) = normalize_slots(&config, registry);
        if slots.is_empty() {
            warnings.push("dashboard has no valid slots".into());
        }
        Self { config, slots, warnings }
    }

    /// Startup falls back to stock defaults when a document is invalid.
    pub fn initial(path: &Path, registry: &WidgetRegistry) -> Self {
        Self::load(path, registry).unwrap_or_else(|error| {
            let config = DashboardConfig::default();
            let (slots, mut warnings) = normalize_slots(&config, registry);
            warnings.insert(0, format!("failed to load dashboard config: {error}"));
            Self { config, slots, warnings }
        })
    }

    /// A removed or invalid document must not replace a previously loaded layout.
    pub fn reload(path: &Path, registry: &WidgetRegistry) -> anyhow::Result<Self> {
        let config = DashboardConfig::load_for_reload(path, registry)
            .map_err(|error| anyhow::anyhow!("failed to load dashboard config: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("dashboard config {} was removed; retaining last-good state", path.display()))?;
        Ok(Self::normalize(config, registry))
    }
}

#[cfg(test)]
mod tests {
    use super::DashboardLayout;
    use crate::dashboard::widgets::WidgetRegistry;

    #[test]
    fn layout_reads_defaults_and_legacy_without_materializing_or_replacing_bad_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        let registry = WidgetRegistry::with_defaults();
        let missing = DashboardLayout::initial(&path, &registry);
        assert!(!path.exists());
        assert!(missing.slots.iter().any(|slot| slot.widget == "timers"));
        assert!(DashboardLayout::reload(&path, &registry).is_err());
        std::fs::write(&path, b" ").unwrap();
        assert_eq!(DashboardLayout::initial(&path, &registry), missing);
        assert_eq!(DashboardLayout::reload(&path, &registry).unwrap(), missing);
        let legacy = br#"{"version":1,"slots":[{"widget":"active_timers","row":0,"col":0}]}"#;
        std::fs::write(&path, legacy).unwrap();
        let layout = DashboardLayout::load(&path, &registry).unwrap();
        assert_eq!(layout.slots.len(), 1);
        assert_eq!(layout.slots[0].widget, "timers");
        assert_eq!(std::fs::read(&path).unwrap(), legacy);
        std::fs::write(&path, b"{bad").unwrap();
        assert!(DashboardLayout::reload(&path, &registry).is_err());
        assert!(!DashboardLayout::initial(&path, &registry).warnings.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"{bad");
    }
}
