//! Checked native shell and Dock/Pop Out transitions for JSON.
use super::json_utility::JsonWorkspace;
use super::utility_mount::UtilityPopup;
use super::*;
use multi_launcher::settings::{RememberedPresentation, UtilityToolKey};

pub(super) fn shell_geometry_mount_is_current(
    mounted_in_popup: bool,
    receipt: &utility_mount::MountReceipt,
) -> bool {
    mounted_in_popup && receipt.is_current()
}

/// A checked geometry observer owned by one utility shell mount.
pub(super) struct ShellGeometry {
    key: UtilityToolKey,
    pub(super) receipt: utility_mount::MountReceipt,
    stamp: NativeGeometryStamp,
    slot: NativeGeometrySlot,
    _binding: NativeGeometryBinding,
    factory_size: Option<PreferredWindowSize>,
}
impl ShellGeometry {
    pub(super) fn attach(
        native: NativeWindow,
        key: UtilityToolKey,
        receipt: utility_mount::MountReceipt,
        sequence: Arc<AtomicU64>,
        wake: Arc<HashWakeSignal>,
    ) -> Result<Self, String> {
        if !receipt.is_current() {
            return Err("utility mount changed".into());
        }
        let target = checked_geometry_target(native)
            .ok_or_else(|| "utility HWND could not be checked".to_owned())?;
        let generation = sequence.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let stamp = NativeGeometryStamp {
            session_epoch: generation,
            mount_generation: generation,
        };
        let slot = new_native_geometry_slot();
        let factory_size = native_window_metrics(target, 1.0)
            .ok()
            .flatten()
            .and_then(outer_size_in_points);
        let binding = NativeGeometryBinding::attach_checked(
            target,
            key,
            stamp,
            slot.clone(),
            1.0,
            sequence,
            Arc::new(move |_| wake.notify()),
        )?;
        Ok(Self {
            key,
            receipt,
            stamp,
            slot,
            _binding: binding,
            factory_size,
        })
    }
    pub(super) fn take(
        &self,
    ) -> Option<(UtilityNativeGeometrySample, Option<PreferredWindowSize>)> {
        if !self.receipt.is_current() {
            return None;
        }
        let sample = take_native_geometry(&self.slot, self.key, self.stamp)?;
        let convert = |rect: NativeWindowRect| PhysicalWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        Some((
            UtilityNativeGeometrySample {
                position: convert(sample.position),
                position_scale: sample.position_scale,
                position_sequence: sample.position_sequence,
                size: sample.size.map(convert),
                size_sequence: sample.size_sequence,
            },
            self.factory_size,
        ))
    }
}

