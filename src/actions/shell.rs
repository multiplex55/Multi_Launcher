use crate::plugins::shell::use_wezterm;

pub fn build_shell_command(cmd: &str, keep_open: bool) -> (std::process::Command, String) {
    let arg = if keep_open { "/K" } else { "/C" };
    if use_wezterm() {
        let mut c = std::process::Command::new("wezterm");
        c.arg("start").arg("--").arg("cmd").arg(arg).arg(cmd);
        let desc = format!("wezterm start -- cmd {arg} {cmd}");
        (c, desc)
    } else {
        let mut c = std::process::Command::new("cmd");
        c.arg(arg).arg(cmd);
        let desc = format!("cmd {arg} {cmd}");
        (c, desc)
    }
}

pub fn run(cmd: &str, keep_open: bool) -> anyhow::Result<()> {
    let (mut command, _) = build_shell_command(cmd, keep_open);
    command.spawn().map(|_| ()).map_err(|e| e.into())
}

pub fn add(name: &str, args: &str) -> anyhow::Result<()> {
    crate::plugins::shell::append_shell_cmd(crate::plugins::shell::SHELL_CMDS_FILE, name, args)?;
    Ok(())
}

pub fn remove(name: &str) -> anyhow::Result<()> {
    crate::plugins::shell::remove_shell_cmd(crate::plugins::shell::SHELL_CMDS_FILE, name)?;
    Ok(())
}
