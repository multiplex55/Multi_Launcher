use std::{error::Error, fmt, ops::Range};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationError {
    InvalidRange,
    InvalidBoundary,
    InvalidHeadingLevel,
}

impl fmt::Display for MutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => write!(f, "invalid range"),
            Self::InvalidBoundary => write!(f, "range is not on a UTF-8 character boundary"),
            Self::InvalidHeadingLevel => write!(f, "heading level must be between 1 and 6"),
        }
    }
}

impl Error for MutationError {}

pub fn char_range_to_byte_range(
    content: &str,
    char_range: Range<usize>,
) -> Result<Range<usize>, MutationError> {
    if char_range.start > char_range.end {
        return Err(MutationError::InvalidRange);
    }
    let char_count = content.chars().count();
    if char_range.end > char_count {
        return Err(MutationError::InvalidRange);
    }
    Ok(char_to_byte_index(content, char_range.start)..char_to_byte_index(content, char_range.end))
}

pub fn char_index_to_byte_index(content: &str, char_index: usize) -> Result<usize, MutationError> {
    if char_index > content.chars().count() {
        return Err(MutationError::InvalidRange);
    }
    Ok(char_to_byte_index(content, char_index))
}

fn char_to_byte_index(content: &str, char_index: usize) -> usize {
    content
        .char_indices()
        .map(|(idx, _)| idx)
        .nth(char_index)
        .unwrap_or(content.len())
}

fn validate_byte_range(content: &str, range: &Range<usize>) -> Result<(), MutationError> {
    if range.start > range.end || range.end > content.len() {
        return Err(MutationError::InvalidRange);
    }
    if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
        return Err(MutationError::InvalidBoundary);
    }
    Ok(())
}

pub fn toggle_checkbox_by_byte_range(
    content: &str,
    marker_byte_range: Range<usize>,
) -> Result<String, MutationError> {
    validate_byte_range(content, &marker_byte_range)?;
    let replacement = match content
        .get(marker_byte_range.clone())
        .ok_or(MutationError::InvalidBoundary)?
    {
        "[ ]" => "[x]",
        "[x]" | "[X]" => "[ ]",
        _ => return Err(MutationError::InvalidRange),
    };
    replace_byte_range(content, marker_byte_range, replacement)
}

pub fn toggle_checkbox_by_char_range(
    content: &str,
    marker_char_range: Range<usize>,
) -> Result<String, MutationError> {
    let byte_range = char_range_to_byte_range(content, marker_char_range)?;
    toggle_checkbox_by_byte_range(content, byte_range)
}

pub fn add_alias_metadata(content: &str, alias: &str) -> Result<String, MutationError> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(content.to_string());
    }
    if let Some((start, end, body)) = frontmatter(content) {
        if alias_exists(body, alias) {
            return Ok(content.to_string());
        }
        let insertion = if let Some(line) = aliases_line(body) {
            let absolute = start + line.end;
            insert_at(content, absolute, &format!("  - {}\n", yaml_scalar(alias)))?
        } else {
            let absolute = end - 4; // before closing ---\n
            insert_at(
                content,
                absolute,
                &format!("aliases:\n  - {}\n", yaml_scalar(alias)),
            )?
        };
        Ok(insertion)
    } else {
        Ok(format!(
            "---\naliases:\n  - {}\n---\n{}",
            yaml_scalar(alias),
            content
        ))
    }
}

pub fn remove_alias_metadata(content: &str, alias: &str) -> Result<String, MutationError> {
    let alias = alias.trim();
    let Some((start, _end, body)) = frontmatter(content) else {
        return Ok(content.to_string());
    };
    let Some(alias_line) = find_alias_item_line(body, alias) else {
        return Ok(content.to_string());
    };
    let abs = start + alias_line.start..start + alias_line.end;
    let mut updated = replace_byte_range(content, abs, "")?;
    if let Some((s, _e, b)) = frontmatter(&updated) {
        if let Some(header) = aliases_line(b) {
            let has_items = b[header.end..]
                .lines()
                .take_while(|line| line.starts_with("  - ") || line.trim().is_empty())
                .any(|line| line.starts_with("  - "));
            if !has_items {
                updated = replace_byte_range(&updated, s + header.start..s + header.end, "")?;
            }
        }
    }
    if let Some((_s, e, b)) = frontmatter(&updated) {
        if b.trim().is_empty() {
            updated = replace_byte_range(&updated, 0..e, "")?;
        }
    } else if updated.starts_with("---\n---\n") {
        updated = replace_byte_range(&updated, 0..8, "")?;
    }
    Ok(updated)
}

