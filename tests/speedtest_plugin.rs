use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::speedtest::SpeedTestPlugin;
use std::fs::write;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

#[test]
fn search_runs_speedtest() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    write(
        tmp.path(),
        b"#!/bin/sh\necho 'Download: 12.3 Mbit/s'\necho 'Upload: 4.5 Mbit/s'\n",
    )
    .unwrap();
    let mut perms = tmp.as_file().metadata().unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(tmp.path(), perms).unwrap();

    std::env::set_var("ML_SPEEDTEST_CMD", tmp.path());

    let plugin = SpeedTestPlugin::default();
    // Spawn test
    plugin.search("speed");
    std::thread::sleep(Duration::from_millis(200));
    let results = plugin.search("speed");
    assert_eq!(results.len(), 1);
    assert_ne!(results[0].label, "Running speed test...");
}
