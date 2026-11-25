//! Checked execution of ordinary local Windows shell links.
use anyhow::Context as _;
use std::{
    ffi::{OsStr, OsString},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
};
use windows::{
    Win32::{
        Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, GetDriveTypeW},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, STGM_READ,
        },
        UI::{
            Shell::{
                IShellLinkDataList, IShellLinkW, SLDF_DISABLE_KNOWNFOLDER_RELATIVE_TRACKING,
                SLDF_DISABLE_LINK_PATH_TRACKING, SLDF_ENABLE_TARGET_METADATA,
                SLDF_FORCE_NO_LINKTRACK, SLDF_HAS_ARGS, SLDF_HAS_ICONLOCATION, SLDF_HAS_ID_LIST,
                SLDF_HAS_LINK_INFO, SLDF_HAS_NAME, SLDF_HAS_RELPATH, SLDF_HAS_WORKINGDIR,
                SLDF_NO_KF_ALIAS, SLDF_PERSIST_VOLUME_ID_RELATIVE, SLDF_UNALIAS_ON_SAVE,
                SLDF_UNICODE, SLGP_RAWPATH, ShellLink,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{Interface, PCWSTR},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Shortcut {
    target: PathBuf,
    arguments: OsString,
    directory: PathBuf,
    flags: u32,
    show: i32,
}

// Supported flags describe ordinary link metadata and tracking only. Unknown
// flags and alternate launch mechanisms fail closed.
const ALLOWED_FLAGS: u32 = (SLDF_HAS_ID_LIST.0
    | SLDF_HAS_LINK_INFO.0
    | SLDF_HAS_NAME.0
    | SLDF_HAS_RELPATH.0
    | SLDF_HAS_WORKINGDIR.0
    | SLDF_HAS_ARGS.0
    | SLDF_HAS_ICONLOCATION.0
    | SLDF_UNICODE.0
    | SLDF_FORCE_NO_LINKTRACK.0
    | SLDF_ENABLE_TARGET_METADATA.0
    | SLDF_DISABLE_LINK_PATH_TRACKING.0
    | SLDF_DISABLE_KNOWNFOLDER_RELATIVE_TRACKING.0
    | SLDF_NO_KF_ALIAS.0
    | SLDF_UNALIAS_ON_SAVE.0
    | SLDF_PERSIST_VOLUME_ID_RELATIVE.0) as u32;

pub fn shortcut_path_is_admissible(path: &str) -> bool {
    ordinary_local_path(Path::new(path), Some("lnk"))
}

fn ordinary_local_path(path: &Path, extension: Option<&str>) -> bool {
    let Some(raw) = path.to_str() else {
        return false;
    };
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || raw.trim() != raw
        || raw
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*' | '%'))
        || raw[2..].contains(':')
    {
        return false;
    }
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Prefix(_)))
        || !matches!(components.next(), Some(Component::RootDir))
    {
        return false;
    }
    for component in components {
        let Component::Normal(part) = component else {
            return false;
        };
        let Some(part) = part.to_str() else {
            return false;
        };
        if part.ends_with(' ') || part.ends_with('.') {
            return false;
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or(part)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        let port = stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"));
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        ) || port.is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        }) {
            return false;
        }
    }
    extension.is_none_or(|ext| {
        path.extension()
            .is_some_and(|value| value.eq_ignore_ascii_case(ext))
    })
}

fn check_local_object(path: &Path, directory: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        ordinary_local_path(path, None),
        "Shortcut path is not an ordinary absolute local path"
    );
    let raw = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let drive = [raw[0], b':' as u16, b'\\' as u16, 0];
    let kind = unsafe { GetDriveTypeW(PCWSTR(drive.as_ptr())) };
    // GetDriveTypeW: removable=2, fixed=3, CD-ROM=5, RAM disk=6.
    anyhow::ensure!(
        matches!(kind, 2 | 3 | 5 | 6),
        "Shortcut path is not on an available local volume"
    );
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&prefix)
            .with_context(|| format!("Cannot inspect {}", prefix.display()))?;
        anyhow::ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
            "Shortcut path contains a reparse point"
        );
        if prefix == path {
            anyhow::ensure!(
                if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                },
                "Shortcut object has the wrong type"
            );
        } else {
            anyhow::ensure!(
                metadata.is_dir(),
                "Shortcut path ancestor is not a directory"
            );
        }
    }
    Ok(())
}

fn read_shortcut(path: &Path) -> anyhow::Result<Shortcut> {
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .context("Cannot initialize COM for shortcut")?;
    let _com = ComGuard;
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .context("Cannot create shortcut reader")?;
    let persist: IPersistFile = link.cast().context("Cannot read shortcut persistence")?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { persist.Load(PCWSTR(wide.as_ptr()), STGM_READ) }.context("Cannot load shortcut")?;
    let flags = unsafe { link.cast::<IShellLinkDataList>()?.GetFlags() }
        .context("Cannot read shortcut flags")?;
    let show = unsafe { link.GetShowCmd() }
        .context("Cannot read shortcut show mode")?
        .0;
    let mut target = vec![0u16; 32768];
    let mut arguments = vec![0u16; 32768];
    let mut directory = vec![0u16; 32768];
    unsafe {
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .context("Cannot read shortcut target")?;
        link.GetArguments(&mut arguments)
            .context("Cannot read shortcut arguments")?;
        link.GetWorkingDirectory(&mut directory)
            .context("Cannot read shortcut working directory")?;
    }
    fn value(buffer: &[u16]) -> anyhow::Result<OsString> {
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .context("Shortcut field is too long")?;
        anyhow::ensure!(end < buffer.len() - 1, "Shortcut field may be truncated");
        Ok(OsString::from_wide(&buffer[..end]))
    }
    Ok(Shortcut {
        target: PathBuf::from(value(&target)?),
        arguments: value(&arguments)?,
        directory: PathBuf::from(value(&directory)?),
        flags,
        show,
    })
}

