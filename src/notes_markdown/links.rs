use url::Url;

use super::parse::{fence_info, leading_spaces, line_spans};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapLinksReport {
    pub content: String,
    pub wrapped: usize,
    pub skipped_existing: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtectedRange {
    start: usize,
    end: usize,
    count_existing: bool,
}

pub fn wrap_plain_urls(content: &str) -> WrapLinksReport {
    let protected = protected_ranges(content);
    let skipped_existing = protected
        .iter()
        .filter(|range| {
            range.count_existing && contains_valid_recognized_url(&content[range.start..range.end])
        })
        .count();

    let mut output = String::with_capacity(content.len());
    let mut cursor = 0usize;
    let mut wrapped = 0usize;

    for range in unprotected_ranges(content.len(), &protected) {
        output.push_str(&content[cursor..range.start]);
        let (segment, count) = wrap_segment(content, range.start, range.end);
        output.push_str(&segment);
        wrapped += count;
        cursor = range.end;
    }
    output.push_str(&content[cursor..]);

    WrapLinksReport {
        content: output,
        wrapped,
        skipped_existing,
    }
}

fn protected_ranges(content: &str) -> Vec<ProtectedRange> {
    let mut ranges = Vec::new();
    add_fenced_code_ranges(content, &mut ranges);
    add_inline_code_ranges(content, &mut ranges);
    add_bracket_construct_ranges(content, &mut ranges);
    add_angle_ranges(content, &mut ranges);
    ranges.sort_by_key(|range| (range.start, range.end));
    merge_ranges(ranges)
}

fn add_fenced_code_ranges(content: &str, ranges: &mut Vec<ProtectedRange>) {
    let lines = line_spans(content);
    let mut in_fence: Option<(u8, usize, usize)> = None;

    for line in lines {
        let indent = leading_spaces(line.text);
        let trimmed = &line.text[indent..];
        let fence = fence_info(trimmed);
        if let Some((marker, len, start)) = in_fence {
            if let Some((close_marker, close_len)) = fence
                && close_marker == marker
                && close_len >= len
            {
                ranges.push(ProtectedRange {
                    start,
                    end: line.end,
                    count_existing: false,
                });
                in_fence = None;
            }
        } else if let Some((marker, len)) = fence.filter(|_| indent <= 3) {
            in_fence = Some((marker, len, line.start));
        }
    }

    if let Some((_, _, start)) = in_fence {
        ranges.push(ProtectedRange {
            start,
            end: content.len(),
            count_existing: false,
        });
    }
}

fn add_inline_code_ranges(content: &str, ranges: &mut Vec<ProtectedRange>) {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let tick_len = run_len(bytes, i, b'`');
        if tick_len >= 3 && is_line_start_or_after_spaces(bytes, i) {
            i += tick_len;
            continue;
        }
        let mut j = i + tick_len;
        while j < bytes.len() {
            if bytes[j] == b'`' && run_len(bytes, j, b'`') == tick_len {
                ranges.push(ProtectedRange {
                    start: i,
                    end: j + tick_len,
                    count_existing: false,
                });
                i = j + tick_len;
                break;
            }
            j += 1;
        }
        if j >= bytes.len() {
            i += tick_len;
        }
    }
}

