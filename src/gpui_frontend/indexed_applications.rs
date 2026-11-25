//! One-shot, cancellable stock application index and checked row admission.
use super::RootWindow;
use gpui_kit::Context;
use multi_launcher::{
    actions::{Action, search::CachedSearchEntry},
    common::query::ActionFilterMetadata,
    indexer::coordinator::{IndexConfig, IndexCoordinator},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(super) struct Owner {
    cancelled: Arc<AtomicBool>,
    coordinator: Arc<Mutex<Option<Arc<IndexCoordinator>>>>,
    pub snapshot: Option<Identity>,
}

pub(super) struct Identity {
    pub generation: u64,
    pub roots: Vec<std::path::PathBuf>,
}

impl Identity {
    pub fn admits(
        &self,
        generation: u64,
        index: usize,
        actions: &[Action],
        action: &Action,
    ) -> bool {
        self.generation == generation && actions.get(index) == Some(action)
    }
}

pub(super) fn admitted_result<'a>(
    result: &super::LauncherSearchResult,
    identity: Option<&'a Identity>,
    indexed_actions: &[Action],
) -> Option<&'a [std::path::PathBuf]> {
    let super::LauncherResultOrigin::Indexed { generation, index } = &result.origin else {
        return None;
    };
    let identity = identity?;
    identity
        .admits(*generation, *index, indexed_actions, &result.action)
        .then_some(identity.roots.as_slice())
}

pub(super) struct Snapshot {
    pub config: IndexConfig,
    pub generation: u64,
    pub roots: Vec<std::path::PathBuf>,
    pub actions: Vec<Action>,
    pub cached: Vec<CachedSearchEntry>,
    pub metadata: Vec<ActionFilterMetadata>,
    pub completion_index: fst::Map<Vec<u8>>,
}

