//! Exact mounted host authority for Clipboard Modify effects.
use super::*;

#[derive(Clone)]
pub(super) struct HostAuthority {
    pub host: AnyWindowHandle,
    pub native: NativeWindow,
    pub mount: utility_mount::MountReceipt,
    visibility: RootVisibility,
    visibility_revision: u64,
    admission: Arc<RestartAdmission>,
    popout: bool,
}

impl HostAuthority {
    pub(super) fn capture(
        view: &ClipboardModifyView,
        window: &Window,
        cx: &Context<ClipboardModifyView>,
    ) -> Option<Self> {
        let root = view.root.upgrade()?;
        let root = root.read(cx);
        let host = window.window_handle();
        let native = view.native?;
        let tracked = if view.popout {
            root.clipboard_modify_popup.as_ref().is_some_and(|popup| {
                popup.window.window_id() == host.window_id() && popup.native == Some(native)
            })
        } else {
            root.root_window.is_some_and(|current| current.window_id() == host.window_id())
                && root.native_window == Some(native)
        };
        if !view.is_mounted_in(host.window_id(), view.popout)
            || view.rehosting
            || root.restart_admission.is_fenced()
            || !tracked
            || !root.clipboard_modify_view.as_ref().is_some_and(|current| {
                current.entity_id() == cx.entity().entity_id()
            })
            || current_foreground_identity() != Some(native.identity)
        {
            return None;
        }
        let visibility = root.root_visibility.clone();
        let receipt = Self {
            host,
            native,
            mount: view.mount.clone(),
            visibility_revision: visibility.request_revision.load(Ordering::Acquire),
            visibility,
            admission: root.restart_admission.clone(),
            popout: view.popout,
        };
        receipt.current().then_some(receipt)
    }

    pub(super) fn current(&self) -> bool {
        self.mount.is_current()
            && !self.admission.is_fenced()
            && json_utility::native_host_current(
                &self.visibility,
                self.visibility_revision,
                Some(self.native),
                self.popout,
            )
            && current_foreground_identity() == Some(self.native.identity)
    }

    pub(super) fn current_for(
        &self,
        view: &ClipboardModifyView,
        window: &Window,
        cx: &Context<ClipboardModifyView>,
    ) -> bool {
        Self::capture(view, window, cx).is_some_and(|current| {
            current.host.window_id() == self.host.window_id()
                && current.native == self.native
                && current.popout == self.popout
                && current.mount.matches(&self.mount)
                && self.current()
        })
    }
}
