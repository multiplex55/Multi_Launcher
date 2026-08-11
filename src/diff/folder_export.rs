//! Deterministic, UI-independent folder comparison exports.
use crate::diff::folder_compare::{FolderEntry, FolderModel, FolderStatus};
use crate::diff::model::FolderDisplayFilter;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderExportFormat {
    Csv,
    Html,
    PlainText,
}

/// An owned export input.  Constructing this value is the synchronization
/// boundary: later scan/refinement changes to `FolderModel` cannot affect it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderExportSnapshot {
    pub rows: Vec<FolderExportRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderExportRow {
    pub relative_path: String,
    pub status: FolderStatus,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_modified: Option<SystemTime>,
    pub right_modified: Option<SystemTime>,
    pub content_checked: bool,
}

impl FolderExportSnapshot {
    pub fn complete(model: &FolderModel) -> Self {
        Self::from_entries(model.entries.values(), false)
    }

    pub fn filtered(
        model: &FolderModel,
        filter: FolderDisplayFilter,
        path_filter: &str,
        descending: bool,
    ) -> Self {
        let query = normalized_path_text(Path::new(path_filter)).to_lowercase();
        let entries: Vec<_> = model
            .entries
            .values()
            .filter(|entry| {
                normalized_path_text(&entry.relative_path)
                    .to_lowercase()
                    .contains(&query)
                    && status_matches(&filter, entry.effective_status)
            })
            .collect();
        Self::from_entries(entries, descending)
    }

    fn from_entries<'a>(
        entries: impl IntoIterator<Item = &'a FolderEntry>,
        descending: bool,
    ) -> Self {
        let mut rows: Vec<_> = entries
            .into_iter()
            .map(|entry| FolderExportRow {
                relative_path: normalized_path_text(&entry.relative_path),
                status: entry.effective_status,
                left_size: entry
                    .left
                    .as_ref()
                    .and_then(|side| side.metadata.as_ref())
                    .map(|m| m.size),
                right_size: entry
                    .right
                    .as_ref()
                    .and_then(|side| side.metadata.as_ref())
                    .map(|m| m.size),
                left_modified: entry
                    .left
                    .as_ref()
                    .and_then(|side| side.metadata.as_ref())
                    .and_then(|m| m.modified),
                right_modified: entry
                    .right
                    .as_ref()
                    .and_then(|side| side.metadata.as_ref())
                    .and_then(|m| m.modified),
                content_checked: entry.content_checked,
            })
            .collect();
        rows.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        if descending {
            rows.reverse();
        }
        Self { rows }
    }

    pub fn render(&self, format: FolderExportFormat) -> String {
        match format {
            FolderExportFormat::Csv => self.csv(),
            FolderExportFormat::Html => self.html(),
            FolderExportFormat::PlainText => self.plain_text(),
        }
    }

    fn csv(&self) -> String {
        let mut out = "relative path,status,left size,right size,left modified time,right modified time,content checked\r\n".to_owned();
        for r in &self.rows {
            let cells = [
                csv_escape(&r.relative_path),
                status(r.status).into(),
                optional(r.left_size),
                optional(r.right_size),
                time(r.left_modified),
                time(r.right_modified),
                r.content_checked.to_string(),
            ];
            out.push_str(&cells.join(","));
            out.push_str("\r\n");
        }
        out
    }
    fn plain_text(&self) -> String {
        let mut out = "relative path\tstatus\tleft size\tright size\tleft modified time\tright modified time\tcontent checked\n".to_owned();
        for r in &self.rows {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                text_escape(&r.relative_path),
                status(r.status),
                optional(r.left_size),
                optional(r.right_size),
                time(r.left_modified),
                time(r.right_modified),
                r.content_checked
            ));
        }
        out
    }
    fn html(&self) -> String {
        let mut out = "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Folder comparison</title>\n<table>\n<thead><tr><th>Relative path</th><th>Status</th><th>Left size</th><th>Right size</th><th>Left modified time</th><th>Right modified time</th><th>Content checked</th></tr></thead>\n<tbody>\n".to_owned();
        for r in &self.rows {
            out.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n", html_escape(&r.relative_path), status(r.status), optional(r.left_size), optional(r.right_size), time(r.left_modified), time(r.right_modified), r.content_checked));
        }
        out.push_str("</tbody>\n</table>\n");
        out
    }
}

