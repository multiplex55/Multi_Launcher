use crate::actions::Action;
use crate::plugin::Plugin;

pub struct UnitConvertPlugin;

fn convert(value: f64, from: &str, to: &str) -> Option<f64> {
    match (from, to) {
        ("km", "mi") => Some(value * 0.621_371),
        ("mi", "km") => Some(value / 0.621_371),
        ("m", "ft") => Some(value * 3.280_84),
        ("ft", "m") => Some(value / 3.280_84),
        ("kg", "lb") => Some(value * 2.204_62),
        ("lb", "kg") => Some(value / 2.204_62),
        ("c", "f") => Some(value * 9.0 / 5.0 + 32.0),
        ("f", "c") => Some((value - 32.0) * 5.0 / 9.0),
        ("nm", "ft") => Some(value * 6076.12),
        ("ft", "nm") => Some(value / 6076.12),
        ("nm", "mi") => Some(value * 1.150_78),
        ("mi", "nm") => Some(value / 1.150_78),
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
        let rest = if let Some(r) = trimmed.strip_prefix("conv ") {
            r
        } else if let Some(r) = trimmed.strip_prefix("convert ") {
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
}