trait HostedUtility: Render + Sized + 'static {
    const KIND: utility_mount::UtilityKind;
    const PANEL: Panel;
    const KEY: &'static str;
    const TITLE: &'static str;
    const INITIAL: (f32, f32);
    const MINIMUM: (f32, f32);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>>;
    fn popup(root: &RootWindow) -> Option<&UtilityPopup>;
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup>;
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow>;
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool);
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>);
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>);
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>);
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>);
    fn observe(root: &mut RootWindow, id: WindowId, cx: &mut Context<RootWindow>);
    fn root(&self) -> WeakEntity<RootWindow>;
    fn open(&self) -> bool;
    fn rehosting(&self) -> bool;
    fn close_blocked(&self, _: &App) -> bool { false }
    fn activation_lost(&mut self, _: &mut Context<Self>) {}
    fn activated(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn outer_scroll(&self) -> Option<ScrollHandle> { None }
    fn popout(&self) -> bool;
    fn fenced(&self) -> bool;
    fn receipt(&self) -> utility_mount::MountReceipt;
    fn mounted(&self, id: WindowId, popout: bool) -> bool;
    fn replace(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn set_popout(&mut self, value: bool);
    fn set_rehosting(&mut self, value: bool, cx: &mut Context<Self>);
    fn set_error(&mut self, value: Option<String>);
    fn set_geometry_error(&mut self, value: Option<String>);
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn shutdown(&mut self, cx: &mut Context<Self>);
    fn pinned(root: &RootWindow) -> bool;
}
impl HostedUtility for JsonWorkspace {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::Json;
    const PANEL: Panel = Panel::JsonUtilityDialog;
    const KEY: &'static str = "json-utility";
    const TITLE: &'static str = "JSON Utility";
    const INITIAL: (f32, f32) = (700.0, 480.0);
    const MINIMUM: (f32, f32) = (340.0, 220.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> {
        root.json_view.clone()
    }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> {
        root.json_popup.as_ref()
    }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> {
        &mut root.json_popup
    }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> {
        &mut root.json_retiring
    }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) {
        root.drain_json_native_geometry(cx, detach);
    }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        root.reattach_json_geometry(cx);
    }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        root.set_json_size_error(message, cx);
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) {
        root.remember_json_presentation(mode, cx);
    }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        root.retire_json_workspace(cx);
    }
    fn observe(root: &mut RootWindow, id: WindowId, cx: &mut Context<RootWindow>) {
        root.observe_structured_pin_activation(Self::PANEL, id, cx);
    }
    fn activation_lost(&mut self, cx: &mut Context<Self>) { self.retire_jump(cx); }
    fn root(&self) -> WeakEntity<RootWindow> {
        self.root.clone()
    }
    fn open(&self) -> bool {
        self.open
    }
    fn rehosting(&self) -> bool {
        self.rehosting
    }
    fn popout(&self) -> bool {
        self.popout
    }
    fn fenced(&self) -> bool {
        self.admission.is_fenced()
    }
    fn receipt(&self) -> utility_mount::MountReceipt {
        self.mount_receipt()
    }
    fn mounted(&self, id: WindowId, popout: bool) -> bool {
        self.is_mounted_in(id, popout)
    }
    fn replace(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_mount(host, native, None, point(px(0.0), px(0.0)), window, cx);
    }
    fn set_popout(&mut self, value: bool) {
        self.popout = value;
    }
    fn set_rehosting(&mut self, value: bool, _: &mut Context<Self>) {
        self.rehosting = value;
    }
    fn set_error(&mut self, value: Option<String>) {
        self.presentation_error = value;
    }
    fn set_geometry_error(&mut self, value: Option<String>) {
        self.geometry_error = value;
    }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_dock_focus(window, cx);
    }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_popout_focus(window, cx);
    }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
    }
    fn shutdown(&mut self, _: &mut Context<Self>) {
        JsonWorkspace::shutdown(self);
    }
    fn pinned(root: &RootWindow) -> bool {
        root.structured_pins.saved(Self::PANEL)
    }
}
impl HostedUtility for super::regex_tester::RegexWorkspace {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::Regex;
    const PANEL: Panel = Panel::RegexTesterDialog;
    const KEY: &'static str = "regex-tester";
    const TITLE: &'static str = "Regex Tester";
    const INITIAL: (f32, f32) = (960.0, 680.0);
    const MINIMUM: (f32, f32) = (360.0, 240.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> {
        root.regex_view.clone()
    }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> {
        root.regex_popup.as_ref()
    }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> {
        &mut root.regex_popup
    }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> {
        &mut root.regex_retiring
    }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) {
        root.drain_regex_native_geometry(cx, detach);
    }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        root.reattach_regex_geometry(cx);
    }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        root.set_regex_size_error(message, cx);
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) {
        root.remember_host_presentation(Self::KEY, mode, cx);
    }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        if let Some(view) = root.regex_view.clone() {
            view.update(cx, |view, _| view.shutdown());
        }
    }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn activation_lost(&mut self, cx: &mut Context<Self>) { self.retire_jump(cx); }
    fn root(&self) -> WeakEntity<RootWindow> {
        self.root.clone()
    }
    fn open(&self) -> bool {
        self.open
    }
    fn rehosting(&self) -> bool {
        self.rehosting
    }
    fn outer_scroll(&self) -> Option<ScrollHandle> { Some(self.mount.outer_scroll.clone()) }
    fn close_blocked(&self, _: &App) -> bool {
        self.clipboard_pending || self.presets.blocks_host()
    }
    fn popout(&self) -> bool {
        self.popout
    }
    fn fenced(&self) -> bool {
        self.admission.is_fenced()
    }
    fn receipt(&self) -> utility_mount::MountReceipt {
        self.mount_receipt()
    }
    fn mounted(&self, id: WindowId, popout: bool) -> bool {
        self.is_mounted_in(id, popout)
    }
    fn replace(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_mount(host, native, window, cx);
    }
    fn set_popout(&mut self, value: bool) {
        self.popout = value;
    }
    fn set_rehosting(&mut self, value: bool, _: &mut Context<Self>) {
        self.rehosting = value;
    }
    fn set_error(&mut self, value: Option<String>) {
        self.presentation_error = value;
    }
    fn set_geometry_error(&mut self, value: Option<String>) {
        self.geometry_error = value;
    }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_dock_focus(window, cx);
    }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_popout_focus(window, cx);
    }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
    }
    fn shutdown(&mut self, _: &mut Context<Self>) {
        super::regex_tester::RegexWorkspace::shutdown(self);
    }
    fn pinned(_: &RootWindow) -> bool {
        false
    }
}
impl HostedUtility for super::clipboard_history::ClipboardHistory {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::ClipboardHistory;
    const PANEL: Panel = Panel::ClipboardDialog;
    const KEY: &'static str = "clipboard-history";
    const TITLE: &'static str = "Clipboard History";
    const INITIAL: (f32, f32) = (360.0, 240.0);
    const MINIMUM: (f32, f32) = (100.0, 100.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> {
        root.clipboard_history.clone()
    }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> {
        root.clipboard_popup.as_ref()
    }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> {
        &mut root.clipboard_popup
    }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> {
        &mut root.clipboard_retiring
    }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) {
        root.drain_clipboard_native_geometry(cx, detach);
    }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        root.reattach_clipboard_geometry(cx);
    }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        root.set_clipboard_size_error(message, cx);
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) {
        root.remember_host_presentation(Self::KEY, mode, cx);
    }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) {
        if let Some(view) = root.clipboard_history.clone() {
            view.update(cx, |view, _| view.shutdown());
        }
    }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn root(&self) -> WeakEntity<RootWindow> {
        self.root.clone()
    }
    fn open(&self) -> bool {
        self.open
    }
    fn rehosting(&self) -> bool {
        self.rehosting
    }
    fn close_blocked(&self, _: &App) -> bool {
        self.busy()
    }
    fn popout(&self) -> bool {
        self.popout
    }
    fn fenced(&self) -> bool {
        self.admission.is_fenced()
    }
    fn receipt(&self) -> utility_mount::MountReceipt {
        self.mount_receipt()
    }
    fn mounted(&self, id: WindowId, popout: bool) -> bool {
        self.is_mounted_in(id, popout)
    }
    fn replace(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_mount(host, native, window, cx);
    }
    fn set_popout(&mut self, value: bool) {
        self.popout = value;
    }
    fn set_rehosting(&mut self, value: bool, _: &mut Context<Self>) {
        self.rehosting = value;
    }
    fn set_error(&mut self, value: Option<String>) {
        self.presentation_error = value;
    }
    fn set_geometry_error(&mut self, value: Option<String>) {
        self.geometry_error = value;
    }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_dock_focus(window, cx);
    }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_popout_focus(window, cx);
    }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
    }
    fn shutdown(&mut self, _: &mut Context<Self>) {
        super::clipboard_history::ClipboardHistory::shutdown(self);
    }
    fn pinned(_: &RootWindow) -> bool {
        false
    }
}
impl HostedUtility for super::clipboard_modify_view::ClipboardModifyView {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::ClipboardModify;
    const PANEL: Panel = Panel::ClipboardModifyDialog;
    const KEY: &'static str = "clipboard-modify";
    const TITLE: &'static str = "Clipboard Modify";
    const INITIAL: (f32, f32) = (900.0, 640.0);
    const MINIMUM: (f32, f32) = (520.0, 360.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> { root.clipboard_modify_view.clone() }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> { root.clipboard_modify_popup.as_ref() }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> { &mut root.clipboard_modify_popup }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> { &mut root.clipboard_modify_retiring }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) { root.drain_host_geometry::<Self>(cx, detach); }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) { root.reattach_host_geometry::<Self>(cx); }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        if let Some(view) = root.clipboard_modify_view.clone() { view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); }); }
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) { root.remember_host_presentation(Self::KEY, mode, cx); }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) { if let Some(view) = root.clipboard_modify_view.clone() { view.update(cx, |view, _| view.shutdown_host()); } }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn activated(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.wake_pending_read(window, cx); }
    fn root(&self) -> WeakEntity<RootWindow> { self.root.clone() }
    fn open(&self) -> bool { self.open }
    fn rehosting(&self) -> bool { self.rehosting }
    fn close_blocked(&self, _: &App) -> bool { self.blocks_close() }
    fn popout(&self) -> bool { self.popout }
    fn fenced(&self) -> bool { self.admission.is_fenced() }
    fn receipt(&self) -> utility_mount::MountReceipt { self.mount_receipt() }
    fn mounted(&self, id: WindowId, popout: bool) -> bool { self.is_mounted_in(id, popout) }
    fn replace(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) { self.replace_mount(host, native, window, cx); }
    fn set_popout(&mut self, value: bool) { self.popout = value; }
    fn set_rehosting(&mut self, value: bool, _: &mut Context<Self>) { self.rehosting = value; }
    fn set_error(&mut self, value: Option<String>) { self.presentation_error = value; }
    fn set_geometry_error(&mut self, value: Option<String>) { self.geometry_error = value; }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.after_host(window, cx); }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.after_host(window, cx); }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.focus_host(window, cx); }
    fn shutdown(&mut self, _: &mut Context<Self>) { self.shutdown_host(); }
    fn pinned(_: &RootWindow) -> bool { false }
}

