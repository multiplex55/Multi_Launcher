use super::{
    HashInputMode, HashPresentationMode, PhysicalKeydown,
    jump_mode::{
        JumpGeometryDraft, JumpGeometryElement, JumpGeometryFrame, JumpModeController,
        jump_hint_badge,
    },
};
use gpui_kit::{AnyElement, Context, FocusHandle, IntoElement, ParentElement, Styled, div};
use multi_launcher::{
    gui::theme::{SemanticPalette, ThemeMetrics},
    launcher::jump_mode::{
        SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
    },
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HashControl {
    Source,
    Clipboard,
    Text,
    File,
    Expected,
    Algorithm,
    Calculate,
    Dock,
    Popout,
    Close,
    Copy,
    Cancel,
}
impl HashControl {
    fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Clipboard => "clipboard",
            Self::Text => "text",
            Self::File => "file",
            Self::Expected => "expected",
            Self::Algorithm => "algorithm",
            Self::Calculate => "calculate",
            Self::Dock => "dock",
            Self::Popout => "popout",
            Self::Close => "close",
            Self::Copy => "copy",
            Self::Cancel => "cancel",
        }
    }
    fn metadata(self) -> (&'static str, u64, TargetKind) {
        match self {
            Self::Source => ("Source", 0, TargetKind::ComboBox),
            Self::Clipboard => ("Load Clipboard", 1, TargetKind::Button),
            Self::Text => ("Text", 2, TargetKind::TextEntry),
            Self::File => ("Choose File", 3, TargetKind::Button),
            Self::Expected => ("Expected", 4, TargetKind::TextEntry),
            Self::Algorithm => ("Algorithm", 5, TargetKind::ComboBox),
            Self::Calculate => ("Calculate", 6, TargetKind::Button),
            Self::Dock => ("Dock", 7, TargetKind::Button),
            Self::Popout => ("Pop Out", 8, TargetKind::Button),
            Self::Close => ("Close", 9, TargetKind::Button),
            Self::Copy => ("Copy Digest", 10, TargetKind::Button),
            Self::Cancel => ("Cancel", 11, TargetKind::Button),
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Source,
            Self::Clipboard,
            Self::Text,
            Self::File,
            Self::Expected,
            Self::Algorithm,
            Self::Calculate,
            Self::Dock,
            Self::Popout,
            Self::Close,
            Self::Copy,
            Self::Cancel,
        ]
        .into_iter()
        .find(|role| role.name() == name)
    }
}

#[derive(Clone, Copy)]
pub(super) struct HashEligibility {
    pub current: bool,
    pub mode: HashInputMode,
    pub presentation: HashPresentationMode,
    pub has_job: bool,
    pub is_running: bool,
    pub picker_pending: bool,
    pub clipboard_pending: bool,
    pub clipboard_copy_pending: bool,
    pub can_copy_digest: bool,
}

impl HashEligibility {
    pub fn allows(self, role: HashControl) -> bool {
        if !self.current {
            return false;
        }
        match role {
            HashControl::Source => !self.picker_pending && !self.clipboard_copy_pending,
            HashControl::Clipboard | HashControl::Close => !self.clipboard_pending,
            HashControl::Text => self.mode == HashInputMode::Text && !self.clipboard_copy_pending,
            HashControl::File => {
                self.mode == HashInputMode::File
                    && !self.picker_pending
                    && !self.clipboard_copy_pending
            }
            HashControl::Expected | HashControl::Algorithm => !self.clipboard_copy_pending,
            HashControl::Calculate => !self.has_job && !self.clipboard_copy_pending,
            HashControl::Dock => {
                self.presentation != HashPresentationMode::Docked && !self.clipboard_copy_pending
            }
            HashControl::Popout => {
                self.presentation != HashPresentationMode::Popout && !self.clipboard_copy_pending
            }
            HashControl::Copy => {
                self.can_copy_digest && !self.clipboard_pending && !self.picker_pending
            }
            HashControl::Cancel => self.is_running && !self.clipboard_copy_pending,
        }
    }
}

pub(super) struct HashJumpRender<'a> {
    pub jump: &'a HashJump,
    pub generation: u64,
    pub draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
    pub eligibility: HashEligibility,
    pub palette: SemanticPalette,
    pub metrics: ThemeMetrics,
}

impl HashJumpRender<'_> {
    pub fn wrap<E: IntoElement>(&self, element: E, role: HashControl) -> AnyElement {
        self.jump.wrap(
            element,
            role,
            self.generation,
            self.draft.clone().filter(|_| self.eligibility.allows(role)),
            self.palette,
            self.metrics,
        )
    }
}

pub(super) struct HashJump {
    pub controller: JumpModeController,
    pub frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
    pub clipboard: FocusHandle,
    pub file: FocusHandle,
    pub calculate: FocusHandle,
    pub cancel: FocusHandle,
    pub copy: FocusHandle,
    pub dock: FocusHandle,
    pub popout: FocusHandle,
    pub close: FocusHandle,
}

impl HashJump {
    pub fn new(
        controller: JumpModeController,
        cx: &mut Context<super::HashGeneratorWindow>,
    ) -> Self {
        Self {
            controller,
            frame: Rc::new(RefCell::new(None)),
            clipboard: cx.focus_handle(),
            file: cx.focus_handle(),
            calculate: cx.focus_handle(),
            cancel: cx.focus_handle(),
            copy: cx.focus_handle(),
            dock: cx.focus_handle(),
            popout: cx.focus_handle(),
            close: cx.focus_handle(),
        }
    }
    pub fn button(&self, role: HashControl) -> Option<FocusHandle> {
        Some(
            match role {
                HashControl::Clipboard => &self.clipboard,
                HashControl::File => &self.file,
                HashControl::Calculate => &self.calculate,
                HashControl::Cancel => &self.cancel,
                HashControl::Copy => &self.copy,
                HashControl::Dock => &self.dock,
                HashControl::Popout => &self.popout,
                HashControl::Close => &self.close,
                _ => return None,
            }
            .clone(),
        )
    }
    pub fn id(generation: u64, role: HashControl) -> TargetId {
        TargetId::new(
            SurfaceId::dialog("hash-generator", generation),
            SemanticTargetId::named(role.name()),
            TargetGeneration(generation),
        )
    }
    pub fn wrap<E: IntoElement>(
        &self,
        element: E,
        role: HashControl,
        generation: u64,
        draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
        palette: SemanticPalette,
        metrics: ThemeMetrics,
    ) -> AnyElement {
        let Some(draft) = draft else {
            return element.into_any_element();
        };
        let id = Self::id(generation, role);
        let (label, order, kind) = role.metadata();
        let badge = jump_hint_badge(
            self.controller.hint_for(&id),
            &id,
            generation,
            Rc::clone(&self.frame),
            palette,
            metrics,
        );
        let wrapped = div()
            .relative()
            .w_full()
            .min_w(gpui_kit::px(0.0))
            .child(element)
            .children(badge);
        JumpGeometryElement::new(wrapped, TargetMetadata::new(id, label, kind, order), draft)
            .into_any_element()
    }
    pub fn key(
        &mut self,
        receipt: PhysicalKeydown,
        generation: u64,
    ) -> (bool, bool, Option<TargetId>) {
        let frame = self.frame.borrow();
        let retired = self.controller.reconcile_frame(frame.as_ref());
        let outcome = self
            .controller
            .process_key_down(receipt, frame.as_ref(), generation);
        (
            outcome.consumed,
            outcome.repaint || retired,
            outcome.selected_target,
        )
    }
}
