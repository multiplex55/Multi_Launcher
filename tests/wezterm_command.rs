use multi_launcher::gui::build_wezterm_command;
use std::path::Path;

#[test]
fn builds_wezterm_command() {
    let note = Path::new("note.txt");
    let (cmd, _cmd_str) = build_wezterm_command(note, "nvim");
    assert_eq!(cmd.get_program().to_string_lossy(), "wezterm");
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["start", "--", "nvim", "note.txt"]);
}