impl HostedUtility for super::file_search::FileSearchView {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::FileSearch;
    const PANEL: Panel = Panel::FileSearchDialog;
    const KEY: &'static str = "file-search";
    const TITLE: &'static str = "File Search";
    const INITIAL: (f32, f32) = (760.0, 560.0);
    const MINIMUM: (f32, f32) = (520.0, 360.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> { root.file_search.clone() }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> { root.file_search_popup.as_ref() }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> { &mut root.file_search_popup }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> { &mut root.file_search_retiring }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) { root.drain_host_geometry::<Self>(cx, detach); }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) { root.reattach_host_geometry::<Self>(cx); }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        if let Some(view) = root.file_search.clone() { view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); }); }
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) { root.remember_host_presentation(Self::KEY, mode, cx); }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) { if let Some(view) = root.file_search.clone() { view.update(cx, |view, cx| view.shutdown(cx)); } }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn root(&self) -> WeakEntity<RootWindow> { self.root.clone() }
    fn open(&self) -> bool { self.open }
    fn rehosting(&self) -> bool { self.rehosting }
    fn close_blocked(&self, cx: &App) -> bool { self.host_effect_pending(cx) }
    fn activation_lost(&mut self, _: &mut Context<Self>) { self.retire_explorer(); }
    fn activated(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.wake_pending_popup(window, cx); }
    fn outer_scroll(&self) -> Option<ScrollHandle> { Some(self.outer_scroll.clone()) }
    fn popout(&self) -> bool { self.popout }
    fn fenced(&self) -> bool { self.admission.is_fenced() }
    fn receipt(&self) -> utility_mount::MountReceipt { self.mount_receipt() }
    fn mounted(&self, id: WindowId, popout: bool) -> bool { self.is_mounted_in(id, popout) }
    fn replace(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) { self.replace_mount(host, native, window, cx); }
    fn set_popout(&mut self, value: bool) { self.popout = value; }
    fn set_rehosting(&mut self, value: bool, cx: &mut Context<Self>) { self.set_host_rehosting(value, cx); }
    fn set_error(&mut self, value: Option<String>) { self.presentation_error = value; }
    fn set_geometry_error(&mut self, value: Option<String>) { self.geometry_error = value; }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.settle_host_focus(window, cx); }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.settle_host_focus(window, cx); }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.settle_host_focus(window, cx); }
    fn shutdown(&mut self, cx: &mut Context<Self>) { self.shutdown(cx); }
    fn pinned(_: &RootWindow) -> bool { false }
}

impl HostedUtility for super::folder_size::FolderSizeView {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::FolderSize;
    const PANEL: Panel = Panel::FolderSizeDialog;
    const KEY: &'static str = "folder-size";
    const TITLE: &'static str = "Folder Size";
    const INITIAL: (f32, f32) = (900.0, 620.0);
    const MINIMUM: (f32, f32) = (560.0, 360.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> { root.folder_size_view.clone() }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> { root.folder_size_popup.as_ref() }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> { &mut root.folder_size_popup }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> { &mut root.folder_size_retiring }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) { root.drain_host_geometry::<Self>(cx, detach); }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) { root.reattach_host_geometry::<Self>(cx); }
    fn size_error(root: &mut RootWindow, value: Option<String>, cx: &mut Context<RootWindow>) { if let Some(view) = root.folder_size_view.clone() { view.update(cx, |view, cx| { view.geometry_error = value; cx.notify(); }); } }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) { root.remember_host_presentation(Self::KEY, mode, cx); }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) { if let Some(view) = root.folder_size_view.clone() { view.update(cx, |view, _| view.shutdown()); } }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn root(&self) -> WeakEntity<RootWindow> { self.root.clone() }
    fn open(&self) -> bool { self.open }
    fn rehosting(&self) -> bool { self.rehosting }
    fn close_blocked(&self, _: &App) -> bool { self.host_close_blocked() }
    fn popout(&self) -> bool { self.popout }
    fn fenced(&self) -> bool { self.admission.is_fenced() }
    fn receipt(&self) -> utility_mount::MountReceipt { self.mount.clone() }
    fn mounted(&self, id: WindowId, popout: bool) -> bool { self.is_mounted_in(id, popout) }
    fn replace(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, _: &mut Window, cx: &mut Context<Self>) { self.replace_mount(host, native, cx); }
    fn set_popout(&mut self, value: bool) { self.popout = value; }
    fn set_rehosting(&mut self, value: bool, _: &mut Context<Self>) { self.rehosting = value; }
    fn set_error(&mut self, value: Option<String>) { self.presentation_error = value; }
    fn set_geometry_error(&mut self, value: Option<String>) { self.geometry_error = value; }
    fn after_dock(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() && self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
            self.focus_host(window, cx);
        }
    }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.focus_host(window, cx); }
    fn shutdown(&mut self, _: &mut Context<Self>) { self.shutdown(); }
    fn pinned(_: &RootWindow) -> bool { false }
}