pub fn rename_alias_metadata(
    content: &str,
    old_alias: &str,
    new_alias: &str,
) -> Result<String, MutationError> {
    let new_alias = new_alias.trim();
    let without_old = remove_alias_metadata(content, old_alias)?;
    if new_alias.is_empty() {
        Ok(without_old)
    } else {
        add_alias_metadata(&without_old, new_alias)
    }
}

pub fn insert_callout(
    content: &str,
    byte_index: usize,
    kind: &str,
    title: &str,
) -> Result<String, MutationError> {
    validate_byte_range(content, &(byte_index..byte_index))?;
    insert_at(content, byte_index, &format_callout(kind, title, ""))
}

pub fn insert_callout_at_char_index(
    content: &str,
    char_index: usize,
    kind: &str,
    title: &str,
) -> Result<String, MutationError> {
    insert_callout(
        content,
        char_index_to_byte_index(content, char_index)?,
        kind,
        title,
    )
}

pub fn wrap_selection_in_callout(
    content: &str,
    byte_range: Range<usize>,
    kind: &str,
    title: &str,
) -> Result<String, MutationError> {
    validate_byte_range(content, &byte_range)?;
    let selected = content
        .get(byte_range.clone())
        .ok_or(MutationError::InvalidBoundary)?;
    replace_byte_range(content, byte_range, &format_callout(kind, title, selected))
}

pub fn wrap_char_selection_in_callout(
    content: &str,
    char_range: Range<usize>,
    kind: &str,
    title: &str,
) -> Result<String, MutationError> {
    wrap_selection_in_callout(
        content,
        char_range_to_byte_range(content, char_range)?,
        kind,
        title,
    )
}

pub fn insert_heading(
    content: &str,
    byte_index: usize,
    level: u8,
    text: &str,
) -> Result<String, MutationError> {
    if !(1..=6).contains(&level) {
        return Err(MutationError::InvalidHeadingLevel);
    }
    validate_byte_range(content, &(byte_index..byte_index))?;
    insert_at(
        content,
        byte_index,
        &format!("{} {}\n", "#".repeat(level as usize), text),
    )
}

pub fn insert_heading_at_char_index(
    content: &str,
    char_index: usize,
    level: u8,
    text: &str,
) -> Result<String, MutationError> {
    insert_heading(
        content,
        char_index_to_byte_index(content, char_index)?,
        level,
        text,
    )
}

pub fn insert_checkbox(
    content: &str,
    byte_index: usize,
    text: &str,
    checked: bool,
) -> Result<String, MutationError> {
    validate_byte_range(content, &(byte_index..byte_index))?;
    insert_at(
        content,
        byte_index,
        &format!("- [{}] {}\n", if checked { "x" } else { " " }, text),
    )
}

pub fn insert_checkbox_at_char_index(
    content: &str,
    char_index: usize,
    text: &str,
    checked: bool,
) -> Result<String, MutationError> {
    insert_checkbox(
        content,
        char_index_to_byte_index(content, char_index)?,
        text,
        checked,
    )
}

pub fn append_section(
    content: &str,
    level: u8,
    heading: &str,
    body: &str,
) -> Result<String, MutationError> {
    if !(1..=6).contains(&level) {
        return Err(MutationError::InvalidHeadingLevel);
    }
    let mut updated = String::from(content);
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&format!("{} {}\n", "#".repeat(level as usize), heading));
    updated.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        updated.push('\n');
    }
    Ok(updated)
}

pub fn replace_heading_text(
    content: &str,
    heading_byte_range: Range<usize>,
    new_text: &str,
) -> Result<String, MutationError> {
    validate_byte_range(content, &heading_byte_range)?;
    let line = content
        .get(heading_byte_range.clone())
        .ok_or(MutationError::InvalidBoundary)?;
    let hashes = line.trim_start().bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return Err(MutationError::InvalidRange);
    }
    let indent_len = line.len() - line.trim_start().len();
    let replacement = format!(
        "{}{} {}{}",
        &line[..indent_len],
        "#".repeat(hashes),
        new_text,
        line_end(line)
    );
    replace_byte_range(content, heading_byte_range, &replacement)
}

fn line_end(line: &str) -> &str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

fn replace_byte_range(
    content: &str,
    range: Range<usize>,
    replacement: &str,
) -> Result<String, MutationError> {
    validate_byte_range(content, &range)?;
    let mut out =
        String::with_capacity(content.len() - (range.end - range.start) + replacement.len());
    out.push_str(&content[..range.start]);
    out.push_str(replacement);
    out.push_str(&content[range.end..]);
    Ok(out)
}
fn insert_at(content: &str, byte_index: usize, insertion: &str) -> Result<String, MutationError> {
    replace_byte_range(content, byte_index..byte_index, insertion)
}

