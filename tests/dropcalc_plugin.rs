use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::dropcalc::DropCalcPlugin;

#[test]
fn half_twice() {
    let plugin = DropCalcPlugin;
    let results = plugin.search("drop 1/2 2");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "75.00% chance after 2 tries");
    assert_eq!(results[0].action, "calc:75.00");
}
