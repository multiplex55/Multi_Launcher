//! Narrow Search projection and live execution checks for saved Favorites.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::fav::{FavBrowsePlugin, FavPlugin};

pub(super) struct Provider {
    watched: Option<FavPlugin>,
}
impl Provider {
    pub fn new(watch: bool) -> Self {
        Self {
            watched: watch.then(FavPlugin::new),
        }
    }
}
impl Plugin for Provider {
    fn name(&self) -> &str {
        "favorites"
    }
    fn description(&self) -> &str {
        "Run saved favorite commands (prefix: `fav`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn commands(&self) -> Vec<Action> {
        let mut commands = FavBrowsePlugin.commands();
        commands.push(Action {
            label: "fav list".into(),
            desc: "Fav".into(),
            action: "query:fav list".into(),
            args: None,
        });
        commands
    }
    fn search(&self, query: &str) -> Vec<Action> {
        project(query, self.watched.as_ref().map(|fav| fav as &dyn Plugin))
    }
}
fn project(query: &str, watched: Option<&dyn Plugin>) -> Vec<Action> {
    let browse = FavBrowsePlugin.search(query);
    if !browse.is_empty() {
        return browse;
    }
    let query = query.trim();
    if multi_launcher::common::strip_prefix_ci(query, "fav add").is_some()
        || multi_launcher::common::strip_prefix_ci(query, "fav rm").is_some()
    {
        return Vec::new();
    }
    watched
        .map(|fav| {
            fav.search(query)
                .into_iter()
                .filter(is_executable)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(windows)]
pub(super) fn is_executable(action: &Action) -> bool {
    let bytes = action.action.as_bytes();
    action.desc == "Fav"
        && bytes.len() >= 4
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        && external_launch::saved_external(action).is_some()
        && multi_launcher::actions::exec::ordinary_local_path_is_admissible(std::path::Path::new(
            &action.action,
        ))
}

#[cfg(not(windows))]
pub(super) fn is_executable(_: &Action) -> bool {
    false
}

#[cfg(windows)]
pub(super) fn checked_launch_with(
    action: &Action,
    path: &str,
    launch: impl FnOnce(&str, Option<&str>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(is_executable(action), "Unsupported favorite target");
    let entries = multi_launcher::plugins::fav::load_favs(path)?;
    anyhow::ensure!(
        entries.iter().any(|entry| entry.label == action.label
            && entry.action == action.action
            && entry.args == action.args),
        "Favorite changed or no longer exists"
    );
    multi_launcher::actions::exec::checked_local_file(std::path::Path::new(&action.action))?;
    launch(&action.action, action.args.as_deref())
}

#[cfg(not(windows))]
pub(super) fn checked_launch_with(
    _: &Action,
    _: &str,
    _: impl FnOnce(&str, Option<&str>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::bail!("Favorite executables require Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::{Provider, checked_launch_with, is_executable, project};
    use multi_launcher::actions::Action;
    use multi_launcher::plugin::Plugin;

    struct Fixture(Vec<Action>);
    impl Plugin for Fixture {
        fn name(&self) -> &str {
            "favorites"
        }
        fn description(&self) -> &str {
            "fixture"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
        fn search(&self, _: &str) -> Vec<Action> {
            self.0.clone()
        }
    }

    #[test]
    fn favorite_projection_keeps_browse_and_rejects_mutations_and_unsupported_rows() {
        let executable = Action {
            label: "Editor".into(),
            desc: "Fav".into(),
            action: r"C:\Apps\Editor.exe".into(),
            args: None,
        };
        let unsupported = Action {
            action: "https://example.com".into(),
            ..executable.clone()
        };
        let provider = Fixture(vec![executable.clone(), unsupported]);
        assert_eq!(
            project("fav list", Some(&provider)),
            vec![executable.clone()]
        );
        assert_eq!(project("fav ed", Some(&provider)), vec![executable]);
        assert!(project("fav rm edit", Some(&provider)).is_empty());
        assert!(
            project("fav", Some(&provider))[0]
                .action
                .starts_with("fav:dialog:")
        );
        assert!(
            project("fav add name", Some(&provider))[0]
                .action
                .starts_with("fav:dialog:")
        );
        let commands = Provider::new(false).commands();
        assert!(
            commands
                .iter()
                .any(|action| action.label == "fav list" && action.action == "query:fav list")
        );
        assert!(!commands.iter().any(|action| action.label == "fav rm"));
    }

    #[test]
    fn favorite_execution_rechecks_exact_store_and_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("favorite.exe");
        std::fs::write(&exe, b"test").unwrap();
        let store = dir.path().join("fav.json");
        let path = store.to_str().unwrap();
        let action = Action {
            label: "Favorite".into(),
            desc: "Fav".into(),
            action: exe.to_str().unwrap().into(),
            args: Some("--exact value".into()),
        };
        let entry = multi_launcher::plugins::fav::FavEntry {
            label: action.label.clone(),
            action: action.action.clone(),
            args: action.args.clone(),
        };
        std::fs::write(&store, serde_json::to_vec(&vec![entry]).unwrap()).unwrap();
        assert!(is_executable(&action));
        let mut launched = false;
        checked_launch_with(&action, path, |target, args| {
            launched = true;
            assert_eq!(target, action.action);
            assert_eq!(args, action.args.as_deref());
            Ok(())
        })
        .unwrap();
        assert!(launched);
        let mut changed = action.clone();
        changed.args = None;
        assert!(
            checked_launch_with(&changed, path, |_, _| panic!("changed favorite launched"))
                .is_err()
        );
        std::fs::remove_file(&exe).unwrap();
        assert!(
            checked_launch_with(&action, path, |_, _| panic!("missing executable launched"))
                .is_err()
        );
        std::fs::write(&store, b"{").unwrap();
        assert!(
            checked_launch_with(&action, path, |_, _| panic!("malformed favorite launched"))
                .is_err()
        );
        std::fs::remove_file(&store).unwrap();
        assert!(
            checked_launch_with(&action, path, |_, _| panic!("missing favorite launched")).is_err()
        );
        assert!(!is_executable(&Action {
            action: r"C:\Apps\Doc.txt".into(),
            ..action.clone()
        }));
        assert!(!is_executable(&Action {
            action: r"\\server\share\x.exe".into(),
            ..action.clone()
        }));
    }
}
