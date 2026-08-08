use url::Url;

use super::parse::{fence_info, leading_spaces, line_spans};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapLinksReport {
    pub content: String,
    pub wrapped: usize,
    pub skipped_existing: usize,
}

/// The result of converting web addresses and absolute Windows paths in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainLinkConversionReport {
    pub content: String,
    pub web_links: usize,
    pub directories: usize,
    pub files: usize,
    pub skipped_existing: usize,
    pub skipped_invalid_paths: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathClassification {
    File,
    Directory,
    Missing,
}

/// Converts plain web addresses and existing absolute Windows filesystem paths.
///
/// Unlike [`wrap_plain_urls`], this is deliberately allowed to consult the
/// filesystem and uses compact, escaped link labels.
pub fn convert_plain_links(content: &str) -> PlainLinkConversionReport {
    convert_plain_links_with(content, |path| match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => PathClassification::File,
        Ok(metadata) if metadata.is_dir() => PathClassification::Directory,
        _ => PathClassification::Missing,
    })
}

fn convert_plain_links_with(
    content: &str,
    mut classify: impl FnMut(&str) -> PathClassification,
) -> PlainLinkConversionReport {
    let protected = protected_ranges(content);
    let skipped_existing = protected
        .iter()
        .filter(|range| {
            range.count_existing && contains_new_recognized_target(&content[range.start..range.end])
        })
        .count();
    let mut report = PlainLinkConversionReport {
        content: String::with_capacity(content.len()),
        web_links: 0,
        directories: 0,
        files: 0,
        skipped_existing,
        skipped_invalid_paths: 0,
    };
    let mut copied = 0;
    for range in unprotected_ranges(content.len(), &protected) {
        report.content.push_str(&content[copied..range.start]);
        convert_range(content, range.start, range.end, &mut classify, &mut report);
        copied = range.end;
    }
    report.content.push_str(&content[copied..]);
    report
}

fn convert_range(
    content: &str,
    start: usize,
    end: usize,
    classify: &mut impl FnMut(&str) -> PathClassification,
    report: &mut PlainLinkConversionReport,
) {
    let mut copied = start;
    let mut i = start;
    while i < end {
        if !content.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if let Some((candidate_end, destination, label)) = new_web_at(content, i, end) {
            report.content.push_str(&content[copied..i]);
            push_link(&mut report.content, &label, &destination);
            report.web_links += 1;
            copied = candidate_end;
            i = candidate_end;
            continue;
        }
        if is_windows_root_at(content, i, end) && path_start_boundary(content, i) {
            let scan_end = path_scan_end(content, i, end);
            let endpoints = path_endpoints(content, i, scan_end);
            let mut valid = Vec::new();
            for endpoint in endpoints {
                let raw = &content[i..endpoint];
                let trimmed = raw.trim_end_matches([' ', '\t']);
                let actual_end = i + trimmed.len();
                if actual_end > i {
                    let kind = classify(trimmed);
                    if kind != PathClassification::Missing {
                        valid.push((actual_end, kind));
                    }
                }
            }
            valid.sort_by_key(|item| item.0);
            valid.dedup_by_key(|item| item.0);
            // If both sides of a prose-looking space are real paths, there is
            // no byte-level evidence that says whether the prose is part of
            // the name. Refuse to guess.
            if valid.len() == 1
                && let Some(&(path_end, kind)) = valid.last()
            {
                let rest = &content[path_end..scan_end];
                let partial_child = rest.starts_with(['\\', '/'])
                    || rest
                        .trim_start_matches([' ', '\t'])
                        .starts_with(['\\', '/']);
                if !partial_child {
                    let raw = &content[i..path_end];
                    if let Some(destination) = windows_file_url(raw) {
                        report.content.push_str(&content[copied..i]);
                        push_link(&mut report.content, &path_label(raw), destination.as_str());
                        match kind {
                            PathClassification::File => report.files += 1,
                            PathClassification::Directory => report.directories += 1,
                            PathClassification::Missing => unreachable!(),
                        }
                        copied = path_end;
                        i = path_end;
                        continue;
                    }
                }
            }
            report.skipped_invalid_paths += 1;
            // Move beyond the root marker, rather than the line, so an invalid
            // path cannot hide a later independent URL or path candidate.
            i += 3.min(scan_end - i);
            continue;
        }
        i += content[i..].chars().next().unwrap().len_utf8();
    }
    report.content.push_str(&content[copied..end]);
}

