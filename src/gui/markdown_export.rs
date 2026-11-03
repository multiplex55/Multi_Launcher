//! Shared filesystem publication safeguards for explicit Markdown exports.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::notes_markdown::{
    LocalRasterAsset, LocalRasterFormat, LocalRasterResolver, MAX_LOCAL_RASTER_AGGREGATE_BYTES,
    MAX_LOCAL_RASTER_BYTES, MAX_MARKDOWN_HTML_IMAGES, MarkdownExportArtifact, MarkdownExportFormat,
    MarkdownPreview, export_html,
};
use pulldown_cmark::{Event, Options, Parser, Tag};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ParsedNoteImageTarget {
    pub(super) rel: String,
    pub(super) full: PathBuf,
    pub(super) width: Option<f32>,
}

/// Shared with the Note preview so `assets/...|width` keeps the same authored
/// spelling while export applies a stricter root-only file policy.
pub(super) fn parse_note_image_target(target: &str, assets_root: &Path) -> ParsedNoteImageTarget {
    let (rel, width) = if let Some((path, width)) = target.split_once('|') {
        (path.trim(), width.trim().parse::<f32>().ok())
    } else {
        (target.trim(), None)
    };
    let full = if let Some(stripped) = rel.strip_prefix("assets/") {
        assets_root.join(stripped)
    } else {
        PathBuf::from(rel)
    };
    ParsedNoteImageTarget {
        rel: rel.to_owned(),
        full,
        width,
    }
}

pub(crate) fn export_note_html(
    source: &str,
    assets_root: &Path,
) -> Result<MarkdownExportArtifact, String> {
    // Analyze and enforce all source/line/fence bounds before touching local
    // assets. The accepted HTML encoder then reuses this exact preview.
    let preview = MarkdownPreview::analyze(source).map_err(|error| error.to_string())?;
    let assets = NoteRasterAssets::load(source, assets_root);
    export_html(&preview, Some(&assets)).map_err(|error| error.to_string())
}

struct OwnedRaster {
    bytes: Vec<u8>,
    format: LocalRasterFormat,
}

struct NoteRasterAssets {
    by_destination: HashMap<String, usize>,
    width_by_destination: HashMap<String, Option<f32>>,
    files: Vec<OwnedRaster>,
}

impl NoteRasterAssets {
    fn load(source: &str, assets_root: &Path) -> Self {
        Self::load_with_limits(
            source,
            assets_root,
            MAX_LOCAL_RASTER_BYTES,
            MAX_LOCAL_RASTER_AGGREGATE_BYTES,
        )
    }

    fn load_with_limits(
        source: &str,
        assets_root: &Path,
        per_image_read_limit: usize,
        aggregate_read_limit: usize,
    ) -> Self {
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_FOOTNOTES;
        let mut destinations = Vec::new();
        let mut seen = HashSet::new();
        let mut image_count = 0usize;
        for (event, _) in Parser::new_ext(source, options).into_offset_iter() {
            if let Event::Start(Tag::Image { dest_url, .. }) = event {
                image_count = image_count.saturating_add(1);
                if image_count > MAX_MARKDOWN_HTML_IMAGES {
                    break;
                }
                let destination = dest_url.to_string();
                if seen.insert(destination.clone()) {
                    destinations.push(destination);
                }
            }
        }
        if destinations.is_empty() {
            return Self::default();
        }
        let Ok(root) = fs::canonicalize(assets_root) else {
            return Self::default();
        };
        if !root.is_dir() {
            return Self::default();
        }

        let mut result = Self::default();
        let mut paths = HashMap::<PathBuf, usize>::new();
        let mut rejected_paths = HashSet::<PathBuf>::new();
        let mut aggregate_read_bytes = 0usize;
        for destination in destinations {
            let parsed = parse_note_image_target(&destination, assets_root);
            result
                .width_by_destination
                .insert(destination.clone(), parsed.width);
            let Some(relative) = destination_path_under_assets(&parsed.rel) else {
                continue;
            };
            let candidate = root.join(relative);
            let Ok(path) = fs::canonicalize(candidate) else {
                continue;
            };
            if !path.starts_with(&root) {
                continue;
            }
            if let Some(index) = paths.get(&path).copied() {
                result.by_destination.insert(destination, index);
                continue;
            }
            if rejected_paths.contains(&path) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.file_type().is_file() {
                continue;
            }
            let remaining = aggregate_read_limit.saturating_sub(aggregate_read_bytes);
            let read_limit = per_image_read_limit.saturating_add(1).min(remaining);
            if read_limit == 0 {
                rejected_paths.insert(path);
                continue;
            }
            let Ok(file) = File::open(&path) else {
                continue;
            };
            let mut bytes = Vec::new();
            let read_result = file.take(read_limit as u64).read_to_end(&mut bytes);
            aggregate_read_bytes = aggregate_read_bytes.saturating_add(bytes.len());
            if read_result.is_err()
                || bytes.is_empty()
                || bytes.len() > per_image_read_limit
                // Reaching the per-image or remaining aggregate cap leaves
                // EOF unproven, so do not accept a possibly truncated image.
                || bytes.len() == read_limit
            {
                rejected_paths.insert(path);
                continue;
            }
            let Some(format) = raster_format(&bytes) else {
                rejected_paths.insert(path);
                continue;
            };
            let index = result.files.len();
            result.files.push(OwnedRaster { bytes, format });
            paths.insert(path, index);
            result.by_destination.insert(destination, index);
        }
        result
    }
}

