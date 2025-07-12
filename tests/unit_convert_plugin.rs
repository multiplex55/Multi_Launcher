use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::unit_convert::UnitConvertPlugin;

#[test]
fn km_to_mi() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 km to mi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 km = 0.6214 mi");
    assert_eq!(results[0].action, "clipboard:0.6214");
}

#[test]
fn f_to_c() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 32 f to c");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "32 f = 0.0000 c");
    assert_eq!(results[0].action, "clipboard:0.0000");
}

#[test]
fn cm_to_in() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 100 cm to in");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "100 cm = 39.3701 in");
    assert_eq!(results[0].action, "clipboard:39.3701");
}

#[test]
fn l_to_gal() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 l to gal");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 l = 0.2642 gal");
    assert_eq!(results[0].action, "clipboard:0.2642");
}
