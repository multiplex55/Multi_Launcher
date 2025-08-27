pub fn run(cmd: &str, keep_open: bool) -> anyhow::Result<()> {
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.arg(if keep_open { "/K" } else { "/C" }).arg(cmd);
        c
    };
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
