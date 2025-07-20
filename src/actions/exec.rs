use std::path::Path;

pub fn launch(path: &str, args: Option<&str>) -> anyhow::Result<()> {
    let path = Path::new(path);
    let is_exe = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);

    if is_exe || args.is_some() {
        let mut command = std::process::Command::new(path);
        if let Some(arg_str) = args {
            if let Some(list) = shlex::split(arg_str) {
                command.args(list);
            } else {
                command.args(arg_str.split_whitespace());
            }
        }
        command.spawn().map(|_| ()).map_err(|e| e.into())
    } else {
        open::that(path).map_err(|e| e.into())
    }
}
