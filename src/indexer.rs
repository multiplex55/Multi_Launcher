use crate::actions::Action;
use walkdir::WalkDir;

pub fn index_paths(paths: &[String]) -> Vec<Action> {
    let mut results = Vec::new();
    for p in paths {
        for entry in WalkDir::new(p).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                    results.push(Action {
                        label: name.to_string(),
                        desc: entry.path().display().to_string(),
                        action: entry.path().display().to_string(),
                    });
                }
            }
        }
    }
    results
}
