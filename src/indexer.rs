use crate::actions::Action;
use walkdir::WalkDir;

/// Index the provided filesystem paths and return a list of [`Action`]s.
///
/// Any errors encountered while traversing the directory tree are logged and
/// returned to the caller.
pub fn index_paths(paths: &[String]) -> anyhow::Result<Vec<Action>> {
    let mut results = Vec::new();
    for p in paths {
        for entry in WalkDir::new(p).into_iter() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!(path = %p, error = %e, "failed to read directory entry");
                    return Err(e.into());
                }
            };
            if entry.file_type().is_file() {
                if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                    results.push(Action {
                        label: name.to_string(),
                        desc: entry.path().display().to_string(),
                        action: entry.path().display().to_string(),
                        args: None,
                    });
                }
            }
        }
    }
    Ok(results)
}
