//! Shared mounted utility Jump transport; feature adapters own scopes and targets.
use super::*;
use multi_launcher::launcher::jump_mode::{
    SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use std::{cell::RefCell, rc::Rc};
static NEXT_MOUNT: AtomicU64 = AtomicU64::new(1);
pub(super) fn target_id(kind: &str, generation: u64, name: impl Into<String>) -> TargetId {
    TargetId::new(
        SurfaceId::dialog(kind, generation),
        SemanticTargetId::named(name),
        TargetGeneration(generation),
    )
}
pub(super) struct ScopedJump {
    pub controller: JumpModeController,
    pub generation: u64,
    pub frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
    kind: &'static str,
}
impl ScopedJump {
    pub fn new(kind: &'static str, controller: JumpModeController) -> Self {
        Self {
            controller,
            generation: NEXT_MOUNT.fetch_add(1, Ordering::Relaxed),
            frame: Rc::new(RefCell::new(None)),
            kind,
        }
    }
    pub fn id(&self, name: impl Into<String>) -> TargetId {
        target_id(self.kind, self.generation, name)
    }
    pub fn begin(&self) -> Option<Rc<RefCell<JumpGeometryDraft>>> {
        self.controller
            .enabled
            .then(|| Rc::new(RefCell::new(JumpGeometryDraft::new(self.generation))))
    }
    pub fn wrap<E: IntoElement>(
        &self,
        element: E,
        name: String,
        label: &'static str,
        kind: TargetKind,
        order: u64,
        draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
        app: &App,
    ) -> AnyElement {
        let Some(draft) = draft else {
            return element.into_any_element();
        };
        let id = self.id(name.clone());
        let resolved = appearance::current(app);
        let badge = jump_hint_badge(
            self.controller.hint_for(&id),
            &id,
            self.generation,
            self.frame.clone(),
            resolved.palette,
            resolved.metrics,
        );
        let body = div()
            .id(SharedString::from(format!("{}-jump-{name}", self.kind)))
            .relative()
            .child(element)
            .children(badge);
        JumpGeometryElement::new(body, TargetMetadata::new(id, label, kind, order), draft)
            .into_any_element()
    }
    pub fn publish<T: 'static>(
        &self,
        draft: Rc<RefCell<JumpGeometryDraft>>,
        cx: &mut Context<T>,
        callback: impl Fn(&mut T, &JumpGeometryFrame, &Window, &mut Context<T>, bool) + 'static,
    ) -> AnyElement {
        let resolved = appearance::current(cx);
        jump_geometry_publisher(
            draft,
            self.frame.clone(),
            cx.entity().downgrade(),
            self.controller.session_generation(),
            Rc::new(self.controller.target_ids().to_vec()),
            self.controller.alphabet.clone(),
            resolved.metrics,
            callback,
        )
        .into_any_element()
    }
}
fn exact_host(
    root: &RootWindow,
    popup: Option<&utility_mount::UtilityPopup>,
    popout: bool,
    receipt: PhysicalKeydown,
    window: &Window,
) -> bool {
    let (native, matches, visible) = if popout {
        let Some(popup) = popup else {
            return false;
        };
        (
            popup.native,
            popup.window.window_id() == window.window_handle().window_id(),
            true,
        )
    } else {
        (
            root.native_window,
            root.root_window
                .is_some_and(|host| host.window_id() == window.window_handle().window_id()),
            root.root_visibility.active.load(Ordering::Acquire)
                && root.root_visibility.visible.load(Ordering::Acquire),
        )
    };
    host_matches(
        native,
        matches,
        visible,
        !popout || window.is_window_active(),
        receipt,
    )
}
pub(super) fn dispatch(
    root: &WeakEntity<RootWindow>,
    contexts: &[KeyContext],
    receipt: PhysicalKeydown,
    window: &mut Window,
    app: &mut App,
) -> bool {
    let dialog = context_stack_has_primary(contexts, "Dialog")
        || context_stack_has_primary(contexts, "AlertDialog");
    let popup = context_stack_has_primary(contexts, "PopupMenu");
    let json = context_stack_has_primary(contexts, "JsonUtilityWindow");
    let xml = context_stack_has_primary(contexts, "XmlUtilityWindow");
    if !dialog && !json && !xml {
        return false;
    }
    root.update(app, |root, cx| {
        if dialog {
            let Some(view) = root.xml_view.clone() else {
                return false;
            };
            let state = view.read(cx);
            if !state.save_review_pending()
                || !window.is_window_active()
                || !exact_host(root, root.xml_popup.as_ref(), state.popout, receipt, window)
            {
                return false;
            }
            return view.update(cx, |view, cx| {
                view.handle_review_jump(receipt, window, cx)
            });
        }
        if popup {
            return false;
        }
        if json {
            let Some(view) = root.json_view.clone() else {
                return false;
            };
            let state = view.read(cx);
            if !exact_host(
                root,
                root.json_popup.as_ref(),
                state.popout,
                receipt,
                window,
            ) {
                return false;
            }
            return view.update(cx, |view, cx| view.handle_json_jump(receipt, window, cx));
        }
        if xml {
            let Some(view) = root.xml_view.clone() else {
                return false;
            };
            let state = view.read(cx);
            if !exact_host(root, root.xml_popup.as_ref(), state.popout, receipt, window) {
                return false;
            }
            return view.update(cx, |view, cx| view.handle_xml_jump(receipt, window, cx));
        }
        false
    })
    .unwrap_or(false)
}
pub(super) fn consume_owned_held(
    controller: &mut JumpModeController,
    receipt: PhysicalKeydown,
) -> bool {
    if !controller.owns_key(receipt.virtual_key) {
        return false;
    }
    if receipt.held {
        return true;
    }
    controller.release_fresh_key(receipt);
    false
}
fn ownership_allows(host: bool, visible: bool, active: bool, hwnd: bool, foreground: bool) -> bool {
    host && visible && active && hwnd && foreground
}
pub(super) fn host_matches(
    native: Option<NativeWindow>,
    host_matches: bool,
    visible: bool,
    active: bool,
    receipt: PhysicalKeydown,
) -> bool {
    native
        .filter(|native| native_window_is_valid(*native))
        .is_some_and(|native| {
            ownership_allows(
                host_matches,
                visible,
                active,
                receipt.hwnd == native.identity.hwnd,
                current_foreground_identity() == Some(native.identity),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{consume_owned_held, ownership_allows};
    use crate::gpui_frontend::{JumpModeController, Modifiers, PhysicalKeydown};
    use multi_launcher::settings::{HintAlphabet, JumpModeChord};
    #[test]
    fn modal_latch_consumes_only_owned_held_and_releases_on_fresh() {
        let mut controller =
            JumpModeController::new(true, JumpModeChord::default(), HintAlphabet::default());
        controller.held_owned_vkeys.insert(0x1b);
        let held = PhysicalKeydown {
            hwnd: 1,
            virtual_key: 0x1b,
            scan_code: 0,
            extended: false,
            modifiers: Modifiers::default(),
            held: true,
        };
        assert!(consume_owned_held(&mut controller, held));
        assert!(!consume_owned_held(
            &mut controller,
            PhysicalKeydown {
                held: false,
                ..held
            }
        ));
        assert!(!consume_owned_held(&mut controller, held));
    }
    #[test]
    fn scoped_native_dispatch_requires_exact_active_foreground_owner() {
        assert!(ownership_allows(true, true, true, true, true));
        for denied in [
            (false, true, true, true, true),
            (true, false, true, true, true),
            (true, true, false, true, true),
            (true, true, true, false, true),
            (true, true, true, true, false),
        ] {
            assert!(!ownership_allows(
                denied.0, denied.1, denied.2, denied.3, denied.4
            ));
        }
    }
}
