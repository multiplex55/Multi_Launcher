use crate::actions::Action;
use crate::plugin::Plugin;
use std::f64::consts::PI;

pub struct UnitConvertPlugin;

fn normalize_unit(unit: &str) -> Option<&'static str> {
    let u = unit.to_lowercase();
    match u.as_str() {
        // length
        "m" | "meter" | "meters" => Some("m"),
        "km" | "kilometer" | "kilometers" => Some("km"),
        "mi" | "mile" | "miles" => Some("mi"),
        "ft" | "foot" | "feet" => Some("ft"),
        "in" | "inch" | "inches" => Some("in"),
        "cm" | "centimeter" | "centimeters" => Some("cm"),
        "mm" | "millimeter" | "millimeters" => Some("mm"),
        "nm" | "nauticalmile" | "nauticalmiles" => Some("nm"),

        // mass
        "kg" | "kilogram" | "kilograms" => Some("kg"),
        "g" | "gram" | "grams" => Some("g"),
        "lb" | "pound" | "pounds" | "lbs" => Some("lb"),
        "oz" | "ounce" | "ounces" => Some("oz"),

        // temperature
        "c" | "celsius" | "centigrade" | "°c" => Some("c"),
        "f" | "fahrenheit" | "°f" => Some("f"),
        "k" | "kelvin" | "kelvins" => Some("k"),

        // volume
        "l" | "liter" | "liters" | "litre" | "litres" => Some("l"),
        "ml" | "milliliter" | "milliliters" | "millilitre" | "millilitres" => Some("ml"),
        "gal" | "gallon" | "gallons" => Some("gal"),

        // area
        "sq_m" | "m2" | "squaremeter" | "squaremeters" => Some("sq_m"),
        "sq_ft" | "ft2" | "squarefoot" | "squarefeet" => Some("sq_ft"),
        "ha" | "hectare" | "hectares" => Some("ha"),
        "ac" | "acre" | "acres" => Some("ac"),

        // speed
        "kph" | "km/h" | "kilometerperhour" => Some("kph"),
        "mph" | "mileperhour" => Some("mph"),
        "mps" | "m/s" => Some("mps"),
        "fps" | "ft/s" => Some("fps"),

        // pressure
        "atm" => Some("atm"),
        "pa" | "pascal" | "pascals" => Some("pa"),
        "bar" => Some("bar"),
        "psi" => Some("psi"),

        // energy
        "j" | "joule" | "joules" => Some("j"),
        "kj" | "kilojoule" | "kilojoules" => Some("kj"),
        "cal" | "calorie" | "calories" => Some("cal"),
        "kcal" | "kilocalorie" | "kilocalories" => Some("kcal"),
        "wh" | "watt-hour" | "watt hour" => Some("wh"),
        "kwh" | "kilowatt-hour" | "kilowatt hour" => Some("kwh"),
        "btu" | "btus" => Some("btu"),
        "ftlb" | "footpound" | "foot-pound" | "ft-lb" => Some("ftlb"),
        "ev" | "electronvolt" | "electronvolts" => Some("ev"),

        // power
        "w" | "watt" | "watts" => Some("w"),
        "kw" | "kilowatt" | "kilowatts" => Some("kw"),
        "mw" | "megawatt" | "megawatts" => Some("mw"),
        "mwatt" | "milliwatt" | "milliwatts" => Some("mwatt"),
        "hp" | "horsepower" => Some("hp"),

        // data
        "bit" | "bits" | "b" => Some("bit"),
        "byte" | "bytes" => Some("byte"),
        "kb" | "kilobyte" | "kilobytes" => Some("kb"),
        "kib" | "kibibyte" | "kibibytes" => Some("kib"),
        "kbit" | "kilobit" | "kilobits" => Some("kbit"),
        "kibit" | "kibibit" | "kibibits" => Some("kibit"),
        "mb" | "megabyte" | "megabytes" => Some("mb"),
        "mib" | "mebibyte" | "mebibytes" => Some("mib"),
        "mbit" | "megabit" | "megabits" => Some("mbit"),
        "mibit" | "mebibit" | "mebibits" => Some("mibit"),
        "gb" | "gigabyte" | "gigabytes" => Some("gb"),
        "gib" | "gibibyte" | "gibibytes" => Some("gib"),
        "gbit" | "gigabit" | "gigabits" => Some("gbit"),
        "gibit" | "gibibit" | "gibibits" => Some("gibit"),
        "tb" | "terabyte" | "terabytes" => Some("tb"),
        "tib" | "tebibyte" | "tebibytes" => Some("tib"),
        "tbit" | "terabit" | "terabits" => Some("tbit"),
        "tibit" | "tebibit" | "tebibits" => Some("tibit"),

        // time
        "ns" | "nanosecond" | "nanoseconds" => Some("ns"),
        "us" | "microsecond" | "microseconds" | "μs" => Some("us"),
        "ms" | "millisecond" | "milliseconds" => Some("ms"),
        "s" | "sec" | "second" | "seconds" => Some("s"),
        "min" | "minute" | "minutes" => Some("min"),
        "h" | "hr" | "hour" | "hours" => Some("h"),
        "day" | "days" | "d" => Some("day"),
        "week" | "weeks" | "wk" => Some("week"),
        "month" | "months" | "mo" => Some("month"),
        "year" | "years" | "yr" => Some("year"),

        // fuel economy
        "kpl" | "km/l" | "kmperliter" => Some("kpl"),
        "l/100km" | "lper100km" => Some("lp100km"),
        "mpg" | "milespergallon" | "milepergallon" => Some("mpg"),
        "mpgimp" | "mpg_uk" | "mileperimperialgallon" => Some("mpgimp"),

        // angle
        "deg" | "degree" | "degrees" => Some("deg"),
        "rad" | "radian" | "radians" => Some("rad"),
        "grad" | "gradian" | "gradians" | "gon" => Some("grad"),
        "arcmin" | "arcminute" | "arcminutes" => Some("arcmin"),
        "arcsec" | "arcsecond" | "arcseconds" => Some("arcsec"),
        "rev" | "revolution" | "revolutions" | "turn" | "turns" => Some("rev"),
        _ => None,
    }
}