impl Default for NoteRasterAssets {
    fn default() -> Self {
        Self {
            by_destination: HashMap::new(),
            width_by_destination: HashMap::new(),
            files: Vec::new(),
        }
    }
}

impl LocalRasterResolver for NoteRasterAssets {
    fn resolve<'a>(&'a self, relative_destination: &str) -> Option<LocalRasterAsset<'a>> {
        let index = *self.by_destination.get(relative_destination)?;
        let file = self.files.get(index)?;
        Some(LocalRasterAsset {
            format: file.format,
            bytes: &file.bytes,
        })
    }

    fn width_hint_px(&self, relative_destination: &str) -> Option<f32> {
        self.width_by_destination
            .get(relative_destination)
            .copied()
            .flatten()
    }
}

fn destination_path_under_assets(relative: &str) -> Option<&Path> {
    let path = relative.strip_prefix("assets/")?;
    if path.is_empty()
        || path.len() > 8_192
        || path.contains(':')
        || path.contains('\\')
        || path.starts_with('/')
        || path.chars().any(char::is_control)
    {
        return None;
    }
    let path = Path::new(path);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(path)
}

fn raster_format(bytes: &[u8]) -> Option<LocalRasterFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(LocalRasterFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(LocalRasterFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(LocalRasterFormat::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(LocalRasterFormat::WebP)
    } else {
        None
    }
}

pub(crate) fn publish_export(
    artifact: &MarkdownExportArtifact,
    destination: &Path,
    active_path: Option<&Path>,
) -> Result<(), String> {
    if let Some(active) = active_path {
        if paths_equal(active, destination).map_err(|error| error.to_string())? {
            return Err("Export cannot overwrite the active Markdown document.".into());
        }
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(format!(
                "Refusing to overwrite occupied export destination {}.",
                destination.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not inspect export destination {}: {error}",
                destination.display()
            ));
        }
    }
    crate::common::atomic_file::save_atomic(destination, artifact.bytes())
        .map_err(|error| format!("Could not write export {}: {error}", destination.display()))
}

pub(crate) fn paths_equal(left: &Path, right: &Path) -> io::Result<bool> {
    let left = resolved_path(left)?;
    let right = resolved_path(right)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
        let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
        let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
        if left.len() > i32::MAX as usize || right.len() > i32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is too long",
            ));
        }
        let result = unsafe { CompareStringOrdinal(&left, &right, BOOL(1)) };
        if result.0 == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result == CSTR_EQUAL)
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

