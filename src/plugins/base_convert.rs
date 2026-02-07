use crate::actions::Action;
use crate::plugin::Plugin;
use shlex;

pub struct BaseConvertPlugin;

fn normalize(base: &str) -> Option<&'static str> {
    match base.to_lowercase().as_str() {
        "bin" | "binary" => Some("bin"),
        "hex" | "hexadecimal" => Some("hex"),
        "oct" | "octal" => Some("oct"),
        "dec" | "decimal" => Some("dec"),
        "text" | "string" => Some("text"),
        _ => None,
    }
}

fn parse_query(query: &str) -> Option<(String, String, String)> {
    let tokens = shlex::split(query.trim())?;
    if tokens.len() < 4 {
        return None;
    }
    let to_idx = tokens.len() - 2;
    if !tokens.get(to_idx)?.eq_ignore_ascii_case("to") {
        return None;
    }
    let value = tokens.get(0)?.to_string();
    let from = normalize(tokens.get(1)?)?.to_string();
    let to = normalize(tokens.last()?)?.to_string();
    Some((value, from, to))
}

fn bin_to_hex(s: &str) -> Option<String> {
    u128::from_str_radix(s, 2).ok().map(|n| format!("{:x}", n))
}

fn hex_to_bin(s: &str) -> Option<String> {
    u128::from_str_radix(s, 16).ok().map(|n| format!("{:b}", n))
}

fn bin_to_oct(s: &str) -> Option<String> {
    u128::from_str_radix(s, 2).ok().map(|n| format!("{:o}", n))
}

fn oct_to_bin(s: &str) -> Option<String> {
    u128::from_str_radix(s, 8).ok().map(|n| format!("{:b}", n))
}

fn dec_to_bin(s: &str) -> Option<String> {
    u128::from_str_radix(s, 10).ok().map(|n| format!("{:b}", n))
}

fn dec_to_hex(s: &str) -> Option<String> {
    u128::from_str_radix(s, 10).ok().map(|n| format!("{:x}", n))
}

fn dec_to_oct(s: &str) -> Option<String> {
    u128::from_str_radix(s, 10).ok().map(|n| format!("{:o}", n))
}

fn text_to_hex(s: &str) -> Option<String> {
    Some(hex::encode(s.as_bytes()))
}

fn hex_to_text(s: &str) -> Option<String> {
    let bytes = hex::decode(s).ok()?;
    String::from_utf8(bytes).ok()
}

fn text_to_bin(s: &str) -> Option<String> {
    Some(
        s.as_bytes()
            .iter()
            .map(|b| format!("{:08b}", b))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn bin_to_text(s: &str) -> Option<String> {
    let clean = s.replace(' ', "");
    if clean.len() % 8 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..clean.len())
        .step_by(8)
        .map(|i| {
            clean
                .get(i..i + 8)
                .and_then(|chunk| u8::from_str_radix(chunk, 2).ok())
        })
        .collect();
    let bytes = bytes?;
    String::from_utf8(bytes).ok()
}

fn convert(value: &str, from: &str, to: &str) -> Option<String> {
    match (from, to) {
        ("bin", "hex") => bin_to_hex(value),
        ("hex", "bin") => hex_to_bin(value),
        ("bin", "oct") => bin_to_oct(value),
        ("oct", "bin") => oct_to_bin(value),
        ("dec", "bin") => dec_to_bin(value),
        ("dec", "hex") => dec_to_hex(value),
        ("dec", "oct") => dec_to_oct(value),
        ("hex", "text") => hex_to_text(value),
        ("text", "hex") => text_to_hex(value),
        ("text", "bin") => text_to_bin(value),
        ("bin", "text") => bin_to_text(value),
        _ => None,
    }
}

impl Plugin for BaseConvertPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const CONV_PREFIX: &str = "conv ";
        const CONVERT_PREFIX: &str = "convert ";
        let rest = if let Some(r) = crate::common::strip_prefix_ci(query.trim_start(), CONV_PREFIX)
        {
            r
        } else if let Some(r) = crate::common::strip_prefix_ci(query.trim_start(), CONVERT_PREFIX) {
            r
        } else {
            return Vec::new();
        };
        if let Some((value, from, to)) = parse_query(rest) {
            if let Some(res) = convert(&value, &from, &to) {
                let label = format!("{value} {from} = {res} {to}");
                return vec![Action {
                    label,
                    desc: "Base Convert".into(),
                    action: format!("clipboard:{res}"),
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
        "base_convert"
    }

    fn description(&self) -> &str {
        "Convert between bases (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Base Convert".into(),
                action: "query:conv ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "convert".into(),
                desc: "Base Convert".into(),
                action: "query:convert ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
        ]
    }
}
