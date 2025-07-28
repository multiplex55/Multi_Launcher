use std::path::Path;

pub fn launch(path: &str, args: Option<&str>) -> anyhow::Result<()> {
    let path = Path::new(path);
    let is_exe = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);

    let has_args = args.map(|a| !a.trim().is_empty()).unwrap_or(false);

    if is_exe || has_args {
        let mut command = std::process::Command::new(path);
        if let Some(arg_str) = args {
            let arg_str = arg_str.trim();
            if !arg_str.is_empty() {
                if let Some(list) = shlex::split(arg_str) {
                    command.args(list);
                } else {
                    command.args(arg_str.split_whitespace());
                }
            }
        }
        command.spawn().map(|_| ()).map_err(|e| e.into())
    } else {
        open::that(path).map_err(|e| e.into())
    }
}