pub(crate) fn resolved_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = absolute;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(&current) {
            Ok(mut resolved) => {
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = current.file_name() else {
                    return Err(error);
                };
                missing.push(name.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(error);
                };
                current = parent.to_path_buf();
            }
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        path.set_extension(extension);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image_025::DynamicImage::ImageRgba8(image_025::ImageBuffer::from_pixel(
            width,
            height,
            image_025::Rgba([40, 80, 120, 255]),
        ));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image_025::ImageFormat::Png)
            .expect("PNG fixture should encode");
        encoded.into_inner()
    }

    #[test]
    fn note_html_export_reads_bounded_assets_and_preserves_width_and_warnings() {
        let directory = tempfile::tempdir().unwrap();
        let assets = directory.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("local.png"), png_bytes(2, 2)).unwrap();
        fs::write(directory.path().join("outside.png"), png_bytes(1, 1)).unwrap();
        let source = concat!(
            "![local alt](assets/local.png|140)\n\n",
            "![escape](assets/../outside.png)\n\n",
            "![remote](https://example.com/remote.png)\n\n",
            "```mermaid\nflowchart TD\nA -->\n```\n",
        );
        let artifact = export_note_html(source, &assets).unwrap();
        let html = std::str::from_utf8(artifact.bytes()).unwrap();
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("alt=\"local alt\" width=\"140px\""));
        assert!(html.contains("escape"));
        assert!(html.contains("remote"));
        assert!(html.contains("Mermaid preview unavailable"));
        assert_eq!(artifact.warnings().unavailable_images(), 1);
        assert_eq!(artifact.warnings().blocked_remote_images(), 1);
        assert_eq!(artifact.warnings().unavailable_mermaid_previews(), 1);
    }

    #[test]
    fn note_html_export_budgets_bytes_read_even_for_rejected_assets() {
        let directory = tempfile::tempdir().unwrap();
        let assets = directory.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("bad-one.bin"), vec![b'x'; 129]).unwrap();
        fs::write(assets.join("bad-two.bin"), vec![b'y'; 129]).unwrap();
        fs::write(assets.join("valid.png"), png_bytes(1, 1)).unwrap();
        let source = concat!(
            "![first](assets/bad-one.bin)\n\n",
            "![second](assets/bad-two.bin)\n\n",
            "![valid](assets/valid.png)\n",
        );

        let resolver = NoteRasterAssets::load_with_limits(source, &assets, 128, 256);
        let preview = MarkdownPreview::analyze(source).unwrap();
        let artifact = export_html(&preview, Some(&resolver)).unwrap();
        let html = std::str::from_utf8(artifact.bytes()).unwrap();

        assert!(!html.contains("data:image/png;base64,"));
        assert!(html.contains("valid"));
        assert_eq!(artifact.warnings().unavailable_images(), 3);
    }

    #[test]
    fn export_publication_refuses_active_deleted_and_occupied_paths() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = crate::notes_markdown::export_markdown("# exact\r\n").unwrap();
        let deleted_active = directory.path().join("deleted.md");
        let same_deleted_destination = directory.path().join(".").join("deleted.md");
        assert!(!deleted_active.exists());
        let error = publish_export(&artifact, &same_deleted_destination, Some(&deleted_active))
            .unwrap_err();
        assert!(error.contains("active Markdown"));
        assert!(!deleted_active.exists());

        let occupied = directory.path().join("occupied.html");
        fs::write(&occupied, b"keep me").unwrap();
        let error = publish_export(&artifact, &occupied, None).unwrap_err();
        assert!(error.contains("occupied export destination"));
        assert_eq!(fs::read(&occupied).unwrap(), b"keep me");

        let failed_parent = directory.path().join("not-a-directory");
        fs::write(&failed_parent, b"keep me too").unwrap();
        let failed_write = failed_parent.join("failed.md");
        let error = publish_export(&artifact, &failed_write, None).unwrap_err();
        assert!(
            error.contains("Could not write export")
                || error.contains("Could not inspect export destination")
        );
        assert_eq!(fs::read(&failed_parent).unwrap(), b"keep me too");
        assert!(!failed_write.exists());

        #[cfg(windows)]
        {
            let mixed_case = directory.path().join("Deleted.MD");
            assert!(paths_equal(&deleted_active, &mixed_case).unwrap());
            assert!(publish_export(&artifact, &mixed_case, Some(&deleted_active)).is_err());
        }
    }
}
