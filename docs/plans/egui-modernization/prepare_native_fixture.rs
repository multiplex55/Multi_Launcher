//! Creates one bounded, isolated native QA workspace using the Settings and
//! Action serializers from explicitly selected fast-dev artifacts. This helper
//! is intentionally not wired into Cargo.toml; compile it with
//! build_fixture_helper.ps1 and pass that build's sidecar receipt when running.

use multi_launcher::actions::Action;
use multi_launcher::common::persistence::save_json_atomic;
use multi_launcher::settings::model::Settings;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;

const BASELINE_SOURCE_SHA: &str = "72a01bd5451060d3f6b640b83acd081d7db76e77";
const MAX_ACTIONS: usize = 10_000;
const MAX_NOTES: usize = 5_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 7 {
        return Err("usage: prepare_native_fixture <run-dir> <actions> <notes> <build-head> <exe> <toolchain> <build-receipt>".into());
    }

    let requested_root = PathBuf::from(&args[0]);
    let action_count = args[1].to_string_lossy().parse::<usize>()?;
    let note_count = args[2].to_string_lossy().parse::<usize>()?;
    let build_head = args[3].to_string_lossy().into_owned();
    let executable = PathBuf::from(&args[4]);
    let toolchain = args[5].to_string_lossy().into_owned();
    let build_receipt_path = PathBuf::from(&args[6]);
    if action_count > MAX_ACTIONS || note_count > MAX_NOTES {
        return Err(format!(
            "fixture counts exceed actions <= {MAX_ACTIONS}, notes <= {MAX_NOTES}"
        )
        .into());
    }

    let current_dir = env::current_dir()?;
    let base = current_dir.join("target/egui-modernization/native-baseline");
    fs::create_dir_all(&base)?;
    let base = base.canonicalize()?;
    let root = if requested_root.is_absolute() {
        requested_root
    } else {
        current_dir.join(requested_root)
    };
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("run directory must have a UTF-8 final component")?;
    if !name.starts_with("run-") {
        return Err(
            "run directory must be a new run-* child of target/egui-modernization/native-baseline"
                .into(),
        );
    }
    let parent = root
        .parent()
        .ok_or("run directory has no parent")?
        .canonicalize()?;
    if parent != base {
        return Err(
            "run directory must be directly beneath target/egui-modernization/native-baseline"
                .into(),
        );
    }
    if root.exists() {
        return Err(format!(
            "refusing to reuse existing fixture directory {}",
            root.display()
        )
        .into());
    }
    if !executable.is_file() {
        return Err(format!(
            "built app executable is not a file: {}",
            executable.display()
        )
        .into());
    }
    let helper_executable = env::current_exe()?;
    let helper_executable_bytes = fs::read(&helper_executable)?;
    let helper_executable_sha256 = sha256_hex(&helper_executable_bytes);
    let build_receipt_bytes = fs::read(&build_receipt_path)?;
    let build_receipt: serde_json::Value = serde_json::from_slice(&build_receipt_bytes)?;
    validate_build_receipt(&build_receipt, &helper_executable_sha256)?;
    let build_receipt_sha256 = sha256_hex(&build_receipt_bytes);

    fs::create_dir(&root)?;
    fs::write(root.join("helper-build-receipt.json"), &build_receipt_bytes)?;
    let notes_dir = root.join("notes");
    let templates_dir = root.join("templates");
    let screenshots_dir = root.join("screenshots");
    fs::create_dir(&notes_dir)?;
    fs::create_dir(&templates_dir)?;

    let mut settings = Settings::default();
    settings.hotkey = Some("Ctrl+Alt+F10".into());
    settings.quit_hotkey = None;
    settings.help_hotkey = None;
    settings.index_paths = Some(Vec::new());
    settings.max_indexed_items = None;
    settings.plugin_dirs = Some(Vec::new());
    settings.enabled_plugins = Some(HashSet::new());
    settings.enabled_capabilities = Some(HashMap::new());
    settings.debug_logging = false;
    settings.log_file = None;
    settings.screenshot_dir = Some(screenshots_dir.to_string_lossy().into_owned());
    settings.screenshot_save_file = false;
    settings.screenshot_auto_save = false;
    settings.pinned_panels.clear();
    settings.dashboard.enabled = false;
    settings.dashboard.show_when_query_empty = false;
    settings.dashboard.config_path =
        Some(root.join("dashboard.json").to_string_lossy().into_owned());
    settings.multi_manager.enabled = false;
    settings.multi_manager.workspaces_path = root
        .join("multi_manager_workspaces.json")
        .to_string_lossy()
        .into_owned();
    settings.multi_manager.bindings_path = root
        .join("multi_manager_bindings.json")
        .to_string_lossy()
        .into_owned();
    settings.radial.enabled = false;
    settings.radial.shared_tap_hold = false;
    settings.radial.global_item_inputs = false;

    let settings_path = root.join("settings.json");
    settings.save(&settings_path.to_string_lossy())?;
    let loaded_settings = Settings::load(&settings_path.to_string_lossy())?;
    if loaded_settings.hotkey != Some("Ctrl+Alt+F10".into())
        || loaded_settings.index_paths != Some(Vec::new())
        || loaded_settings.plugin_dirs != Some(Vec::new())
        || loaded_settings.enabled_plugins != Some(HashSet::new())
        || loaded_settings.radial.enabled
        || loaded_settings.multi_manager.enabled
    {
        return Err("settings serializer/load round-trip changed an isolation field".into());
    }

    let actions = (0..action_count)
        .map(|index| Action {
            label: format!("M0-B fixture action {index:05}"),
            desc: "Synthetic query-set row; Enter only changes launcher query".into(),
            action: format!("query:fixture-enter-{index:05}"),
            args: None,
        })
        .collect::<Vec<_>>();
    let actions_path = root.join("actions.json");
    save_json_atomic(&actions_path, &actions)?;
    let loaded_actions: Vec<Action> = serde_json::from_slice(&fs::read(&actions_path)?)?;
    if loaded_actions != actions {
        return Err("action serializer/load round-trip changed fixture rows".into());
    }

    let mut note_receipts = Vec::with_capacity(note_count);
    for index in 0..note_count {
        let filename = format!("m0b-note-{index:05}.md");
        let relative_path = format!("notes/{filename}");
        let content = format!(
            "# M0-B Synthetic Note {index:05}\n\nFixture note {index:05} generated for isolated baseline reflow and selection checks.\n\nSynthetic token: m0b-note-fixture-{index:05}.\n"
        );
        fs::write(root.join(&relative_path), content.as_bytes())?;
        note_receipts.push(serde_json::json!({
            "path": relative_path,
            "sha256": sha256_hex(content.as_bytes()),
        }));
    }

    let executable_bytes = fs::read(&executable)?;
    let settings_bytes = fs::read(&settings_path)?;
    let actions_bytes = fs::read(&actions_path)?;
    let manifest = serde_json::json!({
        "schema": 1,
        "fixture_kind": "m0b-native-synthetic",
        "source_code_baseline_sha": BASELINE_SOURCE_SHA,
        "build_head": build_head,
        "toolchain": toolchain,
        "fixture_helper_executable_sha256": helper_executable_sha256.as_str(),
        "fixture_helper_build_receipt_file": "helper-build-receipt.json",
        "fixture_helper_build_receipt_sha256": build_receipt_sha256.as_str(),
        "executable_path_at_generation": executable.to_string_lossy(),
        "executable_sha256": sha256_hex(&executable_bytes),
        "run_directory_leaf": name,
        "settings_sha256": sha256_hex(&settings_bytes),
        "actions_sha256": sha256_hex(&actions_bytes),
        "action_count": action_count,
        "action_semantics": "query:<synthetic-text>; in-app query update only",
        "notes_directory": "notes",
        "note_count": note_count,
        "notes": note_receipts,
        "templates_directory": "templates",
        "clipboard_sync": "must be suppressed by process-scoped ML_SKIP_CLIPBOARD_SYNC=1",
        "notes_path_env": "ML_NOTES_DIR=fixture-root/notes",
        "settings_path_env": "process current directory is fixture root",
        "manifest_hash_note": "manifest hash is external to avoid self-reference",
        "partial_settings_json": false,
    });
    let manifest_path = root.join("fixture-manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;

    println!("fixture_root={}", root.display());
    println!("manifest={}", manifest_path.display());
    println!("executable_sha256={}", sha256_hex(&executable_bytes));
    println!("fixture_helper_executable_sha256={helper_executable_sha256}");
    println!("fixture_helper_build_receipt_sha256={build_receipt_sha256}");
    println!("settings_sha256={}", sha256_hex(&settings_bytes));
    println!("actions_sha256={}", sha256_hex(&actions_bytes));
    println!("action_count={action_count} note_count={note_count}");
    Ok(())
}