struct UtilityShell<T: HostedUtility> {
    workspace: Entity<T>,
    _activation: Subscription,
}
impl<T: HostedUtility> Render for UtilityShell<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(SharedString::from(format!("{}-popout-scroll", T::KEY)))
            .size_full()
            .overflow_y_scroll()
            .when_some(self.workspace.read(cx).outer_scroll(), |div, scroll| div.track_scroll(&scroll))
            .when(
                self.workspace
                    .read(cx)
                    .mounted(window.window_handle().window_id(), true),
                |div| div.child(self.workspace.clone()),
            )
    }
}

fn retiring_utilities(
    root: &mut RootWindow,
    kind: utility_mount::UtilityKind,
) -> &mut Vec<RetiringHashWindow> {
    match kind {
        utility_mount::UtilityKind::FileSearch => &mut root.file_search_retiring,
        utility_mount::UtilityKind::FolderSize => &mut root.folder_size_retiring,
        utility_mount::UtilityKind::Xml => &mut root.xml_retiring,
        utility_mount::UtilityKind::Json => &mut root.json_retiring,
        utility_mount::UtilityKind::Regex => &mut root.regex_retiring,
        utility_mount::UtilityKind::ClipboardHistory => &mut root.clipboard_retiring,
        utility_mount::UtilityKind::ClipboardModify => &mut root.clipboard_modify_retiring,
    }
}

pub(super) async fn retire_failed_utility_candidate(
    root: &WeakEntity<RootWindow>,
    window: AnyWindowHandle,
    native: Option<NativeWindow>,
    kind: utility_mount::UtilityKind,
    cx: &mut AsyncApp,
) {
    let retiring = RetiringHashWindow {
        window,
        native_window: native,
    };
    let signal = Arc::new(HashWakeSignal::default());
    let wait_signal = signal.clone();
    let _ = root.update(cx, |root, cx| {
        retiring_utilities(root, kind).push(retiring);
        if native.is_none() {
            let id = window.window_id();
            let owner = cx.entity().downgrade();
            let closed_signal = wait_signal.clone();
            let subscription = cx.on_window_closed(move |app, closed| {
                if closed == id {
                    let _ = owner.update(app, |root, _| {
                        retiring_utilities(root, kind).retain(|item| item.window.window_id() != id)
                    });
                    closed_signal.notify();
                }
            });
            cx.spawn(async move |_, _| {
                let _subscription = subscription;
                wait_signal.wait().await;
            })
            .detach();
        }
    });
    if native.is_some() && remove_and_wait_for_hash_window(cx, retiring).await {
        let _ = root.update(cx, |root, _| {
            retiring_utilities(root, kind)
                .retain(|item| item.window.window_id() != window.window_id())
        });
    } else if native.is_none() {
        if cx
            .update(|app| window.update(app, |_, window, _| window.remove_window()))
            .is_err()
        {
            let _ = root.update(cx, |root, _| {
                retiring_utilities(root, kind)
                    .retain(|item| item.window.window_id() != window.window_id())
            });
            signal.notify();
        }
    }
}