fn add_bracket_construct_ranges(content: &str, ranges: &mut Vec<ProtectedRange>) {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !content.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if protected_contains(ranges, i) {
            i += 1;
            continue;
        }
        if bytes[i] == b'!' && content[i..].starts_with("![[") {
            if let Some(end) = content[i + 3..].find("]]").map(|off| i + 3 + off + 2) {
                ranges.push(ProtectedRange {
                    start: i,
                    end,
                    count_existing: false,
                });
                i = end;
                continue;
            }
        }
        if content[i..].starts_with("[[") {
            if let Some(end) = content[i + 2..].find("]]").map(|off| i + 2 + off + 2) {
                ranges.push(ProtectedRange {
                    start: i,
                    end,
                    count_existing: false,
                });
                i = end;
                continue;
            }
        }
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(end) = markdown_link_end(content, i + 1) {
                ranges.push(ProtectedRange {
                    start: i,
                    end,
                    count_existing: true,
                });
                i = end;
                continue;
            }
        } else if bytes[i] == b'[' {
            if let Some(end) = markdown_link_end(content, i) {
                ranges.push(ProtectedRange {
                    start: i,
                    end,
                    count_existing: true,
                });
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

fn markdown_link_end(content: &str, label_start: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let label_end = find_byte(bytes, label_start + 1, b']')?;
    if bytes.get(label_end + 1) != Some(&b'(') {
        return None;
    }
    let mut depth = 1usize;
    let mut i = label_end + 2;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn add_angle_ranges(content: &str, ranges: &mut Vec<ProtectedRange>) {
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !content.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if protected_contains(ranges, i) {
            i += 1;
            continue;
        }
        if bytes[i] == b'<' {
            if let Some(off) = content[i + 1..].find('>') {
                let end = i + 1 + off + 1;
                let inner = &content[i + 1..end - 1];
                let count_existing = is_valid_url_candidate(inner).is_some();
                ranges.push(ProtectedRange {
                    start: i,
                    end,
                    count_existing,
                });
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

fn wrap_segment(content: &str, start: usize, end: usize) -> (String, usize) {
    let mut out = String::with_capacity(end - start);
    let mut cursor = start;
    let mut copied_until = start;
    let mut count = 0;

    while cursor < end {
        if !content.is_char_boundary(cursor) {
            cursor += 1;
            continue;
        }

        if let Some((candidate_end, normalized)) = candidate_at(content, cursor, end) {
            let (raw_end, _) =
                scan_raw_candidate(content, cursor, end).expect("validated candidate scans");
            out.push_str(&content[copied_until..cursor]);
            let label = &content[cursor..candidate_end];
            out.push('[');
            out.push_str(label);
            out.push_str("](");
            out.push_str(&normalized);
            out.push(')');
            out.push_str(&content[candidate_end..raw_end]);
            count += 1;
            cursor = raw_end;
            copied_until = raw_end;
            continue;
        }

        cursor += content[cursor..].chars().next().unwrap().len_utf8();
    }

    out.push_str(&content[copied_until..end]);
    (out, count)
}

fn candidate_at(content: &str, i: usize, end: usize) -> Option<(usize, String)> {
    if i > 0 && is_url_word(content.as_bytes()[i - 1]) {
        return None;
    }
    let kind = if content[i..].starts_with("https://") || content[i..].starts_with("http://") {
        "direct"
    } else if content[i..].starts_with("www.") {
        "www"
    } else {
        return None;
    };
    let (raw_end, candidate) = scan_raw_candidate(content, i, end)?;
    let normalized = if kind == "www" {
        format!("https://{candidate}")
    } else {
        candidate.to_string()
    };
    is_valid_url_candidate(&normalized).map(|_| (raw_end, normalized))
}

fn scan_raw_candidate(content: &str, i: usize, end: usize) -> Option<(usize, &str)> {
    let mut raw_end = i;
    for (off, ch) in content[i..end].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
            break;
        }
        raw_end = i + off + ch.len_utf8();
    }
    if raw_end == i {
        return None;
    }
    let mut candidate_end = raw_end;
    loop {
        let s = &content[i..candidate_end];
        if let Some(ch) = s.chars().next_back() {
            if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?') || is_unmatched_closer(s, ch) {
                candidate_end -= ch.len_utf8();
                continue;
            }
        }
        break;
    }
    (candidate_end > i).then_some((candidate_end, &content[i..candidate_end]))
}

fn is_unmatched_closer(s: &str, ch: char) -> bool {
    let (open, close) = match ch {
        ')' => ('(', ')'),
        ']' => ('[', ']'),
        '}' => ('{', '}'),
        _ => return false,
    };
    s.chars().filter(|&c| c == close).count() > s.chars().filter(|&c| c == open).count()
}

fn is_valid_url_candidate(candidate: &str) -> Option<()> {
    let url = Url::parse(candidate).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(())?;
    url.host_str().filter(|host| !host.is_empty()).map(|_| ())
}

fn contains_valid_recognized_url(s: &str) -> bool {
    let mut i = 0;
    while i < s.len() {
        if s.is_char_boundary(i) && candidate_at(s, i, s.len()).is_some() {
            return true;
        }
        i += 1;
    }
    false
}

fn unprotected_ranges(len: usize, protected: &[ProtectedRange]) -> Vec<ProtectedRange> {
    let mut ranges = Vec::new();
    let mut cursor = 0;
    for range in protected {
        if cursor < range.start {
            ranges.push(ProtectedRange {
                start: cursor,
                end: range.start,
                count_existing: false,
            });
        }
        cursor = cursor.max(range.end);
    }
    if cursor < len {
        ranges.push(ProtectedRange {
            start: cursor,
            end: len,
            count_existing: false,
        });
    }
    ranges
}

fn merge_ranges(ranges: Vec<ProtectedRange>) -> Vec<ProtectedRange> {
    let mut merged: Vec<ProtectedRange> = Vec::new();
    for range in ranges.into_iter().filter(|r| r.start < r.end) {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            last.count_existing |= range.count_existing;
            continue;
        }
        merged.push(range);
    }
    merged
}

fn protected_contains(ranges: &[ProtectedRange], index: usize) -> bool {
    ranges
        .iter()
        .any(|range| range.start <= index && index < range.end)
}

fn run_len(bytes: &[u8], start: usize, byte: u8) -> usize {
    bytes[start..].iter().take_while(|&&b| b == byte).count()
}
fn find_byte(bytes: &[u8], start: usize, byte: u8) -> Option<usize> {
    bytes
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, &b)| (b == byte).then_some(i))
}
fn is_url_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}
fn is_line_start_or_after_spaces(bytes: &[u8], i: usize) -> bool {
    bytes[..i]
        .iter()
        .rev()
        .take_while(|&&b| b != b'\n')
        .all(|&b| b == b' ')
}

