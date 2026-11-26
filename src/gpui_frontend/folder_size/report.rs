//! Retained presentation of the shared Folder Size report.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::folder_size::report::{FolderSizeReport, KindFilter, ProjectionKey, SortColumn, SortDirection};
use multi_launcher::folder_size::FolderSizeEntryKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode { Table, Treemap, Both }

pub(super) struct ReportUi {
    pub(super) report: FolderSizeReport,
    mode: Mode,
    width: f32,
    scroll: gpui_kit::UniformListScrollHandle,
    tile_generation: Option<u64>,
    tile_focus: HashMap<usize, FocusHandle>,
    pub(super) menu: Option<super::actions::Menu>,
    pub(super) menu_serial: u64,
}
impl ReportUi { pub(super) fn reset_host(&mut self) { self.width = 0.0; self.tile_focus.clear(); self.menu = None; self.menu_serial = self.menu_serial.wrapping_add(1); } }
impl Default for ReportUi {
    fn default() -> Self { Self { report: FolderSizeReport::default(), mode: Mode::Table, width: 0.0,
        scroll: gpui_kit::UniformListScrollHandle::default(), tile_generation: None, tile_focus: HashMap::new(), menu: None, menu_serial: 0 } }
}

fn retain_mounted_focus<T>(focus: &mut HashMap<usize, T>, mounted: &HashSet<usize>) {
    focus.retain(|index, _| mounted.contains(index));
}