impl Owner {
    pub fn retire(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let coordinator = self
            .coordinator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(coordinator) = coordinator {
            let _ = coordinator.shutdown();
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.retire();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexedKind {
    Executable,
    Shortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IndexedTarget<'a> {
    dos: &'a str,
    kind: IndexedKind,
}

#[cfg(windows)]
fn ordinary_target(target: &str) -> Option<IndexedTarget<'_>> {
    let dos = target.strip_prefix(r"\\?\")?;
    let path = std::path::Path::new(dos);
    if !multi_launcher::actions::exec::ordinary_local_path_is_admissible(path) {
        return None;
    }
    let extension = path.extension()?;
    let kind = if extension.eq_ignore_ascii_case("exe") {
        IndexedKind::Executable
    } else if extension.eq_ignore_ascii_case("lnk") {
        IndexedKind::Shortcut
    } else {
        return None;
    };
    Some(IndexedTarget { dos, kind })
}
#[cfg(not(windows))]
fn ordinary_target(_: &str) -> Option<IndexedTarget<'_>> {
    None
}

fn root_contains(root: &std::path::Path, target: &std::path::Path) -> bool {
    target
        .strip_prefix(root)
        .is_ok_and(|tail| !tail.as_os_str().is_empty())
}

pub(super) fn row_matches(action: &Action, roots: &[std::path::PathBuf]) -> bool {
    if action.args.is_some() || action.action != action.desc {
        return false;
    }
    let Some(target) = ordinary_target(&action.action) else {
        return false;
    };
    roots
        .iter()
        .any(|root| root_contains(root, std::path::Path::new(target.dos)))
}

#[cfg(windows)]
pub(super) fn checked_launch_with(
    action: &Action,
    roots: &[std::path::PathBuf],
    launch_executable: impl FnOnce(&str) -> anyhow::Result<()>,
    launch_shortcut: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        row_matches(action, roots),
        "Invalid indexed application target"
    );
    let target = ordinary_target(&action.action)
        .ok_or_else(|| anyhow::anyhow!("Invalid indexed application path"))?;
    multi_launcher::actions::exec::checked_local_file(std::path::Path::new(target.dos))?;
    match target.kind {
        IndexedKind::Executable => launch_executable(&action.action),
        IndexedKind::Shortcut => launch_shortcut(target.dos),
    }
}

#[cfg(not(windows))]
pub(super) fn checked_launch_with(
    _: &Action,
    _: &[std::path::PathBuf],
    _: impl FnOnce(&str) -> anyhow::Result<()>,
    _: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::bail!("Indexed applications require Windows")
}

fn scan(
    config: IndexConfig,
    cancelled: Arc<AtomicBool>,
    slot: Arc<Mutex<Option<Arc<IndexCoordinator>>>>,
) -> anyhow::Result<Snapshot> {
    #[cfg(not(windows))]
    {
        let _ = (config, cancelled, slot);
        anyhow::bail!("Indexed applications require Windows")
    }
    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        for root in config.roots() {
            if cancelled.load(Ordering::Acquire) {
                anyhow::bail!("Index retired");
            }
            let path = std::path::Path::new(root);
            multi_launcher::actions::exec::checked_local_directory(path)?;
            let canonical = std::fs::canonicalize(path)?;
            let text = canonical
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid index root"))?;
            let dos = text.strip_prefix(r"\\?\").unwrap_or(text);
            if !multi_launcher::actions::exec::ordinary_local_path_is_admissible(
                std::path::Path::new(dos),
            ) {
                anyhow::bail!("Invalid local index root");
            }
            roots.push(std::path::PathBuf::from(dos));
        }
        let coordinator = Arc::new(IndexCoordinator::new()?);
        {
            let mut guard = slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if cancelled.load(Ordering::Acquire) {
                anyhow::bail!("Index retired");
            }
            *guard = Some(coordinator.clone());
        }
        let generation = coordinator.submit(config.clone())?;
        let _ = coordinator.wait_for_completion(generation)?;
        let receipt = coordinator
            .take_result()
            .ok_or_else(|| anyhow::anyhow!("Index result unavailable"))?;
        coordinator.validate_acknowledged_result(generation, &config)?;
        anyhow::ensure!(
            receipt.generation() == generation && receipt.config() == &config,
            "Stale index result"
        );
        let mut actions = Vec::new();
        let mut cached = Vec::new();
        let mut metadata = Vec::new();
        for action in receipt
            .outcome()
            .as_ref()
            .map_err(|failure| anyhow::anyhow!("{}", failure.message()))?
            .iter()
        {
            anyhow::ensure!(!cancelled.load(Ordering::Acquire), "Index retired");
            if !row_matches(action, &roots) {
                continue;
            }
            let Some(path) = ordinary_target(&action.action) else {
                continue;
            };
            anyhow::ensure!(!cancelled.load(Ordering::Acquire), "Index retired");
            if multi_launcher::actions::exec::checked_local_file(std::path::Path::new(path.dos))
                .is_err()
            {
                continue;
            }
            anyhow::ensure!(!cancelled.load(Ordering::Acquire), "Index retired");
            cached.push(CachedSearchEntry::from_action(action));
            metadata.push(ActionFilterMetadata::from_action(action));
            actions.push(action.clone());
        }
        anyhow::ensure!(!cancelled.load(Ordering::Acquire), "Index retired");
        let completion_index = multi_launcher::completion::build_index(&[], &actions);
        anyhow::ensure!(!cancelled.load(Ordering::Acquire), "Index retired");
        let _ = coordinator.shutdown();
        Ok(Snapshot {
            config,
            generation,
            roots,
            actions,
            cached,
            metadata,
            completion_index,
        })
    }
}

fn query_needs_refresh(query: &str) -> bool {
    query.trim().is_empty() || multi_launcher::actions::search::parse_app_prefix(query).is_some()
}

impl RootWindow {
    pub(super) fn start_application_index(&mut self, cx: &mut Context<Self>) {
        let config = self.search_catalog.index_config.clone();
        if config.roots().is_empty() {
            return;
        }
        let cancelled = self.indexed_applications.cancelled.clone();
        let slot = self.indexed_applications.coordinator.clone();
        let task = cx
            .background_executor()
            .spawn(async move { scan(config, cancelled, slot) });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |root, cx| {
                if root.indexed_applications.cancelled.load(Ordering::Acquire)
                    || root.restart_admission.is_fenced()
                {
                    return;
                }
                match result {
                    Ok(snapshot) if snapshot.config == root.search_catalog.index_config => {
                        root.indexed_applications.snapshot =
                            Some(root.search_catalog.install_index(snapshot));
                        let query = root.query.read(cx).value().to_string();
                        root.search_completion
                            .reset_for_query(&root.search_catalog, &query);
                        if query_needs_refresh(&query) {
                            root.refresh_launcher_results_with_viewport(query, false, cx);
                        }
                        cx.notify();
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(error = %error, "Application index unavailable"),
                }
            });
        })
        .detach();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{checked_launch_with, ordinary_target, root_contains, row_matches};
    use multi_launcher::actions::Action;
    use std::path::{Path, PathBuf};

    fn row(path: &str) -> Action {
        Action {
            label: "Tool.exe".into(),
            desc: path.into(),
            action: path.into(),
            args: None,
        }
    }