impl RootWindow {
    fn reattach_clipboard_geometry(&mut self, cx: &mut Context<Self>) { self.reattach_host_geometry::<super::clipboard_history::ClipboardHistory>(cx); }
    fn reattach_json_geometry(&mut self, cx: &mut Context<Self>) {
        self.reattach_host_geometry::<JsonWorkspace>(cx);
    }
    fn reattach_regex_geometry(&mut self, cx: &mut Context<Self>) {
        self.reattach_host_geometry::<super::regex_tester::RegexWorkspace>(cx);
    }
    fn reattach_host_geometry<T: HostedUtility>(&mut self, cx: &mut Context<Self>) {
        let Some(view) = T::workspace(self) else {
            return;
        };
        let Some(popup) = T::popup(self) else {
            return;
        };
        let Some(native) = popup.native else {
            return;
        };
        let workspace = view.read(cx);
        let receipt = workspace.receipt();
        if !shell_geometry_mount_is_current(
            workspace.mounted(popup.window.window_id(), true),
            &receipt,
        ) {
            return;
        }
        let Ok(key) = UtilityToolKey::parse(T::KEY) else {
            return;
        };
        match ShellGeometry::attach(
            native,
            key,
            receipt,
            self.hash_geometry_sequence.clone(),
            self.hash_wake_signal.clone(),
        ) {
            Ok(binding) => {
                if let Some(popup) = T::popup_mut(self).as_mut() {
                    popup.geometry = Some(binding);
                }
            }
            Err(error) => T::size_error(
                self,
                Some(format!(
                    "Could not observe {} window geometry: {error}",
                    T::TITLE
                )),
                cx,
            ),
        }
    }
    pub(super) fn drain_clipboard_native_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::clipboard_history::ClipboardHistory>(cx, detach); }
    pub(super) fn drain_json_native_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        self.drain_host_geometry::<JsonWorkspace>(cx, detach);
    }
    pub(super) fn drain_regex_native_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        self.drain_host_geometry::<super::regex_tester::RegexWorkspace>(cx, detach);
    }
    fn drain_host_geometry<T: HostedUtility>(&mut self, cx: &mut Context<Self>, detach: bool) {
        let Some(popup) = T::popup_mut(self).as_mut() else {
            return;
        };
        let receipt = popup
            .geometry
            .as_ref()
            .map(|geometry| geometry.receipt.clone());
        let sample = popup.geometry.as_ref().and_then(ShellGeometry::take);
        if detach {
            drop(popup.geometry.take());
        }
        let Some((sample, factory)) = sample else {
            return;
        };
        let Ok(key) = UtilityToolKey::parse(T::KEY) else {
            return;
        };
        let mut store = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let factory = store
            .effective_native_geometry(key)
            .is_none()
            .then_some(factory)
            .flatten();
        let result = store.observe_native_geometry(key, sample, factory);
        drop(store);
        match result {
            Ok(Some(_)) => self.start_utility_size_ack_poll(cx),
            Err(error) => {
                let message = format!("Could not save {} window geometry: {error}", T::TITLE);
                tracing::warn!(%message);
                if let (Some(view), Some(receipt)) = (T::workspace(self), receipt) {
                    let view = view.downgrade();
                    cx.spawn(async move |_, cx| {
                        let _ = view.update(cx, |view, cx| {
                            if receipt.is_current() && view.open() {
                                view.set_geometry_error(Some(message));
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                }
            }
            _ => {}
        }
    }
    pub(super) fn set_clipboard_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) { if let Some(view) = self.clipboard_history.clone() { view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); }); } }
    pub(super) fn set_clipboard_modify_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(view) = self.clipboard_modify_view.clone() {
            view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); });
        }
    }
    pub(super) fn set_file_search_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); }); }
    }
    pub(super) fn set_regex_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(view) = self.regex_view.clone() {
            view.update(cx, |view, cx| {
                view.geometry_error = message;
                cx.notify();
            });
        }
    }
    pub(super) fn set_json_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(view) = self.json_view.clone() {
            view.update(cx, |view, cx| {
                view.geometry_error = message;
                cx.notify();
            });
        }
    }
    pub(super) fn report_presentation_failures(
        &mut self,
        failures: Vec<(UtilityToolKey, String)>,
        cx: &mut Context<Self>,
    ) {
        for (key, error) in failures {
            if key == self.hash_tool_key {
                self.hash_presentation_error = Some(error.clone());
                *self
                    .hash_presentation_feedback
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(error);
                self.notify_hash_mount(cx);
            } else if UtilityToolKey::parse("json-utility").ok() == Some(key) {
                if let Some(view) = self.json_view.clone() {
                    view.update(cx, |view, cx| {
                        view.presentation_error = Some(error);
                        cx.notify();
                    });
                }
            } else if UtilityToolKey::parse("clipboard-history").ok() == Some(key) {
                if let Some(view) = self.clipboard_history.clone() { view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); }); }
            } else if UtilityToolKey::parse("clipboard-modify").ok() == Some(key) {
                if let Some(view) = self.clipboard_modify_view.clone() {
                    view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); });
                }
            } else if UtilityToolKey::parse("file-preview").ok() == Some(key) {
                if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| view.report_preview_presentation_error(error, cx)); }
            } else if UtilityToolKey::parse("folder-size").ok() == Some(key) {
                if let Some(view) = self.folder_size_view.clone() { view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); }); }
            } else if UtilityToolKey::parse("file-search").ok() == Some(key) {
                if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); }); }
            } else if UtilityToolKey::parse("regex-tester").ok() == Some(key) {
                if let Some(view) = self.regex_view.clone() {
                    view.update(cx, |view, cx| {
                        view.presentation_error = Some(error);
                        cx.notify();
                    });
                }
            } else if UtilityToolKey::parse("xml-toolkit").ok() == Some(key) {
                if let Some(view) = self.xml_view.clone() {
                    view.update(cx, |view, cx| {
                        view.feedback = Some(error);
                        cx.notify();
                    });
                }
            }
        }
    }
    fn remember_json_presentation(
        &mut self,
        presentation: RememberedPresentation,
        cx: &mut Context<Self>,
    ) {
        self.remember_host_presentation("json-utility", presentation, cx);
    }
    pub(super) fn remember_host_presentation(
        &mut self,
        tool_key: &str,
        presentation: RememberedPresentation,
        cx: &mut Context<Self>,
    ) {
        let Ok(key) = UtilityToolKey::parse(tool_key) else {
            return;
        };
        let store = self.utility_presentations.clone();
        let update = store.remember(key, presentation);
        if update == UtilityPresentationUpdate::Unchanged {
            return;
        }
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                let failures = store.poll();
                if !failures.is_empty() {
                    let _ = root.update(cx, |root, cx| {
                        root.report_presentation_failures(failures, cx)
                    });
                }
                if !store.has_pending() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    }
    pub(super) fn drain_folder_size_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::folder_size::FolderSizeView>(cx, detach); }
    pub(super) fn start_folder_size_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) { self.start_hosted_rehost::<super::folder_size::FolderSizeView>(popout, gesture, source, cx); }
    pub(super) fn drain_file_search_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::file_search::FileSearchView>(cx, detach); }
    pub(super) fn start_file_search_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::file_search::FileSearchView>(popout, gesture, source, cx);
    }
    pub(super) fn start_json_rehost(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        self.start_hosted_rehost::<JsonWorkspace>(popout, gesture, source, cx);
    }
    pub(super) fn start_clipboard_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::clipboard_history::ClipboardHistory>(popout, gesture, source, cx);
    }
    pub(super) fn start_clipboard_modify_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::clipboard_modify_view::ClipboardModifyView>(popout, gesture, source, cx);
    }
    pub(super) fn drain_clipboard_modify_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        self.drain_host_geometry::<super::clipboard_modify_view::ClipboardModifyView>(cx, detach);
    }
    pub(super) fn start_regex_rehost(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        self.start_hosted_rehost::<super::regex_tester::RegexWorkspace>(
            popout, gesture, source, cx,
        );
    }
    fn start_hosted_rehost<T: HostedUtility>(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = T::workspace(self) else {
            return;
        };
        if !source.is_current()
            || !workspace.read(cx).rehosting()
            || workspace.read(cx).popout() == popout
        {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            workspace.update(cx, |view, cx| view.set_rehosting(false, cx));
            return;
        };
        let Some(root_window) = self.root_window else {
            workspace.update(cx, |view, cx| view.set_rehosting(false, cx));
            return;
        };
        let root = cx.entity().downgrade();
        let visibility = self.root_visibility.clone();
        let key = UtilityToolKey::parse(T::KEY).ok();
        let geometry = key.and_then(|key| {
            let mut store = self
                .utility_size_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            store.track_key(key);
            store.effective_native_geometry(key)
        });
        let root_native = self.native_window;
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            if popout {
                let shell_workspace = workspace.clone();
                let opened = cx.update(|app| {
                    let options = WindowOptions {
                        window_bounds: Some(WindowBounds::centered(
                            size(px(T::INITIAL.0), px(T::INITIAL.1)),
                            app,
                        )),
                        window_min_size: Some(size(px(T::MINIMUM.0), px(T::MINIMUM.1))),
                        titlebar: Some(TitlebarOptions {
                            title: Some(T::TITLE.into()),
                            ..Default::default()
                        }),
                        show: false,
                        focus: false,
                        ..Default::default()
                    };
                    gpui_kit::open_window(options, app, move |window, app| {
                        let weak = shell_workspace.downgrade();
                        let local_id = window.window_handle().window_id();
                        window.on_window_should_close(app, move |_, app| {
                            weak.update(app, |view, cx| {
                                let tracked_source = view.root().upgrade().and_then(|root| {
                                    T::popup(root.read(cx)).map(|popup| popup.window.window_id())
                                });
                                let source_in_transfer = utility_mount::transfer_source_is_owned(
                                    view.rehosting(),
                                    tracked_source,
                                    local_id,
                                );
                                if source_in_transfer {
                                    return false;
                                }
                                if view
                                    .root()
                                    .upgrade()
                                    .is_some_and(|root| T::pinned(root.read(cx)))
                                    && view.mounted(local_id, true)
                                {
                                    return false;
                                }
                                if !view.mounted(local_id, true) {
                                    return true;
                                }
                                if view.rehosting() || view.close_blocked(cx) {
                                    return false;
                                }
                                let _ = view.root().update(cx, |root, cx| T::drain(root, cx, true));
                                view.shutdown(cx);
                                true
                            })
                            .unwrap_or(true)
                        });
                        app.new(|cx| {
                            let activation = cx.observe_window_activation(
                                window,
                                |shell: &mut UtilityShell<T>, window, cx| {
                                    if !window.is_window_active() {
                                        if shell.workspace.read(cx).mounted(window.window_handle().window_id(), true) {
                                            shell.workspace.update(cx, |view, cx| view.activation_lost(cx));
                                        }
                                    } else {
                                        if shell.workspace.read(cx).mounted(window.window_handle().window_id(), true) {
                                            let weak = shell.workspace.downgrade();
                                            let receipt = shell.workspace.read(cx).receipt();
                                            window.defer(cx, move |window, app| {
                                                let _ = weak.update(app, |view, cx| {
                                                    if receipt.is_current() && view.mounted(window.window_handle().window_id(), true) {
                                                        view.activated(window, cx);
                                                    }
                                                });
                                            });
                                        }
                                        let root = shell.workspace.read(cx).root();
                                        let id = window.window_handle().window_id();
                                        let _ =
                                            root.update(cx, |root, cx| T::observe(root, id, cx));
                                    }
                                },
                            );
                            UtilityShell {
                                workspace: shell_workspace,
                                _activation: activation,
                            }
                        })
                    })
                });
                let (window, _shell) = match opened {
                    Ok(opened) => opened,
                    Err(error) => {
                        let _ = workspace.update(cx, |view, cx| {
                            view.set_rehosting(false, cx);
                            view.set_error(Some(format!(
                                "Could not open {} window: {error}",
                                T::TITLE
                            )));
                            cx.notify();
                        });
                        return;
                    }
                };
                let mounted =
                    update_kit_window_content(window, &workspace, cx, |view, window_ref, cx| {
                        if !source.is_current()
                            || !view.open()
                            || !view.rehosting()
                            || view.fenced()
                        {
                            return Err("utility mount changed".to_owned());
                        }
                        let native = checked_native_window(window_ref)?;
                        if cfg!(windows) && native.is_none() {
                            return Err("utility window has no checked native handle".to_owned());
                        }
                        view.replace(window, native, window_ref, cx);
                        view.set_popout(true);
                        Ok(native)
                    });
                let native = match mounted {
                    Ok(Ok(native)) => native,
                    _ => {
                        let candidate_native = cx
                            .update(|app| {
                                window.update(app, |_, window, _| {
                                    checked_native_window(window).ok().flatten()
                                })
                            })
                            .ok()
                            .flatten();
                        retire_failed_utility_candidate(
                            &root,
                            window,
                            candidate_native,
                            T::KIND,
                            cx,
                        )
                        .await;
                        let _ = workspace.update(cx, |view, cx| {
                            view.set_rehosting(false, cx);
                            view.set_error(Some(format!("Could not attach {} window.", T::TITLE)));
                            cx.notify();
                        });
                        return;
                    }
                };
                let mounted_receipt = cx.update(|app| workspace.read(app).receipt());
                if let (Some(native), Some(geometry)) = (native, geometry) {
                    if mounted_receipt.is_current() && native_window_is_valid(native) {
                        if let Some(target) = checked_geometry_target(native) {
                            if let Err(error) = restore_native_window_geometry(
                                target,
                                Some(geometry),
                                (T::MINIMUM.0 as i32, T::MINIMUM.1 as i32),
                                1.0,
                            ) {
                                tracing::warn!(%error, utility = T::TITLE, "could not restore utility window geometry");
                            }
                        }
                    }
                }
                let installed = root
                    .update(cx, |root, cx| {
                        if T::popup(root).is_some()
                            || !T::workspace(root)
                                .as_ref()
                                .is_some_and(|view| view.entity_id() == workspace.entity_id())
                            || !mounted_receipt.is_current()
                            || root.restart_admission.is_fenced()
                        {
                            return false;
                        }
                        let root_weak = cx.entity().downgrade();
                        let receipt = mounted_receipt.clone();
                        let id = window.window_id();
                        let subscription = cx.on_window_closed(move |app, closed| {
                            if closed != id {
                                return;
                            }
                            let _ = root_weak.update(app, |root, cx| {
                                if T::popup(root)
                                    .is_some_and(|popup| popup.window.window_id() == id)
                                    && (receipt.is_current()
                                        || T::workspace(root)
                                            .as_ref()
                                            .is_some_and(|view| !view.read(cx).open()))
                                {
                                    T::drain(root, cx, true);
                                    *T::popup_mut(root) = None;
                                    let retiring = RetiringHashWindow {
                                        window,
                                        native_window: native,
                                    };
                                    T::retiring(root).push(retiring);
                                    let retirement_owner = cx.entity().downgrade();
                                    cx.spawn(async move |_, cx| {
                                        if wait_for_hash_window_retirement(cx, native).await {
                                            let _ = retirement_owner.update(cx, |root, _| {
                                                T::retiring(root)
                                                    .retain(|item| item.window.window_id() != id)
                                            });
                                        }
                                    })
                                    .detach();
                                    T::retire(root, cx);
                                    panel_stack::retire(&mut root.panel_stack, T::PANEL);
                                    cx.notify();
                                }
                            });
                        });
                        *T::popup_mut(root) = Some(UtilityPopup {
                            window,
                            native,
                            _subscription: subscription,
                            geometry: None,
                        });
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                let mut displayed = false;
                if installed {
                    if let Some(native) = native {
                        if mounted_receipt.is_current() && native_window_is_valid(native) {
                            displayed = match set_native_window_visible(native, true) {
                                Ok(()) => true,
                                Err(error) => {
                                    tracing::warn!(%error, utility = T::TITLE, "could not show utility window");
                                    false
                                }
                            };
                            let authorized = gesture.is_some_and(|gesture| {
                                gesture.foreground_authorized
                                    && super::json_utility::native_host_current(
                                        &visibility,
                                        gesture.visibility_revision,
                                        Some(gesture.source),
                                        false,
                                    )
                                    && current_foreground_identity()
                                        == Some(gesture.source.identity)
                            });
                            if displayed && authorized && mounted_receipt.is_current() {
                                let _ = activate_native_window(native);
                            }
                            if displayed && mounted_receipt.is_current() {
                                let _ = update_kit_window_content(
                                    window,
                                    &workspace,
                                    cx,
                                    |view, window, cx| {
                                        if mounted_receipt.is_current()
                                            && view
                                                .mounted(window.window_handle().window_id(), true)
                                        {
                                            view.focus(window, cx);
                                        }
                                    },
                                );
                            }
                        }
                    }
                }
                if installed && displayed && mounted_receipt.is_current() {
                    let _ = root.update(cx, |root, cx| {
                        if let (Some(native), Some(key)) = (native, key) {
                            match ShellGeometry::attach(
                                native,
                                key,
                                mounted_receipt.clone(),
                                root.hash_geometry_sequence.clone(),
                                root.hash_wake_signal.clone(),
                            ) {
                                Ok(binding) => {
                                    if let Some(popup) = T::popup_mut(root).as_mut() {
                                        popup.geometry = Some(binding);
                                    }
                                }
                                Err(error) => T::size_error(
                                    root,
                                    Some(format!(
                                        "Could not observe {} window geometry: {error}",
                                        T::TITLE
                                    )),
                                    cx,
                                ),
                            }
                        }
                        T::remember(root, RememberedPresentation::Popout, cx);
                        workspace.update(cx, |view, cx| {
                            view.set_rehosting(false, cx);
                            cx.notify();
                        });
                    });
                    let _ =
                        update_kit_window_content(window, &workspace, cx, |view, window, cx| {
                            if mounted_receipt.is_current()
                                && view.mounted(window.window_handle().window_id(), true)
                            {
                                view.after_popout(window, cx);
                            }
                        });
                } else {
                    let _ = root.update(cx, |root, _| {
                        if T::popup(root)
                            .is_some_and(|popup| popup.window.window_id() == window.window_id())
                        {
                            *T::popup_mut(root) = None;
                        }
                    });
                    let _ = update_kit_window_content(
                        root_window,
                        &workspace,
                        cx,
                        |view, window, cx| {
                            if mounted_receipt.is_current() {
                                view.replace(root_window, root_native, window, cx);
                                view.set_popout(false);
                                view.set_rehosting(false, cx);
                                view.set_error(Some(format!(
                                    "Could not show {} window.",
                                    T::TITLE
                                )));
                                if gesture.is_some_and(|gesture| gesture.foreground_authorized)
                                    && root_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    })
                                {
                                    view.focus(window, cx);
                                }
                                cx.notify();
                            }
                        },
                    );
                    retire_failed_utility_candidate(&root, window, native, T::KIND, cx).await;
                }
            } else {
                let source_popup = root
                    .update(cx, |root, _| {
                        T::popup(root).map(|popup| (popup.window, popup.native))
                    })
                    .ok()
                    .flatten();
                let Some((popup, popup_native)) = source_popup else {
                    let _ = workspace.update(cx, |view, cx| view.set_rehosting(false, cx));
                    return;
                };
                let _ = root.update(cx, |root, cx| T::drain(root, cx, true));
                let mounted =
                    update_kit_window_content(root_window, &workspace, cx, |view, window, cx| {
                        if !source.is_current()
                            || !view.open()
                            || !view.rehosting()
                            || view.fenced()
                        {
                            return None;
                        }
                        view.replace(root_window, root_native, window, cx);
                        view.set_popout(false);
                        Some(view.receipt())
                    })
                    .ok()
                    .flatten();
                let Some(mount) = mounted else {
                    let _ = workspace.update(cx, |view, cx| view.set_rehosting(false, cx));
                    let _ = root.update(cx, |root, cx| {
                        if T::popup(root)
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                            && source.is_current()
                        {
                            T::reattach(root, cx);
                        }
                    });
                    return;
                };
                let committed = root
                    .update(cx, |root, cx| {
                        if !T::popup(root)
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                            || !mount.is_current()
                        {
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !committed {
                    let _ = update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                        if mount.is_current() {
                            view.replace(popup, popup_native, window, cx);
                            view.set_popout(true);
                            view.set_rehosting(false, cx);
                        }
                    });
                    let _ = root.update(cx, |root, cx| {
                        if T::popup(root)
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                        {
                            T::reattach(root, cx);
                        }
                    });
                    return;
                }
                {
                    let ack = gesture.and_then(|gesture| {
                        visibility.request_utility_visible(T::KIND, gesture, mount.clone())
                    });
                    let Some((ack, accepted)) = ack else {
                        let _ =
                            update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                                if mount.is_current() {
                                    view.replace(popup, popup_native, window, cx);
                                    view.set_popout(true);
                                    view.set_rehosting(false, cx);
                                    view.set_error(Some(format!(
                                        "Could not show the launcher to dock {}.",
                                        T::TITLE
                                    )));
                                    if popup_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    }) {
                                        view.focus(window, cx);
                                    }
                                    cx.notify();
                                }
                            });
                        let _ = root.update(cx, |root, cx| T::reattach(root, cx));
                        return;
                    };
                    ack.wait().await;
                    let revision = accepted.load(Ordering::Acquire);
                    let accepted = revision != 0
                        && mount.is_current()
                        && root_native.is_some_and(|native| {
                            super::json_utility::native_host_current(
                                &visibility,
                                revision,
                                Some(native),
                                false,
                            ) && current_foreground_identity() == Some(native.identity)
                        });
                    if !accepted {
                        let _ =
                            update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                                if mount.is_current() {
                                    view.replace(popup, popup_native, window, cx);
                                    view.set_popout(true);
                                    view.set_rehosting(false, cx);
                                    view.set_error(Some(format!(
                                        "Could not focus the launcher to dock {}.",
                                        T::TITLE
                                    )));
                                    if popup_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    }) {
                                        view.focus(window, cx);
                                    }
                                    cx.notify();
                                }
                            });
                        let _ = root.update(cx, |root, cx| T::reattach(root, cx));
                        return;
                    }
                }
                let _ = root.update(cx, |root, cx| {
                    if T::popup(root).is_some_and(|old| old.window.window_id() == popup.window_id())
                    {
                        *T::popup_mut(root) = None;
                        T::retiring(root).push(RetiringHashWindow {
                            window: popup,
                            native_window: popup_native,
                        });
                        T::remember(root, RememberedPresentation::Docked, cx);
                        cx.notify();
                    }
                });
                let retired = remove_and_wait_for_hash_window(
                    cx,
                    RetiringHashWindow {
                        window: popup,
                        native_window: popup_native,
                    },
                )
                .await;
                if retired {
                    let _ = root.update(cx, |root, _| {
                        T::retiring(root)
                            .retain(|item| item.window.window_id() != popup.window_id())
                    });
                }
                let _ =
                    update_kit_window_content(root_window, &workspace, cx, |view, window, cx| {
                        view.set_rehosting(false, cx);
                        if mount.is_current() {
                            view.after_dock(window, cx);
                        }
                        cx.notify();
                    });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        ForegroundIdentity, HashWakeSignal, NativeWindow, RootVisibilityRequest, hash_dock,
        utility_mount,
    };
    use super::shell_geometry_mount_is_current;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    #[test]
    fn geometry_reattachment_requires_current_popup_mount() {
        let owner = utility_mount::MountOwner::default();
        let receipt = owner.attach();
        assert!(shell_geometry_mount_is_current(true, &receipt));
        assert!(!shell_geometry_mount_is_current(false, &receipt));
        owner.attach();
        assert!(!shell_geometry_mount_is_current(true, &receipt));
    }
    #[test]
    fn json_dock_receipt_requires_current_mount_and_visibility_revision() {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let gesture = hash_dock::Gesture {
            source: NativeWindow {
                identity: ForegroundIdentity {
                    hwnd: 1,
                    process_id: 2,
                    thread_id: 3,
                },
            },
            foreground_authorized: true,
            visibility_revision: 5,
        };
        let signal = Arc::new(HashWakeSignal::default());
        let request = RootVisibilityRequest::DockUtility(
            utility_mount::UtilityKind::Json,
            hash_dock::UtilityReveal {
                gesture,
                mount: mount.clone(),
                acknowledgement: hash_dock::UtilityAcknowledgement {
                    signal,
                    accepted_revision: Arc::new(AtomicU64::new(0)),
                },
            },
        );
        assert!(request.is_current(5, false, false));
        assert!(!request.is_current(6, false, false));
        assert!(!request.is_current(5, false, true));
        owner.retire();
        assert!(!request.is_current(5, false, false));
    }
    #[test]
    fn json_dock_acknowledgement_records_only_explicit_acceptance() {
        let signal = Arc::new(HashWakeSignal::default());
        let revision = Arc::new(AtomicU64::new(0));
        let denied = hash_dock::UtilityAcknowledgement {
            signal: signal.clone(),
            accepted_revision: revision.clone(),
        };
        drop(denied);
        assert_eq!(revision.load(Ordering::Acquire), 0);
        let accepted = hash_dock::UtilityAcknowledgement {
            signal: Arc::new(HashWakeSignal::default()),
            accepted_revision: revision.clone(),
        };
        accepted.accept(7);
        drop(accepted);
        assert_eq!(revision.load(Ordering::Acquire), 7);
    }
}
