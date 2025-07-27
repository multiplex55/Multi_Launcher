use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::volume::VolumePlugin;

#[test]
fn search_set_zero() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol 0");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "volume:set:0");
    } else {
        assert!(results.is_empty());
    }
}

#[test]
fn search_set_fifty() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol 50");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "volume:set:50");
    } else {
        assert!(results.is_empty());
    }
}

#[test]
fn search_mute_active() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol ma");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "volume:mute_active");
    } else {
        assert!(results.is_empty());
    }
}

#[test]
fn search_plain_vol() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "volume:dialog");
    } else {
        assert!(results.is_empty());
    }
}

#[test]
fn search_pid_level() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol pid 42 30");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "volume:pid:42:30");
    } else {
        assert!(results.is_empty());
    }
}

#[test]
fn search_name_level_missing() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol name definitely_not_real.exe 20");
    assert!(results.is_empty());
}
