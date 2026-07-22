use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    save_atomic_with_replace(path, bytes, replace_existing)
}

fn save_atomic_with_replace(
    path: &Path,
    bytes: &[u8],
    replace: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config");
    let tmp = unique_path(dir, &format!(".{name}.tmp"));
    let res = (|| -> Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace(&tmp, path)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

pub fn backup_file(path: &Path, reason: &str) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config");
    for i in 0..1000u32 {
        let ts = timestamp();
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("-{i}")
        };
        let backup = dir.join(format!("{stem}.{reason}.{ts}{suffix}.bak"));
        match fs::hard_link(path, &backup) {
            Ok(()) => return Ok(Some(backup)),
            Err(_) => {
                if !backup.exists() {
                    fs::copy(path, &backup)
                        .with_context(|| format!("backup {}", path.display()))?;
                    return Ok(Some(backup));
                }
            }
        }
    }
    anyhow::bail!(
        "unable to create collision-safe backup for {}",
        path.display()
    )
}

fn unique_path(dir: &Path, prefix: &str) -> PathBuf {
    for i in 0..1000u32 {
        let p = dir.join(format!("{prefix}.{}.{}", timestamp(), i));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{prefix}.{}.fallback", timestamp()))
}

fn timestamp() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}{:09}", d.as_secs(), d.subsec_nanos())
}

#[cfg(not(windows))]
fn replace_existing(src: &Path, dst: &Path) -> Result<()> {
    fs::rename(src, dst).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_atomic_failed_replace_preserves_existing_destination_and_removes_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("note.md");
        fs::write(&dest, b"original markdown").expect("write existing destination");

        let result = save_atomic_with_replace(&dest, b"updated markdown", |_tmp, _dst| {
            anyhow::bail!("deterministic replace failure")
        });

        assert!(result.is_err());
        assert_eq!(
            fs::read(&dest).expect("read destination"),
            b"original markdown"
        );
        let temp_entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read notes dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".note.md.tmp."))
            })
            .collect();
        assert!(
            temp_entries.is_empty(),
            "temporary files should be removed after a failed replace: {temp_entries:?}"
        );
    }
}

#[cfg(windows)]
fn replace_existing(src: &Path, dst: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;
    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let s = wide(src);
    let d = wide(dst);
    unsafe {
        MoveFileExW(
            PCWSTR(s.as_ptr()),
            PCWSTR(d.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .with_context(|| {
        format!(
            "replace {} with {} using MoveFileExW",
            dst.display(),
            src.display()
        )
    })
}
