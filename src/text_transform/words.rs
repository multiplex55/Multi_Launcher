#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Kind {
    Upper,
    Lower,
    Letter,
    Digit,
    Other,
}
fn kind(c: char) -> Kind {
    if c.is_numeric() {
        Kind::Digit
    } else if c.is_uppercase() {
        Kind::Upper
    } else if c.is_lowercase() {
        Kind::Lower
    } else if c.is_alphabetic() {
        Kind::Letter
    } else {
        Kind::Other
    }
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..chars.len() {
        let (idx, c) = chars[i];
        if c.is_whitespace() || c == '-' || c == '_' || (!c.is_alphanumeric()) {
            if let Some(st) = start.take() {
                out.push(Token {
                    text: input[st..idx].to_string(),
                });
            }
            continue;
        }
        if let Some(st) = start {
            let prev = chars[i - 1].1;
            let pk = kind(prev);
            let ck = kind(c);
            let next = chars.get(i + 1).map(|(_, n)| kind(*n));
            let split =
                matches!(
                    (pk, ck),
                    (Kind::Lower, Kind::Upper)
                        | (Kind::Letter, Kind::Digit)
                        | (Kind::Upper, Kind::Digit)
                        | (Kind::Lower, Kind::Digit)
                        | (Kind::Digit, Kind::Upper)
                        | (Kind::Digit, Kind::Lower)
                        | (Kind::Digit, Kind::Letter)
                ) || (pk == Kind::Upper && ck == Kind::Upper && matches!(next, Some(Kind::Lower)));
            if split {
                out.push(Token {
                    text: input[st..idx].to_string(),
                });
                start = Some(idx);
            }
        } else {
            start = Some(idx);
        }
    }
    if let Some(st) = start {
        out.push(Token {
            text: input[st..].to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokenizer_boundaries() {
        let got: Vec<_> = tokenize("HTTPServer2Config")
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(got, ["HTTP", "Server", "2", "Config"]);
        let got: Vec<_> = tokenize("foo-bar_baz qux!42Zed")
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(got, ["foo", "bar", "baz", "qux", "42", "Zed"]);
    }
}
