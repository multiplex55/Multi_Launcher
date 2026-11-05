//! Bounded local raster intake for immutable visual-document assets.

use super::visual::{
    MAX_VISUAL_ASSET_BYTES, MAX_VISUAL_ASSET_DIMENSION, MAX_VISUAL_ASSET_PIXELS, VisualAsset,
    VisualAssetFormat, VisualAssetId,
};
use image::ImageFormat;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct PreparedVisualAsset {
    pub(crate) asset: VisualAsset,
    pub(crate) rgba: Arc<[u8]>,
}

#[derive(Clone)]
pub(crate) struct DecodedVisualAsset {
    pub(crate) id: VisualAssetId,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Arc<[u8]>,
}

/// Read and decode one local regular file. The file path is used only during
/// intake; only its verified content and metadata leave this function.
pub(crate) fn prepare_visual_asset_file(
    path: &Path,
    renderer_max_side: usize,
) -> Result<PreparedVisualAsset, String> {
    precheck_local_disk_path(path)?;
    let file = File::open(path).map_err(|error| format!("could not open image: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("could not inspect image file: {error}"))?;
    if !metadata.is_file() {
        return Err("selected image is not a regular file".into());
    }
    if metadata.len() > MAX_VISUAL_ASSET_BYTES as u64 {
        return Err(format!(
            "image exceeds the {MAX_VISUAL_ASSET_BYTES}-byte embedded asset limit"
        ));
    }
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(MAX_VISUAL_ASSET_BYTES)
        .min(MAX_VISUAL_ASSET_BYTES);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_VISUAL_ASSET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read image: {error}"))?;
    prepare_visual_asset_bytes(bytes, renderer_max_side)
}

#[cfg(windows)]
fn precheck_local_disk_path(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    use std::path::{Component, PathBuf, Prefix};
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, GetDriveTypeW};
    use windows::core::PCWSTR;

    if !path.is_absolute() {
        return Err("image must be selected from an absolute local disk path".into());
    }
    let prefix = path.components().find_map(|component| match component {
        Component::Prefix(prefix) => Some(prefix.kind()),
        _ => None,
    });
    let drive = match prefix {
        Some(Prefix::Disk(drive) | Prefix::VerbatimDisk(drive)) => drive,
        Some(Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..)) => {
            return Err("image must be on a local disk, not a UNC or device namespace".into());
        }
        // Other verbatim namespaces (for example GLOBALROOT) are not ordinary
        // local file paths, even if they happen to name a mounted volume.
        Some(Prefix::Verbatim(..)) | None => {
            return Err("image must be selected from an absolute local disk path".into());
        }
    };
    let mut root = [0u16; 4];
    root[0] = u16::from(drive.to_ascii_uppercase());
    root[1] = u16::from(b':');
    root[2] = u16::from(b'\\');
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
    // GetDriveTypeW values: unknown=0, no-root=1, remote=4. Disk types 2, 3,
    // 5, and 6 are local removable, fixed, optical, or RAM disks.
    if !matches!(drive_type, 2 | 3 | 5 | 6) {
        return Err("image must be on a local disk; remote drives are not embedded".into());
    }

    for component in path.components() {
        match component {
            Component::Normal(name) if is_dos_device_name(name) => {
                return Err("Windows device aliases cannot be embedded as images".into());
            }
            Component::CurDir | Component::ParentDir => {
                return Err("image path must not traverse relative components".into());
            }
            _ => {}
        }
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(name) => current.push(name),
            Component::CurDir | Component::ParentDir => unreachable!("checked above"),
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
                return Err("image path cannot traverse reparse points or symbolic links".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!("could not inspect local image path: {error}"));
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
fn is_dos_device_name(component: &std::ffi::OsStr) -> bool {
    let name = component.to_string_lossy();
    let name = name.trim_end_matches([' ', '.']);
    let base = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let bytes = base.as_bytes();
    matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (bytes.len() == 4
        && matches!(&bytes[..3], b"COM" | b"LPT")
        && matches!(bytes[3], b'1'..=b'9'))
}

#[cfg(not(windows))]
fn precheck_local_disk_path(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(crate) fn prepare_visual_asset_bytes(
    bytes: Vec<u8>,
    renderer_max_side: usize,
) -> Result<PreparedVisualAsset, String> {
    let (format, width, height) = crate::image_compare::inspect_local_png_jpeg(
        &bytes,
        MAX_VISUAL_ASSET_BYTES,
        MAX_VISUAL_ASSET_PIXELS,
    )?;
    if width > MAX_VISUAL_ASSET_DIMENSION || height > MAX_VISUAL_ASSET_DIMENSION {
        return Err(format!(
            "image dimensions exceed the {MAX_VISUAL_ASSET_DIMENSION}-pixel side limit"
        ));
    }
    if usize::try_from(width).unwrap_or(usize::MAX) > renderer_max_side
        || usize::try_from(height).unwrap_or(usize::MAX) > renderer_max_side
    {
        return Err(format!(
            "image exceeds the renderer's {renderer_max_side}-pixel texture side limit"
        ));
    }
    let decoded = crate::image_compare::decode_local_png_jpeg(
        &bytes,
        MAX_VISUAL_ASSET_BYTES,
        MAX_VISUAL_ASSET_PIXELS,
    )?;
    if decoded.format != format || decoded.width != width || decoded.height != height {
        return Err("decoded image differs from its bounded preflight header".into());
    }
    let format = match decoded.format {
        ImageFormat::Png => VisualAssetFormat::Png,
        ImageFormat::Jpeg => VisualAssetFormat::Jpeg,
        _ => return Err("only PNG and JPEG images can be embedded".into()),
    };
    let id = VisualAssetId::new(hex::encode(Sha256::digest(&bytes)))
        .map_err(|error| error.to_string())?;
    let payload = Arc::from(bytes.into_boxed_slice());
    Ok(PreparedVisualAsset {
        asset: VisualAsset::from_verified_parts(id, format, decoded.width, decoded.height, payload),
        rgba: decoded.rgba,
    })
}

pub(crate) fn decode_visual_asset_payload(
    asset: &VisualAsset,
    renderer_max_side: usize,
) -> Result<DecodedVisualAsset, String> {
    let (format, width, height) = crate::image_compare::inspect_local_png_jpeg(
        asset.payload(),
        MAX_VISUAL_ASSET_BYTES,
        MAX_VISUAL_ASSET_PIXELS,
    )?;
    if width > MAX_VISUAL_ASSET_DIMENSION || height > MAX_VISUAL_ASSET_DIMENSION {
        return Err(format!(
            "image dimensions exceed the {MAX_VISUAL_ASSET_DIMENSION}-pixel side limit"
        ));
    }
    if usize::try_from(width).unwrap_or(usize::MAX) > renderer_max_side
        || usize::try_from(height).unwrap_or(usize::MAX) > renderer_max_side
    {
        return Err(format!(
            "image exceeds the renderer's {renderer_max_side}-pixel texture side limit"
        ));
    }
    let expected_format = match asset.format() {
        VisualAssetFormat::Png => ImageFormat::Png,
        VisualAssetFormat::Jpeg => ImageFormat::Jpeg,
    };
    if format != expected_format || width != asset.dimensions().0 || height != asset.dimensions().1
    {
        return Err("embedded image metadata does not match its payload".into());
    }
    let decoded = crate::image_compare::decode_local_png_jpeg(
        asset.payload(),
        MAX_VISUAL_ASSET_BYTES,
        MAX_VISUAL_ASSET_PIXELS,
    )?;
    if decoded.format != expected_format
        || decoded.width != asset.dimensions().0
        || decoded.height != asset.dimensions().1
    {
        return Err("embedded image metadata does not match its payload".into());
    }
    Ok(DecodedVisualAsset {
        id: asset.id().clone(),
        width: decoded.width,
        height: decoded.height,
        rgba: decoded.rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::{prepare_visual_asset_bytes, prepare_visual_asset_file};
    use crate::diagram::visual::{
        MAX_VISUAL_ASSET_BYTES, MAX_VISUAL_ASSET_DIMENSION, MAX_VISUAL_ASSET_PIXELS,
    };
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn encoded_png(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, Rgba([31, 89, 144, 255]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn visual_asset_preflight_accepts_compressed_png_before_bounded_decode() {
        let bytes = encoded_png(512, 512);
        assert!(bytes.len() < MAX_VISUAL_ASSET_BYTES);
        let prepared = prepare_visual_asset_bytes(bytes, usize::MAX).unwrap();
        assert_eq!(prepared.asset.dimensions(), (512, 512));
        assert_eq!(prepared.rgba.len(), 512 * 512 * 4);
        assert!(prepared.rgba.len() > MAX_VISUAL_ASSET_BYTES);
        assert_eq!(
            prepared.asset.format(),
            super::super::visual::VisualAssetFormat::Png
        );
        assert_eq!(prepared.asset.id().as_str().len(), 64);
    }

    #[test]
    fn visual_asset_preflight_rejects_oversize_signature_and_pixel_bombs_before_decode() {
        let oversized = vec![0; MAX_VISUAL_ASSET_BYTES + 1];
        assert!(
            prepare_visual_asset_bytes(oversized, usize::MAX)
                .unwrap_err()
                .contains("byte limit")
        );
        assert!(
            prepare_visual_asset_bytes(b"<svg><script/>".to_vec(), usize::MAX)
                .unwrap_err()
                .contains("PNG or JPEG")
        );

        let mut png_header = vec![0; 24];
        png_header[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png_header[8..12].copy_from_slice(&13u32.to_be_bytes());
        png_header[12..16].copy_from_slice(b"IHDR");
        png_header[16..20].copy_from_slice(&((MAX_VISUAL_ASSET_PIXELS as u32) + 1).to_be_bytes());
        png_header[20..24].copy_from_slice(&1u32.to_be_bytes());
        assert!(
            prepare_visual_asset_bytes(png_header, usize::MAX)
                .unwrap_err()
                .contains("pixel limit")
        );

        let mut oversized_side = vec![0; 24];
        oversized_side[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        oversized_side[8..12].copy_from_slice(&13u32.to_be_bytes());
        oversized_side[12..16].copy_from_slice(b"IHDR");
        oversized_side[16..20].copy_from_slice(&(MAX_VISUAL_ASSET_DIMENSION + 1).to_be_bytes());
        oversized_side[20..24].copy_from_slice(&1u32.to_be_bytes());
        assert!(
            prepare_visual_asset_bytes(oversized_side, usize::MAX)
                .unwrap_err()
                .contains("dimensions exceed")
        );

        assert!(
            prepare_visual_asset_bytes(encoded_png(512, 512), 256)
                .unwrap_err()
                .contains("texture side limit")
        );
    }

    #[test]
    fn visual_asset_file_import_embeds_bytes_without_retaining_its_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("icon.png");
        let bytes = encoded_png(3, 2);
        std::fs::write(&path, &bytes).unwrap();
        let prepared = prepare_visual_asset_file(&path, usize::MAX).unwrap();
        assert_eq!(prepared.asset.dimensions(), (3, 2));
        let json = serde_json::to_string(&prepared.asset).unwrap();
        assert!(!json.contains(path.to_str().unwrap()));
    }

    #[cfg(windows)]
    #[test]
    fn visual_asset_path_preflight_rejects_unc_verbatim_unc_and_device_namespaces() {
        use super::precheck_local_disk_path;
        use std::path::Path;

        for path in [
            r"\\server\share\image.png",
            r"\\?\UNC\server\share\image.png",
            r"\\.\pipe\diagram-image",
        ] {
            assert!(precheck_local_disk_path(Path::new(path)).is_err(), "{path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn visual_asset_path_preflight_rejects_dos_device_aliases_without_opening_them() {
        use super::precheck_local_disk_path;
        use std::path::Path;

        for name in [
            "CON", "con.txt", "PRN.png", "AUX", "NUL.jpg", "COM1", "LPT9.bmp",
        ] {
            let path = Path::new(r"C:\local").join(name);
            assert!(
                precheck_local_disk_path(&path).is_err(),
                "{}",
                path.display()
            );
        }
    }
}