fn normalized_path_text(path: &Path) -> String {
    path.iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
fn status(s: FolderStatus) -> &'static str {
    match s {
        FolderStatus::Identical => "identical",
        FolderStatus::Different => "different",
        FolderStatus::LeftOnly => "left only",
        FolderStatus::RightOnly => "right only",
        FolderStatus::LeftNewer => "left newer",
        FolderStatus::RightNewer => "right newer",
        FolderStatus::PendingContentComparison => "pending content comparison",
        FolderStatus::Unreadable => "unreadable",
        FolderStatus::Error => "error",
    }
}
fn status_matches(f: &FolderDisplayFilter, s: FolderStatus) -> bool {
    f.matches(s)
}
fn optional(v: Option<u64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}
fn time(v: Option<SystemTime>) -> String {
    v.map(|x| DateTime::<Utc>::from(x).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}
fn csv_escape(v: &str) -> String {
    if v.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.into()
    }
}
fn html_escape(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
fn text_escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::folder_compare::{EntryKind, EntryMetadata, EntrySide, FolderEntry};
    use std::collections::BTreeMap;
    fn entry(
        path: &str,
        status: FolderStatus,
        left: Option<u64>,
        right: Option<u64>,
    ) -> FolderEntry {
        let side = |size| EntrySide {
            path: path.into(),
            metadata: Some(EntryMetadata {
                kind: EntryKind::File,
                size,
                modified: None,
                identity: None,
            }),
            error: None,
        };
        FolderEntry {
            relative_path: path.into(),
            left: left.map(side),
            right: right.map(side),
            metadata_status: status,
            effective_status: status,
            content_checked: false,
        }
    }
    #[test]
    fn statuses_absent_metadata_and_stable_unicode_order() {
        let statuses = [
            FolderStatus::Identical,
            FolderStatus::Different,
            FolderStatus::LeftOnly,
            FolderStatus::RightOnly,
            FolderStatus::LeftNewer,
            FolderStatus::RightNewer,
            FolderStatus::PendingContentComparison,
            FolderStatus::Unreadable,
            FolderStatus::Error,
        ];
        let mut model = FolderModel {
            entries: BTreeMap::new(),
            revision: 1,
        };
        for (i, s) in statuses.into_iter().enumerate() {
            let path = format!("é/{:02}", statuses.len() - i);
            model.entries.insert(
                format!("key{i}"),
                entry(
                    &path,
                    s,
                    (s != FolderStatus::RightOnly).then_some(i as u64),
                    (s != FolderStatus::LeftOnly).then_some(i as u64),
                ),
            );
        }
        let snap = FolderExportSnapshot::complete(&model);
        assert_eq!(snap.rows.len(), 9);
        assert!(
            snap.rows
                .windows(2)
                .all(|w| w[0].relative_path <= w[1].relative_path)
        );
        assert!(
            snap.rows
                .iter()
                .find(|r| r.status == FolderStatus::LeftOnly)
                .unwrap()
                .right_size
                .is_none()
        );
        assert!(
            snap.rows
                .iter()
                .find(|r| r.status == FolderStatus::RightOnly)
                .unwrap()
                .left_size
                .is_none()
        );
    }
    #[test]
    fn filtered_is_a_snapshot_and_not_the_complete_model() {
        let mut m = FolderModel::default();
        m.entries.insert(
            "a".into(),
            entry("a", FolderStatus::Identical, Some(1), Some(1)),
        );
        m.entries.insert(
            "b".into(),
            entry("b", FolderStatus::LeftOnly, Some(2), None),
        );
        let snap = FolderExportSnapshot::filtered(&m, FolderDisplayFilter::LeftOnly, "", false);
        m.entries.clear();
        assert_eq!(snap.rows.len(), 1);
        assert_eq!(snap.rows[0].relative_path, "b");
    }
    #[test]
    fn csv_quotes_and_html_escapes() {
        let mut m = FolderModel::default();
        m.entries.insert(
            "x".into(),
            entry("a,\"b\n<&>.txt", FolderStatus::Identical, None, None),
        );
        let s = FolderExportSnapshot::complete(&m);
        assert!(s.csv().contains("\"a,\"\"b\n<&>.txt\""));
        assert!(s.html().contains("&lt;&amp;&gt;"));
    }
}
