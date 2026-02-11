use multi_launcher::settings::{NoteGraphSettings, Settings};

#[test]
fn note_graph_settings_defaults_are_populated() {
    let cfg = NoteGraphSettings::default();
    assert_eq!(cfg.max_nodes, 220);
    assert!(cfg.show_labels);
    assert_eq!(cfg.label_zoom_threshold, 0.55);
    assert_eq!(cfg.layout_iterations_per_frame, 2);
    assert_eq!(cfg.repulsion_strength, 3000.0);
    assert_eq!(cfg.link_distance, 60.0);
    assert_eq!(cfg.local_graph_depth, 1);
    assert!(cfg.include_tags.is_empty());
    assert!(cfg.exclude_tags.is_empty());
}

#[test]
fn settings_deserialize_backward_compatible_for_note_graph_fields() {
    let json = r#"{
        "hotkey": "F2",
        "note_graph": {
            "max_nodes": 333
        }
    }"#;

    let settings: Settings = serde_json::from_str(json).expect("settings should deserialize");
    assert_eq!(settings.note_graph.max_nodes, 333);
    assert!(settings.note_graph.show_labels);
    assert_eq!(settings.note_graph.label_zoom_threshold, 0.55);
    assert_eq!(settings.note_graph.layout_iterations_per_frame, 2);
    assert_eq!(settings.note_graph.repulsion_strength, 3000.0);
    assert_eq!(settings.note_graph.link_distance, 60.0);
    assert_eq!(settings.note_graph.local_graph_depth, 1);
    assert!(settings.note_graph.include_tags.is_empty());
    assert!(settings.note_graph.exclude_tags.is_empty());
}