#[cfg(test)]
mod tests {
    use super::wrap_plain_urls;

    fn wrap(content: &str) -> (String, usize, usize) {
        let report = wrap_plain_urls(content);
        (report.content, report.wrapped, report.skipped_existing)
    }

    #[test]
    fn one_url() {
        assert_eq!(
            wrap("Go https://example.com now"),
            (
                "Go [https://example.com](https://example.com) now".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn multiple_urls_one_line() {
        assert_eq!(
            wrap("https://a.com and www.b.com"),
            (
                "[https://a.com](https://a.com) and [www.b.com](https://www.b.com)".into(),
                2,
                0
            )
        );
    }
    #[test]
    fn url_at_beginning() {
        assert_eq!(
            wrap("https://example.com end"),
            (
                "[https://example.com](https://example.com) end".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn url_at_end() {
        assert_eq!(
            wrap("see https://example.com"),
            (
                "see [https://example.com](https://example.com)".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn http() {
        assert_eq!(
            wrap("http://example.com"),
            ("[http://example.com](http://example.com)".into(), 1, 0)
        );
    }
    #[test]
    fn https() {
        assert_eq!(
            wrap("https://example.com"),
            ("[https://example.com](https://example.com)".into(), 1, 0)
        );
    }
    #[test]
    fn www_normalization() {
        assert_eq!(
            wrap("www.example.com/docs"),
            (
                "[www.example.com/docs](https://www.example.com/docs)".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn unicode_surrounding_text() {
        assert_eq!(
            wrap("Привет https://пример.рф/путь 🌱"),
            (
                "Привет [https://пример.рф/путь](https://пример.рф/путь) 🌱".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn query_strings() {
        assert_eq!(
            wrap("https://example.com/search?q=a&x=1"),
            (
                "[https://example.com/search?q=a&x=1](https://example.com/search?q=a&x=1)".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn fragments() {
        assert_eq!(
            wrap("https://example.com/a#frag"),
            (
                "[https://example.com/a#frag](https://example.com/a#frag)".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn balanced_parentheses() {
        assert_eq!(
            wrap("https://example.com/a_(b)."),
            (
                "[https://example.com/a_(b)](https://example.com/a_(b)).".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn sentence_punctuation() {
        assert_eq!(
            wrap("See https://example.com/a, ok?"),
            (
                "See [https://example.com/a](https://example.com/a), ok?".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn existing_markdown_links() {
        assert_eq!(
            wrap("[x https://example.com](https://example.com)"),
            ("[x https://example.com](https://example.com)".into(), 0, 1)
        );
    }
    #[test]
    fn existing_markdown_images() {
        assert_eq!(
            wrap("![alt](https://example.com/i.png)"),
            ("![alt](https://example.com/i.png)".into(), 0, 1)
        );
    }
    #[test]
    fn wiki_links() {
        assert_eq!(
            wrap("[[https://example.com]]"),
            ("[[https://example.com]]".into(), 0, 0)
        );
    }
    #[test]
    fn obsidian_image_embeds() {
        assert_eq!(
            wrap("![[https://example.com/image.png]]"),
            ("![[https://example.com/image.png]]".into(), 0, 0)
        );
    }
    #[test]
    fn inline_code() {
        assert_eq!(
            wrap("`https://example.com`"),
            ("`https://example.com`".into(), 0, 0)
        );
    }
    #[test]
    fn inline_backtick_spans_with_interior_backticks() {
        assert_eq!(
            wrap("``code ` https://example.com``"),
            ("``code ` https://example.com``".into(), 0, 0)
        );
    }
    #[test]
    fn backtick_fenced_code_blocks() {
        assert_eq!(
            wrap("```\nhttps://example.com\n```\nhttps://a.com"),
            (
                "```\nhttps://example.com\n```\n[https://a.com](https://a.com)".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn tilde_fenced_code_blocks() {
        assert_eq!(
            wrap("~~~\nhttps://example.com\n~~~~"),
            ("~~~\nhttps://example.com\n~~~~".into(), 0, 0)
        );
    }
    #[test]
    fn html_attributes() {
        assert_eq!(
            wrap("<a href=\"https://example.com\">https://a.com</a>"),
            (
                "<a href=\"https://example.com\">[https://a.com](https://a.com)</a>".into(),
                1,
                0
            )
        );
    }
    #[test]
    fn markdown_autolinks() {
        assert_eq!(
            wrap("<https://example.com>"),
            ("<https://example.com>".into(), 0, 1)
        );
    }
    #[test]
    fn mixed_newline_styles() {
        assert_eq!(
            wrap("a\r\nhttps://a.com\nb\rhttps://b.com\r\n"),
            (
                "a\r\n[https://a.com](https://a.com)\nb\r[https://b.com](https://b.com)\r\n".into(),
                2,
                0
            )
        );
    }
    #[test]
    fn no_eligible_urls() {
        assert_eq!(
            wrap("example.com mailto:x ftp://x C:\\x \\\\server\\share"),
            (
                "example.com mailto:x ftp://x C:\\x \\\\server\\share".into(),
                0,
                0
            )
        );
    }
    #[test]
    fn idempotency() {
        let first = wrap_plain_urls("https://a.com and <https://b.com>");
        let second = wrap_plain_urls(&first.content);
        assert_eq!(second.content, first.content);
        assert_eq!(second.wrapped, 0);
        assert_eq!(second.skipped_existing, 2);
    }
    #[test]
    fn skipped_existing_counts_only_markdown_and_autolink() {
        assert_eq!(wrap("[x](https://a.com) `https://b.com` <span data-u=\"https://c.com\"></span> <https://d.com>"), ("[x](https://a.com) `https://b.com` <span data-u=\"https://c.com\"></span> <https://d.com>".into(), 0, 2));
    }
}