fn checked_with(
    path: &Path,
    check: impl Fn(&Path, bool) -> anyhow::Result<()>,
    read: impl FnOnce(&Path) -> anyhow::Result<Shortcut>,
    launch: impl FnOnce(&Shortcut) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        ordinary_local_path(path, Some("lnk")),
        "Invalid shortcut path"
    );
    check(path, false)?;
    let shortcut = read(path)?;
    anyhow::ensure!(
        shortcut.flags & !ALLOWED_FLAGS == 0 && shortcut.show == SW_SHOWNORMAL.0,
        "Shortcut requires an unsupported launch mode"
    );
    anyhow::ensure!(
        ordinary_local_path(&shortcut.target, Some("exe")),
        "Shortcut target is not an ordinary local executable"
    );
    anyhow::ensure!(
        shortcut.flags & SLDF_HAS_WORKINGDIR.0 as u32 != 0,
        "Shortcut has no explicit working directory"
    );
    anyhow::ensure!(
        ordinary_local_path(&shortcut.directory, None),
        "Shortcut working directory is not an ordinary local directory"
    );
    check(&shortcut.target, false)?;
    check(&shortcut.directory, true)?;
    launch(&shortcut)
}

pub fn launch_shortcut(path: impl AsRef<OsStr>) -> anyhow::Result<()> {
    checked_with(
        Path::new(path.as_ref()),
        check_local_object,
        read_shortcut,
        |shortcut| {
            let mut command = std::process::Command::new(&shortcut.target);
            command.current_dir(&shortcut.directory);
            if !shortcut.arguments.is_empty() {
                command.raw_arg(&shortcut.arguments);
            }
            command
                .spawn()
                .context("Cannot launch shortcut executable")?;
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn shortcut() -> Shortcut {
        Shortcut {
            target: PathBuf::from(r"C:\Program Files\Tool\tool.exe"),
            arguments: OsString::from(r#""two words" "" a\"b"#),
            directory: PathBuf::from(r"C:\Work Folder"),
            flags: 0xBB,
            show: SW_SHOWNORMAL.0,
        }
    }

    #[test]
    fn shortcut_checked_pipeline_forwards_exact_arguments_and_refuses_unsafe_modes() {
        let path = Path::new(r"C:\Links\Tool.lnk");
        let log = RefCell::new(Vec::new());
        let expected = shortcut();
        assert!(ordinary_local_path(Path::new(r"C:\"), None));
        assert!(!ordinary_local_path(Path::new("C:"), None));
        for malformed in [r"C:\CONIN$.lnk", r"C:\Links\COM¹.lnk", r"C:\AUX .lnk"] {
            assert!(!shortcut_path_is_admissible(malformed));
        }
        let mut no_start_in = expected.clone();
        no_start_in.flags &= !(SLDF_HAS_WORKINGDIR.0 as u32);
        assert!(
            checked_with(
                path,
                |_, _| Ok(()),
                |_| Ok(no_start_in),
                |_| panic!("no Start In launched")
            )
            .is_err()
        );
        checked_with(
            path,
            |path, directory| {
                log.borrow_mut().push((path.to_path_buf(), directory));
                Ok(())
            },
            |_| Ok(expected.clone()),
            |value| {
                assert_eq!(value, &expected);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            *log.borrow(),
            vec![
                (path.to_path_buf(), false),
                (expected.target.clone(), false),
                (expected.directory.clone(), true)
            ]
        );
        for flag in [0x200, 0x400, 0x1000, 0x2000, 0x20000, 0x2000000] {
            let mut bad = expected.clone();
            bad.flags |= flag;
            assert!(
                checked_with(
                    path,
                    |_, _| Ok(()),
                    |_| Ok(bad),
                    |_| panic!("unsafe shortcut launched")
                )
                .is_err()
            );
        }
        for (target, directory, show) in [
            (r"C:\Tools\file.txt", r"C:\Work", SW_SHOWNORMAL.0),
            (r"\\server\share\tool.exe", r"C:\Work", SW_SHOWNORMAL.0),
            (r"C:\Tools\tool.exe", "", SW_SHOWNORMAL.0),
            (r"C:\Tools\tool.exe", r"C:\Work", 3),
        ] {
            let mut bad = expected.clone();
            bad.target = target.into();
            bad.directory = directory.into();
            bad.show = show;
            assert!(
                checked_with(
                    path,
                    |_, _| Ok(()),
                    |_| Ok(bad),
                    |_| panic!("unsafe shortcut launched")
                )
                .is_err()
            );
        }
        assert!(
            checked_with(
                path,
                |_, _| anyhow::bail!("reparse or missing"),
                |_| panic!("invalid link read"),
                |_| panic!("invalid link launched")
            )
            .is_err()
        );
        assert!(
            checked_with(
                path,
                |path, _| if path == expected.target {
                    anyhow::bail!("reparse target")
                } else {
                    Ok(())
                },
                |_| Ok(expected.clone()),
                |_| panic!("invalid target launched")
            )
            .is_err()
        );
    }
}
