//! Typed display-only classification of direct Downloads observations.
mod store;

use crate::file_operations::ScanCancellation;
use crate::file_transfer::{
    AdmissionError, DownloadsScan, DownloadsScanCompletion, EntryKind, EntrySnapshot,
    downloads_review_key, scan_downloads_directory,
};
use std::{
    collections::HashMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub(crate) use store::{DownloadsPreferences, DownloadsStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Folder,
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Installer,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Unknown,
    WebHost(String),
}
#[derive(Debug)]
pub(crate) struct Row {
    pub entry: EntrySnapshot,
    pub review_proof: [u8; 32],
    pub category: Category,
    pub incomplete: bool,
    pub large: bool,
    pub older_installer: bool,
    pub suspected_same_size: bool,
    pub origin: Origin,
}
#[derive(Debug)]
pub(crate) struct Skipped {
    pub name: std::ffi::OsString,
    pub reason: AdmissionError,
}
#[derive(Debug)]
pub(crate) struct Scan {
    pub root: crate::file_transfer::ParentSnapshot,
    pub rows: Vec<Row>,
    pub skipped: Vec<Skipped>,
    pub completion: DownloadsScanCompletion,
}

/// Use the explicit preference even when that location is unavailable.
pub(crate) fn proposed_root(configured: Option<&Path>) -> std::io::Result<std::path::PathBuf> {
    match configured {
        Some(path) => Ok(path.to_path_buf()),
        None => crate::platform::downloads::downloads_known_folder(),
    }
}

pub(crate) fn scan(
    path: &Path,
    cancel: &ScanCancellation,
    now: SystemTime,
) -> Result<Scan, AdmissionError> {
    scan_downloads_directory(path, cancel).map(|raw| classify(raw, now))
}
fn classify(raw: DownloadsScan, now: SystemTime) -> Scan {
    let now_ticks = now
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| d.as_secs().checked_add(11_644_473_600))
        .and_then(|n| n.checked_mul(10_000_000))
        .and_then(|n| i64::try_from(n).ok());
    let cutoff = now_ticks.and_then(|ticks| ticks.checked_sub(30 * 24 * 60 * 60 * 10_000_000_i64));
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for item in raw.rows {
        match item.entry {
            Err(reason) => skipped.push(Skipped {
                name: item.name,
                reason,
            }),
            Ok(entry) => {
                let name = entry.leaf().as_os_str().to_string_lossy();
                let (category, incomplete) = classify_name(&name, entry.metadata().kind);
                let meta = entry.metadata();
                let older_installer = category == Category::Installer
                    && cutoff.is_some_and(|cutoff| meta.modified > 0 && meta.modified <= cutoff);
                let large = meta.kind == EntryKind::File
                    && meta.byte_len.is_some_and(|n| n >= 100 * 1024 * 1024);
                rows.push(Row {
                    review_proof: downloads_review_key(&entry),
                    entry,
                    category,
                    incomplete,
                    large,
                    older_installer,
                    suspected_same_size: false,
                    origin: item
                        .zone
                        .as_deref()
                        .map(parse_zone)
                        .unwrap_or(Origin::Unknown),
                });
            }
        }
    }
    let mut sizes = HashMap::<u64, usize>::new();
    for row in &rows {
        if row.entry.metadata().kind == EntryKind::File {
            if let Some(n) = row.entry.metadata().byte_len {
                *sizes.entry(n).or_default() += 1;
            }
        }
    }
    for row in &mut rows {
        row.suspected_same_size = row.entry.metadata().kind == EntryKind::File
            && row
                .entry
                .metadata()
                .byte_len
                .is_some_and(|n| sizes.get(&n).copied().unwrap_or(0) > 1);
    }
    Scan {
        root: raw.root,
        rows,
        skipped,
        completion: raw.completion,
    }
}
fn classify_name(name: &str, kind: EntryKind) -> (Category, bool) {
    if kind == EntryKind::Directory {
        return (Category::Folder, false);
    }
    let lower = name.to_ascii_lowercase();
    let incomplete = [".crdownload", ".part", ".partial", ".download"]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    let ext = lower.rsplit_once('.').map_or("", |(_, ext)| ext);
    let category = match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" => Category::Image,
        "mp4" | "mkv" | "mov" | "avi" | "webm" => Category::Video,
        "mp3" | "wav" | "flac" | "m4a" | "ogg" => Category::Audio,
        "pdf" | "doc" | "docx" | "txt" | "md" | "xls" | "xlsx" | "ppt" | "pptx" => {
            Category::Document
        }
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" => Category::Archive,
        "exe" | "msi" | "msix" | "msp" => Category::Installer,
        _ => Category::Other,
    };
    (category, incomplete)
}
/// Only ZoneTransfer's single valid Internet/Restricted ZoneId and an HTTP(S)
/// HostUrl supply a hostname; no URL or referrer is retained in the model.
fn parse_zone(bytes: &[u8]) -> Origin {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Origin::Unknown;
    };
    let mut section = false;
    let mut seen_section = false;
    let mut zone = None;
    let mut host = None;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Origin::Unknown;
            }
            section = line.eq_ignore_ascii_case("[ZoneTransfer]");
            if section {
                if seen_section {
                    return Origin::Unknown;
                }
                seen_section = true;
            }
            continue;
        }
        if !section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Origin::Unknown;
        };
        if key.trim().eq_ignore_ascii_case("ZoneId") {
            if zone.is_some() {
                return Origin::Unknown;
            }
            zone = value.trim().parse::<u8>().ok().filter(|n| *n <= 4);
            if zone.is_none() {
                return Origin::Unknown;
            }
        } else if key.trim().eq_ignore_ascii_case("HostUrl") {
            if host.is_some() {
                return Origin::Unknown;
            }
            let Ok(url) = url::Url::parse(value.trim()) else {
                return Origin::Unknown;
            };
            if !matches!(url.scheme(), "http" | "https") {
                return Origin::Unknown;
            }
            let Some(name) = url.host_str() else {
                return Origin::Unknown;
            };
            host = Some(name.to_owned());
        }
    }
    match (seen_section, zone, host) {
        (true, Some(3 | 4), Some(host)) => Origin::WebHost(host),
        _ => Origin::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_triage_explicit_missing_root_does_not_fall_back() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        assert_eq!(proposed_root(Some(&missing)).unwrap(), missing);
        assert!(!missing.exists());
    }

    #[test]
    fn downloads_triage_category_and_incomplete() {
        assert_eq!(
            classify_name("SETUP.MSI", EntryKind::File),
            (Category::Installer, false)
        );
        assert_eq!(
            classify_name("image.jpg.crdownload", EntryKind::File),
            (Category::Other, true)
        );
        assert_eq!(
            classify_name("archive.zip", EntryKind::Directory),
            (Category::Folder, false)
        );
    }
    #[test]
    fn downloads_triage_zone_requires_valid_header_zone_and_http_host() {
        assert_eq!(
            parse_zone(
                b"[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/path?secret=1\r\n"
            ),
            Origin::WebHost("example.com".into())
        );
        for input in [
            b"ZoneId=3\nHostUrl=https://example.com".as_slice(),
            b"[ZoneTransfer]\nZoneId=3\nHostUrl=file:///tmp",
            b"[ZoneTransfer]\nZoneId=3\nZoneId=4\nHostUrl=https://example.com",
            b"[ZoneTransfer]\nZoneId=2\nHostUrl=https://example.com",
            b"\xff",
        ] {
            assert_eq!(parse_zone(input), Origin::Unknown);
        }
    }
}