fn push_link(out: &mut String, label: &str, destination: &str) {
    out.push('[');
    out.push_str(&escape_label(label));
    out.push_str("](");
    out.push_str(destination);
    out.push(')');
}

fn escape_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn new_web_at(content: &str, i: usize, end: usize) -> Option<(usize, String, String)> {
    if i > 0 && is_url_word(content.as_bytes()[i - 1]) {
        return None;
    }
    let www = content[i..end].starts_with("www.");
    if !www && !content[i..end].starts_with("http://") && !content[i..end].starts_with("https://") {
        return None;
    }
    let mut raw_end = i;
    for (offset, ch) in content[i..end].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
            break;
        }
        raw_end = i + offset + ch.len_utf8();
    }
    let mut candidate_end = raw_end;
    while candidate_end > i {
        let s = &content[i..candidate_end];
        let ch = s.chars().next_back()?;
        if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?') || is_unmatched_closer(s, ch) {
            candidate_end -= ch.len_utf8();
        } else {
            break;
        }
    }
    let raw = &content[i..candidate_end];
    let destination = if www {
        format!("https://{raw}")
    } else {
        raw.to_owned()
    };
    let url = Url::parse(&destination).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_owned();
    let without_www = host
        .strip_prefix("www.")
        .or_else(|| {
            host.get(..4)
                .filter(|prefix| prefix.eq_ignore_ascii_case("www."))
                .map(|_| &host[4..])
        })
        .unwrap_or(&host);
    let ordinary = without_www.contains('.') && without_www.parse::<std::net::IpAddr>().is_err();
    let label = if ordinary {
        without_www.split('.').next().unwrap()
    } else {
        without_www
    }
    .to_owned();
    Some((candidate_end, destination, label))
}

fn is_windows_root_at(content: &str, i: usize, end: usize) -> bool {
    let b = content.as_bytes();
    (i + 2 < end && b[i].is_ascii_alphabetic() && b[i + 1] == b':' && b[i + 2] == b'\\')
        || (i + 1 < end && b[i] == b'\\' && b[i + 1] == b'\\' && unc_has_share(&content[i..end]))
}

fn unc_has_share(s: &str) -> bool {
    let mut parts = s[2..].split('\\');
    parts.next().is_some_and(|p| !p.is_empty()) && parts.next().is_some_and(|p| !p.is_empty())
}

fn path_start_boundary(content: &str, i: usize) -> bool {
    i == 0
        || content[..i].chars().next_back().is_some_and(|c| {
            c.is_whitespace() || matches!(c, '(' | '[' | '{' | ':' | '>' | '"' | '\'')
        })
}

fn path_scan_end(content: &str, start: usize, end: usize) -> usize {
    content[start..end]
        .find(['\r', '\n'])
        .map_or(end, |n| start + n)
}

fn path_endpoints(content: &str, start: usize, end: usize) -> Vec<usize> {
    let mut points = vec![end];
    for (off, ch) in content[start..end].char_indices() {
        // Square brackets are valid Windows filename characters (and labels
        // escape them later), so they must not truncate a candidate such as
        // `C:\Backups\[Archive]`. A period, on the other hand, is a useful
        // progressively-tested endpoint for sentence-final paths; an interior
        // period is harmless because only probe-confirmed endpoints are used.
        if matches!(ch, ' ' | '\t' | ',' | ';' | '.' | ')' | '}') {
            points.push(start + off);
        }
    }
    points
}

fn path_label(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.as_bytes()[1] == b':' {
        return trimmed.to_owned();
    }
    std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|name| !name.contains('\\'))
        .or_else(|| trimmed.rsplit(['\\', '/']).find(|p| !p.is_empty()))
        .unwrap_or(trimmed)
        .to_owned()
}

fn windows_file_url(path: &str) -> Option<Url> {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//") {
        let (host, tail) = rest.split_once('/')?;
        let mut url = Url::parse(&format!("file://{host}/")).ok()?;
        {
            let mut segments = url.path_segments_mut().ok()?;
            segments.pop_if_empty();
            segments.extend(tail.split('/'));
        }
        encode_markdown_sensitive_url_path(&mut url);
        Some(url)
    } else {
        let mut url = Url::parse("file:///").ok()?;
        {
            let mut segments = url.path_segments_mut().ok()?;
            segments.pop_if_empty();
            segments.extend(normalized.split('/'));
        }
        encode_markdown_sensitive_url_path(&mut url);
        Some(url)
    }
}

