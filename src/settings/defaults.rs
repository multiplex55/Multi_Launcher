use std::path::PathBuf;

pub fn default_note_graph_max_nodes() -> usize {
    220
}
pub fn default_note_graph_label_zoom_threshold() -> f32 {
    0.55
}
pub fn default_note_graph_layout_iterations_per_frame() -> usize {
    2
}
pub fn default_note_graph_repulsion_strength() -> f32 {
    3000.0
}
pub fn default_note_graph_link_distance() -> f32 {
    60.0
}
pub fn default_note_graph_local_graph_depth() -> usize {
    1
}
pub fn default_alpha() -> u8 {
    255
}
pub fn default_toasts() -> bool {
    true
}
pub fn default_toast_duration() -> f32 {
    3.0
}
pub fn default_scale() -> Option<f32> {
    Some(1.0)
}
pub fn default_history_limit() -> usize {
    100
}
pub fn default_clipboard_limit() -> usize {
    20
}
pub fn default_fuzzy_weight() -> f32 {
    1.0
}
pub fn default_usage_weight() -> f32 {
    1.0
}
pub fn default_query_autocomplete() -> bool {
    true
}
pub fn default_page_jump() -> usize {
    5
}
pub fn default_true() -> bool {
    true
}
pub fn default_follow_mouse() -> bool {
    true
}
pub fn default_always_on_top() -> bool {
    true
}
pub fn default_timer_refresh() -> f32 {
    1.0
}
pub fn default_net_refresh() -> f32 {
    1.0
}
pub fn default_dashboard_enabled() -> bool {
    true
}
pub fn default_show_dashboard_when_empty() -> bool {
    true
}
pub fn default_note_panel_size() -> (f32, f32) {
    (420.0, 320.0)
}
pub fn default_note_save_on_close() -> bool {
    false
}
pub fn default_note_show_details() -> bool {
    false
}
pub fn default_note_more_limit() -> usize {
    5
}
pub fn default_query_results_layout_rows() -> usize {
    3
}
pub fn default_query_results_layout_cols() -> usize {
    2
}

pub fn default_log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("launcher.log")))
        .unwrap_or_else(|| PathBuf::from("launcher.log"))
}

pub fn default_launcher_hotkey() -> Option<String> {
    if std::env::var("ML_DEFAULT_HOTKEY_NONE").is_ok() {
        None
    } else {
        Some("F2".into())
    }
}

pub fn default_multi_manager_workspaces_path() -> String {
    "multi_manager_workspaces.json".into()
}
pub fn default_multi_manager_bindings_path() -> String {
    "multi_manager_bindings.json".into()
}
pub fn default_multi_manager_hotkey_poll_ms() -> u64 {
    50
}