#[derive(Clone)]
pub(super) struct Receipt {
    pub(super) mount: utility_mount::MountReceipt,
    pub(super) key: ProjectionKey,
    pub(super) directory: Option<(u64, usize)>,
    pub(super) filter: KindFilter,
    pub(super) root: ExplicitDirectoryRoot,
    mode: Mode,
}
fn report_receipt_current(report: &FolderSizeReport, key: ProjectionKey,
    directory: Option<(u64, usize)>, filter: KindFilter, row: Option<usize>) -> bool {
    report.projection().is_some_and(|projection| projection.key() == key)
        && report.directory() == directory && report.filter() == filter
        && row.is_none_or(|index| report.visible_rows().is_some_and(|rows| rows.contains(&index)))
}
impl Receipt {
    pub(super) fn capture(view: &FolderSizeView) -> Option<Self> {
        Some(Self { mount: view.mount.clone(), key: view.report_ui.report.projection()?.key(),
            directory: view.report_ui.report.directory(), filter: view.report_ui.report.filter(),
            root: view.report_root.clone()?, mode: view.report_ui.mode })
    }
    pub(super) fn current(&self, view: &mut FolderSizeView, window: &mut Window, cx: &mut Context<FolderSizeView>) -> bool {
        view.sync_report();
        self.mount.is_current() && self.mount.matches(&view.mount) && view.gesture_current(window, cx)
            && view.report_root.as_ref() == Some(&self.root) && view.root_choice.as_ref() == Some(&self.root)
            && view.report_ui.mode == self.mode
            && report_receipt_current(&view.report_ui.report, self.key, self.directory, self.filter, None)
    }
    pub(super) fn row_current(&self, view: &mut FolderSizeView, index: usize, window: &mut Window, cx: &mut Context<FolderSizeView>) -> bool {
        self.current(view, window, cx)
            && report_receipt_current(&view.report_ui.report, self.key, self.directory, self.filter, Some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::{report_receipt_current, retain_mounted_focus, Mode, ReportUi};
    use multi_launcher::batch_rename::ExplicitDirectoryRoot;
    use multi_launcher::folder_size::report::KindFilter;
    use multi_launcher::folder_size::FolderSizeSession;
    use std::time::{Duration, Instant};

    #[test]
    fn tile_focus_cache_keeps_only_mounted_rows() {
        let mut handles = [(1, "one"), (2, "two"), (3, "three")].into_iter().collect();
        let mounted = [2, 4].into_iter().collect();
        retain_mounted_focus(&mut handles, &mounted);
        assert_eq!(handles.len(), 1);
        assert_eq!(handles.get(&2), Some(&"two"));
    }

    #[test]
    fn mode_changes_keep_selection_without_rescan_and_receipts_expire() {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("productivity_expansion");
        std::fs::create_dir_all(&parent).unwrap();
        let fixture = tempfile::tempdir_in(parent).unwrap();
        std::fs::write(fixture.path().join("file.txt"), b"abc").unwrap();
        let root = ExplicitDirectoryRoot::new(fixture.path().to_path_buf()).unwrap();
        let session = FolderSizeSession::new(|| {});
        session.refresh(root.clone(), true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let view = loop {
            let view = session.view();
            if view.result_is_current() { break view; }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        let mut ui = ReportUi::default();
        ui.report.sync(&view, Some(&root), Some(&root));
        ui.report.sync_visible_rows();
        let key = ui.report.projection().unwrap().key();
        let row = ui.report.visible_rows().unwrap()[0];
        assert!(ui.report.select(key.generation(), row));
        for mode in [Mode::Treemap, Mode::Both, Mode::Table] {
            ui.mode = mode;
            assert_eq!(ui.report.selected(), Some((key.generation(), row)));
            assert_eq!(session.view().generation(), key.generation());
            assert!(report_receipt_current(&ui.report, key, None, KindFilter::All, Some(row)));
        }
        ui.report.set_filter(KindFilter::Folders);
        assert!(!report_receipt_current(&ui.report, key, None, KindFilter::All, Some(row)));
        assert!(!report_receipt_current(&ui.report, key, None, KindFilter::Folders, Some(row)));
        session.refresh(root.clone(), true).unwrap();
        let next = loop {
            let view = session.view();
            if view.result_is_current() && view.generation() != key.generation() { break view; }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        ui.report.sync(&next, Some(&root), Some(&root));
        ui.report.sync_visible_rows();
        assert!(!report_receipt_current(&ui.report, key, None, KindFilter::Folders, Some(row)));
        session.close();
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 { return format!("{bytes} B"); }
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit + 1 < UNITS.len() { amount /= 1024.0; unit += 1; }
    format!("{amount:.1} {}", UNITS[unit])
}

fn row_status(row: &multi_launcher::folder_size::report::FolderSizeRow) -> String {
    use multi_launcher::folder_size::FolderSizeOmission;
    match row.kind() {
        FolderSizeEntryKind::Directory if row.complete() => "Complete".into(),
        FolderSizeEntryKind::Directory => "Incomplete".into(),
        FolderSizeEntryKind::File => "Measured".into(),
        FolderSizeEntryKind::Omitted(reason) => format!("Skipped: {}", match reason {
            FolderSizeOmission::ReparsePoint => "reparse point",
            FolderSizeOmission::CloudPlaceholder => "cloud placeholder",
            FolderSizeOmission::Inaccessible => "inaccessible",
            FolderSizeOmission::UnsupportedEntry => "unsupported entry",
            FolderSizeOmission::UnsupportedIdentity => "unsupported identity",
            FolderSizeOmission::RepeatedDirectoryIdentity => "repeated directory identity",
        }),
    }
}

pub(super) fn render(view: &mut FolderSizeView, window: &mut Window, cx: &mut Context<FolderSizeView>) -> AnyElement {
    let Some(receipt) = Receipt::capture(view) else {
        view.report_ui.tile_focus.clear();
        return div().child("Waiting for scan results.").into_any_element();
    };
    let mut controls = div().flex().flex_wrap().gap_1();
    for (mode, label) in [(Mode::Table, "Table"), (Mode::Treemap, "Treemap"), (Mode::Both, "Both")] {
        let issued = receipt.clone();
        controls = controls.child(Button::new(SharedString::from(format!("folder-size-mode-{label}"))).label(label)
            .selected(view.report_ui.mode == mode).on_click(cx.listener(move |view, _, window, cx| {
                if issued.current(view, window, cx) { view.report_ui.mode = mode; cx.notify(); }
            })));
    }
    for (filter, label) in [(KindFilter::All, "All"), (KindFilter::Files, "Files"), (KindFilter::Folders, "Folders")] {
        let issued = receipt.clone();
        controls = controls.child(Button::new(SharedString::from(format!("folder-size-filter-{label}"))).label(label)
            .selected(view.report_ui.report.filter() == filter).on_click(cx.listener(move |view, _, window, cx| {
                if issued.current(view, window, cx) { view.report_ui.report.set_filter(filter); cx.notify(); }
            })));
    }
    let selected = view.report_ui.report.selected();
    let can_open = selected.is_some_and(|(generation, index)| generation == receipt.key.generation()
        && view.report_ui.report.visible_rows().is_some_and(|rows| rows.contains(&index))
        && view.report_ui.report.projection().and_then(|p| p.row(index)).is_some_and(|r| r.kind() == FolderSizeEntryKind::Directory));
    let issued = receipt.clone();
    controls = controls.child(Button::new("folder-size-open-directory").label("Open folder").disabled(!can_open)
        .on_click(cx.listener(move |view, _, window, cx| {
            if let Some((generation, index)) = selected
                && issued.row_current(view, index, window, cx)
                && view.report_ui.report.selected() == Some((generation, index)) {
                view.report_ui.report.drill(generation, index); cx.notify();
            }
        })));
    let issued = receipt.clone();
    controls = controls.child(Button::new("folder-size-up").label("Up").disabled(receipt.directory.is_none())
        .on_click(cx.listener(move |view, _, window, cx| {
            if issued.current(view, window, cx) && let Some((generation, index)) = issued.directory {
                view.report_ui.report.up(generation, index); cx.notify();
            }
        })));
    let mut panel = div().flex().flex_col().gap_2().child(controls)
        .child(Label::new(view.report_ui.report.directory_path().or_else(|| view.report_root.as_ref().map(|r| r.as_path()))
            .map_or_else(String::new, |p| p.display().to_string())));
    if let Some(menu) = view.report_ui.menu.clone() {
        let serial = menu.serial;
        if menu.receipt.row_current(view, menu.index, window, cx)
            && view.report_ui.report.selected() == Some((menu.receipt.key.generation(), menu.index)) {
            let omitted = view.report_ui.report.projection().and_then(|p| p.row(menu.index))
                .is_none_or(|row| matches!(row.kind(), FolderSizeEntryKind::Omitted(_)));
            panel = panel.child(div().id("folder-size-row-menu").role(Role::Menu).aria_label("Folder Size entry actions")
                .track_focus(&view.menu_focus).flex().gap_1()
                .on_mouse_down_out(cx.listener(move |view, _, _, cx| {
                    if view.report_ui.menu.as_ref().is_some_and(|menu| menu.serial == serial) {
                        view.dismiss_report_menu(); cx.notify();
                    }
                }))
                .child(Button::new("folder-size-menu-copy").label("Copy path").on_click(cx.listener(move |view, _, window, cx|
                    view.choose_menu_action(serial, super::actions::Action::Copy, window, cx))))
                .child(Button::new("folder-size-menu-reveal").label("Reveal in Explorer").disabled(omitted).on_click(cx.listener(move |view, _, window, cx|
                    view.choose_menu_action(serial, super::actions::Action::RevealRow, window, cx)))));
        } else { view.dismiss_report_menu(); }
    }
    if matches!(view.report_ui.mode, Mode::Table | Mode::Both) {
        let mut headings = div().flex().items_center().gap_1().w_full();
        for (column, label) in [(SortColumn::Name, "Name"), (SortColumn::Type, "Type"), (SortColumn::Size, "Logical size"), (SortColumn::Share, "Share"), (SortColumn::Status, "Status")] {
            let issued = receipt.clone();
            let suffix = if view.report_ui.report.sort_column() == column {
                if view.report_ui.report.sort_direction() == SortDirection::Ascending { " ↑" } else { " ↓" }
            } else { "" };
            let button = Button::new(SharedString::from(format!("folder-size-sort-{label}"))).label(format!("{label}{suffix}"))
                .on_click(cx.listener(move |view, _, window, cx| {
                    if issued.current(view, window, cx) { view.report_ui.report.set_sort(column); cx.notify(); }
                }));
            let cell = div().overflow_hidden().child(button);
            headings = headings.child(match column {
                SortColumn::Name => cell.flex_1().min_w(px(0.0)),
                SortColumn::Type => cell.w(px(80.0)),
                SortColumn::Size => cell.w(px(112.0)),
                SortColumn::Share => cell.w(px(75.0)),
                SortColumn::Status => cell.w(px(170.0)),
            });
        }
        let rows = view.report_ui.report.visible_rows().cloned().unwrap_or_default();
        let projection = view.report_ui.report.projection().unwrap().clone();
        let owner = cx.entity().downgrade();
        let issued = receipt.clone();
        let list = gpui_kit::uniform_list("folder-size-report-rows", rows.len(), move |range, _, _| {
            range.filter_map(|position| {
                let index = *rows.get(position)?;
                let row = projection.row(index)?;
                let (bytes, share) = row.measurement(projection.total_logical_bytes());
                let kind = match row.kind() { FolderSizeEntryKind::File => "File", FolderSizeEntryKind::Directory => "Folder", FolderSizeEntryKind::Omitted(_) => "Skipped" };
                let target = owner.clone(); let issued = issued.clone();
                let menu_owner = owner.clone(); let menu_receipt = issued.clone();
                let right_owner = owner.clone(); let right_receipt = issued.clone();
                let name = Button::new(SharedString::from(format!("folder-size-row-{index}"))).label(row.name().to_owned())
                    .selected(selected == Some((issued.key.generation(), index)))
                    .on_click(move |_, window, app| { let _ = target.update(app, |view, cx| {
                        if issued.row_current(view, index, window, cx) { view.report_ui.report.select(issued.key.generation(), index); cx.notify(); }
                    }); });
                let menu = Button::new(SharedString::from(format!("folder-size-row-menu-{index}"))).label("…")
                    .on_click(move |_, window, app| { let _ = menu_owner.update(app, |view, cx|
                        view.open_row_menu(menu_receipt.clone(), index, window, cx)); });
                Some(div().id(SharedString::from(format!("folder-size-report-row-{index}"))).flex().items_center().gap_1().w_full().h(px(32.0))
                    .on_mouse_down(MouseButton::Right, move |_, window, app| {
                        let _ = right_owner.update(app, |view, cx| view.open_row_menu(right_receipt.clone(), index, window, cx));
                        app.stop_propagation();
                    })
                    .child(div().flex_1().min_w(px(0.0)).overflow_hidden().flex().child(name).child(menu))
                    .child(div().w(px(80.0)).overflow_hidden().child(Label::new(kind)))
                    .child(div().w(px(112.0)).overflow_hidden().child(Label::new(bytes.map_or_else(|| "—".into(), format_bytes))))
                    .child(div().w(px(75.0)).overflow_hidden().child(Label::new(share.map_or_else(|| "—".into(), |n| format!("{n:.1}%")))))
                    .child(div().w(px(170.0)).overflow_hidden().child(Label::new(row_status(row))))
                    .into_any_element())
            }).collect::<Vec<_>>()
        }).track_scroll(&view.report_ui.scroll).h(px(260.0)).w_full();
        panel = panel.child(div().id("folder-size-table-scroll").w_full().overflow_x_scroll()
            .child(div().min_w(px(650.0)).w_full().flex().flex_col().child(headings).child(list)));
    }
    if matches!(view.report_ui.mode, Mode::Treemap | Mode::Both) {
        let width = view.report_ui.width;
        let height = 260.0f32;
        let layout = (width > 0.0).then(|| view.report_ui.report.treemap(width as f64, height as f64)).flatten();
        let mut area = div().id("folder-size-treemap").relative().w_full().h(px(height)).overflow_hidden();
        let mut summary = String::new();
        if let Some(layout) = layout {
            const MAX_TILES: usize = 512;
            if view.report_ui.tile_generation != Some(receipt.key.generation()) {
                view.report_ui.tile_generation = Some(receipt.key.generation());
                view.report_ui.tile_focus.clear();
            }
            let eligible = layout.tiles.iter().filter(|tile| tile.rect.width >= 2.0 && tile.rect.height >= 2.0).count();
            let mounted = layout.tiles.iter().filter(|tile| tile.rect.width >= 2.0 && tile.rect.height >= 2.0)
                .take(MAX_TILES).map(|tile| tile.row).collect::<HashSet<_>>();
            retain_mounted_focus(&mut view.report_ui.tile_focus, &mounted);
            summary = format!("{} zero · {} unknown · {} skipped · {} incomplete · {} not shown",
                layout.zero_count, layout.unknown_count, layout.omitted_count, layout.provisional_count,
                layout.tiles.len().saturating_sub(eligible.min(MAX_TILES)));
            for tile in layout.tiles.iter().filter(|tile| tile.rect.width >= 2.0 && tile.rect.height >= 2.0).take(MAX_TILES) {
                let index = tile.row;
                let Some(row) = view.report_ui.report.projection().and_then(|p| p.row(index)) else { continue; };
                let issued = receipt.clone();
                let key_receipt = issued.clone();
                let menu_receipt = issued.clone();
                let label = row.name().to_owned();
                let color = if selected == Some((issued.key.generation(), index)) { 0x3979a5 } else { 0x466273 };
                let label_visible = tile.rect.width >= 80.0 && tile.rect.height >= 32.0;
                let focus = view.report_ui.tile_focus.entry(index).or_insert_with(|| cx.focus_handle()).clone();
                let focused = focus.is_focused(window);
                let click_focus = focus.clone();
                let key_focus = focus.clone();
                area = area.child(div().id(SharedString::from(format!("folder-size-tile-{index}"))).absolute()
                    .left(px(tile.rect.x as f32)).top(px(tile.rect.y as f32))
                    .w(px(tile.rect.width as f32)).h(px(tile.rect.height as f32))
                    .track_focus(&focus).tab_stop(true).role(Role::Button).aria_label(label.clone())
                    .overflow_hidden().bg(rgb(color)).when(label_visible, |tile| tile.child(Label::new(label)))
                    .when(focused, |tile| tile.border_1().border_color(rgb(0xffffff)))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |view, _, window, cx| {
                        if (view.report_ui.width - width).abs() <= 0.5 {
                            view.open_row_menu(menu_receipt.clone(), index, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                        if key_focus.is_focused(window) && matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && (view.report_ui.width - width).abs() <= 0.5
                            && key_receipt.row_current(view, index, window, cx) {
                            view.report_ui.report.select(key_receipt.key.generation(), index);
                            cx.stop_propagation(); cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if (view.report_ui.width - width).abs() <= 0.5 && issued.row_current(view, index, window, cx) {
                            click_focus.focus(window, cx);
                            view.report_ui.report.select(issued.key.generation(), index); cx.notify();
                        }
                    })));
            }
        } else {
            view.report_ui.tile_focus.clear();
        }
        let owner = cx.entity().downgrade(); let mount = receipt.mount.clone(); let host = view.host;
        let view_width = view.report_ui.width;
        area = area.child(canvas(move |bounds, window, app| {
            let width = bounds.size.width.as_f32();
            let owner = owner.clone(); let mount = mount.clone();
            if window.window_handle().window_id() == host.window_id() && (width - view_width).abs() > 0.5 {
                window.defer(app, move |_, app| { let _ = owner.update(app, |view, cx| {
                    if mount.is_current() && mount.matches(&view.mount) && (view.report_ui.width - width).abs() > 0.5 {
                        view.report_ui.width = width; cx.notify();
                    }
                }); });
            }
        }, |_, _, _, _| {}).absolute().size_full());
        panel = panel.child(area).child(Label::new(summary));
    } else {
        view.report_ui.tile_focus.clear();
    }
    panel.into_any_element()
}
