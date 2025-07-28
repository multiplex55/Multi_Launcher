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

#[test]
fn kwh_to_j() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 kwh to j");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 kwh = 3600000.0000 j");
    assert_eq!(results[0].action, "clipboard:3600000.0000");
}

#[test]
fn kw_to_w() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 2 kw to w");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "2 kw = 2000.0000 w");
    assert_eq!(results[0].action, "clipboard:2000.0000");
}

#[test]
fn bit_to_byte() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 8 bit to byte");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "8 bit = 1.0000 byte");
    assert_eq!(results[0].action, "clipboard:1.0000");
}

#[test]
fn h_to_min() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 2 h to min");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "2 h = 120.0000 min");
    assert_eq!(results[0].action, "clipboard:120.0000");
}

#[test]
fn mpg_to_kpl() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 30 mpg to kpl");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "30 mpg = 12.7543 kpl");
    assert_eq!(results[0].action, "clipboard:12.7543");
}

#[test]
fn deg_to_rad() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 180 deg to rad");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "180 deg = 3.1416 rad");
    assert_eq!(results[0].action, "clipboard:3.1416");
}
