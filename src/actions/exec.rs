use std::path::Path;

pub fn launch(path: &str, args: Option<&str>) -> anyhow::Result<()> {
    launch_with_identity(path, args).map(|_| ())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchIdentity {
    pub pid: Option<u32>,
    pub target: String,
}

/// Launch an external target while retaining process identity when the platform launcher exposes it.
pub fn launch_with_identity(path: &str, args: Option<&str>) -> anyhow::Result<LaunchIdentity> {
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
        command
            .spawn()
            .map(|child| LaunchIdentity {
                pid: Some(child.id()),
                target: path.to_string_lossy().into_owned(),
            })
            .map_err(|e| e.into())
    } else {
        open::that(path)
            .map(|_| LaunchIdentity {
                pid: None,
                target: path.to_string_lossy().into_owned(),
            })
            .map_err(|e| e.into())
    }
}
