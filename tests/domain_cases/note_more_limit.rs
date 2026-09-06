use multi_launcher::settings::Settings;

#[test]
fn default_note_more_limit_is_five() {
    assert_eq!(Settings::default().note_more_limit, 5);
}