fn validate_build_receipt(
    receipt: &serde_json::Value,
    helper_executable_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if receipt.get("schema").and_then(serde_json::Value::as_u64) != Some(1)
        || receipt.get("profile").and_then(serde_json::Value::as_str) != Some("fast-dev")
        || receipt
            .get("helper_executable_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(helper_executable_sha256)
    {
        return Err("build receipt does not match this fast-dev fixture helper executable".into());
    }

    let libraries = receipt
        .get("libraries")
        .and_then(serde_json::Value::as_object)
        .ok_or("build receipt has no library artifact map")?;
    for crate_name in ["multi_launcher", "serde_json", "sha2"] {
        let artifact = libraries
            .get(crate_name)
            .ok_or_else(|| format!("build receipt is missing the {crate_name} artifact"))?;
        let filename = artifact
            .get("file")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("build receipt has no filename for {crate_name}"))?;
        let path = artifact
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("build receipt has no path for {crate_name}"))?;
        let hash = artifact
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("build receipt has no SHA-256 for {crate_name}"))?;
        let expected_prefix = format!("lib{crate_name}-");
        let is_canonical_launcher = crate_name == "multi_launcher"
            && filename == "libmulti_launcher.rlib"
            && path == "target/fast-dev/libmulti_launcher.rlib";
        let is_hashed_dependency = filename.starts_with(expected_prefix.as_str())
            && filename.ends_with(".rlib")
            && filename.len() > expected_prefix.len() + ".rlib".len()
            && path == format!("target/fast-dev/deps/{filename}");
        if (!is_canonical_launcher && !is_hashed_dependency)
            || hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(
                format!("build receipt has an invalid {crate_name} artifact identity").into(),
            );
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
