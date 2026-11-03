use crate::actions::Action;
use crate::common::numeric_base::{NumericBase, convert_unsigned};
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
    let value = tokens.first()?.to_string();
    let from = normalize(tokens.get(1)?)?.to_string();
    let to = normalize(tokens.last()?)?.to_string();
    Some((value, from, to))
}

pub(crate) fn recognizes_conversion(query: &str) -> bool {
    parse_query(query).is_some()
}

fn conversion_query(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let split = trimmed.find(char::is_whitespace)?;
    let (prefix, rest) = trimmed.split_at(split);
    if !prefix.eq_ignore_ascii_case("conv") && !prefix.eq_ignore_ascii_case("convert") {
        return None;
    }
    Some(rest.trim_start())
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
    if !clean.len().is_multiple_of(8) {
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
    let numeric_bases = match (from, to) {
        ("bin", "hex") => Some((NumericBase::Binary, NumericBase::Hexadecimal)),
        ("hex", "bin") => Some((NumericBase::Hexadecimal, NumericBase::Binary)),
        ("bin", "oct") => Some((NumericBase::Binary, NumericBase::Octal)),
        ("oct", "bin") => Some((NumericBase::Octal, NumericBase::Binary)),
        ("dec", "bin") => Some((NumericBase::Decimal, NumericBase::Binary)),
        ("dec", "hex") => Some((NumericBase::Decimal, NumericBase::Hexadecimal)),
        ("dec", "oct") => Some((NumericBase::Decimal, NumericBase::Octal)),
        ("bin", "dec") => Some((NumericBase::Binary, NumericBase::Decimal)),
        ("hex", "dec") => Some((NumericBase::Hexadecimal, NumericBase::Decimal)),
        ("oct", "dec") => Some((NumericBase::Octal, NumericBase::Decimal)),
        _ => None,
    };
    if let Some((from, to)) = numeric_bases {
        return convert_unsigned(value, from, to);
    }

    match (from, to) {
        ("hex", "text") => hex_to_text(value),
        ("text", "hex") => text_to_hex(value),
        ("text", "bin") => text_to_bin(value),
        ("bin", "text") => bin_to_text(value),
        _ => None,
    }
}

impl Plugin for BaseConvertPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(rest) = conversion_query(query) else {
            return Vec::new();
        };
        if let Some((value, from, to)) = parse_query(rest)
            && let Some(res) = convert(&value, &from, &to)
        {
            let label = format!("{value} {from} = {res} {to}");
            return vec![Action {
                label,
                desc: "Base Convert".into(),
                action: format!("clipboard:{res}"),
                args: None,
            }];
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
            },
            Action {
                label: "convert".into(),
                desc: "Base Convert".into(),
                action: "query:convert ".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::BaseConvertPlugin;
    use crate::plugin::Plugin;

    #[test]
    fn plugin_numeric_pairs_and_text_paths_keep_their_existing_contract() {
        let plugin = BaseConvertPlugin;
        let cases = [
            ("conv 1010 bin to hex", "1010 bin = a hex", "clipboard:a"),
            (
                "conv ff hex to bin",
                "ff hex = 11111111 bin",
                "clipboard:11111111",
            ),
            ("conv 1010 bin to oct", "1010 bin = 12 oct", "clipboard:12"),
            ("conv 17 oct to bin", "17 oct = 1111 bin", "clipboard:1111"),
            ("conv 10 dec to bin", "10 dec = 1010 bin", "clipboard:1010"),
            ("conv 255 dec to hex", "255 dec = ff hex", "clipboard:ff"),
            ("conv 255 dec to oct", "255 dec = 377 oct", "clipboard:377"),
            ("conv 1010 bin to dec", "1010 bin = 10 dec", "clipboard:10"),
            ("conv ff hex to dec", "ff hex = 255 dec", "clipboard:255"),
            ("conv 17 oct to dec", "17 oct = 15 dec", "clipboard:15"),
            (
                "conv hi text to hex",
                "hi text = 6869 hex",
                "clipboard:6869",
            ),
            (
                "conv 6869 hex to text",
                "6869 hex = hi text",
                "clipboard:hi",
            ),
            (
                "conv hi text to bin",
                "hi text = 01101000 01101001 bin",
                "clipboard:01101000 01101001",
            ),
            (
                "conv \"01101000 01101001\" bin to text",
                "01101000 01101001 bin = hi text",
                "clipboard:hi",
            ),
        ];

        for (query, label, action) in cases {
            let results = plugin.search(query);
            assert_eq!(results.len(), 1, "query: {query}");
            assert_eq!(results[0].label, label, "query: {query}");
            assert_eq!(results[0].action, action, "query: {query}");
        }
        assert!(plugin.search("conv 17 oct to hex").is_empty());
    }
}