fn format_callout(kind: &str, title: &str, body: &str) -> String {
    let mut out = format!(
        "> [!{}]{}{}\n",
        kind.trim().to_ascii_uppercase(),
        if title.trim().is_empty() { "" } else { " " },
        title.trim()
    );
    for line in body.lines() {
        out.push_str("> ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[derive(Clone, Copy)]
struct LocalRange {
    start: usize,
    end: usize,
}

fn frontmatter(content: &str) -> Option<(usize, usize, &str)> {
    let rest = content.strip_prefix("---\n")?;
    let close = rest.find("\n---\n")?;
    Some((4, 4 + close + 5, &rest[..close + 1]))
}
fn aliases_line(body: &str) -> Option<LocalRange> {
    line_ranges(body).find(|r| body[r.start..r.end].trim() == "aliases:")
}
fn alias_exists(body: &str, alias: &str) -> bool {
    find_alias_item_line(body, alias).is_some()
}
fn find_alias_item_line(body: &str, alias: &str) -> Option<LocalRange> {
    line_ranges(body).find(|r| {
        let line = body[r.start..r.end].trim();
        line.strip_prefix("- ")
            .is_some_and(|v| unquote(v.trim()) == alias)
    })
}
fn line_ranges(s: &str) -> impl Iterator<Item = LocalRange> + '_ {
    let mut start = 0;
    s.split_inclusive('\n').map(move |line| {
        let end = start + line.len();
        let r = LocalRange { start, end };
        start = end;
        r
    })
}
fn yaml_scalar(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_headings_insert_append_and_replace() {
        let content = "Intro 🌱\n";
        let inserted = insert_heading_at_char_index(content, 8, 2, "日記 Café").unwrap();
        assert_eq!(inserted, "Intro 🌱\n## 日記 Café\n");
        let appended = append_section(&inserted, 3, "次", "本文 🌍").unwrap();
        assert!(appended.ends_with("\n### 次\n本文 🌍\n"));
        let start = appended.find("##").unwrap();
        let end = appended[start..].find('\n').map(|i| start + i + 1).unwrap();
        let replaced = replace_heading_text(&appended, start..end, "Résumé Ω").unwrap();
        assert!(replaced.contains("## Résumé Ω\n"));
    }

    #[test]
    fn unicode_aliases_add_remove_and_rename_preserve_body() {
        let content = "# タイトル\nBody 🌱\n";
        let added = add_alias_metadata(content, "Café 日記").unwrap();
        assert_eq!(
            added,
            "---\naliases:\n  - Café 日記\n---\n# タイトル\nBody 🌱\n"
        );
        let renamed = rename_alias_metadata(&added, "Café 日記", "京都 🦀").unwrap();
        assert_eq!(
            renamed,
            "---\naliases:\n  - \"京都 🦀\"\n---\n# タイトル\nBody 🌱\n"
        );
        let removed = remove_alias_metadata(&renamed, "京都 🦀").unwrap();
        assert_eq!(removed, content);
    }

    #[test]
    fn unicode_checkboxes_toggle_and_insert() {
        let content = "前置き 🌱\n- [ ] Café 日記\n";
        let marker = content.find("[ ]").unwrap();
        let toggled = toggle_checkbox_by_byte_range(content, marker..marker + 3).unwrap();
        assert_eq!(toggled, "前置き 🌱\n- [x] Café 日記\n");
        let inserted =
            insert_checkbox_at_char_index(&toggled, toggled.chars().count(), "終わり Ω", true)
                .unwrap();
        assert!(inserted.ends_with("- [x] 終わり Ω\n"));
    }

    #[test]
    fn unicode_selection_wraps_callout() {
        let content = "Alpha\n選択 🌱\nOmega\n";
        let start = content.find('選').unwrap();
        let end = content.find("Omega").unwrap();
        let wrapped = wrap_selection_in_callout(content, start..end, "tip", "読む").unwrap();
        assert_eq!(wrapped, "Alpha\n> [!TIP] 読む\n> 選択 🌱\nOmega\n");
        let inserted = insert_callout_at_char_index("🌱", 1, "note", "次").unwrap();
        assert_eq!(inserted, "🌱> [!NOTE] 次\n");
    }

    #[test]
    fn invalid_byte_and_char_boundaries_return_errors() {
        let content = "é [ ]";
        assert_eq!(
            toggle_checkbox_by_byte_range(content, 1..4),
            Err(MutationError::InvalidBoundary)
        );
        assert_eq!(
            insert_heading_at_char_index(content, 99, 1, "Nope"),
            Err(MutationError::InvalidRange)
        );
        assert_eq!(
            wrap_char_selection_in_callout(content, 3..2, "note", ""),
            Err(MutationError::InvalidRange)
        );
    }
}
