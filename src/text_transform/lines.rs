use crate::text_transform::{
    TextTransformError,
    newlines::{analyze, render_lines},
};
use std::cmp::Ordering;
fn nat_cmp(a: &str, b: &str) -> Ordering {
    let (mut ia, mut ib) = (0, 0);
    let (ba, bb) = (a.as_bytes(), b.as_bytes());
    while ia < ba.len() && ib < bb.len() {
        if ba[ia].is_ascii_digit() && bb[ib].is_ascii_digit() {
            let (sa, sb) = (ia, ib);
            while ia < ba.len() && ba[ia].is_ascii_digit() {
                ia += 1
            }
            while ib < bb.len() && bb[ib].is_ascii_digit() {
                ib += 1
            }
            let na: u128 = a[sa..ia].parse().unwrap_or(0);
            let nb: u128 = b[sb..ib].parse().unwrap_or(0);
            let o = na.cmp(&nb);
            if o != Ordering::Equal {
                return o;
            }
        } else {
            let ca = a[ia..].chars().next().unwrap();
            let cb = b[ib..].chars().next().unwrap();
            let o = ca
                .to_lowercase()
                .to_string()
                .cmp(&cb.to_lowercase().to_string());
            if o != Ordering::Equal {
                return o;
            }
            ia += ca.len_utf8();
            ib += cb.len_utf8();
        }
    }
    ba.len().cmp(&bb.len())
}
pub fn natural_sort(input: &str, desc: bool) -> String {
    let a = analyze(input);
    let mut v = a.lines.clone();
    v.sort_by(|x, y| if desc { nat_cmp(y, x) } else { nat_cmp(x, y) });
    render_lines(&v, &a, true)
}
pub fn dedupe_exact(input: &str) -> String {
    let a = analyze(input);
    let mut seen = std::collections::HashSet::new();
    let v: Vec<_> = a
        .lines
        .iter()
        .filter(|l| seen.insert((*l).clone()))
        .cloned()
        .collect();
    render_lines(&v, &a, true)
}
pub fn reverse(input: &str) -> String {
    let a = analyze(input);
    let mut v = a.lines.clone();
    v.reverse();
    render_lines(&v, &a, true)
}
pub fn remove_blank(input: &str) -> String {
    let a = analyze(input);
    let v: Vec<_> = a
        .lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    render_lines(&v, &a, true)
}
pub fn number_nonblank(input: &str) -> String {
    let a = analyze(input);
    let mut n = 1;
    let v = a
        .lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                l.clone()
            } else {
                let s = format!("{n}. {l}");
                n += 1;
                s
            }
        })
        .collect::<Vec<_>>();
    render_lines(&v, &a, true)
}
pub fn bullet_nonblank(input: &str) -> String {
    let a = analyze(input);
    let v = a
        .lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                l.clone()
            } else {
                format!("- {l}")
            }
        })
        .collect::<Vec<_>>();
    render_lines(&v, &a, true)
}
pub fn comma_join(input: &str) -> String {
    analyze(input).lines.join(", ")
}
pub fn csv_newline_split(input: &str) -> Result<String, TextTransformError> {
    let a = analyze(input);
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(cur.trim().to_string());
                cur.clear()
            }
            _ => cur.push(c),
        }
    }
    if quoted {
        return Err(TextTransformError::InvalidCsvLikeInput(
            "unterminated quoted field".into(),
        ));
    }
    fields.push(cur.trim().to_string());
    Ok(render_lines(&fields, &a, false))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nat() {
        assert_eq!(
            natural_sort("item10\nitem2\nitem1", false),
            "item1\nitem2\nitem10"
        )
    }
    #[test]
    fn csv() {
        assert_eq!(
            csv_newline_split("a, \"b,c\", \"d\"\"e\"").unwrap(),
            "a\r\nb,c\r\nd\"e"
        );
        assert!(csv_newline_split("\"nope").is_err())
    }
}
