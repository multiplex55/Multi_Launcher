use multi_launcher::settings::NetUnit;

#[test]
fn default_is_auto() {
    assert_eq!(NetUnit::default(), NetUnit::Auto);
}

#[test]
fn format_units() {
    assert_eq!(format!("{}", NetUnit::Kb), "kB/s");
    assert_eq!(format!("{}", NetUnit::Mb), "MB/s");
}
