use multi_launcher::settings::Settings;

#[test]
fn default_query_autocomplete_enabled() {
    assert!(Settings::default().query_autocomplete);
}
