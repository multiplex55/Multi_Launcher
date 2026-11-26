//! Local XSD validation controls and picker authority.
use super::xml_utility::XmlWorkspace;
use super::*;
use multi_launcher::xml_toolkit_session::{XmlParseStatus, XmlTreeStatus};

pub(super) fn pane_close_blocked(native_picker: bool) -> bool {
    native_picker
}
pub(super) fn loaded_source_may_focus(owned_focus_current: bool, root_foreground: bool) -> bool {
    owned_focus_current && root_foreground
}

pub(super) fn file_effect_pending(picker: bool, read: bool) -> bool {
    picker || read
}

pub(super) fn picker_launch_current(
    mount: &utility_mount::MountReceipt,
    source: u64,
    expected_source: u64,
    admission_open: bool,
    owner_current: bool,
) -> bool {
    mount.is_current() && source == expected_source && admission_open && owner_current
}

pub(super) fn picker_receipt_current(
    mount: &utility_mount::MountReceipt,
    live: bool,
    source: u64,
    expected_source: u64,
    schema_current: bool,
    host_current: bool,
) -> bool {
    mount.is_current() && live && source == expected_source && schema_current && host_current
}

pub(super) fn render(view: &mut XmlWorkspace, cx: &mut Context<XmlWorkspace>) -> AnyElement {
    let mount = view.tree_mount();
    let choose_mount = mount.clone();
    let clear_mount = mount.clone();
    let validate_mount = mount.clone();
    let snapshot = view.current_snapshot();
    let live = view.schema_live();
    let can_choose =
        live && !view.save_review_pending() && view.clipboard_available() && snapshot.is_some();
    let can_clear = live
        && snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.schema.selected_path.is_some());
    let can_validate = live
        && snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.schema.selected_path.is_some()
                && snapshot.parse == XmlParseStatus::Ready
                && matches!(snapshot.tree, XmlTreeStatus::Ready(_))
        });
    let stamp = snapshot.as_ref().and_then(|snapshot| {
        snapshot
            .revision
            .map(|source| (source, snapshot.schema.revision))
    });
    let mut panel = div()
        .id("xml-validation-panel")
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(Label::new("XSD validation"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("xml-schema-choose")
                        .label("Choose local XSD…")
                        .disabled(!can_choose)
                        .tab_stop(can_choose)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if choose_mount.is_current() {
                                view.schema_choose(window, cx);
                            }
                        })),
                )
                .child(
                    Button::new("xml-schema-clear")
                        .label("Clear XSD")
                        .disabled(!can_clear)
                        .tab_stop(can_clear)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if clear_mount.is_current() {
                                if let Some((source, schema)) = stamp {
                                    view.schema_clear(source, schema, cx);
                                }
                            }
                        })),
                )
                .child(
                    Button::new("xml-schema-validate")
                        .label("Validate against XSD")
                        .disabled(!can_validate)
                        .tab_stop(can_validate)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if validate_mount.is_current() {
                                if let Some((source, schema)) = stamp {
                                    view.schema_validate(source, schema, cx);
                                }
                            }
                        })),
                ),
        );
    let path = snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .schema
                .selected_path
                .as_ref()
                .map(|path| path.as_path())
        })
        .or_else(|| view.schema_path_draft());
    panel = panel.child(path.map_or_else(
        || "No local XSD selected.".to_owned(),
        |path| {
            format!(
                "Selected XSD: {}",
                multi_launcher::xml_schema_view::bounded_path(path)
            )
        },
    ));
    panel = panel.child("Resolution stays under the selected XSD folder and accepts relative xs:include and xs:import paths. Imports need an explicit schemaLocation; imports inside included files are unsupported. xsi:schemaLocation is ignored, and network or absolute references are never fetched.");
    if let Some((message, tone)) = snapshot
        .as_ref()
        .and_then(|snapshot| multi_launcher::xml_schema_view::status_message(snapshot))
    {
        panel = panel.child(
            div()
                .id("xml-schema-status")
                .role(
                    if tone == multi_launcher::xml_schema_view::SchemaStatusTone::Error {
                        Role::Alert
                    } else {
                        Role::Status
                    },
                )
                .child(message),
        );
    }
    if view.schema_picker_pending() {
        panel = panel.child("Choosing file…");
    }
    panel.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::utility_mount::MountOwner;
    use super::{
        file_effect_pending, loaded_source_may_focus, pane_close_blocked, picker_launch_current,
        picker_receipt_current,
    };
    #[test]
    fn detached_read_allows_pane_close_and_focus_requires_same_owner_and_foreground() {
        assert!(pane_close_blocked(true));
        assert!(!pane_close_blocked(false));
        assert!(file_effect_pending(false, true));
        assert!(loaded_source_may_focus(true, true));
        assert!(!loaded_source_may_focus(false, true));
        assert!(!loaded_source_may_focus(true, false));
    }
    #[test]
    fn picker_or_read_holds_the_file_lifecycle_open() {
        assert!(!file_effect_pending(false, false));
        assert!(file_effect_pending(true, false));
        assert!(file_effect_pending(false, true));
    }
    #[test]
    fn delayed_picker_launch_requires_live_owner_mount_and_source() {
        let owner = MountOwner::default();
        let mount = owner.attach();
        assert!(picker_launch_current(&mount, 7, 7, true, true));
        assert!(!picker_launch_current(&mount, 8, 7, true, true));
        assert!(!picker_launch_current(&mount, 7, 7, false, true));
        assert!(!picker_launch_current(&mount, 7, 7, true, false));
        owner.retire();
        assert!(!picker_launch_current(&mount, 7, 7, true, true));
    }
    #[test]
    fn picker_receipt_rejects_source_schema_host_and_mount_changes() {
        let owner = MountOwner::default();
        let mount = owner.attach();
        assert!(picker_receipt_current(&mount, true, 4, 4, true, true));
        assert!(!picker_receipt_current(&mount, false, 4, 4, true, true));
        assert!(!picker_receipt_current(&mount, true, 5, 4, true, true));
        assert!(!picker_receipt_current(&mount, true, 4, 4, false, true));
        assert!(!picker_receipt_current(&mount, true, 4, 4, true, false));
        owner.retire();
        assert!(!picker_receipt_current(&mount, true, 4, 4, true, true));
    }
}
