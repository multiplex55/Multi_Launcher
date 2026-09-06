use multi_launcher::diff::query::{DiffOpenPayload, OPEN_PREFIX, decode_payload};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::diff::DiffPlugin;

fn payload(query: &str) -> DiffOpenPayload {
    let action = DiffPlugin::default().search(query).remove(0).action;
    decode_payload(
        action
            .strip_prefix(OPEN_PREFIX)
            .expect("structured diff action"),
    )
    .unwrap()
}

#[test]
fn exposes_name_command_and_prefix() {
    let plugin = DiffPlugin::default();
    assert_eq!(plugin.name(), "diff");
    assert_eq!(plugin.query_prefixes(), &["diff"]);
    assert_eq!(plugin.commands().len(), 1);
}

#[test]
fn empty_one_and_two_argument_commands_are_encoded() {
    assert_eq!(
        payload("diff"),
        DiffOpenPayload {
            left: None,
            right: None
        }
    );
    assert_eq!(
        payload("diff left"),
        DiffOpenPayload {
            left: Some("left".into()),
            right: None
        }
    );
    assert_eq!(
        payload("diff left right"),
        DiffOpenPayload {
            left: Some("left".into()),
            right: Some("right".into())
        }
    );
}

#[test]
fn quotes_windows_paths_and_rejects_malformed_quote() {
    let parsed = payload(r#"diff "C:\Users\A B\left.txt" "D:\right.txt""#);
    assert_eq!(parsed.left.as_deref(), Some(r"C:\Users\A B\left.txt"));
    assert_eq!(parsed.right.as_deref(), Some(r"D:\right.txt"));
    assert_eq!(
        DiffPlugin::default().search("diff \"unfinished")[0].action,
        "error"
    );
}

#[test]
fn does_not_interfere_with_other_prefixes() {
    assert!(DiffPlugin::default().search("fs file query").is_empty());
    assert!(DiffPlugin::default().search("folder docs").is_empty());
}
