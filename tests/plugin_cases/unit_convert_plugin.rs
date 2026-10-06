use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::base_convert::BaseConvertPlugin;
use multi_launcher::plugins::unit_convert::UnitConvertPlugin;

#[test]
fn km_to_mi() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 km to mi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 km = 0.621371 mi");
    assert_eq!(results[0].action, "clipboard:0.621371 mi");
}

#[test]
fn f_to_c() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 32 f to c");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "32 f = 0 c");
    assert_eq!(results[0].action, "clipboard:0 c");
}

#[test]
fn cm_to_in() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 100 cm to in");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "100 cm = 39.3701 in");
    assert_eq!(results[0].action, "clipboard:39.3701 in");
}

#[test]
fn l_to_gal() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 l to gal");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 l = 0.264172 gal");
    assert_eq!(results[0].action, "clipboard:0.264172 gal");
}

#[test]
fn kwh_to_j() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 kwh to j");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 kwh = 3600000 j");
    assert_eq!(results[0].action, "clipboard:3600000 j");
}

#[test]
fn kw_to_w() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 2 kw to w");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "2 kw = 2000 w");
    assert_eq!(results[0].action, "clipboard:2000 w");
}

#[test]
fn bit_to_byte() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 8 bit to byte");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "8 bit = 1 byte");
    assert_eq!(results[0].action, "clipboard:1 byte");
}

#[test]
fn h_to_min() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 2 h to min");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "2 h = 120 min");
    assert_eq!(results[0].action, "clipboard:120 min");
}

#[test]
fn mpg_to_kpl() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 30 mpg to kpl");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "30 mpg = 12.7543 kpl");
    assert_eq!(results[0].action, "clipboard:12.7543 kpl");
}

#[test]
fn deg_to_rad() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 180 deg to rad");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "180 deg = 3.14159 rad");
    assert_eq!(results[0].action, "clipboard:3.14159 rad");
}

#[test]
fn kg_to_lb() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 kg to lb");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 kg = 2.20462 lb");
    assert_eq!(results[0].action, "clipboard:2.20462 lb");
}

#[test]
fn sq_m_to_sq_ft() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 sq_m to sq_ft");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 sq_m = 10.7639 sq_ft");
    assert_eq!(results[0].action, "clipboard:10.7639 sq_ft");
}

#[test]
fn kph_to_mph() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 100 kph to mph");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "100 kph = 62.1371 mph");
    assert_eq!(results[0].action, "clipboard:62.1371 mph");
}

#[test]
fn bar_to_psi() {
    let plugin = UnitConvertPlugin;
    let results = plugin.search("conv 1 bar to psi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1 bar = 14.5038 psi");
    assert_eq!(results[0].action, "clipboard:14.5038 psi");
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

#[test]
fn supports_fractional_and_compound_sources() {
    let plugin = UnitConvertPlugin;
    let fraction = plugin.search("conv 1/2 cup to ml");
    assert_eq!(fraction.len(), 1);
    assert_eq!(fraction[0].label, "1/2 cup = 118.294 ml");
    assert_eq!(fraction[0].action, "clipboard:118.294 ml");

    let compound = plugin.search("conv 6 ft 2 in to cm");
    assert_eq!(compound.len(), 1);
    assert_eq!(compound[0].label, "6 ft 2 in = 187.96 cm");
    assert_eq!(compound[0].action, "clipboard:187.96 cm");
}

#[test]
fn supports_expanded_units_and_case_sensitive_data_units() {
    let plugin = UnitConvertPlugin;
    let square = plugin.search("conv 100 m^2 to ft^2");
    assert_eq!(square[0].label, "100 m^2 = 1076.39 ft^2");
    assert_eq!(square[0].action, "clipboard:1076.39 ft^2");

    let imperial = plugin.search("conv 1 imperial gallon to l");
    assert_eq!(imperial[0].label, "1 imperial gallon = 4.54609 l");

    let data = plugin.search("conv 1 MB to Mb");
    assert_eq!(data[0].label, "1 MB = 8 Mb");
    assert_eq!(data[0].action, "clipboard:8 Mb");

    let rate = plugin.search("conv 100 MB/s to Mbps");
    assert_eq!(rate[0].label, "100 MB/s = 800 Mbps");
    assert_eq!(rate[0].action, "clipboard:800 Mbps");

    let torque = plugin.search("conv 1 Nm to lb-ft");
    assert_eq!(torque[0].label, "1 Nm = 0.737562 lb-ft");
    assert_eq!(torque[0].action, "clipboard:0.737562 lb-ft");

    let frequency = plugin.search("conv 60 hz to khz");
    assert_eq!(frequency[0].label, "60 hz = 0.06 khz");
}

#[test]
fn preserves_approximation_information_and_smart_formatting() {
    let plugin = UnitConvertPlugin;
    let month = plugin.search("conv 1 month to day");
    assert_eq!(month.len(), 1);
    assert_eq!(month[0].label, "1 month = 30 day");
    assert_eq!(month[0].desc, "Approximate unit conversion");

    let ordinary = plugin.search("convert 100 cm to m");
    assert_eq!(ordinary[0].label, "100 cm = 1 m");
    assert_eq!(ordinary[0].action, "clipboard:1 m");
}

#[test]
fn reports_physical_errors_but_stays_silent_without_unit_intent() {
    let plugin = UnitConvertPlugin;
    let unknown_source = plugin.search("conv 3 foobar to cm");
    assert_eq!(unknown_source.len(), 1);
    assert_eq!(unknown_source[0].label, "Unknown unit: foobar");
    assert_eq!(unknown_source[0].action, "noop:Unknown unit: foobar");

    let unknown_destination = plugin.search("conv 3 km to foobar");
    assert_eq!(unknown_destination.len(), 1);
    assert_eq!(unknown_destination[0].label, "Unknown unit: foobar");

    let incompatible = plugin.search("conv 3 km to kg");
    assert_eq!(incompatible[0].label, "Cannot convert length to mass");
    assert!(incompatible[0].action.starts_with("noop:"));

    let malformed = plugin.search("conv 1/0 km to cm");
    assert_eq!(malformed.len(), 1);
    assert!(malformed[0].label.starts_with("Invalid number:"));
    assert!(malformed[0].action.starts_with("noop:"));

    assert!(plugin.search("conv hello world to foo").is_empty());
}

#[test]
fn valid_base_queries_do_not_receive_physical_conversion_results_or_errors() {
    let unit = UnitConvertPlugin;
    let base = BaseConvertPlugin;

    for query in [
        "conv ff hex to dec",
        "conv 1010 bin to dec",
        "conv 17 oct to dec",
        "conv \"m\" text to hex",
        "conv \"mile marker\" text to hex",
    ] {
        assert!(
            unit.search(query).is_empty(),
            "physical result for {query:?}"
        );
        assert_eq!(base.search(query).len(), 1, "base result for {query:?}");
    }
}

#[test]
fn arbitrary_unicode_queries_do_not_panic_during_prefix_matching() {
    let plugin = UnitConvertPlugin;
    assert!(plugin.search("½").is_empty());
    assert!(plugin.search("½conv 1 km to m").is_empty());
}
