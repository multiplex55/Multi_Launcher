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

fn inherited_popup_target_allowed(mount: bool, installed: bool, presented: bool, modal_clear: bool) -> bool {
    mount && installed && presented && modal_clear
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
    fn transfer_source_current(&self, _: &RootWindow, _: &Window, _: &App, _: bool) -> bool { true }
    fn transfer_commit_current(&self, _: &RootWindow, _: &App) -> bool { true }
    fn transfer_requires_activation(&self) -> bool { false }
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

impl HostedUtility for super::snippets::SnippetsView {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::Snippets;
    const PANEL: Panel = Panel::SnippetDialog;
    const KEY: &'static str = "snippets";
    const TITLE: &'static str = "Snippets";
    const INITIAL: (f32, f32) = (600.0, 500.0);
    const MINIMUM: (f32, f32) = (160.0, 160.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> { root.snippets_view.clone() }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> { root.snippets_popup.as_ref() }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> { &mut root.snippets_popup }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> { &mut root.snippets_retiring }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) { root.drain_host_geometry::<Self>(cx, detach); }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) { root.reattach_host_geometry::<Self>(cx); }
    fn size_error(root: &mut RootWindow, message: Option<String>, cx: &mut Context<RootWindow>) {
        if let Some(view) = root.snippets_view.clone() { view.update(cx, |view, cx| { view.geometry_error = message; cx.notify(); }); }
    }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) { root.remember_host_presentation(Self::KEY, mode, cx); }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) { if let Some(view) = root.snippets_view.clone() { view.update(cx, |view, cx| view.shutdown_host(cx)); } }
    fn observe(_: &mut RootWindow, _: WindowId, _: &mut Context<RootWindow>) {}
    fn activated(&mut self, window: &mut Window, cx: &mut Context<Self>) { if self.has_pending_named() { self.after_host(window, cx); } }
    fn root(&self) -> WeakEntity<RootWindow> { self.root.clone() }
    fn open(&self) -> bool { self.open }
    fn rehosting(&self) -> bool { self.rehosting }
    fn transfer_source_current(&self, root: &RootWindow, window: &Window, cx: &App, modal: bool) -> bool { self.named_source_current(root, window, cx, modal) }
    fn transfer_commit_current(&self, root: &RootWindow, cx: &App) -> bool { self.named_intent_current(root, cx) }
    fn transfer_requires_activation(&self) -> bool { self.has_pending_named() || self.has_cold_origin() }
    fn close_blocked(&self, _: &App) -> bool { self.busy() }
    fn outer_scroll(&self) -> Option<ScrollHandle> { Some(self.outer_scroll.clone()) }
    fn popout(&self) -> bool { self.popout }
    fn fenced(&self) -> bool { self.admission.is_fenced() }
    fn receipt(&self) -> utility_mount::MountReceipt { self.mount_receipt() }
    fn mounted(&self, id: WindowId, popout: bool) -> bool { self.is_mounted_in(id, popout) }
    fn replace(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) { self.replace_mount(host, native, window, cx); }
    fn set_popout(&mut self, value: bool) { self.popout = value; }
    fn set_rehosting(&mut self, value: bool, cx: &mut Context<Self>) { self.set_rehosting(value, cx); }
    fn set_error(&mut self, value: Option<String>) { self.presentation_error = value; }
    fn set_geometry_error(&mut self, value: Option<String>) { self.geometry_error = value; }
    fn after_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.after_host(window, cx); }
    fn after_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.after_host(window, cx); }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.focus_host(window, cx); }
    fn shutdown(&mut self, cx: &mut Context<Self>) { self.shutdown_host(cx); }
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
    fn replace(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) { self.replace_mount(host, native, window, cx); }
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

