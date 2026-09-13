use crate::universal_actions::PersistedUniversalActionRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const RADIAL_FILE: &str = "radial.json";
pub const RADIAL_ASSETS_DIRECTORY: &str = "radial_assets";

pub mod limits {
    pub const MAX_MENUS: usize = 256;
    pub const MAX_RINGS_PER_MENU: usize = 16;
    pub const MAX_CELLS_PER_RING: usize = 128;
    pub const MAX_TOTAL_CELLS: usize = 8_192;
    pub const MAX_SUBMENU_DEPTH: usize = 16;
    pub const MAX_SKINS: usize = 128;
    pub const MAX_CONTEXT_RULES: usize = 512;
    pub const MAX_CUSTOM_TRIGGERS: usize = 128;
    pub const MAX_TEXTURE_DIMENSION: u32 = 8_192;
    pub const MAX_TEXTURE_BYTES: u64 = 128 * 1024 * 1024;
    pub const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;
    pub const MAX_QUEUED_COMMANDS: usize = 256;
}

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

stable_id!(MenuId);
stable_id!(RingId);
stable_id!(CellId);
stable_id!(SkinId);
stable_id!(ContextRuleId);
stable_id!(TriggerId);
stable_id!(SessionId);

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ConfigRevision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvocationId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RadialFeatureSettings {
    pub enabled: bool,
    pub shared_tap_hold: bool,
    pub hold_threshold_ms: u64,
}

impl Default for RadialFeatureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            shared_tap_hold: true,
            hold_threshold_ms: 350,
        }
    }
}

impl RadialFeatureSettings {
    pub fn hold_threshold(&self) -> Duration {
        Duration::from_millis(self.hold_threshold_ms)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    StickyClick,
    ReleaseToSelect,
    HoldAndClick,
}

impl Default for InteractionMode {
    fn default() -> Self {
        Self::StickyClick
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    CircularCells,
    Wedges,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmenuPresentation {
    Cascade,
    SameCenter,
}

impl Default for SubmenuPresentation {
    fn default() -> Self {
        Self::Cascade
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterActionPolicy {
    Inherit,
    KeepOpen,
    CloseCurrentMenu,
    CloseTree,
}

impl Default for AfterActionPolicy {
    fn default() -> Self {
        Self::Inherit
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Override<T> {
    Inherit,
    Value(T),
    Clear,
}

impl<T> Default for Override<T> {
    fn default() -> Self {
        Self::Inherit
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSelector {
    CapturedForeground,
    UnderPointer,
    LastExternal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionBinding {
    Persisted {
        action: PersistedUniversalActionRef,
    },
    Contextual {
        selector: TargetSelector,
        action_id: crate::universal_actions::ActionId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DynamicSource {
    LauncherResults { max_items: usize },
    Favorites,
    RecentItems,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Back,
    Close,
    NextPage,
    PreviousPage,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellContent {
    Action { binding: ActionBinding },
    Submenu { menu_id: MenuId },
    Dynamic { source: DynamicSource },
    Spacer,
    Control { control: Control },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickGesture {
    Primary,
    Secondary,
    CtrlPrimary,
    ShiftPrimary,
    AltPrimary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClickBinding {
    pub gesture: ClickGesture,
    pub action: ActionBinding,
    #[serde(default)]
    pub after_action: AfterActionPolicy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellDefinition {
    pub id: CellId,
    pub label: String,
    pub content: CellContent,
    #[serde(default)]
    pub alternate_clicks: Vec<ClickBinding>,
    #[serde(default)]
    pub after_action: AfterActionPolicy,
    #[serde(default)]
    pub icon: Override<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RingDefinition {
    pub id: RingId,
    pub radius: f32,
    pub cell_radius: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub gap: f32,
    pub cells: Vec<CellDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MenuDefinition {
    pub id: MenuId,
    pub name: String,
    pub layout: LayoutKind,
    pub interaction: InteractionMode,
    #[serde(default)]
    pub submenu_presentation: SubmenuPresentation,
    pub skin_id: SkinId,
    pub center_radius: f32,
    pub rings: Vec<RingDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkinDefinition {
    pub id: SkinId,
    pub name: String,
    pub scale: f32,
    #[serde(default)]
    pub enable_glow: Override<bool>,
    #[serde(default)]
    pub center_image: Override<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerDefinition {
    pub id: TriggerId,
    pub chord: String,
    pub menu_id: MenuId,
    pub scope: TriggerScope,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerScope {
    #[default]
    MenuLocal,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRule {
    pub id: ContextRuleId,
    pub enabled: bool,
    pub priority: i32,
    pub process_name: Option<String>,
    pub menu_id: MenuId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadialDocument {
    pub schema_version: u32,
    pub revision: ConfigRevision,
    pub default_menu_id: MenuId,
    pub menus: Vec<MenuDefinition>,
    pub skins: Vec<SkinDefinition>,
    #[serde(default)]
    pub context_rules: Vec<ContextRule>,
    #[serde(default)]
    pub custom_triggers: Vec<TriggerDefinition>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl Default for RadialDocument {
    fn default() -> Self {
        Self::starter()
    }
}

impl RadialDocument {
    pub fn starter() -> Self {
        let menu_id = MenuId::new("starter");
        let skin_id = SkinId::new("carbon");
        let cells = [
            ("favorites", "Favorites", DynamicSource::Favorites),
            ("recent", "Recent", DynamicSource::RecentItems),
            (
                "results",
                "Launcher results",
                DynamicSource::LauncherResults { max_items: 12 },
            ),
        ]
        .into_iter()
        .map(|(id, label, source)| CellDefinition {
            id: CellId::new(id),
            label: label.into(),
            content: CellContent::Dynamic { source },
            alternate_clicks: Vec::new(),
            after_action: AfterActionPolicy::Inherit,
            icon: Override::Inherit,
        })
        .collect();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: ConfigRevision(1),
            default_menu_id: menu_id.clone(),
            menus: vec![MenuDefinition {
                id: menu_id,
                name: "Starter".into(),
                layout: LayoutKind::CircularCells,
                interaction: InteractionMode::StickyClick,
                submenu_presentation: SubmenuPresentation::Cascade,
                skin_id: skin_id.clone(),
                center_radius: 30.0,
                rings: vec![RingDefinition {
                    id: RingId::new("main"),
                    radius: 92.0,
                    cell_radius: 28.0,
                    rotation_degrees: -90.0,
                    gap: 4.0,
                    cells,
                }],
            }],
            skins: vec![SkinDefinition {
                id: skin_id,
                name: "Carbon".into(),
                scale: 1.0,
                enable_glow: Override::Value(true),
                center_image: Override::Clear,
            }],
            context_rules: Vec::new(),
            custom_triggers: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_is_versioned_and_uses_stable_references() {
        let document = RadialDocument::starter();
        assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(
            document
                .menus
                .iter()
                .any(|menu| menu.id == document.default_menu_id)
        );
        assert_eq!(document.menus[0].skin_id, document.skins[0].id);
    }

    #[test]
    fn serde_preserves_false_zero_clear_and_inheritance() {
        let mut settings = RadialFeatureSettings::default();
        settings.enabled = false;
        settings.hold_threshold_ms = 0;
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["enabled"], false);
        assert_eq!(json["hold_threshold_ms"], 0);
        let overrides = vec![
            Override::<String>::Inherit,
            Override::Clear,
            Override::Value(String::new()),
        ];
        assert_eq!(
            serde_json::from_value::<Vec<Override<String>>>(
                serde_json::to_value(&overrides).unwrap()
            )
            .unwrap(),
            overrides
        );
    }
}
