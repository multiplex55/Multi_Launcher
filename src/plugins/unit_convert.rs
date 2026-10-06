use crate::actions::Action;
use crate::plugin::Plugin;
use crate::unit_conversion::{self, Unit};

pub struct UnitConvertPlugin;

fn parse_query(query: &str) -> Option<(f64, Unit, Unit)> {
    let rest = query.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    if !parts.get(2)?.eq_ignore_ascii_case("to") {
        return None;
    }
    let val: f64 = parts.first()?.parse().ok()?;
    let from = unit_conversion::unit_by_alias(parts.get(1)?)?;
    let to = unit_conversion::unit_by_alias(parts.get(3)?)?;
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

        if let Some((value, from, to)) = parse_query(rest)
            && let Some(result) = unit_conversion::convert(value, from, to)
        {
            let label = format!(
                "{} {} = {:.4} {}",
                value,
                from.symbol(),
                result,
                to.symbol()
            );
            let action = format!("clipboard:{:.4}", result);
            return vec![Action {
                label,
                desc: "Unit convert".into(),
                action,
                args: None,
            }];
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
            },
            Action {
                label: "convert".into(),
                desc: "Unit convert".into(),
                action: "query:convert ".into(),
                args: None,
            },
        ]
    }
}
