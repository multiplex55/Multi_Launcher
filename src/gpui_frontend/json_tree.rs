//! Virtualized JSON tree presentation, backed by checked shared-session receipts.
use super::json_utility::JsonWorkspace;
use super::*;
use multi_launcher::json_transform::JsonSearchFields;
use multi_launcher::json_utility_session::{TreeCopyKind, TreeStamp};
use multi_launcher::json_utility_session::TreeMatch;
use gpui_kit::base::Selectable;
use gpui_kit::component::Sizable as _;

fn apply_row(
    view: &mut JsonWorkspace,
    stamp: TreeStamp,
    id: multi_launcher::json_transform::JsonTreeNodeId,
    toggle: bool,
    cx: &mut Context<JsonWorkspace>,
) {
    if !view.open || view.rehosting || !view.tree_mode || view.admission.is_fenced() {
        return;
    }
    if view.session.tree_stamp() != Some(stamp) { return; }
    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
    let changed = if toggle {
        view.session.tree_toggle(stamp, id)
    } else {
        view.session.tree_select(stamp, id)
    };
    if changed {
        cx.notify();
    }
}

pub(super) fn render(view: &mut JsonWorkspace, cx: &mut Context<JsonWorkspace>) -> AnyElement {
    let stamp = view.session.tree_stamp();
    let count = view.session.tree_row_count();
    let owner = cx.entity().downgrade();
    let mount = view.mount_receipt();
    let row_mount = mount.clone();
    let rows = gpui_kit::uniform_list("json-tree-rows", count, move |range, _, app| {
        let Some(entity) = owner.upgrade() else {
            return Vec::<AnyElement>::new();
        };
        let Some(stamp) = stamp else {
            return Vec::new();
        };
        range
            .filter_map(|index| {
                let row = entity.read(app).session.tree_row(index)?;
                let id = row.id;
                let select_owner = owner.clone();
                let toggle_owner = owner.clone();
                let select_mount = row_mount.clone();
                let toggle_mount = row_mount.clone();
                let mut line = div()
                    .id(SharedString::from(format!(
                        "json-tree-row-{}-{}",
                        stamp.source_generation(),
                        id.index()
                    )))
                    .h(px(32.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl(px((row.depth.min(32) * 12) as f32));
                if row.expandable {
                    line = line.child(
                        Button::new(SharedString::from(format!(
                            "json-tree-toggle-{}-{}", stamp.source_generation(),
                            id.index()
                        )))
                        .label(if row.expanded { "▾" } else { "▸" }).small()
                        .tab_stop(true)
                        .on_click(move |_, _, app| {
                            if !toggle_mount.is_current() { return; }
                            let _ = toggle_owner
                                .update(app, |view, cx| apply_row(view, stamp, id, true, cx));
                        }),
                    );
                }
                let hit = match row.matched { Some(TreeMatch::Key) => " · key match", Some(TreeMatch::Value) => " · value match", Some(TreeMatch::Both) => " · key and value match", None => "" };
                let active = if row.active_match { " · active match" } else { "" };
                let label = format!("{}{hit}{active}", row.label);
                Some(
                    line.child(
                        Button::new(SharedString::from(format!("json-tree-node-{}-{}", stamp.source_generation(), id.index())))
                            .label(label).small().selected(row.selected)
                            .tab_stop(true)
                            .on_click(move |_, _, app| {
                                if !select_mount.is_current() { return; }
                                let _ = select_owner
                                    .update(app, |view, cx| apply_row(view, stamp, id, false, cx));
                            }),
                    )
                    .into_any_element(),
                )
            })
            .collect()
    })
    .track_scroll(&view.tree_scroll)
    .size_full();
    let clear_mount = mount.clone();
    let mut fields = div().flex().flex_wrap().gap_2().child(
        Button::new("json-clear-tree-search")
            .label("Clear search")
            .tab_stop(true)
            .on_click(cx.listener(move |view, _, window, cx| {
                if !clear_mount.is_current() { return; }
                if !view.open || view.rehosting || !view.tree_mode || view.admission.is_fenced() {
                    return;
                }
                view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                view.tree_search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                let fields = view.session.tree_search_fields();
                view.session.tree_set_search(String::new(), fields);
                view.service(cx);
            })),
    );
    for (label, field) in [
        ("Keys", JsonSearchFields::Keys),
        ("Values", JsonSearchFields::Values),
        ("Both", JsonSearchFields::Both),
    ] {
        let field_mount = mount.clone();
        let selected = view.session.tree_search_fields() == field;
        fields = fields.child(
            Button::new(SharedString::from(format!("json-search-{label}")))
                .label(if selected {
                    format!("✓ {label}")
                } else {
                    label.to_owned()
                })
                .tab_stop(true)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if !field_mount.is_current() { return; }
                    if !view.open || view.rehosting || !view.tree_mode || view.admission.is_fenced() {
                        return;
                    }
                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                    let query = view.session.tree_search_query().to_owned();
                    view.session.tree_set_search(query, field);
                    view.service(cx);
                })),
        );
    }
    let (count_matches, active, pending) = view.session.tree_search_status();
    let status = if pending {
        "Searching…".to_owned()
    } else if let Some(index) = active {
        format!("{} / {} matches", index + 1, count_matches)
    } else {
        format!("{count_matches} matches")
    };
    let enabled = stamp.is_some() && active.is_some() && !pending;
    let mut controls = div().flex().flex_wrap().gap_2();
    for (label, previous) in [("Previous", true), ("Next", false)] {
        let navigation_mount = mount.clone();
        controls = controls.child(
            Button::new(SharedString::from(format!("json-match-{label}")))
                .label(label)
                .tab_stop(enabled)
                .disabled(!enabled)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if !navigation_mount.is_current() { return; }
                    if !view.open || view.rehosting || !view.tree_mode || view.admission.is_fenced() {
                        return;
                    }
                    let Some(stamp) = stamp else { return; };
                    if view.session.tree_stamp() != Some(stamp) { return; }
                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                    if let Some(index) = view.session.tree_move_match(stamp, previous) {
                        view.tree_scroll
                            .scroll_to_item(index, gpui_kit::ScrollStrategy::Nearest);
                    }
                    cx.notify();
                })),
        );
    }
    if let Some(stamp) = stamp {
        for (label, expanded) in [("Expand Branch", true), ("Collapse Branch", false)] {
            let branch_mount = mount.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!("json-branch-{label}")))
                    .label(label)
                    .tab_stop(true)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !branch_mount.is_current() { return; }
                        if view.open && !view.rehosting && view.tree_mode && !view.admission.is_fenced()
                            && view.session.tree_stamp() == Some(stamp) {
                            view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                            view.session.tree_expand_selected(stamp, expanded);
                            cx.notify();
                        }
                    })),
            );
        }
        for (label, kind) in [
            ("Copy Value", TreeCopyKind::Value),
            ("Copy Subtree", TreeCopyKind::Subtree),
            ("Copy JSON Pointer", TreeCopyKind::Pointer),
        ] {
            let copy_mount = mount.clone();
            let available = !view.lane.active;
            controls = controls.child(
                Button::new(SharedString::from(format!("json-tree-{label}")))
                    .label(label)
                    .tab_stop(available)
                    .disabled(!available)
                    .on_click(cx.listener(move |view, _, _, cx| { if copy_mount.is_current() { view.copy_tree(stamp, kind, cx); } })),
            );
        }
    }
    let details = view.session.tree_details();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .child(Input::new(&view.tree_search).aria_label("Tree search query"))
        .child(fields)
        .child(controls)
        .child(
            div()
                .id("json-tree-search-status")
                .role(Role::Status)
                .aria_label(status.clone())
                .child(status),
        )
        .child(
            div()
                .id("json-tree-viewport")
                .w_full()
                .h(px(220.0))
                .overflow_hidden()
                .child(rows),
        )
        .children(details.map(|details| {
            div()
                .id("json-tree-details")
                .w_full()
                .max_h(px(100.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .child(Label::new(format!("JSON Pointer: {}", details.path)))
                .child(Label::new(format!("Type: {}", details.kind)))
                .child(Label::new(details.value).whitespace_normal())
                .children(details.truncated.then(|| {
                    Label::new("Display is capped; copy the value for its complete contents.")
                }))
        }))
        .into_any_element()
}
