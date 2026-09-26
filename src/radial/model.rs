use crate::universal_actions::PersistedUniversalActionRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

pub const CURRENT_SCHEMA_VERSION: u32 = 3;
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
    pub const MAX_ASSETS: usize = 2_048;
    pub const MAX_MEDIA_SEARCH_ROOTS: usize = 32;
    pub const MAX_ITEM_SHORTCUTS_PER_CELL: usize = 16;
    pub const MAX_ITEM_HOTSTRINGS_PER_CELL: usize = 16;
    pub const MAX_SAVED_LAUNCHER_QUERY_BYTES: usize = 512;
    pub const MAX_EXACT_COMMAND_BYTES: usize = 4_096;
    pub const MAX_EXACT_COMMAND_ARGS_BYTES: usize = 4_096;
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
stable_id!(AssetId);
stable_id!(ContextRuleId);
stable_id!(TriggerId);
stable_id!(ShortcutId);
stable_id!(HotstringId);
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
    /// `None` retains the document-owned default used by legacy settings.
    pub default_menu_id: Option<MenuId>,
    pub default_interaction: InteractionMode,
    /// Legacy settings which omit this field retain Cascade. Rust-created new
    /// settings use SameCenter for newly created menus.
    #[serde(default = "legacy_submenu_presentation")]
    pub default_submenu_presentation: SubmenuPresentation,
    pub safety_policy: RadialSafetyPolicy,
    pub default_item_input_scope: TriggerScope,
    /// Global item shortcuts/hotstrings are inert unless the user explicitly
    /// opts into their process-wide ownership. Menu-local inputs do not need
    /// this opt-in because they are admitted only for the current menu frame.
    pub global_item_inputs: bool,
    /// Controls whether the complete cell label is shown on ordinary hover.
    pub tooltip_scope: TooltipScope,
    /// Delay before a stationary eligible hover reveals its tooltip.
    pub tooltip_delay_ms: u64,
    /// Retain expected layout diagnostics such as label ellipsis in the
    /// collapsed radial diagnostics view.
    pub show_expected_layout_diagnostics: bool,
}

impl Default for RadialFeatureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            shared_tap_hold: true,
            hold_threshold_ms: 350,
            default_menu_id: None,
            default_interaction: InteractionMode::StickyClick,
            default_submenu_presentation: SubmenuPresentation::SameCenter,
            safety_policy: RadialSafetyPolicy::InheritLauncher,
            default_item_input_scope: TriggerScope::MenuLocal,
            global_item_inputs: false,
            tooltip_scope: TooltipScope::AllCells,
            tooltip_delay_ms: 300,
            show_expected_layout_diagnostics: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TooltipScope {
    Off,
    TruncatedOnly,
    #[default]
    AllCells,
}

impl RadialFeatureSettings {
    pub fn hold_threshold(&self) -> Duration {
        Duration::from_millis(self.hold_threshold_ms)
    }

