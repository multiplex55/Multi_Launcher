use crate::actions::Action;

pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    // If it's an .exe, launch it. If folder, open in explorer.
    if action.action.ends_with(".exe") {
        std::process::Command::new(&action.action)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.into())
    } else {
        open::that(&action.action).map_err(|e| e.into())
    }
}
