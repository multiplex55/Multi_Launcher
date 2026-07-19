use crate::text_transform::newlines::{NewlineStyle, analyze, render_lines};
pub fn trim_text(s: &str) -> String {
    s.trim().to_string()
}
pub fn trim_lines(s: &str) -> String {
    let a = analyze(s);
    let v = a
        .lines
        .iter()
        .map(|l| l.trim().to_string())
        .collect::<Vec<_>>();
    render_lines(&v, &a, true)
}
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::new();
    let mut in_h = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            if !in_h {
                out.push(' ');
                in_h = true
            }
        } else {
            out.push(c);
            in_h = false
        }
    }
    out
}
pub fn tabs_to_four_spaces(s: &str) -> String {
    s.replace('\t', "    ")
}
pub fn normalize_crlf(s: &str) -> String {
    let mut a = analyze(s);
    a.dominant = NewlineStyle::Crlf;
    render_lines(&a.lines, &a, true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collapse_keeps_newlines() {
        assert_eq!(collapse_spaces("a  b\n c\t\td"), "a b\n c d")
    }
}
