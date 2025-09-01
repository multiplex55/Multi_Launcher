use multi_launcher::actions::shell::build_shell_command;
use serial_test::serial;

fn set_use_wezterm(value: bool) {
    use multi_launcher::plugin::Plugin;
    let mut plugin = multi_launcher::plugins::shell::ShellPlugin;
    plugin.apply_settings(&serde_json::json!({ "open_in_wezterm": value }));
}

struct ResetUseWezterm;

impl Drop for ResetUseWezterm {
    fn drop(&mut self) {
        set_use_wezterm(false);
    }
}

#[test]
#[serial]
fn cmd_is_used_when_wezterm_disabled() {
    set_use_wezterm(false);
    let _reset = ResetUseWezterm;

    let (cmd_c, _desc_c) = build_shell_command("echo test", false);
    assert_eq!(cmd_c.get_program().to_string_lossy(), "cmd");
    let args_c: Vec<_> = cmd_c
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args_c, ["/C", "echo test"]);

    let (cmd_k, _desc_k) = build_shell_command("echo test", true);
    assert_eq!(cmd_k.get_program().to_string_lossy(), "cmd");
    let args_k: Vec<_> = cmd_k
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args_k, ["/K", "echo test"]);
}

#[test]
#[serial]
fn wezterm_is_used_when_enabled() {
    set_use_wezterm(true);
    let _reset = ResetUseWezterm;

    let (cmd, _desc) = build_shell_command("echo test", false);
    assert_eq!(cmd.get_program().to_string_lossy(), "wezterm");
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(&args[..3], ["start", "--", "cmd"]);
    assert_eq!(args[3], "/C");
    assert_eq!(args[4], "echo test");
}
