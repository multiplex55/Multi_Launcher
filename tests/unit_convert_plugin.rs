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

#[test]
fn kg_to_lb() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 kg to lb");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 kg = 2.2046 lb");
    assert_eq!(results[0].action, "clipboard:2.2046");
}

#[test]
fn sq_m_to_sq_ft() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 sq_m to sq_ft");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 sq_m = 10.7639 sq_ft");
    assert_eq!(results[0].action, "clipboard:10.7639");
}

#[test]
fn kph_to_mph() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 100 kph to mph");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "100 kph = 62.1373 mph");
    assert_eq!(results[0].action, "clipboard:62.1373");
}

#[test]
fn bar_to_psi() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 bar to psi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 bar = 14.5038 psi");
    assert_eq!(results[0].action, "clipboard:14.5038");
}

#[test]
fn handles_empty_query() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv");
    assert!(results.is_empty());
}

#[test]
fn handles_invalid_input() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv foo");
    assert!(results.is_empty());
}
