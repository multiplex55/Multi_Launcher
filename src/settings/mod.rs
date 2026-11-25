pub mod defaults;
pub mod io;
pub mod model;
pub mod store;
pub mod utility_dialog_sizes;
pub mod utility_presentations;

pub fn normalize_static_window_config(
    follow_mouse: bool,
    static_location_enabled: bool,
    static_pos: Option<(i32, i32)>,
    static_size: Option<(i32, i32)>,
) -> (bool, Option<(i32, i32)>, Option<(i32, i32)>) {
    if follow_mouse { (false, None, None) } else { (static_location_enabled, static_pos, static_size) }
}

pub use defaults::*;
pub use io::{set_settings_path, settings_path};
pub use model::*;
pub(crate) use store::LauncherClosePreferences;
pub use store::{
    SearchCapabilityError, SearchCapabilitySaveAcknowledgement, SearchCapabilitySaveRequest,
    SearchCapabilitySnapshot, provider_capability_enabled, provider_search_capability_enabled,
    SettingsBooleanPreference, SettingsBooleanSaveAcknowledgement, SettingsBooleanSaveError,
    SettingsBooleanSaveRequest, SettingsBooleanSaveToken,
};
pub use utility_dialog_sizes::{
    AppliedWindowSize, NativeWindowGeometry, ObservedWindowSize, PreferredWindowSize,
    SavedWindowPosition, UtilityDialogSizes, UtilitySizeAcknowledgement, UtilitySizePreference,
    UtilitySizeRevision, UtilityToolKey,
};
pub use utility_presentations::{RememberedPresentation, UtilityPresentations};

pub use crate::launcher::jump_mode::config::{
    HintAlphabet, HintAlphabetError, JumpChordError, JumpModeChord,
};
