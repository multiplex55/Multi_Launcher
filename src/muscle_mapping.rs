use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::toast_log::append_toast_log;

/// Path to the default muscle mapping file.
pub const DEFAULT_MUSCLE_MAPPING_FILE: &str = "muscle_mappings.json";

/// Representation of a single muscle mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuscleMapping {
    /// Name of the muscle group.
    pub group: String,
    /// List of muscles belonging to the group.
    pub muscles: Vec<String>,
}

/// Global in-memory store for muscle mappings along with their source
/// timestamp. The timestamp allows conflict resolution when merging
/// mappings from multiple files.
static MUSCLE_MAPPINGS: Lazy<Mutex<HashMap<String, (MuscleMapping, SystemTime)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Save the current in-memory mappings to the default JSON file.
fn save_default_mappings() -> Result<()> {
    let map = MUSCLE_MAPPINGS.lock().unwrap();
    let mappings: Vec<&MuscleMapping> = map.values().map(|(m, _)| m).collect();
    let json = serde_json::to_string_pretty(&mappings)?;
    fs::write(DEFAULT_MUSCLE_MAPPING_FILE, json)?;
    Ok(())
}

/// Load muscle mappings from a set of JSON files and merge them into the
/// global map. Existing entries are replaced if the incoming file is newer
/// (based on creation timestamp).
pub fn load_muscle_mappings<P: AsRef<Path>>(paths: &[P]) -> Result<()> {
    let mut map = MUSCLE_MAPPINGS.lock().unwrap();

    for p in paths {
        let path = p.as_ref();
        let meta = fs::metadata(path)?;
        let ts = meta.created().or_else(|_| meta.modified())?;

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let entries: Vec<MuscleMapping> = serde_json::from_reader(reader)?;

        for m in entries {
            match map.entry(m.group.clone()) {
                Entry::Vacant(e) => {
                    e.insert((m, ts));
                }
                Entry::Occupied(mut e) => {
                    if ts >= e.get().1 {
                        e.insert((m, ts));
                    }
                }
            }
        }

        append_toast_log(&format!("Loaded muscle mappings from {}", path.display()));
    }

    drop(map);
    save_default_mappings()?;

    // Notify GUI to refresh the muscle mapping panel with the new data.
    crate::gui::refresh_muscle_mappings();
    Ok(())
}

/// Retrieve a copy of the current muscle mappings.
pub fn get_muscle_mappings() -> HashMap<String, MuscleMapping> {
    let map = MUSCLE_MAPPINGS.lock().unwrap();
    map.iter()
        .map(|(k, (v, _))| (k.clone(), v.clone()))
        .collect()
}
