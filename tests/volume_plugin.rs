use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::volume::VolumePlugin;

#[test]
fn search_set_zero() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol 0");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "volume:set:0");
}

#[test]
fn search_set_fifty() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol 50");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "volume:set:50");
}

#[test]
fn search_mute_active() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol ma");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "volume:mute_active");
}

#[test]
fn search_plain_vol() {
    let plugin = VolumePlugin;
    let results = plugin.search("vol");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "volume:dialog");
}
