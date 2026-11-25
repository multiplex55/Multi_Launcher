//! Saved folder admission: projection happens only in startup or editor workers.
use multi_launcher::actions::Action;
use multi_launcher::commands::ExternalCommand;
#[cfg(windows)]
use multi_launcher::commands::{Command, parse_action};

#[cfg(windows)]
pub(super) fn candidate(action: &Action) -> Option<ExternalCommand> {
    use super::external_launch;
    if action.args.as_deref().is_some_and(|args| !args.trim().is_empty()) {
        return None;
    }
    let command = parse_action(action).ok()?;
    let Command::External(external) = &command else {
        return None;
    };
    if external.namespace.is_some()
        || external.target != action.action
        || external.args.as_deref().is_some_and(|args| !args.trim().is_empty())
        || !multi_launcher::actions::exec::ordinary_local_path_is_admissible(
            std::path::Path::new(&external.target),
        )
        || external_launch::ordinary_external(&command).is_some()
        || external_launch::document_external(action, &command).is_some()
        || external_launch::url_external(action, &command).is_some()
        || external_launch::shortcut_external(action, &command).is_some()
    {
        return None;
    }
    Some(external.clone())
}

#[cfg(not(windows))]
pub(super) fn candidate(_: &Action) -> Option<ExternalCommand> {
    None
}

pub(crate) fn verified_actions(actions: &[Action]) -> Vec<Action> {
    actions
        .iter()
        .filter(|action| {
            candidate(action).is_some_and(|command| {
                #[cfg(windows)]
                {
                    multi_launcher::actions::exec::checked_local_directory(
                        std::path::Path::new(&command.target),
                    )
                    .is_ok()
                }
                #[cfg(not(windows))]
                {
                    let _ = command;
                    false
                }
            })
        })
        .cloned()
        .collect()
}

pub(super) fn checked_association_with(
    action: &Action,
    command: &ExternalCommand,
    launch: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        candidate(action).as_ref() == Some(command),
        "Invalid saved folder target"
    );
    #[cfg(windows)]
    multi_launcher::actions::exec::checked_local_directory(std::path::Path::new(&command.target))?;
    #[cfg(not(windows))]
    {
        let _ = launch;
        anyhow::bail!("Saved folders require Windows");
    }
    #[cfg(windows)]
    launch(&command.target)
}

#[cfg(all(test, windows))]
mod tests {
    use super::{candidate, checked_association_with, verified_actions};
    use multi_launcher::actions::Action;
    use std::cell::RefCell;

    fn action(path: &std::path::Path) -> Action {
        Action {
            label: "Folder".into(),
            desc: "Saved folder".into(),
            action: path.to_str().unwrap().to_owned(),
            args: None,
        }
    }

    #[test]
    fn saved_folder_projection_and_checked_literal_effect() {
        let temporary = tempfile::tempdir().unwrap();
        let folder = temporary.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        let file = temporary.path().join("file");
        std::fs::write(&file, b"data").unwrap();
        let row = action(&folder);
        let mut args = row.clone();
        args.args = Some(" --flag".into());
        assert!(candidate(&args).is_none());
        let missing = temporary.path().join("missing");
        assert_eq!(
            verified_actions(&[row.clone(), action(&file), action(&missing), args]),
            vec![row.clone()],
        );
        let wrong_type = multi_launcher::actions::exec::checked_local_directory(&file)
            .unwrap_err().to_string();
        assert!(wrong_type.contains("Folder object has the wrong type"));
        let unavailable = multi_launcher::actions::exec::checked_local_directory(&missing)
            .unwrap_err().to_string();
        assert!(unavailable.contains("Cannot inspect folder path"));
        let command = candidate(&row).unwrap();
        let calls = RefCell::new(Vec::new());
        checked_association_with(&row, &command,
            |target| { calls.borrow_mut().push(target.to_owned()); Ok(()) },
        ).unwrap();
        assert_eq!(*calls.borrow(), vec![row.action.clone()]);
        std::fs::remove_dir(&folder).unwrap();
        assert!(checked_association_with(&row, &command,
            |_| panic!("stale folder reached association"),
        ).is_err());
    }

    #[test]
    fn saved_folder_reparse_leaf_and_ancestor_are_rejected_when_available() {
        use std::os::windows::fs::symlink_dir;
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("real");
        std::fs::create_dir_all(directory.join("child")).unwrap();
        let link = temporary.path().join("link");
        if symlink_dir(&directory, &link).is_err() { return; }
        assert!(verified_actions(&[action(&link), action(&link.join("child"))]).is_empty());
    }
}