fn convert(value: f64, from: &str, to: &str) -> Option<f64> {
    // Temperature (special case)
    let temp = match (from, to) {
        ("c", "f") => Some(value * 9.0 / 5.0 + 32.0),
        ("f", "c") => Some((value - 32.0) * 5.0 / 9.0),
        ("c", "k") => Some(value + 273.15),
        ("k", "c") => Some(value - 273.15),
        ("f", "k") => Some((value - 32.0) * 5.0 / 9.0 + 273.15),
        ("k", "f") => Some((value - 273.15) * 9.0 / 5.0 + 32.0),
        _ => None,
    };
    if temp.is_some() {
        return temp;
    }

    macro_rules! linear_conv {
        ($factor_fn:ident) => {
            if let (Some(f1), Some(f2)) = ($factor_fn(from), $factor_fn(to)) {
                return Some(value * f1 / f2);
            }
        };
    }

    linear_conv!(length_factor);
    linear_conv!(mass_factor);
    linear_conv!(volume_factor);
    linear_conv!(area_factor);
    linear_conv!(speed_factor);
    linear_conv!(pressure_factor);
    linear_conv!(energy_factor);
    linear_conv!(power_factor);
    linear_conv!(data_factor);
    linear_conv!(time_factor);

    // Angle has radian conversions
    linear_conv!(angle_factor);

    if let Some(res) = fuel_economy_convert(value, from, to) {
        return Some(res);
    }

    None
}

fn length_factor(u: &str) -> Option<f64> {
    match u {
        "km" => Some(1000.0),
        "m" => Some(1.0),
        "cm" => Some(0.01),
        "mm" => Some(0.001),
        "mi" => Some(1609.34),
        "ft" => Some(0.3048),
        "in" => Some(0.0254),
        "nm" => Some(1852.0),
        _ => None,
    }
}

fn mass_factor(u: &str) -> Option<f64> {
    match u {
        "kg" => Some(1.0),
        "g" => Some(0.001),
        "lb" => Some(0.453_592),
        "oz" => Some(0.028_3495),
        _ => None,
    }
}

fn volume_factor(u: &str) -> Option<f64> {
    match u {
        "l" => Some(1.0),
        "ml" => Some(0.001),
        "gal" => Some(3.785_41),
        // fluid ounce
        "oz" => Some(0.029_5735),
        _ => None,
    }
}

fn area_factor(u: &str) -> Option<f64> {
    match u {
        "sq_m" => Some(1.0),
        "sq_ft" => Some(0.092_903),
        "ha" => Some(10_000.0),
        "ac" => Some(4046.86),
        _ => None,
    }
}

fn speed_factor(u: &str) -> Option<f64> {
    match u {
        "mps" => Some(1.0),
        "kph" => Some(1000.0 / 3600.0),
        "mph" => Some(1609.34 / 3600.0),
        "fps" => Some(0.3048),
        _ => None,
    }
}

fn pressure_factor(u: &str) -> Option<f64> {
    match u {
        "pa" => Some(1.0),
        "atm" => Some(101_325.0),
        "bar" => Some(100_000.0),
        "psi" => Some(6894.757),
        _ => None,
    }
}

