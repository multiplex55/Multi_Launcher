use crate::actions::Action;
use crate::plugin::Plugin;

/// Calculate the probability of at least one success after n tries.
/// Usage: `drop p n` where `p` is a fraction like `1/128` or a decimal
/// probability and `n` is the number of attempts.
pub struct DropCalcPlugin;

fn parse_prob(input: &str) -> Option<f64> {
    if let Some((num, den)) = input.split_once('/') {
        let num: f64 = num.trim().parse().ok()?;
        let den: f64 = den.trim().parse().ok()?;
        if den != 0.0 { Some(num / den) } else { None }
    } else {
        input.trim().parse::<f64>().ok()
    }
}

impl Plugin for DropCalcPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        const PREFIX: &str = "drop ";
        let rest = if trimmed.len() >= PREFIX.len()
            && trimmed[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        {
            trimmed[PREFIX.len()..].trim()
        } else {
            return Vec::new();
        };
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() != 2 {
            return Vec::new();
        }
        let p = match parse_prob(parts[0]) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let n: f64 = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let chance = 1.0 - (1.0 - p).powf(n);
        let percent = chance * 100.0;
        vec![Action {
            label: format!("{percent:.2}% chance after {n} tries"),
            desc: "DropCalc".into(),
            action: format!("calc:{percent:.2}"),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "dropcalc"
    }

    fn description(&self) -> &str {
        "Calculate drop chance after N tries (prefix: `drop`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "drop".into(), desc: "dropcalc".into(), action: "fill:drop ".into(), args: None }]
    }
}