fn encode_markdown_sensitive_url_path(url: &mut Url) {
    // The URL serializer intentionally permits square brackets in paths, but
    // leaving them literal makes a generated Markdown destination needlessly
    // fragile. Feed an escaped path back through `Url` so it remains the sole
    // destination parser/serializer rather than concatenating URL text here.
    if url.path().contains(['[', ']']) {
        let path = url.path().replace('[', "%5B").replace(']', "%5D");
        url.set_path(&path);
    }
}

fn contains_new_recognized_target(s: &str) -> bool {
    (0..s.len()).any(|i| {
        s.is_char_boundary(i)
            && (new_web_at(s, i, s.len()).is_some() || is_windows_root_at(s, i, s.len()))
    })
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
    use super::{PathClassification, convert_plain_links_with, wrap_plain_urls};

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

    fn convert_with(
        content: &str,
        entries: &[(&str, PathClassification)],
    ) -> super::PlainLinkConversionReport {
        convert_plain_links_with(content, |candidate| {
            entries
                .iter()
                .find_map(|(path, kind)| (*path == candidate).then_some(*kind))
                .unwrap_or(PathClassification::Missing)
        })
    }

    #[test]
    fn converter_uses_compact_web_labels_and_preserves_destinations() {
        let report = convert_with(
            "www.google.com, http://google.com https://www.youtube.com:443/watch?v=123#here",
            &[],
        );
        assert_eq!(
            report.content,
            "[google](https://www.google.com), [google](http://google.com) [youtube](https://www.youtube.com:443/watch?v=123#here)"
        );
        assert_eq!(report.web_links, 3);
    }

    #[test]
    fn converter_protects_markdown_code_wiki_and_is_idempotent() {
        let source = "[x](https://a.com) ![x](https://b.com) <https://c.com> [[https://d.com]] ![[https://e.com]] `https://f.com`\n~~~\nhttps://g.com\n~~~\nwww.google.com";
        let first = convert_with(source, &[]);
        assert_eq!(first.web_links, 1);
        assert_eq!(first.skipped_existing, 3);
        let second = convert_with(&first.content, &[]);
        assert_eq!(second.content, first.content);
        assert_eq!(
            (second.web_links, second.files, second.directories),
            (0, 0, 0)
        );
    }

    #[test]
    fn converter_handles_drive_unc_spaces_unicode_and_escaped_labels() {
        let entries = [
            (r"C:\Project Files\[Archive]", PathClassification::Directory),
            (r"\\server\share\文書\README.md", PathClassification::File),
        ];
        let report = convert_with(
            "- C:\\Project Files\\[Archive]\r\nthen \\\\server\\share\\文書\\README.md.",
            &entries,
        );
        assert!(
            report
                .content
                .contains(r"[\[Archive\]](file:///C:/Project%20Files/%5BArchive%5D)")
        );
        assert!(
            report
                .content
                .contains("[README.md](file://server/share/%E6%96%87%E6%9B%B8/README.md).")
        );
        assert_eq!((report.directories, report.files), (1, 1));
        for destination in report
            .content
            .split("](")
            .skip(1)
            .filter_map(|s| s.split(')').next())
        {
            let url = url::Url::parse(destination).unwrap();
            assert_eq!(url.scheme(), "file");
            assert!(!destination.contains(' '));
            assert!(!destination.contains('\\'));
        }
    }

    #[test]
    fn converter_rejects_partial_parent_and_non_absolute_forms() {
        let report = convert_with(
            r"C:\Existing\Missing .\x ..\x C:x ~\x %HOME%\x example.org/path",
            &[(r"C:\Existing", PathClassification::Directory)],
        );
        assert_eq!(
            report.content,
            r"C:\Existing\Missing .\x ..\x C:x ~\x %HOME%\x example.org/path"
        );
        assert_eq!(report.skipped_invalid_paths, 1);
        assert_eq!(
            (report.web_links, report.files, report.directories),
            (0, 0, 0)
        );
    }

    #[test]
    fn converter_preserves_crlf_and_mixes_paths_with_urls() {
        let report = convert_with(
            "C:\\Documents and https://github.com/org/repo\r\n",
            &[(r"C:\Documents", PathClassification::Directory)],
        );
        assert_eq!(
            report.content,
            "[Documents](file:///C:/Documents) and [github](https://github.com/org/repo)\r\n"
        );
        assert_eq!((report.web_links, report.directories), (1, 1));
    }
}