fn energy_factor(u: &str) -> Option<f64> {
    match u {
        "j" => Some(1.0),
        "kj" => Some(1000.0),
        "cal" => Some(4.184),
        "kcal" => Some(4184.0),
        "wh" => Some(3600.0),
        "kwh" => Some(3_600_000.0),
        "btu" => Some(1055.06),
        "ftlb" => Some(1.35582),
        "ev" => Some(1.602_18e-19),
        _ => None,
    }
}

fn power_factor(u: &str) -> Option<f64> {
    match u {
        "w" => Some(1.0),
        "kw" => Some(1000.0),
        "mw" => Some(1_000_000.0),
        "mwatt" => Some(0.001),
        "hp" => Some(745.7),
        _ => None,
    }
}

fn data_factor(u: &str) -> Option<f64> {
    match u {
        // base: byte
        "byte" => Some(1.0),
        "bit" => Some(0.125),
        "kb" => Some(1000.0),
        "kbit" => Some(125.0),
        "kib" => Some(1024.0),
        "kibit" => Some(128.0),
        "mb" => Some(1_000_000.0),
        "mbit" => Some(125_000.0),
        "mib" => Some(1_048_576.0),
        "mibit" => Some(131_072.0),
        "gb" => Some(1_000_000_000.0),
        "gbit" => Some(125_000_000.0),
        "gib" => Some(1_073_741_824.0),
        "gibit" => Some(134_217_728.0),
        "tb" => Some(1_000_000_000_000.0),
        "tbit" => Some(125_000_000_000.0),
        "tib" => Some(1_099_511_627_776.0),
        "tibit" => Some(137_438_953_472.0),
        _ => None,
    }
}

fn time_factor(u: &str) -> Option<f64> {
    match u {
        "ns" => Some(1e-9),
        "us" => Some(1e-6),
        "ms" => Some(1e-3),
        "s" => Some(1.0),
        "min" => Some(60.0),
        "h" => Some(3600.0),
        "day" => Some(86_400.0),
        "week" => Some(604_800.0),
        "month" => Some(2_592_000.0),
        "year" => Some(31_536_000.0),
        _ => None,
    }
}

fn angle_factor(u: &str) -> Option<f64> {
    match u {
        "rad" => Some(1.0),
        "deg" => Some(PI / 180.0),
        "grad" => Some(PI / 200.0),
        "arcmin" => Some(PI / 10_800.0),
        "arcsec" => Some(PI / 648_000.0),
        "rev" => Some(2.0 * PI),
        _ => None,
    }
}

fn fuel_economy_convert(value: f64, from: &str, to: &str) -> Option<f64> {
    // base: km per liter
    fn to_kpl(v: f64, u: &str) -> Option<f64> {
        match u {
            "kpl" => Some(v),
            "lp100km" => Some(100.0 / v),
            "mpg" => Some(v * 0.425_144),
            "mpgimp" => Some(v * 0.354_006),
            _ => None,
        }
    }

    fn from_kpl(kpl: f64, u: &str) -> Option<f64> {
        match u {
            "kpl" => Some(kpl),
            "lp100km" => Some(100.0 / kpl),
            "mpg" => Some(kpl / 0.425_144),
            "mpgimp" => Some(kpl / 0.354_006),
            _ => None,
        }
    }

    let kpl = to_kpl(value, from)?;
    from_kpl(kpl, to)
}

fn parse_query(query: &str) -> Option<(f64, String, String)> {
    let rest = query.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    if !parts.get(2)?.eq_ignore_ascii_case("to") {
        return None;
    }
    let val: f64 = parts.get(0)?.parse().ok()?;
    let from = normalize_unit(parts.get(1)?)?.to_string();
    let to = normalize_unit(parts.get(3)?)?.to_string();
    Some((val, from, to))
}

impl Plugin for UnitConvertPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        const CONV_PREFIX: &str = "conv ";
        const CONVERT_PREFIX: &str = "convert ";
        let rest = if let Some(r) = crate::common::strip_prefix_ci(trimmed, CONV_PREFIX) {
            r
        } else if let Some(r) = crate::common::strip_prefix_ci(trimmed, CONVERT_PREFIX) {
            r
        } else {
            return Vec::new();
        };

        if let Some((value, from, to)) = parse_query(rest) {
            if let Some(result) = convert(value, &from, &to) {
                let label = format!("{} {} = {:.4} {}", value, from, result, to);
                let action = format!("clipboard:{:.4}", result);
                return vec![Action {
                    label,
                    desc: "Unit convert".into(),
                    action,
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "unit_convert"
    }

    fn description(&self) -> &str {
        "Convert between units (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Unit convert".into(),
                action: "query:conv ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "convert".into(),
                desc: "Unit convert".into(),
                action: "query:convert ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
        ]
    }
}
