use std::{fs, path::Path};

fn read(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

#[test]
fn typed_bus_has_no_legacy_or_wildcard_fallback() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bus = read(root, "src/commands/bus.rs");

    for variant in [
        "Launcher",
        "Query",
        "Dialog",
        "Calendar",
        "Note",
        "Link",
        "Todo",
        "MouseGesture",
        "MultiManager",
        "FileSearch",
        "Diff",
        "ClipboardModify",
        "Screenshot",
        "Shell",
        "Clipboard",
        "Calculator",
        "Storage",
        "Timer",
        "System",
        "BrowserTab",
        "Media",
        "Layout",
        "Macro",
        "Crop",
        "External",
    ] {
        assert!(
            bus.contains(&format!("Command::{variant}(")),
            "Command::{variant} must have an explicit typed bus route"
        );
    }

    assert!(
        !bus.contains("_ =>"),
        "the command bus must remain exhaustive"
    );
    assert!(!bus.contains(concat!("execute_legacy", "_command")));
}

#[test]
fn launcher_activation_has_one_parser_and_no_raw_protocol_router() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let actions = read(root, "src/gui/actions.rs");
    let activation_start = actions
        .find("    pub fn activate_action(")
        .expect("normal activation entry point");
    let activation_end = actions
        .find("    pub(crate) fn drain_clipboard_modify_immediate(")
        .expect("activation lifecycle boundary");
    let production_actions = &actions[activation_start..activation_end];

    assert_eq!(
        production_actions.matches("parse_command(").count(),
        1,
        "only normal activation should enter the canonical parser"
    );
    for forbidden in [
        concat!("activate_action", "_legacy"),
        concat!("activate_action", "_confirmed"),
        concat!("execute_legacy", "_command"),
        ".action.starts_with(",
        ".action.strip_prefix(",
        ".action.as_str()",
    ] {
        assert!(
            !production_actions.contains(forbidden),
            "launcher activation contains forbidden raw/legacy route: {forbidden}"
        );
    }

    for relative in [
        "src/commands/host.rs",
        "src/commands/mod.rs",
        "src/gui/command_host.rs",
    ] {
        let source = read(root, relative);
        assert!(
            !source.contains(concat!("LegacyCommand", "Host")),
            "{relative}"
        );
        assert!(
            !source.contains(concat!("execute_legacy", "_command")),
            "{relative}"
        );
    }
    assert!(!root.join("src/launcher/parse.rs").exists());
    assert!(!root.join("src/launcher/plan.rs").exists());
}

#[test]
fn command_handlers_do_not_inspect_original_action_protocols() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let handlers = root.join("src/commands/handlers");

    for entry in fs::read_dir(&handlers).expect("read command handlers") {
        let path = entry.expect("read command handler entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let compact: String = source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        for forbidden in [
            "original_action.action.starts_with(",
            "original_action.action.ends_with(",
            "original_action.action.contains(",
            "original_action.action.strip_prefix(",
            "original_action.action.split(",
            "original_action.action.split_once(",
            "original_action.action.as_str(",
            "original_action.action==",
            "original_action.action!=",
        ] {
            assert!(
                !compact.contains(forbidden),
                "{} inspects the original raw action protocol via {forbidden}",
                path.display()
            );
        }
    }
}