    #[test]
    fn index_publication_refreshes_only_blank_and_app_queries() {
        assert!(super::query_needs_refresh(""));
        assert!(super::query_needs_refresh(" \t"));
        assert!(super::query_needs_refresh("app"));
        assert!(super::query_needs_refresh("APP editor"));
        assert!(!super::query_needs_refresh("a"));
        assert!(!super::query_needs_refresh("= 6 * 7"));
        assert!(!super::query_needs_refresh("app\teditor"));
    }

    #[test]
    fn indexed_identity_rejects_stale_generation_and_replaced_row() {
        let identity = super::Identity {
            generation: 4,
            roots: Vec::new(),
        };
        let first = row(r"C:\Apps\Tool.exe");
        let replacement = row(r"C:\Apps\Other.exe");
        assert!(identity.admits(4, 0, &[first.clone()], &first));
        assert!(!identity.admits(3, 0, &[first.clone()], &first));
        assert!(!identity.admits(4, 0, &[replacement], &first));
        assert!(!identity.admits(4, 1, &[first.clone()], &first));
    }

    #[test]
    fn indexed_paths_are_local_verbatim_executables_with_component_membership() {
        let roots = vec![PathBuf::from(r"C:\Apps")];
        let admitted = row(r"\\?\C:\Apps\Tool.exe");
        assert!(row_matches(&admitted, &roots));
        assert!(!root_contains(
            Path::new(r"C:\Apps"),
            Path::new(r"C:\AppsOther\Tool.exe")
        ));
        assert!(!row_matches(&row(r"\\?\UNC\server\share\Tool.exe"), &roots));
        assert!(!row_matches(&row(r"C:\Apps\Tool.lnk"), &roots));
        assert!(row_matches(&row(r"\\?\C:\Apps\Tool.lnk"), &roots));
        assert!(!row_matches(&row(r"\\?\C:\Apps\Tool.txt"), &roots));
        assert!(!row_matches(&row(r"C:\Apps\Tool.exe"), &roots));
        assert!(ordinary_target(r"\\?\C:\Apps\..\Tool.exe").is_none());
        let mut with_args = admitted;
        with_args.args = Some(String::new());
        assert!(!row_matches(&with_args, &roots));
    }

    #[test]
    fn indexed_shortcut_uses_checked_dos_projection_and_rejects_missing_or_directory() {
        let temp = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(temp.path()).unwrap();
        let root_text = canonical_root.to_str().unwrap();
        let root_dos = PathBuf::from(root_text.strip_prefix(r"\\?\").unwrap_or(root_text));
        let file = temp.path().join("App.lnk");
        std::fs::write(&file, b"fixture only").unwrap();
        let canonical = std::fs::canonicalize(&file).unwrap();
        let action = row(canonical.to_str().unwrap());
        let expected = action.action.strip_prefix(r"\\?\").unwrap().to_owned();
        let mut calls = Vec::new();
        checked_launch_with(
            &action,
            &[root_dos.clone()],
            |_| panic!("shortcut dispatched as executable"),
            |path| {
                calls.push(path.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, [expected]);
        std::fs::remove_file(&file).unwrap();
        assert!(
            checked_launch_with(
                &action,
                &[root_dos.clone()],
                |_| panic!("missing executable"),
                |_| panic!("missing shortcut"),
            )
            .is_err()
        );
        std::fs::create_dir(&file).unwrap();
        assert!(
            checked_launch_with(
                &action,
                &[root_dos],
                |_| panic!("directory executable"),
                |_| panic!("directory shortcut"),
            )
            .is_err()
        );
    }

    #[test]
    fn indexed_checked_launch_revalidates_and_forwards_original_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let root_dos = PathBuf::from(
            root.to_str()
                .unwrap()
                .strip_prefix(r"\\?\")
                .unwrap_or(root.to_str().unwrap()),
        );
        let file = temp.path().join("Tool.exe");
        std::fs::write(&file, b"fixture").unwrap();
        let canonical = std::fs::canonicalize(&file).unwrap();
        let action = row(canonical.to_str().unwrap());
        let mut calls = Vec::new();
        checked_launch_with(
            &action,
            &[root_dos.clone()],
            |target| {
                calls.push(target.to_owned());
                Ok(())
            },
            |_| panic!("executable dispatched as shortcut"),
        )
        .unwrap();
        assert_eq!(calls, [action.action.clone()]);
        std::fs::remove_file(file).unwrap();
        assert!(
            checked_launch_with(
                &action,
                &[root_dos],
                |_| panic!("stale indexed file launched"),
                |_| panic!("stale shortcut launched")
            )
            .is_err()
        );
    }
}