    pub fn effective_default_menu_id(&self, document: &RadialDocument) -> MenuId {
        self.default_menu_id
            .clone()
            .unwrap_or_else(|| document.default_menu_id.clone())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialSafetyPolicy {
    /// Preserve the pre-settings behavior controlled by the launcher's
    /// destructive-action confirmation option.
    #[default]
    InheritLauncher,
    AlwaysConfirmDestructive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

fn legacy_submenu_presentation() -> SubmenuPresentation {
    SubmenuPresentation::Cascade
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Sound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaReference {
    Managed { asset_id: AssetId },
    ExternalFile { path: String },
    SearchPath { file_name: String },
    IconResource { path: String, index: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRecord {
    pub id: AssetId,
    pub kind: MediaKind,
    /// Portable path relative to the application-owned radial asset directory.
    pub relative_path: String,
    pub content_sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaSearchRoots {
    pub image_directories: Vec<String>,
    pub sound_directories: Vec<String>,
    pub search_windows_media_for_sounds: bool,
}

impl Default for MediaSearchRoots {
    fn default() -> Self {
        Self {
            image_directories: Vec::new(),
            sound_directories: Vec::new(),
            search_windows_media_for_sounds: true,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ColorRgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Offset2D {
    pub x: f32,
    pub y: f32,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RenderingQuality {
    Fast,
    #[default]
    Balanced,
    HighQuality,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TooltipMode {
    #[default]
    Disabled,
    Explicit,
    Automatic,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageStyleOverrides {
    pub item_glow: Override<MediaReference>,
    pub menu_outer_rim: Override<MediaReference>,
    pub menu_background: Override<MediaReference>,
    pub item_background: Override<MediaReference>,
    pub item_foreground: Override<MediaReference>,
    pub item_shadow: Override<MediaReference>,
    pub menu_foreground: Override<MediaReference>,
    pub center_background: Override<MediaReference>,
    pub center_image: Override<MediaReference>,
    pub submenu_indicator: Override<MediaReference>,
    pub item_glow_opacity: Override<f32>,
    pub menu_outer_rim_opacity: Override<f32>,
    pub menu_background_opacity: Override<f32>,
    pub item_background_opacity: Override<f32>,
    pub item_foreground_opacity: Override<f32>,
    pub item_shadow_opacity: Override<f32>,
    pub menu_foreground_opacity: Override<f32>,
    pub center_background_opacity: Override<f32>,
    pub center_image_opacity: Override<f32>,
    pub submenu_indicator_opacity: Override<f32>,
    /// Opacity of the cell's own icon (`IconTrans` in RM4), distinct from
    /// decorative item foreground media.
    pub icon_opacity: Override<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeometryStyleOverrides {
    pub menu_scale: Override<f32>,
    pub item_size: Override<f32>,
    pub radius_scale: Override<f32>,
    pub center_size: Override<f32>,
    pub center_image_scale: Override<f32>,
    pub item_image_scale: Override<f32>,
    pub item_image_y_ratio: Override<f32>,
    pub item_background_scale: Override<f32>,
    pub item_foreground_scale: Override<f32>,
    pub item_shadow_scale: Override<f32>,
    pub menu_background_scale: Override<f32>,
    pub menu_foreground_scale: Override<f32>,
    pub center_background_scale: Override<f32>,
    pub submenu_indicator_size: Override<f32>,
    pub submenu_indicator_y_ratio: Override<f32>,
    pub outer_ring_margin: Override<f32>,
    pub outer_rim_width: Override<f32>,
    pub item_background_on_center: Override<bool>,
    pub item_background_on_items: Override<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextStyleOverrides {
    pub visible: Override<bool>,
    pub submenu_indicator_text: Override<String>,
    pub font_family: Override<String>,
    pub font_size: Override<f32>,
    pub color: Override<ColorRgba>,
    pub bold: Override<bool>,
    pub italic: Override<bool>,
    pub underline: Override<bool>,
    pub strikeout: Override<bool>,
    pub shadow_enabled: Override<bool>,
    pub shadow_color: Override<ColorRgba>,
    pub shadow_offset: Override<Offset2D>,
    pub text_box_scale: Override<f32>,
    pub vertical_ratio: Override<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EffectStyleOverrides {
    pub glow_enabled: Override<bool>,
    pub tooltip_mode: Override<TooltipMode>,
    pub menu_shadow_width: Override<f32>,
    pub menu_shadow_inner_color: Override<ColorRgba>,
    pub menu_shadow_outer_color: Override<ColorRgba>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QualityStyleOverrides {
    pub text: Override<RenderingQuality>,
    pub shape: Override<RenderingQuality>,
    pub interpolation: Override<RenderingQuality>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoundStyleOverrides {
    pub on_show: Override<MediaReference>,
    pub on_close: Override<MediaReference>,
    pub on_select: Override<MediaReference>,
    pub on_submenu_show: Override<MediaReference>,
    pub on_submenu_close: Override<MediaReference>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowStyleOverrides {
    pub always_on_top: Override<bool>,
    pub activate_on_show: Override<bool>,
    pub fill_center_hit_zone: Override<bool>,
    pub fill_item_hit_zones: Override<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StyleOverrides {
    pub images: ImageStyleOverrides,
    pub geometry: GeometryStyleOverrides,
    pub text: TextStyleOverrides,
    pub effects: EffectStyleOverrides,
    pub quality: QualityStyleOverrides,
    pub sounds: SoundStyleOverrides,
    pub window: WindowStyleOverrides,
}

macro_rules! style_layer {
    ($name:ident) => {
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub values: StyleOverrides,
        }
    };
}

style_layer!(ApplicationStyleLayer);
style_layer!(UserDefaultStyleLayer);
style_layer!(SelectedSkinStyleLayer);
style_layer!(MenuStyleLayer);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleLayerKind {
    ApplicationFallback,
    UserDefaults,
    SelectedSkin,
    Menu,
    Ring,
    Cell,
}

pub const STYLE_PRECEDENCE: [StyleLayerKind; 6] = [
    StyleLayerKind::ApplicationFallback,
    StyleLayerKind::UserDefaults,
    StyleLayerKind::SelectedSkin,
    StyleLayerKind::Menu,
    StyleLayerKind::Ring,
    StyleLayerKind::Cell,
];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RingStyleLayer {
    pub images: ItemImageStyleOverrides,
    pub geometry: ItemGeometryStyleOverrides,
    pub text: TextStyleOverrides,
    pub quality: QualityStyleOverrides,
    pub sounds: ItemSoundStyleOverrides,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CellStyleLayer {
    pub images: ItemImageStyleOverrides,
    pub geometry: ItemGeometryStyleOverrides,
    pub text: TextStyleOverrides,
    pub quality: QualityStyleOverrides,
    pub sounds: ItemSoundStyleOverrides,
}

/// Only the Radify fields documented for both menu defaults and individual
/// items are legal at ring/cell scope. Menu chrome cannot accidentally leak
/// into a serialized cell override.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemImageStyleOverrides {
    pub item_background: Override<MediaReference>,
    pub submenu_indicator: Override<MediaReference>,
    pub item_background_opacity: Override<f32>,
    pub submenu_indicator_opacity: Override<f32>,
    pub icon_opacity: Override<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemGeometryStyleOverrides {
    pub item_image_scale: Override<f32>,
    pub item_image_y_ratio: Override<f32>,
    pub submenu_indicator_size: Override<f32>,
    pub submenu_indicator_y_ratio: Override<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemSoundStyleOverrides {
    pub on_select: Override<MediaReference>,
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
    LauncherQuery {
        query: String,
        #[serde(default)]
        mode: QueryRunMode,
    },
    ExactCommand {
        command: String,
        #[serde(default)]
        args: Option<String>,
    },
}

/// How an authored single-cell launcher query is expected to behave once the
/// runtime query dispatcher is available. M2 deliberately prepares both modes
/// as deferred work; this value is persistence intent, not permission to run
/// during decoding, import, validation, or preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryRunMode {
    #[default]
    OpenLauncher,
    ExecuteFirst,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DynamicSource {
    /// Compatibility form from schema v1. It reads the invocation's explicitly
    /// supplied query and never the mutable root launcher query field.
    LauncherResults {
        max_items: usize,
    },
    LauncherQuery {
        query: String,
        max_items: usize,
    },
    Favorites,
    RecentItems,
    Clipboard,
    Snippets,
    Notes,
    Windows,
    Macros,
    Applications,
    Dashboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Back,
    Close,
    NextPage,
    PreviousPage,
    Drag,
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

/// A non-executable navigation/control intent bound to a particular pointer
/// gesture. This is distinct from `ClickBinding`: legacy Close/Back/Drag
/// literals must never be converted into callback-shaped actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlClickBinding {
    pub gesture: ClickGesture,
    pub control: Control,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellDefinition {
    pub id: CellId,
    pub label: String,
    pub content: CellContent,
    #[serde(default)]
    pub alternate_clicks: Vec<ClickBinding>,
    #[serde(default)]
    pub alternate_controls: Vec<ControlClickBinding>,
    #[serde(default)]
    pub after_action: AfterActionPolicy,
    #[serde(default)]
    pub secondary_after_action: AfterActionPolicy,
    #[serde(default)]
    pub icon: Override<MediaReference>,
    #[serde(default)]
    pub tooltip: Override<String>,
    #[serde(default)]
    pub style: CellStyleLayer,
    #[serde(default)]
    pub shortcuts: Vec<ItemShortcut>,
    #[serde(default)]
    pub hotstrings: Vec<ItemHotstring>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingDefinition {
    pub id: RingId,
    pub radius: f32,
    pub cell_radius: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub gap: f32,
    pub cells: Vec<CellDefinition>,
    #[serde(default)]
    pub style: RingStyleLayer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuDefinition {
    pub id: MenuId,
    pub name: String,
    pub layout: LayoutKind,
    pub interaction: InteractionMode,
    #[serde(default)]
    pub hover_dwell_ms: Option<u64>,
    #[serde(default)]
    pub submenu_presentation: SubmenuPresentation,
    #[serde(default)]
    pub after_action: AfterActionPolicy,
    #[serde(default)]
    pub center_action: Option<ActionBinding>,
    #[serde(default)]
    pub center_primary_after_action: AfterActionPolicy,
    #[serde(default)]
    pub center_secondary_action: Option<ActionBinding>,
    #[serde(default)]
    pub center_secondary_after_action: AfterActionPolicy,
    #[serde(default)]
    pub center_control: Option<Control>,
    #[serde(default)]
    pub center_secondary_control: Option<Control>,
    #[serde(default)]
    pub background_action: Option<ActionBinding>,
    #[serde(default)]
    pub background_primary_after_action: AfterActionPolicy,
    #[serde(default)]
    pub background_secondary_action: Option<ActionBinding>,
    #[serde(default)]
    pub background_control: Option<Control>,
    #[serde(default)]
    pub background_secondary_control: Option<Control>,
    #[serde(default)]
    pub background_secondary_after_action: AfterActionPolicy,
    #[serde(default)]
    pub mirror_primary_to_secondary: bool,
    pub skin_id: SkinId,
    pub center_radius: f32,
    pub rings: Vec<RingDefinition>,
    #[serde(default)]
    pub style: MenuStyleLayer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinDefinition {
    pub id: SkinId,
    pub name: String,
    #[serde(default)]
    pub style: SelectedSkinStyleLayer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemShortcut {
    pub id: ShortcutId,
    pub chord: String,
    pub gesture: ClickGesture,
    #[serde(default)]
    pub scope: TriggerScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemHotstring {
    pub id: HotstringId,
    pub text: String,
    pub gesture: ClickGesture,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub scope: TriggerScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerDefinition {
    pub id: TriggerId,
    pub chord: String,
    pub menu_id: MenuId,
    pub scope: TriggerScope,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
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
    #[serde(default)]
    pub window_title_contains: Option<String>,
    #[serde(default)]
    pub monitor_id: Option<String>,
    pub menu_id: MenuId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadialDocument {
    pub schema_version: u32,
    pub revision: ConfigRevision,
    pub default_menu_id: MenuId,
    #[serde(default)]
    pub after_action: AfterActionPolicy,
    pub menus: Vec<MenuDefinition>,
    pub skins: Vec<SkinDefinition>,
    #[serde(default)]
    pub user_style_defaults: UserDefaultStyleLayer,
    #[serde(default)]
    pub media_search_roots: MediaSearchRoots,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
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
        let submenu_specs = [
            ("favorites", "Favorites", DynamicSource::Favorites),
            ("applications", "Apps", DynamicSource::Applications),
            ("windows", "Windows", DynamicSource::Windows),
            ("macros", "Macros", DynamicSource::Macros),
            ("notes", "Notes", DynamicSource::Notes),
            ("snippets", "Snippets + Clipboard", DynamicSource::Snippets),
            ("dashboard", "Dashboard", DynamicSource::Dashboard),
        ];
        let mut root_cells = submenu_specs
            .iter()
            .map(|(id, label, _)| {
                starter_cell(
                    &format!("starter-root-{id}"),
                    label,
                    CellContent::Submenu {
                        menu_id: MenuId::new(format!("starter-{id}")),
                    },
                )
            })
            .collect::<Vec<_>>();
        root_cells.insert(
            6,
            starter_cell(
                "starter-root-screen-tools",
                "Screen Tools",
                CellContent::Submenu {
                    menu_id: MenuId::new("starter-screen-tools"),
                },
            ),
        );
        root_cells.push(starter_control(
            "starter-root-close",
            "Close",
            Control::Close,
        ));

        let mut menus = vec![starter_menu(
            menu_id.clone(),
            "Starter",
            skin_id.clone(),
            root_cells,
            Some(Control::Drag),
        )];
        for (id, label, source) in submenu_specs {
            let mut source_cell = starter_cell(
                &format!("starter-{id}-source"),
                label,
                CellContent::Dynamic { source },
            );
            source_cell.after_action = AfterActionPolicy::CloseTree;
            let mut cells = vec![source_cell];
            if id == "snippets" {
                let mut clipboard = starter_cell(
                    "starter-snippets-clipboard-source",
                    "Clipboard",
                    CellContent::Dynamic {
                        source: DynamicSource::Clipboard,
                    },
                );
                clipboard.after_action = AfterActionPolicy::CloseTree;
                cells.push(clipboard);
            }
            cells.extend(starter_navigation(id));
            menus.push(starter_menu(
                MenuId::new(format!("starter-{id}")),
                label,
                skin_id.clone(),
                cells,
                Some(Control::Back),
            ));
        }
        let mut screen_cells = vec![
            starter_action_cell(
                "starter-screen-tools-draw",
                "Screen Draw",
                "Screen Draw",
                "screen_draw:start",
            ),
            starter_action_cell(
                "starter-screen-tools-screenshot",
                "Screenshot region",
                "Screenshot",
                "screenshot:region",
            ),
            starter_action_cell(
                "starter-screen-tools-crop",
                "Screenshot and Clip",
                "Crop screenshot / clip image",
                "crop:screenshot",
            ),
        ];
        screen_cells.extend(starter_navigation("screen-tools"));
        menus.push(starter_menu(
            MenuId::new("starter-screen-tools"),
            "Screen Tools",
            skin_id.clone(),
            screen_cells,
            Some(Control::Back),
        ));

        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: ConfigRevision(1),
            default_menu_id: menu_id,
            after_action: AfterActionPolicy::Inherit,
            menus,
            skins: vec![SkinDefinition {
                id: skin_id,
                name: "Carbon".into(),
                style: SelectedSkinStyleLayer {
                    values: StyleOverrides {
                        images: ImageStyleOverrides {
                            center_image: Override::Clear,
                            ..Default::default()
                        },
                        geometry: GeometryStyleOverrides {
                            menu_scale: Override::Value(1.0),
                            ..Default::default()
                        },
                        effects: EffectStyleOverrides {
                            glow_enabled: Override::Value(true),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                },
            }],
            user_style_defaults: UserDefaultStyleLayer::default(),
            media_search_roots: MediaSearchRoots::default(),
            assets: Vec::new(),
            context_rules: Vec::new(),
            custom_triggers: Vec::new(),
            metadata: BTreeMap::from([(
                "starter_content".into(),
                "favorites_apps_windows_macros_notes_snippets_clipboard_screen_tools_dashboard"
                    .into(),
            )]),
        }
    }
}

fn starter_cell(id: &str, label: &str, content: CellContent) -> CellDefinition {
    CellDefinition {
        id: CellId::new(id),
        label: label.into(),
        content,
        alternate_clicks: Vec::new(),
        alternate_controls: Vec::new(),
        after_action: AfterActionPolicy::Inherit,
        secondary_after_action: AfterActionPolicy::Inherit,
        icon: Override::Inherit,
        tooltip: Override::Inherit,
        style: CellStyleLayer::default(),
        shortcuts: Vec::new(),
        hotstrings: Vec::new(),
    }
}

fn starter_control(id: &str, label: &str, control: Control) -> CellDefinition {
    starter_cell(id, label, CellContent::Control { control })
}

fn starter_navigation(prefix: &str) -> Vec<CellDefinition> {
    vec![
        starter_control(&format!("starter-{prefix}-back"), "Back", Control::Back),
        starter_control(&format!("starter-{prefix}-close"), "Close", Control::Close),
    ]
}

fn starter_action_cell(id: &str, label: &str, description: &str, command: &str) -> CellDefinition {
    let mut cell = starter_cell(
        id,
        label,
        CellContent::Action {
            binding: ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(
                        crate::universal_actions::PersistableActionTargetRef::LegacyAction {
                            action: crate::actions::Action {
                                label: label.into(),
                                desc: description.into(),
                                action: command.into(),
                                args: None,
                            },
                        },
                    ),
                    action_id: crate::universal_actions::action_ids::RESULT_EXECUTE,
                },
            },
        },
    );
    // Capture and external-input handoffs must tear down the radial before
    // the established command/executor path acquires its exclusive owner.
    cell.after_action = AfterActionPolicy::CloseTree;
    cell
}

fn starter_menu(
    id: MenuId,
    name: &str,
    skin_id: SkinId,
    cells: Vec<CellDefinition>,
    center_control: Option<Control>,
) -> MenuDefinition {
    let ring_id = RingId::new(format!("{}-main", id.as_str()));
    MenuDefinition {
        id,
        name: name.into(),
        layout: LayoutKind::CircularCells,
        interaction: InteractionMode::StickyClick,
        hover_dwell_ms: None,
        submenu_presentation: SubmenuPresentation::SameCenter,
        after_action: AfterActionPolicy::Inherit,
        center_action: None,
        center_primary_after_action: AfterActionPolicy::Inherit,
        center_secondary_action: None,
        center_secondary_after_action: AfterActionPolicy::Inherit,
        center_control,
        center_secondary_control: None,
        background_action: None,
        background_primary_after_action: AfterActionPolicy::Inherit,
        background_secondary_action: None,
        background_control: None,
        background_secondary_control: None,
        background_secondary_after_action: AfterActionPolicy::Inherit,
        mirror_primary_to_secondary: false,
        skin_id,
        center_radius: 30.0,
        rings: vec![RingDefinition {
            id: ring_id,
            radius: 92.0,
            cell_radius: 28.0,
            rotation_degrees: -90.0,
            gap: 4.0,
            cells,
            style: RingStyleLayer::default(),
        }],
        style: MenuStyleLayer::default(),
    }
}

pub fn effective_after_action(
    document: &RadialDocument,
    menu: &MenuDefinition,
    cell: AfterActionPolicy,
) -> AfterActionPolicy {
    for policy in [cell, menu.after_action, document.after_action] {
        if policy != AfterActionPolicy::Inherit {
            return policy;
        }
    }
    if menu.interaction == InteractionMode::StickyClick {
        AfterActionPolicy::KeepOpen
    } else {
        AfterActionPolicy::CloseTree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_action_inherits_cell_menu_document_then_interaction_default() {
        let mut document = RadialDocument::starter();
        let menu = &document.menus[0];
        assert_eq!(
            effective_after_action(&document, menu, AfterActionPolicy::Inherit),
            AfterActionPolicy::KeepOpen
        );
        document.after_action = AfterActionPolicy::CloseTree;
        assert_eq!(
            effective_after_action(&document, &document.menus[0], AfterActionPolicy::Inherit),
            AfterActionPolicy::CloseTree
        );
    }

    #[test]
    fn starter_is_versioned_and_uses_stable_references() {
        let document = RadialDocument::starter();
        assert_eq!(document.menus[0].center_control, Some(Control::Drag));
        assert_eq!(document.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(
            document
                .menus
                .iter()
                .any(|menu| menu.id == document.default_menu_id)
        );
        assert_eq!(document.menus[0].skin_id, document.skins[0].id);
        crate::radial::validation::validate(&document).unwrap();

        let menu_ids = document
            .menus
            .iter()
            .map(|menu| menu.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for expected in [
            "starter-favorites",
            "starter-applications",
            "starter-windows",
            "starter-macros",
            "starter-notes",
            "starter-snippets",
            "starter-screen-tools",
            "starter-dashboard",
        ] {
            assert!(
                menu_ids.contains(expected),
                "missing starter menu {expected}"
            );
        }
        let all_cells = document
            .menus
            .iter()
            .flat_map(|menu| menu.rings.iter())
            .flat_map(|ring| ring.cells.iter())
            .collect::<Vec<_>>();
        assert_eq!(
            all_cells
                .iter()
                .map(|cell| cell.id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            all_cells.len(),
            "starter cell IDs must be globally unique"
        );
        for menu in document.menus.iter().skip(1) {
            let controls = menu
                .rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .filter_map(|cell| match &cell.content {
                    CellContent::Control { control } => Some(*control),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(controls.contains(&Control::Back));
            assert!(controls.contains(&Control::Close));
        }
        let commands = all_cells
            .iter()
            .filter_map(|cell| match &cell.content {
                CellContent::Action {
                    binding:
                        ActionBinding::Persisted {
                            action:
                                PersistedUniversalActionRef {
                                    target:
                                        Some(crate::universal_actions::PersistableActionTargetRef::LegacyAction {
                                            action,
                                        }),
                                    ..
                                },
                        },
                } => Some(action.action.as_str()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            commands,
            ["crop:screenshot", "screen_draw:start", "screenshot:region"]
                .into_iter()
                .collect()
        );
        assert!(
            all_cells
                .iter()
                .filter(|cell| { matches!(cell.content, CellContent::Action { .. }) })
                .all(|cell| cell.after_action == AfterActionPolicy::CloseTree)
        );
    }

    #[test]
    fn serde_preserves_false_zero_clear_and_inheritance() {
        let mut settings = RadialFeatureSettings::default();
        settings.enabled = false;
        settings.hold_threshold_ms = 0;
        settings.default_menu_id = Some(MenuId::new("work"));
        settings.default_interaction = InteractionMode::HoldAndClick;
        settings.default_submenu_presentation = SubmenuPresentation::SameCenter;
        settings.safety_policy = RadialSafetyPolicy::AlwaysConfirmDestructive;
        settings.default_item_input_scope = TriggerScope::Global;
        settings.global_item_inputs = true;
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["enabled"], false);
        assert_eq!(json["hold_threshold_ms"], 0);
        assert_eq!(
            serde_json::from_value::<RadialFeatureSettings>(json).unwrap(),
            settings
        );
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

        let mut menu_json = serde_json::to_value(&RadialDocument::starter().menus[0]).unwrap();
        let object = menu_json.as_object_mut().unwrap();
        object.remove("center_primary_after_action");
        object.remove("background_primary_after_action");
        object.remove("submenu_presentation");
        let legacy: MenuDefinition = serde_json::from_value(menu_json).unwrap();
        assert_eq!(legacy.submenu_presentation, SubmenuPresentation::Cascade);
        assert_eq!(
            legacy.center_primary_after_action,
            AfterActionPolicy::Inherit
        );
        assert_eq!(
            legacy.background_primary_after_action,
            AfterActionPolicy::Inherit
        );
    }

    #[test]
    fn authored_query_modes_and_exact_command_arguments_round_trip() {
        let bindings = [
            ActionBinding::LauncherQuery {
                query: "  saved query  ".into(),
                mode: QueryRunMode::OpenLauncher,
            },
            ActionBinding::LauncherQuery {
                query: "type:value".into(),
                mode: QueryRunMode::ExecuteFirst,
            },
            ActionBinding::ExactCommand {
                command: "query:example".into(),
                args: None,
            },
            ActionBinding::ExactCommand {
                command: "external-tool".into(),
                args: Some("--flag value".into()),
            },
        ];
        for binding in bindings {
            let encoded = serde_json::to_vec(&binding).unwrap();
            assert_eq!(
                serde_json::from_slice::<ActionBinding>(&encoded).unwrap(),
                binding
            );
        }

        let defaulted: ActionBinding = serde_json::from_value(serde_json::json!({
            "kind": "launcher_query",
            "query": "default mode"
        }))
        .unwrap();
        assert_eq!(
            defaulted,
            ActionBinding::LauncherQuery {
                query: "default mode".into(),
                mode: QueryRunMode::OpenLauncher,
            }
        );
    }

    #[test]
    fn new_radial_defaults_are_same_center_but_legacy_missing_fields_are_cascade() {
        assert_eq!(
            RadialFeatureSettings::default().default_submenu_presentation,
            SubmenuPresentation::SameCenter
        );
        assert_eq!(
            serde_json::from_value::<RadialFeatureSettings>(serde_json::json!({}))
                .unwrap()
                .default_submenu_presentation,
            SubmenuPresentation::Cascade
        );
        assert!(
            RadialDocument::starter()
                .menus
                .iter()
                .all(|menu| menu.submenu_presentation == SubmenuPresentation::SameCenter)
        );
    }

    #[test]
    fn special_surface_button_policies_round_trip_independently() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.center_primary_after_action = AfterActionPolicy::CloseCurrentMenu;
        menu.center_secondary_after_action = AfterActionPolicy::KeepOpen;
        menu.background_primary_after_action = AfterActionPolicy::CloseTree;
        menu.background_secondary_after_action = AfterActionPolicy::CloseCurrentMenu;
        let decoded: MenuDefinition =
            serde_json::from_value(serde_json::to_value(&menu).unwrap()).unwrap();
        assert_eq!(
            decoded.center_primary_after_action,
            AfterActionPolicy::CloseCurrentMenu
        );
        assert_eq!(
            decoded.center_secondary_after_action,
            AfterActionPolicy::KeepOpen
        );
        assert_eq!(
            decoded.background_primary_after_action,
            AfterActionPolicy::CloseTree
        );
        assert_eq!(
            decoded.background_secondary_after_action,
            AfterActionPolicy::CloseCurrentMenu
        );
    }

    #[test]
    fn style_layers_and_media_preserve_inherit_clear_false_zero_and_empty() {
        let mut document = RadialDocument::starter();
        document
            .user_style_defaults
            .values
            .geometry
            .outer_ring_margin = Override::Value(0.0);
        document.user_style_defaults.values.text.visible = Override::Value(false);
        document.user_style_defaults.values.images.center_image = Override::Clear;
        document.menus[0].style.values.images.item_glow =
            Override::Value(MediaReference::SearchPath {
                file_name: "glow.png".into(),
            });
        document.menus[0].rings[0].style.text.bold = Override::Value(false);
        document.menus[0].rings[0].cells[0].tooltip = Override::Value(String::new());
        document.menus[0].rings[0].cells[0]
            .style
            .geometry
            .item_image_y_ratio = Override::Value(0.0);
        crate::radial::validation::validate(&document).unwrap();
        let decoded: RadialDocument =
            serde_json::from_value(serde_json::to_value(&document).unwrap()).unwrap();
        assert_eq!(decoded, document);
    }
}
