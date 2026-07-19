#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NewlineStyle {
    Crlf,
    Lf,
    Cr,
}
impl NewlineStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewlineAnalysis {
    pub dominant: NewlineStyle,
    pub crlf_count: usize,
    pub lf_count: usize,
    pub cr_count: usize,
    pub ended_with_newline: bool,
    pub lines: Vec<String>,
}
pub fn analyze(s: &str) -> NewlineAnalysis {
    let mut i = 0;
    let b = s.as_bytes();
    let mut last = 0;
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let mut lines = Vec::new();
    while i < b.len() {
        match b[i] {
            b'\r' => {
                lines.push(s[last..i].to_string());
                if i + 1 < b.len() && b[i + 1] == b'\n' {
                    crlf += 1;
                    i += 2;
                    last = i
                } else {
                    cr += 1;
                    i += 1;
                    last = i
                }
            }
            b'\n' => {
                lines.push(s[last..i].to_string());
                lf += 1;
                i += 1;
                last = i
            }
            _ => i += 1,
        }
    }
    if last < b.len() {
        lines.push(s[last..].to_string())
    }
    let ended = last == b.len() && (crlf + lf + cr) > 0;
    let dominant = if crlf >= lf && crlf >= cr {
        NewlineStyle::Crlf
    } else if lf > cr {
        NewlineStyle::Lf
    } else if cr > lf {
        NewlineStyle::Cr
    } else {
        NewlineStyle::Crlf
    };
    NewlineAnalysis {
        dominant,
        crlf_count: crlf,
        lf_count: lf,
        cr_count: cr,
        ended_with_newline: ended,
        lines,
    }
}
pub fn render_lines(
    lines: &[String],
    analysis: &NewlineAnalysis,
    preserve_trailing: bool,
) -> String {
    let sep = analysis.dominant.as_str();
    let mut s = lines.join(sep);
    if preserve_trailing && analysis.ended_with_newline {
        s.push_str(sep)
    }
    s
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed() {
        let a = analyze("a\r\nb\nc\rd\r\n");
        assert_eq!(a.crlf_count, 2);
        assert_eq!(a.lf_count, 1);
        assert_eq!(a.cr_count, 1);
        assert!(a.ended_with_newline);
        assert_eq!(a.dominant, NewlineStyle::Crlf);
        assert_eq!(a.lines, ["a", "b", "c", "d"]);
    }
}
