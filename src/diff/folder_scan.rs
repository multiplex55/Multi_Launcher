//! Bounded, cancellable folder traversal. Directory symlinks/reparse points
//! are reported as entries but are never followed.
use super::folder_compare::{EntryKind, EntryMetadata, EntrySide, normalized_relative};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RootIdentity {
    pub path: PathBuf,
    pub token: Option<(u64, u64)>,
}
impl RootIdentity {
    pub fn new(path: PathBuf) -> Self {
        let token = fs::metadata(&path).ok().and_then(|m| identity(&m));
        Self { path, token }
    }
}
#[derive(Debug, Clone)]
pub struct DiscoveredEntry {
    pub relative_path: PathBuf,
    pub side: EntrySide,
}
#[derive(Debug, Clone)]
pub enum ScanEvent {
    ScanStarted {
        generation: u64,
        root: RootIdentity,
    },
    EntriesDiscovered {
        generation: u64,
        root: RootIdentity,
        entries: Vec<DiscoveredEntry>,
    },
    EntriesUpdated {
        generation: u64,
        root: RootIdentity,
        entries: Vec<DiscoveredEntry>,
    },
    Progress {
        generation: u64,
        root: RootIdentity,
        visited: u64,
    },
    Completed {
        generation: u64,
        root: RootIdentity,
        visited: u64,
    },
    Cancelled {
        generation: u64,
        root: RootIdentity,
        visited: u64,
    },
    Failed {
        generation: u64,
        root: RootIdentity,
        error: String,
    },
}
impl ScanEvent {
    pub fn generation(&self) -> u64 {
        match self {
            Self::ScanStarted { generation, .. }
            | Self::EntriesDiscovered { generation, .. }
            | Self::EntriesUpdated { generation, .. }
            | Self::Progress { generation, .. }
            | Self::Completed { generation, .. }
            | Self::Cancelled { generation, .. }
            | Self::Failed { generation, .. } => *generation,
        }
    }
    pub fn root(&self) -> &RootIdentity {
        match self {
            Self::ScanStarted { root, .. }
            | Self::EntriesDiscovered { root, .. }
            | Self::EntriesUpdated { root, .. }
            | Self::Progress { root, .. }
            | Self::Completed { root, .. }
            | Self::Cancelled { root, .. }
            | Self::Failed { root, .. } => root,
        }
    }
    pub fn is_current(&self, g: u64, r: &RootIdentity) -> bool {
        self.generation() == g && self.root() == r
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanRules {
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}
impl ScanRules {
    /// Validate user-authored glob rules before they are allowed to replace the
    /// rules belonging to an active scan.
    pub fn validated(includes: Vec<String>, excludes: Vec<String>) -> Result<Self, String> {
        fn check(rule: &str) -> Result<(), String> {
            if rule.is_empty() {
                return Err("rules may not be empty".into());
            }
            if rule.starts_with('/') || rule.starts_with('\\') {
                return Err(format!("absolute rule '{rule}' is not allowed"));
            }
            if rule.split(['/', '\\']).any(|part| part == "..") {
                return Err(format!("parent traversal in rule '{rule}' is not allowed"));
            }
            if rule.contains(['[', ']', '\0']) {
                return Err(format!("unsupported syntax in rule '{rule}' (use * and ?)"));
            }
            Ok(())
        }
        for rule in includes.iter().chain(&excludes) {
            check(rule)?;
        }
        Ok(Self { includes, excludes })
    }

    pub fn permits(&self, rel: &Path, is_dir: bool) -> bool {
        let p = rel
            .iter()
            .map(|x| x.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let base = rel.file_name().map_or("".into(), |x| x.to_string_lossy());
        let hit = |pat: &str| {
            let dir = pat.ends_with('/');
            if dir && !is_dir {
                return false;
            }
            let pat = pat.trim_end_matches('/').replace('\\', "/");
            if pat.contains('/') {
                wildcard(&pat, &p)
            } else {
                wildcard(&pat, &base)
            }
        };
        !self.excludes.iter().any(|x| hit(x))
            && (self.includes.is_empty() || self.includes.iter().any(|x| hit(x)) || is_dir)
    }
}
fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == t[j]) {
            i += 1;
            j += 1
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            mark = j
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1
    }
    i == p.len()
}

pub trait FileSystem: Send + Sync + 'static {
    fn read_dir(&self, p: &Path) -> io::Result<Vec<io::Result<PathBuf>>>;
    fn symlink_metadata(&self, p: &Path) -> io::Result<fs::Metadata>;
}
#[derive(Default)]
pub struct RealFileSystem;
impl FileSystem for RealFileSystem {
    fn read_dir(&self, p: &Path) -> io::Result<Vec<io::Result<PathBuf>>> {
        Ok(fs::read_dir(p)?.map(|x| x.map(|x| x.path())).collect())
    }
    fn symlink_metadata(&self, p: &Path) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(p)
    }
}
pub struct ScanHandle {
    pub receiver: Receiver<ScanEvent>,
    cancel: Arc<AtomicBool>,
}
impl ScanHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release)
    }
}
pub fn spawn_scan(root: PathBuf, generation: u64, rules: ScanRules) -> ScanHandle {
    spawn_scan_with(Arc::new(RealFileSystem), root, generation, rules, 128)
}
pub fn spawn_scan_with(
    fs: Arc<dyn FileSystem>,
    root_path: PathBuf,
    generation: u64,
    rules: ScanRules,
    capacity: usize,
) -> ScanHandle {
    let root = RootIdentity::new(root_path.clone());
    let (tx, rx) = mpsc::sync_channel(capacity.max(1));
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    std::thread::spawn(move || scan(fs, &root_path, generation, root, rules, c, tx));
    ScanHandle {
        receiver: rx,
        cancel,
    }
}
fn send(tx: &SyncSender<ScanEvent>, e: ScanEvent, c: &AtomicBool) -> bool {
    if c.load(Ordering::Acquire) {
        return false;
    }
    tx.send(e).is_ok()
}
fn scan(
    fs: Arc<dyn FileSystem>,
    base: &Path,
    g: u64,
    root: RootIdentity,
    rules: ScanRules,
    c: Arc<AtomicBool>,
    tx: SyncSender<ScanEvent>,
) {
    if !send(
        &tx,
        ScanEvent::ScanStarted {
            generation: g,
            root: root.clone(),
        },
        &c,
    ) {
        if c.load(Ordering::Acquire) {
            let _ = tx.send(ScanEvent::Cancelled {
                generation: g,
                root,
                visited: 0,
            });
        }
        return;
    }
    let mut stack = vec![base.to_path_buf()];
    let mut batch = Vec::with_capacity(64);
    let mut visited = 0;
    while let Some(dir) = stack.pop() {
        if c.load(Ordering::Acquire) {
            let _ = tx.send(ScanEvent::Cancelled {
                generation: g,
                root,
                visited,
            });
            return;
        }
        let children = match fs.read_dir(&dir) {
            Ok(x) => x,
            Err(e) => {
                if dir == base {
                    let _ = tx.send(ScanEvent::Failed {
                        generation: g,
                        root,
                        error: e.to_string(),
                    });
                    return;
                }
                batch.push(error_entry(base, &dir, e));
                continue;
            }
        };
        for child in children {
            if c.load(Ordering::Acquire) {
                let _ = tx.send(ScanEvent::Cancelled {
                    generation: g,
                    root,
                    visited,
                });
                return;
            }
            let path = match child {
                Ok(x) => x,
                Err(e) => {
                    batch.push(error_entry(base, &dir, e));
                    continue;
                }
            };
            let rel = match path
                .strip_prefix(base)
                .ok()
                .and_then(|x| normalized_relative(x).ok())
            {
                Some(x) => x,
                None => continue,
            };
            let meta = match fs.symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    batch.push(error_entry(base, &path, e));
                    continue;
                }
            };
            let ft = meta.file_type();
            // Includes only decide visibility; they can never turn a filesystem
            // indirection into a traversable directory.
            let kind = if ft.is_symlink() || is_reparse_point(&meta) {
                EntryKind::Symlink
            } else if ft.is_dir() {
                EntryKind::Directory
            } else if ft.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            if !rules.permits(&rel, kind == EntryKind::Directory) {
                continue;
            }
            visited += 1;
            batch.push(DiscoveredEntry {
                relative_path: rel,
                side: EntrySide {
                    path: path.clone(),
                    metadata: Some(EntryMetadata::from_fs(&meta, kind)),
                    error: None,
                },
            });
            if kind == EntryKind::Directory {
                stack.push(path)
            }
            if batch.len() >= 64 {
                if !send(
                    &tx,
                    ScanEvent::EntriesDiscovered {
                        generation: g,
                        root: root.clone(),
                        entries: std::mem::take(&mut batch),
                    },
                    &c,
                ) {
                    return;
                }
                if !send(
                    &tx,
                    ScanEvent::Progress {
                        generation: g,
                        root: root.clone(),
                        visited,
                    },
                    &c,
                ) {
                    return;
                }
            }
        }
    }
    if !batch.is_empty()
        && !send(
            &tx,
            ScanEvent::EntriesDiscovered {
                generation: g,
                root: root.clone(),
                entries: batch,
            },
            &c,
        )
    {
        return;
    }
    let _ = tx.send(ScanEvent::Progress {
        generation: g,
        root: root.clone(),
        visited,
    });
    let _ = tx.send(ScanEvent::Completed {
        generation: g,
        root,
        visited,
    });
}
fn error_entry(base: &Path, path: &Path, e: io::Error) -> DiscoveredEntry {
    DiscoveredEntry {
        relative_path: path
            .strip_prefix(base)
            .unwrap_or(
                path.file_name()
                    .map(Path::new)
                    .unwrap_or(Path::new("unreadable")),
            )
            .to_path_buf(),
        side: EntrySide {
            path: path.to_path_buf(),
            metadata: None,
            error: Some(e.to_string()),
        },
    }
}
#[cfg(unix)]
fn identity(m: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((m.dev(), m.ino()))
}
#[cfg(not(unix))]
fn identity(_: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns_basename_and_path() {
        let r = ScanRules {
            includes: vec![],
            excludes: vec![".git/".into(), "*.tmp".into(), "a/*.bak".into()],
        };
        assert!(!r.permits(Path::new(".git"), true));
        assert!(!r.permits(Path::new("x/a.tmp"), false));
        assert!(!r.permits(Path::new("a/x.bak"), false));
        assert!(r.permits(Path::new("b/x.bak"), false));
    }

    #[test]
    fn include_only_and_exclude_precedence() {
        let rules = ScanRules::validated(vec!["*.tmp".into()], vec!["secret.tmp".into()]).unwrap();
        assert!(rules.permits(Path::new("nested/keep.tmp"), false));
        assert!(!rules.permits(Path::new("nested/no.txt"), false));
        assert!(!rules.permits(Path::new("nested/secret.tmp"), false));
        assert!(rules.permits(Path::new("nested"), true)); // traversal corridor
    }

    #[test]
    fn common_directory_extension_and_nested_patterns() {
        let rules = ScanRules::validated(
            vec![],
            vec![
                ".git/".into(),
                "target/".into(),
                "*.tmp".into(),
                "*.bak".into(),
                "cache/nested/".into(),
            ],
        )
        .unwrap();
        for path in [".git", "src/.git", "target", "crate/target"] {
            assert!(!rules.permits(Path::new(path), true), "{path}");
        }
        for path in ["a.tmp", "deep/a.bak"] {
            assert!(!rules.permits(Path::new(path), false), "{path}");
        }
        assert!(!rules.permits(Path::new("cache/nested"), true));
        assert!(rules.permits(Path::new("other/nested"), true));
    }

    #[test]
    fn invalid_syntax_is_rejected() {
        assert!(ScanRules::validated(vec!["../outside".into()], vec![]).is_err());
        assert!(ScanRules::validated(vec!["[abc]".into()], vec![]).is_err());
    }

    #[test]
    fn event_rejects_stale_generation_and_wrong_root() {
        let root = RootIdentity {
            path: "a".into(),
            token: None,
        };
        let event = ScanEvent::Progress {
            generation: 7,
            root: root.clone(),
            visited: 1,
        };
        assert!(event.is_current(7, &root));
        assert!(!event.is_current(8, &root));
        assert!(!event.is_current(
            7,
            &RootIdentity {
                path: "b".into(),
                token: None
            }
        ));
    }

    #[test]
    fn recursive_scan_batches_and_does_not_follow_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).unwrap();
        for i in 0..130 {
            fs::write(nested.join(format!("{i}.txt")), b"x").unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&nested, temp.path().join("link")).unwrap();
        let handle = spawn_scan(temp.path().to_path_buf(), 4, ScanRules::default());
        let events: Vec<_> = handle.receiver.iter().collect();
        let batches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                ScanEvent::EntriesDiscovered { entries, .. } => Some(entries),
                _ => None,
            })
            .collect();
        assert!(batches.len() >= 3);
        let paths: std::collections::BTreeSet<_> = batches
            .iter()
            .flat_map(|b| b.iter().map(|e| e.relative_path.clone()))
            .collect();
        assert_eq!(paths.len(), 131 + usize::from(cfg!(unix)));
        #[cfg(unix)]
        assert!(!paths.iter().any(|p| p.starts_with("link/")));
        assert!(matches!(events.last(), Some(ScanEvent::Completed { .. })));
    }
}
