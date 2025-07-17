use crate::actions::Action;
use crate::plugin::Plugin;

pub struct UnitConvertPlugin;

fn convert(value: f64, from: &str, to: &str) -> Option<f64> {
    match (from, to) {
        // Length
        ("km", "mi") => Some(value * 0.621_371),
        ("mi", "km") => Some(value / 0.621_371),
        ("m", "ft") => Some(value * 3.280_84),
        ("ft", "m") => Some(value / 3.280_84),
        ("cm", "in") => Some(value / 2.54),
        ("in", "cm") => Some(value * 2.54),
        ("mm", "in") => Some(value / 25.4),
        ("in", "mm") => Some(value * 25.4),
        ("nm", "ft") => Some(value * 6076.12),
        ("ft", "nm") => Some(value / 6076.12),
        ("nm", "mi") => Some(value * 1.150_78),
        ("mi", "nm") => Some(value / 1.150_78),

        // Weight / Mass
        ("kg", "lb") => Some(value * 2.204_62),
        ("lb", "kg") => Some(value / 2.204_62),
        ("g", "oz") => Some(value / 28.3495),
        ("oz", "g") => Some(value * 28.3495),
        ("g", "kg") => Some(value / 1000.0),
        ("kg", "g") => Some(value * 1000.0),

        // Temperature
        ("c", "f") => Some(value * 9.0 / 5.0 + 32.0),
        ("f", "c") => Some((value - 32.0) * 5.0 / 9.0),
        ("c", "k") => Some(value + 273.15),
        ("k", "c") => Some(value - 273.15),
        ("f", "k") => Some((value - 32.0) * 5.0 / 9.0 + 273.15),
        ("k", "f") => Some((value - 273.15) * 9.0 / 5.0 + 32.0),

        // Volume
        ("l", "gal") => Some(value / 3.785_41),
        ("gal", "l") => Some(value * 3.785_41),
        ("ml", "oz") => Some(value / 29.5735),
        ("oz", "ml") => Some(value * 29.5735),

        // Area
        ("sq_m", "sq_ft") => Some(value * 10.7639),
        ("sq_ft", "sq_m") => Some(value / 10.7639),
        ("ha", "ac") => Some(value * 2.47105),
        ("ac", "ha") => Some(value / 2.47105),

        // Speed
        ("kph", "mph") => Some(value * 0.621_371),
        ("mph", "kph") => Some(value / 0.621_371),
        ("mps", "fps") => Some(value * 3.280_84),
        ("fps", "mps") => Some(value / 3.280_84),

        // Pressure
        ("atm", "pa") => Some(value * 101_325.0),
        ("pa", "atm") => Some(value / 101_325.0),
        ("bar", "psi") => Some(value * 14.5038),
        ("psi", "bar") => Some(value / 14.5038),
        _ => None,
    }
}

fn parse_query(query: &str) -> Option<(f64, String, String)> {
    let rest = query.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    if !parts[2].eq_ignore_ascii_case("to") {
        return None;
    }
    let val: f64 = parts[0].parse().ok()?;
    Some((val, parts[1].to_lowercase(), parts[3].to_lowercase()))
}

impl Plugin for UnitConvertPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        const CONV_PREFIX: &str = "conv ";
        const CONVERT_PREFIX: &str = "convert ";
        let rest = if trimmed.len() >= CONV_PREFIX.len()
            && trimmed[..CONV_PREFIX.len()].eq_ignore_ascii_case(CONV_PREFIX)
        {
            &trimmed[CONV_PREFIX.len()..]
        } else if trimmed.len() >= CONVERT_PREFIX.len()
            && trimmed[..CONVERT_PREFIX.len()].eq_ignore_ascii_case(CONVERT_PREFIX)
        {
            &trimmed[CONVERT_PREFIX.len()..]
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
            Action { label: "conv".into(), desc: "unit_convert".into(), action: "fill:conv ".into(), args: None },
            Action { label: "convert".into(), desc: "unit_convert".into(), action: "fill:convert ".into(), args: None },
        ]
    }
}