impl HostedUtility for super::file_lock::FileLockView {
    const KIND: utility_mount::UtilityKind = utility_mount::UtilityKind::FileLock;
    const PANEL: Panel = Panel::FileLockDialog;
    const KEY: &'static str = "file-lock";
    const TITLE: &'static str = "File Lock Inspector";
    const INITIAL: (f32, f32) = (760.0, 520.0);
    const MINIMUM: (f32, f32) = (520.0, 340.0);
    fn workspace(root: &RootWindow) -> Option<Entity<Self>> { root.file_lock_view.clone() }
    fn popup(root: &RootWindow) -> Option<&UtilityPopup> { root.file_lock_popup.as_ref() }
    fn popup_mut(root: &mut RootWindow) -> &mut Option<UtilityPopup> { &mut root.file_lock_popup }
    fn retiring(root: &mut RootWindow) -> &mut Vec<RetiringHashWindow> { &mut root.file_lock_retiring }
    fn drain(root: &mut RootWindow, cx: &mut Context<RootWindow>, detach: bool) { root.drain_host_geometry::<Self>(cx, detach); }
    fn reattach(root: &mut RootWindow, cx: &mut Context<RootWindow>) { root.reattach_host_geometry::<Self>(cx); }
    fn size_error(root: &mut RootWindow, value: Option<String>, cx: &mut Context<RootWindow>) { if let Some(view) = root.file_lock_view.clone() { view.update(cx, |view, cx| { view.geometry_error = value; cx.notify(); }); } }
    fn remember(root: &mut RootWindow, mode: RememberedPresentation, cx: &mut Context<RootWindow>) { root.remember_host_presentation(Self::KEY, mode, cx); }
    fn retire(root: &mut RootWindow, cx: &mut Context<RootWindow>) { if let Some(view) = root.file_lock_view.clone() { view.update(cx, |view, _| view.shutdown()); } }
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
        if window.is_window_active() && self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)) { self.focus_host(window, cx); }
    }
    fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.focus_host(window, cx); }
    fn shutdown(&mut self, _: &mut Context<Self>) { self.shutdown(); }
    fn pinned(_: &RootWindow) -> bool { false }
}

struct UtilityShell<T: HostedUtility> {
    workspace: Entity<T>,
    _activation: Subscription,
}

fn hosted_source_current<T: HostedUtility>(root: &WeakEntity<RootWindow>, workspace: &Entity<T>, host: AnyWindowHandle, cx: &mut AsyncApp) -> bool {
    cx.update(|app| host.update(app, |_, window, app| {
        let modal = window.has_active_dialog(app);
        root.upgrade().is_some_and(|root| workspace.read(app).transfer_source_current(root.read(app), window, app, modal))
    })).ok().unwrap_or(false)
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
        utility_mount::UtilityKind::FileLock => &mut root.file_lock_retiring,
        utility_mount::UtilityKind::Xml => &mut root.xml_retiring,
        utility_mount::UtilityKind::Json => &mut root.json_retiring,
        utility_mount::UtilityKind::Regex => &mut root.regex_retiring,
        utility_mount::UtilityKind::ClipboardHistory => &mut root.clipboard_retiring,
        utility_mount::UtilityKind::ClipboardModify => &mut root.clipboard_modify_retiring,
        utility_mount::UtilityKind::Snippets => &mut root.snippets_retiring,
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
            } else if UtilityToolKey::parse("snippets").ok() == Some(key) {
                if let Some(view) = self.snippets_view.clone() {
                    view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); });
                }
            } else if UtilityToolKey::parse("file-preview").ok() == Some(key) {
                if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| view.report_preview_presentation_error(error, cx)); }
            } else if UtilityToolKey::parse("file-lock").ok() == Some(key) {
                if let Some(view) = self.file_lock_view.clone() { view.update(cx, |view, cx| { view.presentation_error = Some(error); cx.notify(); }); }
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
    pub(super) fn drain_file_lock_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::file_lock::FileLockView>(cx, detach); }
    pub(super) fn start_file_lock_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) { self.start_hosted_rehost::<super::file_lock::FileLockView>(popout, gesture, source, None, None, cx); }
    pub(super) fn start_file_lock_cold_rehost(&mut self, source: utility_mount::MountReceipt, origin: super::file_lock::ColdOrigin, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::file_lock::FileLockView>(true, None, source, None, Some(origin), cx);
    }
    pub(super) fn drain_folder_size_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::folder_size::FolderSizeView>(cx, detach); }
    pub(super) fn start_folder_size_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) { self.start_hosted_rehost::<super::folder_size::FolderSizeView>(popout, gesture, source, None, None, cx); }
    pub(super) fn start_inherited_folder_size_rehost(&mut self, source: utility_mount::MountReceipt, origin: super::file_search_handoff::FileSearchOrigin, permit: EffectPermit, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::folder_size::FolderSizeView>(true, None, source, Some((origin, permit)), None, cx);
    }
    pub(super) fn start_inherited_file_lock_rehost(&mut self, source: utility_mount::MountReceipt, origin: super::file_search_handoff::FileSearchOrigin, permit: EffectPermit, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::file_lock::FileLockView>(true, None, source, Some((origin, permit)), None, cx);
    }
    pub(super) fn activate_inherited_folder_size_popup(&mut self, origin: super::file_search_handoff::FileSearchOrigin, permit: EffectPermit, cx: &mut Context<Self>) {
        self.activate_file_search_popup::<super::folder_size::FolderSizeView>(origin, permit, cx);
    }
    pub(super) fn activate_inherited_file_lock_popup(&mut self, origin: super::file_search_handoff::FileSearchOrigin, permit: EffectPermit, cx: &mut Context<Self>) {
        self.activate_file_search_popup::<super::file_lock::FileLockView>(origin, permit, cx);
    }
    fn file_search_popup_target_current<T: HostedUtility>(&self, workspace: &Entity<T>, mount: &utility_mount::MountReceipt,
        host: AnyWindowHandle, native: NativeWindow, cx: &mut Context<Self>) -> bool {
        let installed = !self.restart_admission.is_fenced()
            && T::workspace(self).as_ref().is_some_and(|view| view.entity_id() == workspace.entity_id())
            && T::popup(self).is_some_and(|popup| popup.window.window_id() == host.window_id() && popup.native == Some(native))
            && workspace.read(cx).rehosting() && workspace.read(cx).mounted(host.window_id(), true)
            && workspace.read(cx).receipt().matches(mount);
        if !inherited_popup_target_allowed(mount.is_current(), installed, checked_hash_popup_visible(native), true) { return false; }
        let modal_clear = host.update(cx, |_, window, app| {
            let contexts = window.context_stack();
            !window.has_active_dialog(app)
                && !context_stack_has_primary(&contexts, "PopupMenu")
                && !context_stack_has_primary(&contexts, "Dialog")
                && !context_stack_has_primary(&contexts, "AlertDialog")
                && checked_native_window(window).ok().flatten() == Some(native)
        }).unwrap_or(false);
        inherited_popup_target_allowed(mount.is_current(), installed, checked_hash_popup_visible(native), modal_clear)
    }
    fn activate_file_search_popup<T: HostedUtility>(&mut self, origin: super::file_search_handoff::FileSearchOrigin,
        permit: EffectPermit, cx: &mut Context<Self>) {
        let (Some(workspace), Some(popup)) = (T::workspace(self), T::popup(self)) else { return; };
        let Some(native) = popup.native else { return; };
        let host = popup.window; let mount = workspace.read(cx).receipt();
        workspace.update(cx, |view, cx| { view.set_rehosting(true, cx); cx.notify(); });
        let root = cx.entity().downgrade(); let worker_mount = mount.clone();
        let source_mount = origin.mount.clone(); let source_native = origin.native;
        let source_popup = origin.popout; let source_revision = origin.root_revision;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let guard = Arc::new(AtomicU64::new(1));
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let current = root.update(cx, |root, cx| root.file_search_origin_current(&origin, source_native.identity, cx)
                && root.file_search_popup_target_current::<T>(&workspace, &mount, host, native, cx)).unwrap_or(false);
            let accepted = current && activate_for_secondary_window_if(&guard, 1, native, Some(source_native.identity), &||
                worker_mount.is_current() && source_mount.is_current() && !admission.is_fenced()
                    && checked_hash_popup_visible(native) && native_window_is_valid(source_native)
                    && (source_popup || json_utility::native_host_current(&visibility, source_revision, Some(source_native), false)));
            let committed = root.update(cx, |root, cx| {
                if !accepted || current_foreground_identity() != Some(native.identity)
                    || !root.file_search_origin_current(&origin, native.identity, cx)
                    || !root.file_search_popup_target_current::<T>(&workspace, &mount, host, native, cx) { return false; }
                if !root.commit_file_search_handoff(&origin, workspace.entity_id(), host.window_id(), cx) { return false; }
                workspace.update(cx, |view, cx| { view.set_rehosting(false, cx); cx.notify(); });
                if origin.popout { T::remember(root, RememberedPresentation::Popout, cx); }
                true
            }).unwrap_or(false);
            if committed {
                let _ = update_kit_window_content(host, &workspace, cx, |view, window, cx| {
                    if mount.is_current() && view.mounted(window.window_handle().window_id(), true)
                        && !window.has_active_dialog(cx) && current_foreground_identity() == Some(native.identity) { view.focus(window, cx); }
                });
            } else { let _ = root.update(cx, |root, cx| {
                workspace.update(cx, |view, cx| { if mount.is_current() && view.receipt().matches(&mount) { view.set_rehosting(false, cx); cx.notify(); } });
                root.file_search_origin_error(&origin, "The window could not be activated.", cx);
            }); }
        }).detach();
    }
    pub(super) fn drain_file_search_geometry(&mut self, cx: &mut Context<Self>, detach: bool) { self.drain_host_geometry::<super::file_search::FileSearchView>(cx, detach); }
    pub(super) fn start_file_search_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::file_search::FileSearchView>(popout, gesture, source, None, None, cx);
    }
    pub(super) fn start_json_rehost(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        self.start_hosted_rehost::<JsonWorkspace>(popout, gesture, source, None, None, cx);
    }
    pub(super) fn start_clipboard_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::clipboard_history::ClipboardHistory>(popout, gesture, source, None, None, cx);
    }
    pub(super) fn start_clipboard_modify_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::clipboard_modify_view::ClipboardModifyView>(popout, gesture, source, None, None, cx);
    }
    pub(super) fn drain_clipboard_modify_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        self.drain_host_geometry::<super::clipboard_modify_view::ClipboardModifyView>(cx, detach);
    }
    pub(super) fn start_snippets_rehost(&mut self, popout: bool, gesture: Option<hash_dock::Gesture>, source: utility_mount::MountReceipt, cx: &mut Context<Self>) {
        self.start_hosted_rehost::<super::snippets::SnippetsView>(popout, gesture, source, None, None, cx);
    }
    pub(super) fn drain_snippets_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        self.drain_host_geometry::<super::snippets::SnippetsView>(cx, detach);
    }
    pub(super) fn start_regex_rehost(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        self.start_hosted_rehost::<super::regex_tester::RegexWorkspace>(
            popout, gesture, source, None, None, cx,
        );
    }
    fn start_hosted_rehost<T: HostedUtility>(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        inherited: Option<(super::file_search_handoff::FileSearchOrigin, EffectPermit)>,
        cold_lock: Option<super::file_lock::ColdOrigin>,
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
        if let Some(origin) = cold_lock.as_ref() && !self.file_lock_cold_origin_current(origin, cx) {
            workspace.update(cx, |view, cx| { view.set_rehosting(false, cx); cx.notify(); });
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            if let Some((origin, _)) = inherited.as_ref() {
                self.abort_file_search_handoff(origin, workspace.entity_id(), &source, &format!("{} is busy.", T::TITLE), cx);
            } else { workspace.update(cx, |view, cx| view.set_rehosting(false, cx)); }
            return;
        };
        let Some(root_window) = self.root_window else {
            if let Some((origin, _)) = inherited.as_ref() {
                self.abort_file_search_handoff(origin, workspace.entity_id(), &source, "The launcher window is unavailable.", cx);
            } else { workspace.update(cx, |view, cx| view.set_rehosting(false, cx)); }
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
            let inherited = inherited;
            let cold_lock = cold_lock;
            if let Some(origin) = cold_lock.as_ref()
                && !root.update(cx, |root, cx| root.file_lock_cold_origin_current(origin, cx)).unwrap_or(false) {
                let _ = workspace.update(cx, |view, cx| { if source.is_current() { view.set_rehosting(false, cx); cx.notify(); } });
                return;
            }
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
                        if let Some((origin, _)) = inherited.as_ref() {
                            let _ = root.update(cx, |root, cx| root.abort_file_search_handoff(origin, workspace.entity_id(), &source, "Could not open the destination window.", cx));
                        } else { let _ = workspace.update(cx, |view, cx| {
                            view.set_rehosting(false, cx);
                            view.set_error(Some(format!(
                                "Could not open {} window: {error}",
                                T::TITLE
                            )));
                            cx.notify();
                        }); }
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
                        if let Some((origin, _)) = inherited.as_ref() {
                            let _ = root.update(cx, |root, cx| root.abort_file_search_handoff(origin, workspace.entity_id(), &source, "Could not attach the destination window.", cx));
                        }
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
                        if inherited.is_none() { let _ = workspace.update(cx, |view, cx| {
                            view.set_rehosting(false, cx);
                            view.set_error(Some(format!("Could not attach {} window.", T::TITLE)));
                            cx.notify();
                        }); }
                        return;
                    }
                };
                let mounted_receipt = cx.update(|app| workspace.read(app).receipt());
                let requires_activation = cx.update(|app| workspace.read(app).transfer_requires_activation());
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
                let inherited_ready = inherited.as_ref().is_none_or(|(origin, _)| root.update(cx, |root, cx|
                    root.file_search_origin_current(origin, origin.native.identity, cx)).unwrap_or(false));
                let cold_ready = cold_lock.as_ref().is_none_or(|origin| root.update(cx, |root, cx|
                    root.file_lock_cold_origin_current(origin, cx)).unwrap_or(false));
                if installed && inherited_ready && cold_ready && hosted_source_current(&root, &workspace, root_window, cx) {
                    if let Some(native) = native {
                        if mounted_receipt.is_current() && native_window_is_valid(native) {
                            displayed = match set_native_window_visible(native, true) {
                                Ok(()) => true,
                                Err(error) => {
                                    tracing::warn!(%error, utility = T::TITLE, "could not show utility window");
                                    false
                                }
                            };
                            let authorized = if let Some((origin, _)) = inherited.as_ref() {
                                root.update(cx, |root, cx| root.file_search_origin_current(origin, origin.native.identity, cx)).unwrap_or(false)
                            } else if let Some(origin) = cold_lock.as_ref() {
                                root.update(cx, |root, cx| root.file_lock_cold_origin_current(origin, cx)).unwrap_or(false)
                            } else { gesture.is_some_and(|gesture| {
                                gesture.foreground_authorized
                                    && super::json_utility::native_host_current(
                                        &visibility,
                                        gesture.visibility_revision,
                                        Some(gesture.source),
                                        false,
                                    )
                                    && current_foreground_identity()
                                        == Some(gesture.source.identity)
                            }) };
                            if displayed && authorized && mounted_receipt.is_current() && hosted_source_current(&root, &workspace, root_window, cx) {
                                if inherited.is_some() || cold_lock.is_some() || requires_activation {
                                    displayed = activate_native_window(native).is_ok() && current_foreground_identity() == Some(native.identity);
                                } else { let _ = activate_native_window(native); }
                            } else if inherited.is_some() || cold_lock.is_some() || requires_activation { displayed = false; }
                            if displayed && mounted_receipt.is_current() && inherited.is_none() && cold_lock.is_none() {
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
                if displayed && let Some((origin, _)) = inherited.as_ref() {
                    displayed = native.is_some_and(|native| root.update(cx, |root, cx| {
                        if !root.file_search_origin_current(origin, native.identity, cx)
                            || !T::popup(root).is_some_and(|popup| popup.window.window_id() == window.window_id() && popup.native == Some(native))
                            || !mounted_receipt.is_current() { return false; }
                        root.commit_file_search_handoff(origin, workspace.entity_id(), window.window_id(), cx)
                    }).unwrap_or(false));
                }
                if displayed && let Some(origin) = cold_lock.as_ref() {
                    displayed = native.is_some_and(|native| current_foreground_identity() == Some(native.identity)
                        && root.update(cx, |root, cx| {
                            mounted_receipt.is_current()
                                && T::popup(root).is_some_and(|popup| popup.window.window_id() == window.window_id() && popup.native == Some(native))
                                && root.file_lock_cold_origin_retained(origin, cx)
                        }).unwrap_or(false));
                }
                if displayed {
                    displayed = root.update(cx, |root, cx| workspace.read(cx).transfer_commit_current(root, cx)).unwrap_or(false);
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
                                if (inherited.is_some() || cold_lock.is_some()) && native.is_some_and(|native| current_foreground_identity() == Some(native.identity))
                                    && !window.has_active_dialog(cx) {
                                    view.focus(window, cx);
                                } else if inherited.is_none() && cold_lock.is_none() {
                                    view.after_popout(window, cx);
                                }
                            }
                        });
                } else {
                    let cold_restore = cold_lock.as_ref().is_some_and(|origin| root.update(cx, |root, cx|
                        root.file_lock_cold_origin_current(origin, cx)).unwrap_or(false));
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
                                if inherited.is_none() { view.set_error(Some(format!(
                                    "Could not show {} window.", T::TITLE
                                ))); }
                                if !requires_activation && gesture.is_some_and(|gesture| gesture.foreground_authorized)
                                    && root_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    })
                                {
                                    view.focus(window, cx);
                                } else if cold_restore && cold_lock.as_ref().is_some_and(|origin| origin.focus.is_focused(window))
                                    && window.is_window_active() && !window.has_active_dialog(cx)
                                    && root_native.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                                    view.focus(window, cx);
                                }
                                cx.notify();
                            }
                        },
                    );
                    if let Some((origin, _)) = inherited.as_ref() {
                        let restored = cx.update(|app| workspace.read(app).receipt());
                        let _ = root.update(cx, |root, cx| root.abort_file_search_handoff(origin, workspace.entity_id(), &restored, "The destination could not be activated.", cx));
                    }
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
    use super::{inherited_popup_target_allowed, shell_geometry_mount_is_current};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    #[test]
    fn inherited_popup_requires_live_presented_modal_free_target() {
        assert!(inherited_popup_target_allowed(true, true, true, true));
        assert!(!inherited_popup_target_allowed(false, true, true, true));
        assert!(!inherited_popup_target_allowed(true, false, true, true));
        assert!(!inherited_popup_target_allowed(true, true, false, true));
        assert!(!inherited_popup_target_allowed(true, true, true, false));
    }
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
