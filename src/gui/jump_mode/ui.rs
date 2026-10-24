//! Frame-scoped egui target collection and safe focus handoff for Jump Mode.
//!
//! The bridge stores only copied IDs and geometry. A renderer keeps its
//! `Response` for the current call, applies any returned focus directive, and
//! then drops it normally.

use super::{
    HintMap, HintSession, MatchOutcome, SemanticTargetId, SessionLifecycle, SurfaceId,
    TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use eframe::egui;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct NavigationEpoch(pub(crate) u64);

impl NavigationEpoch {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// A generic renderer instruction. `Select` deliberately leaves selection to
/// the owning component and neither directive invokes an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusDirective {
    Focus(TargetKind),
    Select,
}

/// A dialog family that can own a Jump Mode surface once its actual root
/// viewport window has been observed. Some dialogs use immediate viewports or
/// have no root window; those remain blocked until their viewport owner is
/// explicitly integrated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavigationDialogKind {
    FileSearch,
    Snippets,
    ClipboardHistory,
    JsonUtility,
    RegexTester,
    XmlToolkit,
    MouseSettings,
    MouseGestures,
    BatchRename,
    SearchReplace,
    DashboardEditor,
    RadialDesigner,
    MouseGestureSettings,
    HotkeyDirectory,
    TableUtility,
    ImageCompare,
}

impl NavigationDialogKind {
    pub(crate) const ALL: [Self; 16] = [
        Self::FileSearch,
        Self::Snippets,
        Self::ClipboardHistory,
        Self::JsonUtility,
        Self::RegexTester,
        Self::XmlToolkit,
        Self::MouseSettings,
        Self::MouseGestures,
        Self::BatchRename,
        Self::SearchReplace,
        Self::DashboardEditor,
        Self::RadialDesigner,
        Self::MouseGestureSettings,
        Self::HotkeyDirectory,
        Self::TableUtility,
        Self::ImageCompare,
    ];

    const fn index(self) -> usize {
        match self {
            Self::FileSearch => 0,
            Self::Snippets => 1,
            Self::ClipboardHistory => 2,
            Self::JsonUtility => 3,
            Self::RegexTester => 4,
            Self::XmlToolkit => 5,
            Self::MouseSettings => 6,
            Self::MouseGestures => 7,
            Self::BatchRename => 8,
            Self::SearchReplace => 9,
            Self::DashboardEditor => 10,
            Self::RadialDesigner => 11,
            Self::MouseGestureSettings => 12,
            Self::HotkeyDirectory => 13,
            Self::TableUtility => 14,
            Self::ImageCompare => 15,
        }
    }

    pub(crate) const fn surface_kind(self) -> &'static str {
        match self {
            Self::FileSearch => "file-search",
            Self::Snippets => "snippets",
            Self::ClipboardHistory => "clipboard-history",
            Self::JsonUtility => "json-utility",
            Self::RegexTester => "regex-tester",
            Self::XmlToolkit => "xml-toolkit",
            Self::MouseSettings => "mouse-settings",
            Self::MouseGestures => "mouse-gestures",
            Self::BatchRename => "batch-rename",
            Self::SearchReplace => "search-replace",
            Self::DashboardEditor => "dashboard-editor",
            Self::RadialDesigner => "radial-designer",
            Self::MouseGestureSettings => "mouse-gesture-settings",
            Self::HotkeyDirectory => "hotkey-directory",
            Self::TableUtility => "table-utility",
            Self::ImageCompare => "image-compare",
        }
    }

    /// The current root viewport layer ID for a dialog's real `Window`.
    /// Radial Designer is a separate viewport and deliberately has no root
    /// layer identity here.
    pub(crate) fn layer_id(self) -> Option<egui::LayerId> {
        let id = match self {
            Self::FileSearch => egui::Id::new(Some("File Search")),
            Self::Snippets => egui::Id::new(Some("Snippets")),
            Self::ClipboardHistory => egui::Id::new(Some("Clipboard History")),
            Self::JsonUtility => egui::Id::new("json_utility_dialog"),
            Self::RegexTester => egui::Id::new("regex_tester_embedded_window"),
            Self::XmlToolkit => egui::Id::new("xml_toolkit_embedded_window"),
            Self::MouseSettings => egui::Id::new(Some("Mouse Settings")),
            Self::MouseGestures => egui::Id::new(Some("Mouse Gestures")),
            Self::BatchRename => egui::Id::new("batch_rename_preview_dialog"),
            Self::SearchReplace => egui::Id::new("search_replace_preview_dialog"),
            Self::DashboardEditor => egui::Id::new(Some("Dashboard Editor")),
            Self::RadialDesigner => return None,
            Self::MouseGestureSettings => egui::Id::new(Some("Mouse Gesture Settings")),
            Self::HotkeyDirectory => egui::Id::new(Some("Hotkey Directory")),
            Self::TableUtility => egui::Id::new(Some("Table Utility")),
            Self::ImageCompare => egui::Id::new(Some("Image Comparison")),
        };
        Some(egui::LayerId::new(egui::Order::Middle, id))
    }

    fn for_layer(layer: egui::LayerId) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.layer_id() == Some(layer))
    }
}

/// The sole surface allowed to receive Jump Mode navigation in this frame.
/// Unsupported windows and transient owners deliberately map to `Blocked`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum NavigationOwner {
    #[default]
    Root,
    Settings {
        instance: u64,
    },
    Notes {
        instance: u64,
    },
    Todo {
        instance: u64,
    },
    Timer {
        instance: u64,
    },
    PasteSequence {
        instance: u64,
    },
    MkMacro {
        instance: u64,
    },
    Dialog {
        kind: NavigationDialogKind,
        instance: u64,
    },
    /// Inline removal confirmation within the current Snippets window. The
    /// parent window and confirmation have separate identities so crossing
    /// either boundary invalidates the previous hint session.
    SnippetRemoval {
        parent_instance: u64,
        confirmation_instance: u64,
    },
    /// Inline removal confirmation within the current Clipboard History
    /// window. Its generation is separate from the parent browse surface.
    ClipboardRemoval {
        parent_instance: u64,
        confirmation_instance: u64,
    },
    /// The exact per-open Bind Action child inside Mouse Gestures.
    MouseGestureBinding {
        parent_instance: u64,
        binding_instance: u64,
    },
    /// The exact per-request gesture deletion confirmation.
    MouseGestureDelete {
        parent_instance: u64,
        confirmation_instance: u64,
    },
    /// Exact XML overwrite confirmation for one frozen save request. The
    /// parent dialog and save request together own its transient layer.
    XmlOverwriteConfirmation {
        parent_instance: u64,
        confirmation_instance: u64,
    },
    /// One exact Batch Rename confirmation for a frozen Apply plan or a
    /// receipt-specific Undo request.
    BatchRenameConfirmation {
        parent_instance: u64,
        confirmation_instance: u64,
        kind: BatchRenameConfirmationKind,
    },
    /// One exact Search & Replace Apply or receipt-specific confirmation.
    /// `subject` is a payload-free identity for the frozen candidate or receipt.
    SearchReplaceConfirmation {
        parent_instance: u64,
        confirmation_instance: u64,
        kind: SearchReplaceConfirmationKind,
        subject: SearchReplaceConfirmationSubject,
    },
    /// One exact Dashboard Editor confirmation for removing a slot or
    /// resetting the selected widget's settings.
    DashboardEditorConfirmation {
        parent_instance: u64,
        confirmation_instance: u64,
        kind: DashboardEditorConfirmationKind,
    },
    /// One exact, short-lived Radial Designer child window. The subject is an
    /// opaque identity for the pending entity/request, never its displayed
    /// label or authored contents.
    RadialDesignerChild {
        parent_instance: u64,
        child_instance: u64,
        kind: RadialDesignerChildKind,
        subject: u64,
    },
    /// One exact, transient snippet prompt window. Its generation is also
    /// part of the window's egui layer ID.
    SnippetPrompt {
        generation: u64,
    },
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RadialDesignerChildKind {
    CellProperties,
    ScopeChangeGuard,
    AddToRadial,
    ClosePrompt,
    DeleteMessage,
    DeleteRing,
    ResizeResolution,
}

impl RadialDesignerChildKind {
    pub(crate) const fn surface_kind(self) -> &'static str {
        match self {
            Self::CellProperties => "radial-cell-properties",
            Self::ScopeChangeGuard => "radial-scope-change-guard",
            Self::AddToRadial => "radial-add-to-menu",
            Self::ClosePrompt => "radial-close-prompt",
            Self::DeleteMessage => "radial-delete-message",
            Self::DeleteRing => "radial-delete-ring",
            Self::ResizeResolution => "radial-resize-resolution",
        }
    }
}

pub(crate) fn radial_designer_child_window_id(
    parent_instance: u64,
    child_instance: u64,
    kind: RadialDesignerChildKind,
    subject: u64,
) -> egui::Id {
    egui::Id::new((
        "radial-designer-child-window",
        parent_instance,
        child_instance,
        kind,
        subject,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BatchRenameConfirmationKind {
    Apply,
    Undo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SearchReplaceConfirmationKind {
    Apply,
    Restore,
    Recover,
}

impl SearchReplaceConfirmationKind {
    pub(crate) fn surface_kind(self) -> &'static str {
        match self {
            Self::Apply => "search-replace-apply-confirmation",
            Self::Restore => "search-replace-restore-confirmation",
            Self::Recover => "search-replace-recovery-confirmation",
        }
    }

    pub(crate) fn layer_id(self) -> egui::LayerId {
        let id = match self {
            Self::Apply => "replace_apply_confirmation",
            Self::Restore | Self::Recover => "replace_recovery_confirmation",
        };
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SearchReplaceConfirmationSubject {
    Apply {
        search_generation: u64,
        replacement_revision: u64,
    },
    Receipt {
        receipt_identity: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DashboardEditorConfirmationKind {
    RemoveSlot,
    ResetWidgetSettings,
}

impl DashboardEditorConfirmationKind {
    pub(crate) fn surface_kind(self) -> &'static str {
        match self {
            Self::RemoveSlot => "dashboard-editor-remove-confirmation",
            Self::ResetWidgetSettings => "dashboard-editor-reset-confirmation",
        }
    }

    pub(crate) fn window_id(self) -> &'static str {
        match self {
            Self::RemoveSlot => "dashboard_editor_remove_confirmation",
            Self::ResetWidgetSettings => "dashboard_editor_reset_confirmation",
        }
    }
}

pub(crate) fn dashboard_editor_confirmation_layer_id(
    parent_instance: u64,
    confirmation_instance: u64,
    kind: DashboardEditorConfirmationKind,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new((kind.window_id(), parent_instance, confirmation_instance)),
    )
}

pub(crate) fn resolve_dashboard_editor_confirmation_owner(
    ctx: &egui::Context,
    parent_instance: u64,
    confirmation_instance: u64,
    kind: DashboardEditorConfirmationKind,
) -> NavigationOwner {
    let owner = NavigationOwner::DashboardEditorConfirmation {
        parent_instance,
        confirmation_instance,
        kind,
    };
    let child_layer =
        dashboard_editor_confirmation_layer_id(parent_instance, confirmation_instance, kind);
    let parent_layer = NavigationDialogKind::DashboardEditor.layer_id();
    if ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && !egui::Popup::is_any_open(ctx)
        && visible_top_middle_layer(ctx) == Some(child_layer)
        && focused_response_layer(ctx).is_none_or(|focused| {
            focused == child_layer
                || Some(focused) == parent_layer
                || focused.order != egui::Order::Middle
        })
    {
        owner
    } else {
        NavigationOwner::Blocked
    }
}

impl BatchRenameConfirmationKind {
    pub(crate) fn surface_kind(self) -> &'static str {
        match self {
            Self::Apply => "batch-rename-apply-confirmation",
            Self::Undo => "batch-rename-undo-confirmation",
        }
    }
}

/// Exact non-root viewport authority for a surface whose base UI is not a
/// normal ROOT `Window`. The owner and base layer are copied values supplied
/// by that viewport's real renderer callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NavigationViewportBinding {
    pub(crate) viewport: egui::ViewportId,
    pub(crate) owner: NavigationOwner,
    pub(crate) base_layer: egui::LayerId,
}

impl NavigationViewportBinding {
    fn owner_is_admissible(self) -> bool {
        matches!(
            self.owner,
            NavigationOwner::Dialog {
                kind: NavigationDialogKind::RegexTester
                    | NavigationDialogKind::XmlToolkit
                    | NavigationDialogKind::TableUtility
                    | NavigationDialogKind::ImageCompare
                    | NavigationDialogKind::RadialDesigner,
                ..
            } | NavigationOwner::XmlOverwriteConfirmation { .. }
                | NavigationOwner::RadialDesignerChild { .. }
        )
    }

    pub(crate) fn base_is_current(self, ctx: &egui::Context) -> bool {
        let visible_layer = visible_top_middle_layer(ctx);
        let layer_is_current = match self.owner {
            NavigationOwner::XmlOverwriteConfirmation {
                parent_instance,
                confirmation_instance,
            } => {
                self.base_layer
                    == xml_overwrite_confirmation_layer_id(parent_instance, confirmation_instance)
                    && visible_layer == Some(self.base_layer)
            }
            NavigationOwner::RadialDesignerChild {
                parent_instance,
                child_instance,
                kind,
                subject,
            } => {
                self.base_layer
                    == radial_designer_child_layer_id(
                        parent_instance,
                        child_instance,
                        kind,
                        subject,
                    )
                    && visible_layer == Some(self.base_layer)
            }
            _ => visible_layer.is_none(),
        };
        self.owner_is_admissible()
            && ctx.viewport_id() == self.viewport
            && self.viewport != egui::ViewportId::ROOT
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && layer_is_current
    }

    pub(crate) fn focus_is_owned(
        self,
        ctx: &egui::Context,
        popup_layer: Option<egui::LayerId>,
    ) -> bool {
        focused_response_layer(ctx).is_none_or(|layer| {
            layer == self.base_layer
                || Some(layer) == popup_layer
                || (matches!(self.owner, NavigationOwner::RadialDesignerChild { .. })
                    && layer == egui::LayerId::background())
        })
    }
}

pub(crate) fn radial_designer_child_layer_id(
    parent_instance: u64,
    child_instance: u64,
    kind: RadialDesignerChildKind,
    subject: u64,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        radial_designer_child_window_id(parent_instance, child_instance, kind, subject),
    )
}

/// Resolve the already-identified viewport owner only while its actual
/// non-root base viewport remains focused and no foreign Middle/modal layer
/// has taken over. An open popup requires the exact previously observed combo
/// anchor; unknown foreground popups remain blocked.
pub(crate) fn resolve_bound_viewport_navigation_owner(
    ctx: &egui::Context,
    binding: NavigationViewportBinding,
    combo_popup: Option<&ComboPopupOwner>,
) -> NavigationOwner {
    if !binding.base_is_current(ctx) {
        return NavigationOwner::Blocked;
    }

    if egui::Popup::is_any_open(ctx) {
        let Some(popup) = combo_popup.filter(|popup| {
            popup.parent_owner == binding.owner
                && popup.viewport == binding.viewport
                && popup.binding == Some(binding)
                && (popup.is_current(ctx, binding.owner)
                    || popup.is_sizing_pass(ctx, binding.owner))
        }) else {
            return NavigationOwner::Blocked;
        };
        if !binding.focus_is_owned(ctx, Some(popup.popup_layer)) {
            return NavigationOwner::Blocked;
        }
        binding.owner
    } else if combo_popup.is_some() || !binding.focus_is_owned(ctx, None) {
        NavigationOwner::Blocked
    } else {
        binding.owner
    }
}

impl NavigationOwner {
    fn is_supported(self) -> bool {
        !matches!(self, Self::Root | Self::Blocked)
    }

    pub(crate) fn owns_settings(self, instance: u64) -> bool {
        self == Self::Settings { instance }
    }

    pub(crate) fn owns_surface(self, surface: &SurfaceId) -> bool {
        match surface {
            SurfaceId::Dashboard | SurfaceId::LauncherResults => self == Self::Root,
            SurfaceId::Settings { instance } => self.owns_settings(*instance),
            SurfaceId::Dialog { kind, instance } => match self {
                Self::Notes {
                    instance: owner_instance,
                } => kind == "notes" && *instance == owner_instance,
                Self::Todo {
                    instance: owner_instance,
                } => kind == "todo" && *instance == owner_instance,
                Self::Timer {
                    instance: owner_instance,
                } => kind == "timer" && *instance == owner_instance,
                Self::PasteSequence {
                    instance: owner_instance,
                } => kind == "paste-sequence" && *instance == owner_instance,
                Self::MkMacro {
                    instance: owner_instance,
                } => kind == "mkmacro" && *instance == owner_instance,
                Self::Dialog {
                    kind: owner_kind,
                    instance: owner_instance,
                } => kind == owner_kind.surface_kind() && *instance == owner_instance,
                Self::SnippetPrompt { generation } => {
                    kind == "snippet-prompt" && *instance == generation
                }
                _ => false,
            },
            SurfaceId::Popup {
                kind,
                instance,
                owner,
            } => match self {
                Self::SnippetRemoval {
                    parent_instance,
                    confirmation_instance,
                } => {
                    kind == "snippet-removal"
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog {
                                kind,
                                instance,
                            } if kind == "snippets" && *instance == parent_instance
                        )
                }
                Self::ClipboardRemoval {
                    parent_instance,
                    confirmation_instance,
                } => {
                    kind == "clipboard-removal"
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "clipboard-history" && *instance == parent_instance
                        )
                }
                Self::MouseGestureBinding {
                    parent_instance,
                    binding_instance,
                } => {
                    (kind == "mouse-gesture-binding"
                        && *instance == binding_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "mouse-gestures" && *instance == parent_instance
                        ))
                        || (kind.starts_with("combo:") && self.owns_surface(owner))
                }
                Self::MouseGestureDelete {
                    parent_instance,
                    confirmation_instance,
                } => {
                    (kind == "mouse-gesture-delete"
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "mouse-gestures" && *instance == parent_instance
                        ))
                        || (kind.starts_with("combo:") && self.owns_surface(owner))
                }
                Self::XmlOverwriteConfirmation {
                    parent_instance,
                    confirmation_instance,
                } => {
                    kind == "xml-overwrite-confirmation"
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "xml-toolkit" && *instance == parent_instance
                        )
                }
                Self::BatchRenameConfirmation {
                    parent_instance,
                    confirmation_instance,
                    kind: owner_kind,
                } => {
                    kind == owner_kind.surface_kind()
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "batch-rename" && *instance == parent_instance
                        )
                }
                Self::SearchReplaceConfirmation {
                    parent_instance,
                    confirmation_instance,
                    kind: owner_kind,
                    ..
                } => {
                    kind == owner_kind.surface_kind()
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "search-replace" && *instance == parent_instance
                        )
                }
                Self::DashboardEditorConfirmation {
                    parent_instance,
                    confirmation_instance,
                    kind: owner_kind,
                } => {
                    kind == owner_kind.surface_kind()
                        && *instance == confirmation_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "dashboard-editor" && *instance == parent_instance
                        )
                }
                Self::RadialDesignerChild {
                    parent_instance,
                    child_instance,
                    kind: owner_kind,
                    ..
                } => {
                    (kind == owner_kind.surface_kind()
                        && *instance == child_instance
                        && matches!(
                            owner.as_ref(),
                            SurfaceId::Dialog { kind, instance }
                                if kind == "radial-designer" && *instance == parent_instance
                        ))
                        || (kind.starts_with("combo:") && self.owns_surface(owner))
                }
                _ => self.owns_surface(owner),
            },
        }
    }
}

pub(crate) fn settings_layer_id() -> egui::LayerId {
    // `Window::new` hashes the optional title (`Some("Settings")`) as its
    // default area ID. Keep this aligned so owner checks match the real window.
    root_window_layer_id("Settings")
}

pub(crate) fn root_window_layer_id(title: &'static str) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some(title)))
}

pub(crate) fn notes_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Quick Notes")))
}

pub(crate) fn todo_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Todos")))
}

pub(crate) fn timer_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Create Timer")))
}

pub(crate) fn alarm_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Set Alarm")))
}

pub(crate) fn paste_sequence_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Paste Sequences")))
}

pub(crate) fn mkmacro_dialog_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new("mkmacro_main_window"))
}

pub(crate) fn snippet_prompt_layer_id(generation: u64) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new(("snippet_prompt_window", generation)),
    )
}

pub(crate) fn xml_overwrite_confirmation_layer_id(
    parent_instance: u64,
    confirmation_instance: u64,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new((
            "xml_toolkit_overwrite_confirmation",
            parent_instance,
            confirmation_instance,
        )),
    )
}

pub(crate) fn batch_rename_confirmation_layer_id(
    parent_instance: u64,
    confirmation_instance: u64,
    kind: BatchRenameConfirmationKind,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new((
            "batch_rename_confirmation_window",
            parent_instance,
            confirmation_instance,
            kind,
        )),
    )
}

/// Resolve one transient Batch Rename confirmation only while its exact
/// child layer owns the focused ROOT frame. The parent may retain focus during
/// the handoff; another Middle window, popup, or modal blocks the child.
pub(crate) fn resolve_batch_rename_confirmation_owner(
    ctx: &egui::Context,
    parent_instance: u64,
    confirmation_instance: u64,
    kind: BatchRenameConfirmationKind,
) -> NavigationOwner {
    let owner = NavigationOwner::BatchRenameConfirmation {
        parent_instance,
        confirmation_instance,
        kind,
    };
    let child_layer =
        batch_rename_confirmation_layer_id(parent_instance, confirmation_instance, kind);
    let parent_layer = NavigationDialogKind::BatchRename.layer_id();
    if ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && !egui::Popup::is_any_open(ctx)
        && visible_top_middle_layer(ctx) == Some(child_layer)
        && focused_response_layer(ctx).is_none_or(|focused| {
            focused == child_layer
                || Some(focused) == parent_layer
                || focused.order != egui::Order::Middle
        })
    {
        owner
    } else {
        NavigationOwner::Blocked
    }
}

/// Resolve a Search & Replace child only while its exact current confirmation
/// layer owns the focused ROOT frame. A retained response may still belong to
/// its parent during the handoff; other Middle windows and popups fail closed.
pub(crate) fn resolve_search_replace_confirmation_owner(
    ctx: &egui::Context,
    parent_instance: u64,
    confirmation_instance: u64,
    kind: SearchReplaceConfirmationKind,
    subject: SearchReplaceConfirmationSubject,
) -> NavigationOwner {
    let owner = NavigationOwner::SearchReplaceConfirmation {
        parent_instance,
        confirmation_instance,
        kind,
        subject,
    };
    let child_layer = kind.layer_id();
    let parent_layer = NavigationDialogKind::SearchReplace.layer_id();
    if ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && !egui::Popup::is_any_open(ctx)
        && visible_top_middle_layer(ctx) == Some(child_layer)
        && focused_response_layer(ctx).is_none_or(|focused| {
            focused == child_layer
                || Some(focused) == parent_layer
                || focused.order != egui::Order::Middle
        })
    {
        owner
    } else {
        NavigationOwner::Blocked
    }
}

pub(crate) fn mouse_gesture_binding_layer_id(
    parent_instance: u64,
    binding_instance: u64,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new((
            "mouse_gesture_binding_window",
            parent_instance,
            binding_instance,
        )),
    )
}

pub(crate) fn mouse_gesture_delete_layer_id(
    parent_instance: u64,
    confirmation_instance: u64,
) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new((
            "mouse_gesture_delete_confirmation",
            parent_instance,
            confirmation_instance,
        )),
    )
}

/// Resolve one transient Mouse Gestures child only while its real root Window
/// is the foreground owner. Retained focus may still belong to its exact
/// parent during the child handoff, but a different Middle window always
/// blocks the child.
pub(crate) fn resolve_mouse_gesture_child_navigation_owner(
    ctx: &egui::Context,
    owner: NavigationOwner,
    combo_popup: Option<&ComboPopupOwner>,
) -> NavigationOwner {
    let child_layer = match owner {
        NavigationOwner::MouseGestureBinding {
            parent_instance,
            binding_instance,
        } => mouse_gesture_binding_layer_id(parent_instance, binding_instance),
        NavigationOwner::MouseGestureDelete {
            parent_instance,
            confirmation_instance,
        } => mouse_gesture_delete_layer_id(parent_instance, confirmation_instance),
        _ => return NavigationOwner::Blocked,
    };
    let Some(parent_layer) = NavigationDialogKind::MouseGestures.layer_id() else {
        return NavigationOwner::Blocked;
    };
    let focused_layer = focused_response_layer(ctx);
    let base_is_current = ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && visible_top_middle_layer(ctx) == Some(child_layer)
        && focused_layer.is_none_or(|focused| {
            focused == child_layer
                || focused == parent_layer
                || focused.order != egui::Order::Middle
        });
    if !base_is_current {
        return NavigationOwner::Blocked;
    }

    if egui::Popup::is_any_open(ctx) {
        if combo_popup.is_some_and(|popup| {
            popup.parent_owner == owner
                && (popup.is_current(ctx, owner) || popup.is_sizing_pass(ctx, owner))
        }) {
            owner
        } else {
            NavigationOwner::Blocked
        }
    } else if combo_popup.is_some() {
        NavigationOwner::Blocked
    } else {
        owner
    }
}

/// Resolve the exact transient XML save confirmation while it is the live
/// foreground ROOT window. Unknown windows remain blocked by the ordinary
/// resolver.
pub(crate) fn resolve_xml_overwrite_confirmation_owner(
    ctx: &egui::Context,
    parent_instance: u64,
    confirmation_instance: u64,
) -> NavigationOwner {
    let owner = NavigationOwner::XmlOverwriteConfirmation {
        parent_instance,
        confirmation_instance,
    };
    let layer = xml_overwrite_confirmation_layer_id(parent_instance, confirmation_instance);
    if ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && !egui::Popup::is_any_open(ctx)
        && visible_top_middle_layer(ctx) == Some(layer)
        && focused_response_layer(ctx).is_none_or(|focused| {
            focused == layer
                || NavigationDialogKind::XmlToolkit.layer_id() == Some(focused)
                || focused.order != egui::Order::Middle
        })
    {
        owner
    } else {
        NavigationOwner::Blocked
    }
}

/// A snippet prompt is a transient owner rather than a member of the stable
/// dialog inventory. It owns navigation only while its current session's real
/// window is the exact visible top layer in the focused ROOT viewport.
pub(crate) fn resolve_snippet_prompt_navigation_owner(
    ctx: &egui::Context,
    generation: u64,
) -> NavigationOwner {
    if ctx.viewport_id() != egui::ViewportId::ROOT
        || !ctx.input(|input| input.focused)
        || ctx.memory(|memory| memory.top_modal_layer().is_some())
        || egui::Popup::is_any_open(ctx)
    {
        return NavigationOwner::Blocked;
    }

    let layer = snippet_prompt_layer_id(generation);
    let focused_layer = focused_response_layer(ctx);
    if visible_top_middle_layer(ctx) == Some(layer)
        && focused_layer
            .is_none_or(|focused| focused == layer || focused.order != egui::Order::Middle)
    {
        NavigationOwner::SnippetPrompt { generation }
    } else {
        NavigationOwner::Blocked
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NavigationWindows {
    pub(crate) settings: Option<u64>,
    pub(crate) notes: Option<u64>,
    pub(crate) todo: Option<u64>,
    pub(crate) timer: Option<u64>,
    pub(crate) paste_sequence: Option<u64>,
    pub(crate) mkmacro: Option<u64>,
    pub(crate) dialogs: [Option<u64>; NavigationDialogKind::ALL.len()],
}

impl NavigationWindows {
    fn any_open(self) -> bool {
        self.settings.is_some()
            || self.notes.is_some()
            || self.todo.is_some()
            || self.timer.is_some()
            || self.paste_sequence.is_some()
            || self.mkmacro.is_some()
            || self.dialogs.iter().any(Option::is_some)
    }

    pub(crate) fn dialog_instance(self, kind: NavigationDialogKind) -> Option<u64> {
        self.dialogs[kind.index()]
    }
}

/// Return the last visible top-level ordinary window using egui's public
/// layer order. Stale area IDs remain in the order list, so visibility is
/// checked against the previous completed pass.
pub(crate) fn visible_top_middle_layer(ctx: &egui::Context) -> Option<egui::LayerId> {
    ctx.memory(|memory| {
        memory
            .layer_ids()
            .filter(|layer| {
                layer.order == egui::Order::Middle
                    && memory.areas().parent_layer(*layer).is_none()
                    && memory.areas().visible_last_frame(layer)
            })
            .last()
    })
}

pub(crate) fn focused_response_layer(ctx: &egui::Context) -> Option<egui::LayerId> {
    ctx.memory(|memory| memory.focused())
        .and_then(|focused| ctx.read_response(focused))
        .map(|response| response.layer_id)
}

/// Resolve ownership from retained egui layer order and the focused response.
/// Root focus may remain in the background while Settings is raised; focus in
/// another ordinary window always fails closed.
pub(crate) fn resolve_layer_navigation_owner(
    ctx: &egui::Context,
    windows: NavigationWindows,
) -> NavigationOwner {
    if egui::Popup::is_any_open(ctx) {
        return NavigationOwner::Blocked;
    }
    resolve_layer_navigation_owner_parent(ctx, windows)
}

pub(crate) fn resolve_layer_navigation_owner_parent(
    ctx: &egui::Context,
    windows: NavigationWindows,
) -> NavigationOwner {
    if ctx.viewport_id() != egui::ViewportId::ROOT
        || !ctx.input(|input| input.focused)
        || ctx.memory(|memory| memory.top_modal_layer().is_some())
    {
        return NavigationOwner::Blocked;
    }

    let top_window = visible_top_middle_layer(ctx);
    let focused_layer = focused_response_layer(ctx);
    if let Some(layer) = top_window {
        let owner = if layer == settings_layer_id() {
            windows
                .settings
                .map(|instance| NavigationOwner::Settings { instance })
        } else if layer == notes_dialog_layer_id() {
            windows
                .notes
                .map(|instance| NavigationOwner::Notes { instance })
        } else if layer == todo_dialog_layer_id() {
            windows
                .todo
                .map(|instance| NavigationOwner::Todo { instance })
        } else if layer == timer_dialog_layer_id() || layer == alarm_dialog_layer_id() {
            windows
                .timer
                .map(|instance| NavigationOwner::Timer { instance })
        } else if layer == paste_sequence_dialog_layer_id() {
            windows
                .paste_sequence
                .map(|instance| NavigationOwner::PasteSequence { instance })
        } else if layer == mkmacro_dialog_layer_id() {
            windows
                .mkmacro
                .map(|instance| NavigationOwner::MkMacro { instance })
        } else if let Some(kind) = NavigationDialogKind::for_layer(layer) {
            windows
                .dialog_instance(kind)
                .map(|instance| NavigationOwner::Dialog { kind, instance })
        } else {
            None
        };
        return match owner {
            Some(owner)
                if focused_layer.is_none_or(|focused| {
                    focused == layer || focused.order != egui::Order::Middle
                }) =>
            {
                owner
            }
            _ => NavigationOwner::Blocked,
        };
    }

    if windows.any_open() || focused_layer.is_some_and(|layer| layer.order == egui::Order::Middle) {
        NavigationOwner::Blocked
    } else {
        NavigationOwner::Root
    }
}

pub(crate) fn resolve_layer_navigation_owner_with_combo_popup(
    ctx: &egui::Context,
    windows: NavigationWindows,
    combo_popup: Option<&ComboPopupOwner>,
) -> NavigationOwner {
    let parent_owner = resolve_layer_navigation_owner_parent(ctx, windows);
    if parent_owner == NavigationOwner::Blocked {
        return NavigationOwner::Blocked;
    }
    if egui::Popup::is_any_open(ctx) {
        if combo_popup.is_some_and(|popup| popup.is_current(ctx, parent_owner)) {
            parent_owner
        } else {
            NavigationOwner::Blocked
        }
    } else if combo_popup.is_some() {
        NavigationOwner::Blocked
    } else {
        parent_owner
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComboPopupOwner {
    parent_owner: NavigationOwner,
    anchor_id: TargetId,
    viewport: egui::ViewportId,
    binding: Option<NavigationViewportBinding>,
    response_id: egui::Id,
    anchor_layer: egui::LayerId,
    popup_layer: egui::LayerId,
    popup_surface: SurfaceId,
}

impl ComboPopupOwner {
    fn parent_and_focus_are_current(
        &self,
        ctx: &egui::Context,
        parent_owner: NavigationOwner,
    ) -> bool {
        let anchor_is_current = match self.binding {
            Some(binding) => {
                binding.owner == parent_owner
                    && binding.viewport == self.viewport
                    && binding.base_is_current(ctx)
                    && self.anchor_layer == binding.base_layer
            }
            None => {
                self.viewport == egui::ViewportId::ROOT
                    && combo_anchor_layer_is_current(
                        ctx,
                        parent_owner,
                        self.anchor_id.surface(),
                        self.anchor_layer,
                    )
            }
        };
        self.parent_owner == parent_owner
            && parent_owner.owns_surface(self.anchor_id.surface())
            && egui::ComboBox::is_open(ctx, self.response_id)
            && egui::Popup::is_any_open(ctx)
            && anchor_is_current
            && focused_response_layer(ctx)
                .is_none_or(|layer| layer == self.anchor_layer || layer == self.popup_layer)
    }

    /// The first visible ComboBox popup pass is deliberately non-interactive
    /// while egui measures its contents. Preserve the exact open anchor across
    /// that pass so the next rendered pass can verify the popup layer before
    /// taking ownership; this is provenance only, never an eligible target.
    fn is_sizing_pass(&self, ctx: &egui::Context, parent_owner: NavigationOwner) -> bool {
        if !self.parent_and_focus_are_current(ctx, parent_owner) {
            return false;
        }

        let popup_id = self.response_id.with("popup");
        let Some(rect) = ctx.memory(|memory| memory.area_rect(popup_id)) else {
            return false;
        };
        rect.width() > 0.0
            && rect.height() > 0.0
            && egui::containers::AreaState::load(ctx, popup_id)
                .is_some_and(|state| !state.interactable)
    }

    fn is_current(&self, ctx: &egui::Context, parent_owner: NavigationOwner) -> bool {
        if !self.parent_and_focus_are_current(ctx, parent_owner) {
            return false;
        }

        let popup_id = self.response_id.with("popup");
        let Some(rect) = ctx.memory(|memory| memory.area_rect(popup_id)) else {
            return false;
        };
        rect.width() > 0.0
            && rect.height() > 0.0
            && ctx.layer_id_at(rect.center()) == Some(self.popup_layer)
    }
}

fn combo_popup_surface(anchor_id: &TargetId) -> SurfaceId {
    let instance = match anchor_id.surface() {
        SurfaceId::Settings { instance }
        | SurfaceId::Dialog { instance, .. }
        | SurfaceId::Popup { instance, .. } => *instance,
        SurfaceId::Dashboard | SurfaceId::LauncherResults => anchor_id.generation().0,
    };
    let semantic = match anchor_id.semantic() {
        SemanticTargetId::Named(value) => value.clone(),
        SemanticTargetId::RootResult(index) => format!("root-result:{index}"),
    };
    SurfaceId::popup(
        format!("combo:{semantic}"),
        instance,
        anchor_id.surface().clone(),
    )
}

fn combo_anchor_layer_is_current(
    ctx: &egui::Context,
    parent_owner: NavigationOwner,
    anchor_surface: &SurfaceId,
    anchor_layer: egui::LayerId,
) -> bool {
    match parent_owner {
        NavigationOwner::Root => {
            matches!(
                anchor_surface,
                SurfaceId::Dashboard | SurfaceId::LauncherResults
            ) && anchor_layer.order == egui::Order::Background
                && visible_top_middle_layer(ctx).is_none()
        }
        _ => visible_top_middle_layer(ctx) == Some(anchor_layer),
    }
}

impl FocusDirective {
    pub(crate) fn apply_to_response(self, response: &egui::Response) {
        let Self::Focus(kind) = self else {
            return;
        };
        response.request_focus();
        let event_filter = match kind {
            // Native editors use arrows for caret movement or value stepping.
            // Seed that lock with the one-shot focus transfer so egui does not
            // treat the first post-hint arrow as focus navigation.
            TargetKind::TextEntry => Some(egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            }),
            // The widget installs its orientation-specific filter once it has
            // rendered with focus. Until then, preserve the first native arrow
            // on either axis for an orientation-agnostic Slider target.
            TargetKind::Slider => Some(egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            }),
            _ => None,
        };
        if let Some(event_filter) = event_filter {
            response.ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(response.id, event_filter);
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NotesQueryBridgeProvenance {
    pub(crate) notes_instance: u64,
    pub(crate) notes_generation: TargetGeneration,
    pub(crate) root_response_id: egui::Id,
    pub(crate) root_layer_id: egui::LayerId,
    pub(crate) root_viewport: egui::ViewportId,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TargetProvenance {
    NotesQueryBridge(NotesQueryBridgeProvenance),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegisteredTarget {
    metadata: TargetMetadata,
    viewport: egui::ViewportId,
    response_id: egui::Id,
    layer_id: egui::LayerId,
    rect: egui::Rect,
    clipped_rect: egui::Rect,
    ui_clip_rect: egui::Rect,
    paint_label_rect: Option<egui::Rect>,
    enabled: bool,
    eligible: bool,
    epoch: NavigationEpoch,
    provenance: Option<TargetProvenance>,
}

impl RegisteredTarget {
    pub(crate) fn metadata(&self) -> &TargetMetadata {
        &self.metadata
    }

    pub(crate) fn id(&self) -> &TargetId {
        self.metadata.id()
    }

    pub(crate) fn viewport(&self) -> egui::ViewportId {
        self.viewport
    }

    pub(crate) fn response_id(&self) -> egui::Id {
        self.response_id
    }

    pub(crate) fn layer_id(&self) -> egui::LayerId {
        self.layer_id
    }

    pub(crate) fn rect(&self) -> egui::Rect {
        self.rect
    }

    pub(crate) fn clipped_rect(&self) -> egui::Rect {
        self.clipped_rect
    }

    pub(crate) fn ui_clip_rect(&self) -> egui::Rect {
        self.ui_clip_rect
    }

    pub(crate) fn paint_label_rect(&self) -> Option<egui::Rect> {
        self.paint_label_rect
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn eligible(&self) -> bool {
        self.eligible
    }

    pub(crate) fn navigation_epoch(&self) -> NavigationEpoch {
        self.epoch
    }

    pub(crate) fn notes_query_bridge_provenance(&self) -> Option<&NotesQueryBridgeProvenance> {
        match self.provenance.as_ref() {
            Some(TargetProvenance::NotesQueryBridge(provenance)) => Some(provenance),
            None => None,
        }
    }
}

/// An immutable snapshot of the eligible targets from one completed UI pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TargetRegistry {
    targets: Vec<RegisteredTarget>,
    // Combo anchors are retained separately when the popup becomes the active
    // surface, so the next frame can prove which native popup owns navigation.
    combo_anchors: Vec<RegisteredTarget>,
}

impl TargetRegistry {
    pub(crate) fn targets(&self) -> &[RegisteredTarget] {
        &self.targets
    }

    pub(crate) fn metadata(&self) -> impl Iterator<Item = &TargetMetadata> {
        self.targets.iter().map(RegisteredTarget::metadata)
    }

    pub(crate) fn get(&self, id: &TargetId) -> Option<&RegisteredTarget> {
        self.targets.iter().find(|target| target.id() == id)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.targets.len()
    }

    fn prefer_active_surface(&mut self) {
        self.combo_anchors = self
            .targets
            .iter()
            .filter(|target| target.metadata.kind() == TargetKind::ComboBox)
            .cloned()
            .collect();
        let Some(priority) = self
            .targets
            .iter()
            .map(|target| surface_priority(target.id().surface()))
            .max()
        else {
            return;
        };

        // Dashboard and launcher results share the root owner, so both may be
        // collected in one frame. Other unrelated owners at the same depth are
        // not safely orderable here. A nested popup is deeper than its parent,
        // so its exact child controls can take precedence while sibling
        // popups still fail closed.
        let active_non_root_surfaces: HashSet<_> = self
            .targets
            .iter()
            .filter(|target| {
                surface_priority(target.id().surface()) == priority
                    && !is_root_surface(target.id().surface())
            })
            .map(|target| target.id().surface())
            .collect();
        if active_non_root_surfaces.len() > 1 {
            self.targets.clear();
            return;
        }

        self.targets
            .retain(|target| surface_priority(target.id().surface()) == priority);
    }

    fn revalidate(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        notes_query_bridge_allowed: bool,
        active_combo_popup: Option<&ComboPopupOwner>,
        viewport_binding: Option<NavigationViewportBinding>,
    ) {
        let viewport_is_current = match viewport_binding {
            Some(binding) => {
                binding.base_is_current(ctx)
                    && (navigation_owner == NavigationOwner::Blocked
                        || binding.owner == navigation_owner)
            }
            None => ctx.input(|input| input.focused) && ctx.viewport_id() == egui::ViewportId::ROOT,
        };
        let expected_viewport = viewport_binding
            .map(|binding| binding.viewport)
            .unwrap_or(egui::ViewportId::ROOT);
        let popup_open = egui::Popup::is_any_open(ctx);
        self.targets.retain(|target| {
            let root_surface = is_root_surface(target.id().surface());
            let popup_target = is_popup_surface(target.id().surface());
            let surface_owned = navigation_owner.owns_surface(target.id().surface());
            let navigation_available = navigation_owner != NavigationOwner::Blocked;
            let provenance_is_current = match target.notes_query_bridge_provenance() {
                Some(provenance) => {
                    notes_query_bridge_allowed
                        && navigation_owner
                            == (NavigationOwner::Notes {
                                instance: provenance.notes_instance,
                            })
                        && target.id().surface()
                            == &SurfaceId::dialog("notes", provenance.notes_instance)
                        && target.id().generation() == provenance.notes_generation
                        && target.response_id() == provenance.root_response_id
                        && target.layer_id() == provenance.root_layer_id
                        && target.viewport() == egui::ViewportId::ROOT
                        && provenance.root_viewport == egui::ViewportId::ROOT
                }
                None => true,
            };
            let popup_surface_owned = active_combo_popup
                .as_ref()
                .is_some_and(|popup| popup.popup_surface == *target.id().surface());
            viewport_is_current
                && target.viewport == expected_viewport
                && target.eligible
                && target.enabled
                && navigation_available
                && surface_owned
                && provenance_is_current
                && ctx.memory(|memory| memory.allows_interaction(target.layer_id))
                && (!root_surface
                    || (navigation_owner == NavigationOwner::Root
                        && root_targets_allowed
                        && !popup_open))
                && if popup_open {
                    popup_target && popup_surface_owned
                } else {
                    // Popup surfaces are also used by existing inline child
                    // confirmations. Native ComboBox children are only
                    // admitted while their exact egui popup is open.
                    true
                }
        });
        self.combo_anchors.retain(|target| {
            viewport_is_current
                && target.viewport == expected_viewport
                && target.eligible
                && target.enabled
                && navigation_owner != NavigationOwner::Blocked
                && navigation_owner.owns_surface(target.id().surface())
                && ctx.memory(|memory| memory.allows_interaction(target.layer_id))
        });
    }

    fn open_combo_popup(
        &self,
        ctx: &egui::Context,
        parent_owner: NavigationOwner,
    ) -> Result<Option<ComboPopupOwner>, ()> {
        self.open_combo_popup_for_binding(ctx, parent_owner, None)
    }

    fn open_combo_popup_for_binding(
        &self,
        ctx: &egui::Context,
        parent_owner: NavigationOwner,
        binding: Option<NavigationViewportBinding>,
    ) -> Result<Option<ComboPopupOwner>, ()> {
        if !egui::Popup::is_any_open(ctx) {
            return Ok(None);
        }

        if let Some(binding) = binding
            && (!binding.base_is_current(ctx) || binding.owner != parent_owner)
        {
            return Err(());
        }

        let mut open_anchor = None;
        for anchor in &self.combo_anchors {
            if anchor.viewport != binding.map_or(egui::ViewportId::ROOT, |binding| binding.viewport)
                || !anchor.enabled
                || !anchor.eligible
                || !parent_owner.owns_surface(anchor.id().surface())
                || !egui::ComboBox::is_open(ctx, anchor.response_id)
            {
                continue;
            }
            if open_anchor.is_some() {
                return Err(());
            }
            open_anchor = Some(anchor);
        }

        let Some(anchor) = open_anchor else {
            return Err(());
        };
        let popup_id = anchor.response_id.with("popup");
        let owner = ComboPopupOwner {
            parent_owner,
            anchor_id: anchor.id().clone(),
            viewport: anchor.viewport,
            binding,
            response_id: anchor.response_id,
            anchor_layer: anchor.layer_id,
            popup_layer: egui::LayerId::new(egui::Order::Foreground, popup_id),
            popup_surface: combo_popup_surface(anchor.id()),
        };
        let anchor_is_current = match binding {
            Some(binding) => binding.base_is_current(ctx) && anchor.layer_id == binding.base_layer,
            None => combo_anchor_layer_is_current(
                ctx,
                parent_owner,
                anchor.id().surface(),
                anchor.layer_id,
            ),
        };
        if !anchor_is_current
            || !(owner.is_current(ctx, parent_owner) || owner.is_sizing_pass(ctx, parent_owner))
        {
            return Err(());
        }
        Ok(Some(owner))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusIntent {
    target: TargetId,
    viewport: egui::ViewportId,
    epoch: NavigationEpoch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConsumedFocusIntent {
    intent: FocusIntent,
    /// Native response used to validate the semantic target across passes.
    target_response_id: egui::Id,
    /// Some Select targets move focus to a different native widget they own.
    native_focus_owner_id: Option<egui::Id>,
}

impl ConsumedFocusIntent {
    fn focus_owner_id(&self) -> egui::Id {
        self.native_focus_owner_id
            .unwrap_or(self.target_response_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationRejection {
    Disabled,
    NonInteractive,
    EmptyOrClipped,
    BlockedLayer,
    WrongViewport,
    UnfocusedViewport,
    InactiveOwner,
    IdentityOutsideScope,
    DuplicateIdentity,
    FinishedPass,
    CollectorUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationResult {
    Registered {
        id: TargetId,
        directive: Option<FocusDirective>,
    },
    Rejected(RegistrationRejection),
}

#[derive(Default)]
struct PassCollector {
    registry: TargetRegistry,
    duplicate_ids: HashSet<TargetId>,
    pending_focus_intent: Option<FocusIntent>,
    consumed_focus_intent: Option<ConsumedFocusIntent>,
    closed: bool,
}

#[derive(Default)]
struct RootInputFrame {
    viewport: egui::ViewportId,
    owner_blocked: bool,
    resolved: bool,
    capture_text_remainder: bool,
    captured_events: Vec<egui::Event>,
}

/// A per-surface adapter. Cloning a scope keeps only a handle to this frame's
/// collector and its value-only surface namespace.
#[derive(Clone)]
pub struct JumpModeScope {
    collector: Arc<Mutex<PassCollector>>,
    surface: SurfaceId,
    generation: TargetGeneration,
    identity_namespace: String,
    viewport: egui::ViewportId,
    viewport_binding: Option<NavigationViewportBinding>,
    epoch: NavigationEpoch,
    root_owner_allowed: bool,
    navigation_owner: Option<NavigationOwner>,
}

impl JumpModeScope {
    pub(crate) fn child_namespace(&self, child: impl AsRef<str>) -> Self {
        let child = child.as_ref();
        let mut identity_namespace = self.identity_namespace.clone();
        identity_namespace.push('/');
        identity_namespace.push_str(&child.len().to_string());
        identity_namespace.push(':');
        identity_namespace.push_str(child);
        Self {
            identity_namespace,
            ..self.clone()
        }
    }

    pub(crate) fn with_surface(&self, surface: SurfaceId, generation: TargetGeneration) -> Self {
        Self {
            surface,
            generation,
            ..self.clone()
        }
    }

    pub(crate) fn child_popup_scope(&self, kind: impl Into<String>) -> Self {
        let instance = match &self.surface {
            SurfaceId::Settings { instance }
            | SurfaceId::Dialog { instance, .. }
            | SurfaceId::Popup { instance, .. } => *instance,
            SurfaceId::Dashboard | SurfaceId::LauncherResults => self.generation.0,
        };
        self.with_surface(
            SurfaceId::popup(kind, instance, self.surface.clone()),
            self.generation,
        )
    }

    pub(crate) fn combo_popup_scope(&self, anchor_identity: impl AsRef<str>) -> Self {
        let anchor_id = self.target_id(anchor_identity);
        self.with_surface(combo_popup_surface(&anchor_id), self.generation)
    }

    pub(crate) fn target_id(&self, local_identity: impl AsRef<str>) -> TargetId {
        let mut semantic = self.identity_namespace.clone();
        semantic.push('/');
        let local_identity = local_identity.as_ref();
        semantic.push_str(&local_identity.len().to_string());
        semantic.push(':');
        semantic.push_str(local_identity);
        TargetId::new(
            self.surface.clone(),
            SemanticTargetId::named(semantic),
            self.generation,
        )
    }

    /// Record the native widget that owns focus after this Action target's
    /// owner applies its Select directive. The semantic row and its response
    /// remain the handoff's identity and continuity check.
    pub(crate) fn record_select_focus_owner(
        &self,
        target_id: &TargetId,
        focus_owner_id: egui::Id,
    ) -> bool {
        if target_id.surface() != &self.surface || target_id.generation() != self.generation {
            return false;
        }
        let Ok(mut collector) = self.collector.lock() else {
            return false;
        };
        let registered_action = collector.registry.targets().iter().any(|target| {
            target.id() == target_id && target.metadata().kind() == TargetKind::Action
        });
        let Some(consumed) = collector.consumed_focus_intent.as_mut() else {
            return false;
        };
        if !registered_action || consumed.intent.target != *target_id {
            return false;
        }
        consumed.native_focus_owner_id = Some(focus_owner_id);
        true
    }

    /// Register a visible control using its real egui response and current UI
    /// clip. The response itself is never stored in the registry.
    pub(crate) fn register(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        local_identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
    ) -> RegistrationResult {
        let metadata =
            TargetMetadata::new(self.target_id(local_identity), label, kind, visual_order);
        self.register_metadata_with_paint_label_rect(ui, response, metadata, None, None)
    }

    /// Register the one real ROOT query response on the exact Notes surface.
    /// The target remains owned by Notes so it cannot unlock Dashboard or
    /// launcher-result navigation while Notes is foreground.
    pub(crate) fn register_notes_query_bridge(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        notes_instance: u64,
        notes_generation: TargetGeneration,
    ) -> RegistrationResult {
        let expected_owner = NavigationOwner::Notes {
            instance: notes_instance,
        };
        if self.navigation_owner != Some(expected_owner)
            || self.surface != SurfaceId::dialog("notes", notes_instance)
            || self.generation != notes_generation
            || self.viewport != egui::ViewportId::ROOT
            || response.id != egui::Id::new("query_input")
        {
            return RegistrationResult::Rejected(RegistrationRejection::IdentityOutsideScope);
        }

        let metadata = TargetMetadata::new(
            self.target_id("query-focus-bridge"),
            "Search query",
            TargetKind::TextEntry,
            0,
        );
        self.register_metadata_with_paint_label_rect(
            ui,
            response,
            metadata,
            None,
            Some(TargetProvenance::NotesQueryBridge(
                NotesQueryBridgeProvenance {
                    notes_instance,
                    notes_generation,
                    root_response_id: response.id,
                    root_layer_id: response.layer_id,
                    root_viewport: response.ctx.viewport_id(),
                },
            )),
        )
    }

    /// Register a target with the actual text rect painted beside its native
    /// response. This geometry is copied into the current-pass registry only
    /// for non-interactive badge placement.
    pub(crate) fn register_with_paint_label_rect(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        local_identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
        paint_label_rect: egui::Rect,
    ) -> RegistrationResult {
        let metadata =
            TargetMetadata::new(self.target_id(local_identity), label, kind, visual_order);
        self.register_metadata_with_paint_label_rect(
            ui,
            response,
            metadata,
            Some(paint_label_rect),
            None,
        )
    }

    pub(crate) fn register_metadata(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        metadata: TargetMetadata,
    ) -> RegistrationResult {
        self.register_metadata_with_paint_label_rect(ui, response, metadata, None, None)
    }

    fn register_metadata_with_paint_label_rect(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        metadata: TargetMetadata,
        paint_label_rect: Option<egui::Rect>,
        provenance: Option<TargetProvenance>,
    ) -> RegistrationResult {
        if metadata.id().surface() != &self.surface || metadata.id().generation() != self.generation
        {
            return RegistrationResult::Rejected(RegistrationRejection::IdentityOutsideScope);
        }

        let ctx = ui.ctx();
        let viewport_is_current = match self.viewport_binding {
            Some(binding) => {
                binding.viewport == self.viewport
                    && self.navigation_owner == Some(binding.owner)
                    && binding.base_is_current(ctx)
            }
            None => self.viewport == egui::ViewportId::ROOT,
        };
        if !viewport_is_current
            || ctx.viewport_id() != self.viewport
            || response.ctx.viewport_id() != self.viewport
        {
            return RegistrationResult::Rejected(RegistrationRejection::WrongViewport);
        }
        if !ctx.input(|input| input.focused) {
            return RegistrationResult::Rejected(RegistrationRejection::UnfocusedViewport);
        }
        if is_root_surface(&self.surface)
            && (!self.root_owner_allowed || egui::Popup::is_any_open(ctx))
        {
            return RegistrationResult::Rejected(RegistrationRejection::InactiveOwner);
        }
        if let Some(owner) = self.navigation_owner
            && !owner.owns_surface(&self.surface)
        {
            return RegistrationResult::Rejected(RegistrationRejection::InactiveOwner);
        }
        if !ctx.memory(|memory| memory.allows_interaction(response.layer_id)) {
            return RegistrationResult::Rejected(RegistrationRejection::BlockedLayer);
        }
        if !ui.is_enabled() || !response.enabled() {
            return RegistrationResult::Rejected(RegistrationRejection::Disabled);
        }
        if !response.sense.interactive() && !response.sense.is_focusable() {
            return RegistrationResult::Rejected(RegistrationRejection::NonInteractive);
        }

        let rect = response.rect;
        let ui_clip_rect = ui.clip_rect();
        let clipped_rect = rect.intersect(ui_clip_rect);
        if !rect_is_usable(rect) || !rect_is_usable(clipped_rect) {
            return RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped);
        }
        let target = RegisteredTarget {
            metadata,
            viewport: self.viewport,
            response_id: response.id,
            layer_id: response.layer_id,
            rect,
            clipped_rect,
            ui_clip_rect,
            paint_label_rect: paint_label_rect
                .map(|label_rect| label_rect.intersect(ui_clip_rect))
                .filter(|label_rect| rect_is_usable(*label_rect)),
            enabled: true,
            eligible: true,
            epoch: self.epoch,
            provenance,
        };

        let Ok(mut collector) = self.collector.lock() else {
            return RegistrationResult::Rejected(RegistrationRejection::CollectorUnavailable);
        };
        if collector.closed {
            return RegistrationResult::Rejected(RegistrationRejection::FinishedPass);
        }
        let id = target.id().clone();
        if collector.duplicate_ids.contains(&id)
            || collector
                .registry
                .targets
                .iter()
                .any(|existing| existing.id() == &id)
        {
            collector
                .registry
                .targets
                .retain(|existing| existing.id() != &id);
            collector.duplicate_ids.insert(id);
            return RegistrationResult::Rejected(RegistrationRejection::DuplicateIdentity);
        }

        let directive = match collector.pending_focus_intent.clone() {
            Some(intent)
                if intent.target == id
                    && intent.viewport == target.viewport
                    && intent.epoch == target.epoch =>
            {
                collector.pending_focus_intent = None;
                collector.consumed_focus_intent = Some(ConsumedFocusIntent {
                    intent,
                    target_response_id: target.response_id,
                    native_focus_owner_id: None,
                });
                Some(if target.metadata.kind() == TargetKind::Action {
                    FocusDirective::Select
                } else {
                    FocusDirective::Focus(target.metadata.kind())
                })
            }
            _ => None,
        };
        collector.registry.targets.push(target);

        RegistrationResult::Registered { id, directive }
    }

    /// Remove this child surface's targets when it closes during the current
    /// UI pass. A consumed focus intent is kept so frame completion can
    /// surrender focus after it observes that the target is no longer present.
    pub(crate) fn discard_registered_targets(&self) {
        let Ok(mut collector) = self.collector.lock() else {
            return;
        };
        collector
            .registry
            .targets
            .retain(|target| target.id().surface() != &self.surface);
        if collector
            .pending_focus_intent
            .as_ref()
            .is_some_and(|intent| intent.target.surface() == &self.surface)
        {
            collector.pending_focus_intent = None;
        }
    }

    /// Remove only this namespace's targets and pending focus intent from the
    /// current pass. Dialog form controls can therefore keep a stable native
    /// focus surface while document-dependent child targets are invalidated.
    pub(crate) fn discard_registered_namespace(&self) {
        let Ok(mut collector) = self.collector.lock() else {
            return;
        };
        if collector.closed {
            return;
        }
        let namespace_prefix = format!("{}/", self.identity_namespace);
        let belongs_to_namespace = |id: &TargetId| {
            id.surface() == &self.surface
                && id.generation() == self.generation
                && matches!(
                    id.semantic(),
                    super::SemanticTargetId::Named(identity)
                        if identity.starts_with(&namespace_prefix)
                )
        };
        collector
            .registry
            .targets
            .retain(|target| !belongs_to_namespace(target.id()));
        if collector
            .pending_focus_intent
            .as_ref()
            .is_some_and(|intent| belongs_to_namespace(&intent.target))
        {
            collector.pending_focus_intent = None;
        }
    }
}

/// Register a current-pass response through an optional Jump Mode scope and
/// apply only focus directives. A `Select` directive is returned to the owning
/// renderer so it can update the surface's real selection and focus owner.
///
/// `local_identity` should be a static control path or stable entity identity;
/// the scope binds it to the current surface instance and content generation.
/// Nested confirmations must use a scope bound to that confirmation instance.
/// Never put entered text, clipboard contents, secrets, or other payloads in an
/// identity. Visual order advances on every rendered call, including when the
/// optional scope is absent, matching the existing dialog adapter contract.
pub(crate) fn register_and_apply_target(
    scope: Option<&JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: TargetKind,
    visual_order: &mut u64,
) -> Option<FocusDirective> {
    let order = *visual_order;
    *visual_order = visual_order.saturating_add(1);
    let scope = scope?;
    let RegistrationResult::Registered { directive, .. } =
        scope.register(ui, response, local_identity, label, kind, order)
    else {
        return None;
    };

    if let Some(directive @ FocusDirective::Focus(_)) = directive {
        directive.apply_to_response(response);
        Some(directive)
    } else {
        directive
    }
}

/// App-instance-owned coordinator. It separates the in-progress pass, the
/// completed pass, and the last fully completed frame snapshot.
#[derive(Default)]
pub(crate) struct JumpModeUi {
    enabled: bool,
    navigation_epoch: NavigationEpoch,
    navigation_owner: NavigationOwner,
    frame_navigation_owner: NavigationOwner,
    navigation_owner_transitioned: bool,
    active_combo_popup: Option<ComboPopupOwner>,
    settings_open: bool,
    settings_instance: u64,
    notes_open: bool,
    notes_instance: u64,
    todo_open: bool,
    todo_instance: u64,
    mkmacro_open: bool,
    mkmacro_instance: u64,
    timer_window_title: Option<&'static str>,
    timer_instance: u64,
    paste_sequence_open: bool,
    paste_sequence_instance: u64,
    observed_dialogs: [ObservedNavigationDialog; NavigationDialogKind::ALL.len()],
    active_session: Option<HintSession>,
    last_outcome: Option<MatchOutcome>,
    root_input_frame: RootInputFrame,
    held_captured_keys: HashSet<egui::Key>,
    // Keys captured while retiring an invalid hint remain fenced across owner
    // transitions until their physical release is observed.
    invalidated_session_key_fence: HashSet<egui::Key>,
    pending_focus_intent: Option<FocusIntent>,
    collector: Option<Arc<Mutex<PassCollector>>>,
    pass_registry: Option<TargetRegistry>,
    retained_registry: TargetRegistry,
    frame_consumed_focus_intent: Option<ConsumedFocusIntent>,
    last_completed_focus_handoff: Option<ConsumedFocusIntent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ObservedNavigationDialog {
    open: bool,
    instance: u64,
}

impl JumpModeUi {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            if !enabled {
                self.clear();
            }
            return;
        }
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.collector = None;
        self.pass_registry = None;
        self.frame_consumed_focus_intent = None;
        self.root_input_frame = RootInputFrame::default();
        self.frame_navigation_owner = self.navigation_owner;
        self.navigation_owner_transitioned = false;
        if !self.enabled {
            self.clear();
        }
    }

    pub(crate) fn observe_settings_visibility(&mut self, open: bool) -> bool {
        if self.settings_open == open {
            return false;
        }
        self.settings_open = open;
        if open {
            self.settings_instance = self.settings_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn settings_instance(&self) -> u64 {
        self.settings_instance
    }

    pub(crate) fn observe_notes_visibility(&mut self, open: bool) -> bool {
        if self.notes_open == open {
            return false;
        }
        self.notes_open = open;
        if open {
            self.notes_instance = self.notes_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn notes_instance(&self) -> u64 {
        self.notes_instance
    }

    pub(crate) fn observe_todo_visibility(&mut self, open: bool) -> bool {
        if self.todo_open == open {
            return false;
        }
        self.todo_open = open;
        if open {
            self.todo_instance = self.todo_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn todo_instance(&self) -> u64 {
        self.todo_instance
    }

    pub(crate) fn observe_mkmacro_visibility(&mut self, open: bool) -> bool {
        if self.mkmacro_open == open {
            return false;
        }
        self.mkmacro_open = open;
        if open {
            self.mkmacro_instance = self.mkmacro_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn mkmacro_instance(&self) -> u64 {
        self.mkmacro_instance
    }

    pub(crate) fn observe_timer_window(&mut self, title: Option<&'static str>) -> bool {
        if self.timer_window_title == title {
            return false;
        }
        self.timer_window_title = title;
        if title.is_some() {
            self.timer_instance = self.timer_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn timer_instance(&self) -> u64 {
        self.timer_instance
    }

    pub(crate) fn observe_paste_sequence_visibility(&mut self, open: bool) -> bool {
        if self.paste_sequence_open == open {
            return false;
        }
        self.paste_sequence_open = open;
        if open {
            self.paste_sequence_instance = self.paste_sequence_instance.wrapping_add(1).max(1);
        }
        true
    }

    pub(crate) fn paste_sequence_instance(&self) -> u64 {
        self.paste_sequence_instance
    }

    /// Observe the actual open state of each newly supported dialog from its
    /// owning application state. Reordering windows does not affect these
    /// per-open instances; closing and reopening a dialog always advances it.
    pub(crate) fn observe_dialog_visibility(
        &mut self,
        dialogs: impl IntoIterator<Item = (NavigationDialogKind, bool)>,
    ) -> bool {
        let mut changed = false;
        for (kind, open) in dialogs {
            let observed = &mut self.observed_dialogs[kind.index()];
            if observed.open != open {
                observed.open = open;
                if open {
                    observed.instance = observed.instance.wrapping_add(1).max(1);
                }
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn dialog_instance(&self, kind: NavigationDialogKind) -> Option<u64> {
        let observed = self.observed_dialogs[kind.index()];
        observed.open.then_some(observed.instance)
    }

    /// Apply the host's current owner and supported-popup decision. Owner,
    /// dialog-visibility, or popup transitions cancel stale navigation and
    /// block this complete egui frame, including all discarded passes.
    pub(crate) fn update_navigation_owner(
        &mut self,
        ctx: &egui::Context,
        candidate: NavigationOwner,
        supported_dialog_visibility_changed: bool,
    ) -> NavigationOwner {
        self.update_navigation_owner_with_combo_popup(
            ctx,
            candidate,
            supported_dialog_visibility_changed,
            None,
        )
    }

    pub(crate) fn update_navigation_owner_with_combo_popup(
        &mut self,
        ctx: &egui::Context,
        candidate: NavigationOwner,
        supported_dialog_visibility_changed: bool,
        combo_popup: Option<ComboPopupOwner>,
    ) -> NavigationOwner {
        self.update_navigation_owner_with_combo_popup_internal(
            ctx,
            candidate,
            supported_dialog_visibility_changed,
            combo_popup,
            true,
            false,
        )
    }

    pub(crate) fn update_navigation_owner_with_combo_popup_for_query_focus(
        &mut self,
        ctx: &egui::Context,
        candidate: NavigationOwner,
        supported_dialog_visibility_changed: bool,
        combo_popup: Option<ComboPopupOwner>,
        preserve_query_focus: bool,
    ) -> NavigationOwner {
        self.update_navigation_owner_with_combo_popup_internal(
            ctx,
            candidate,
            supported_dialog_visibility_changed,
            combo_popup,
            true,
            preserve_query_focus,
        )
    }

    pub(crate) fn update_navigation_owner_for_viewport(
        &mut self,
        ctx: &egui::Context,
        binding: NavigationViewportBinding,
        application_input_allowed: bool,
    ) -> NavigationOwner {
        if ctx.viewport_id() != binding.viewport {
            self.quarantine_mismatched_viewport();
            return NavigationOwner::Blocked;
        }
        let popup = self.resolve_combo_popup_owner_for_viewport(ctx, binding);
        let candidate = if application_input_allowed {
            resolve_bound_viewport_navigation_owner(ctx, binding, popup.as_ref())
        } else {
            NavigationOwner::Blocked
        };
        self.update_navigation_owner_with_combo_popup_internal(
            ctx, candidate, false, popup, false, false,
        )
    }

    fn surrender_focus_unless_show_query(
        ctx: &egui::Context,
        focus_id: egui::Id,
        preserve_query_focus: bool,
    ) {
        if preserve_query_focus && focus_id == egui::Id::new("query_input") {
            return;
        }
        ctx.memory_mut(|memory| memory.surrender_focus(focus_id));
    }

    fn update_navigation_owner_with_combo_popup_internal(
        &mut self,
        ctx: &egui::Context,
        candidate: NavigationOwner,
        supported_dialog_visibility_changed: bool,
        combo_popup: Option<ComboPopupOwner>,
        root_owner: bool,
        preserve_query_focus: bool,
    ) -> NavigationOwner {
        let owner_changed =
            supported_dialog_visibility_changed || candidate != self.navigation_owner;
        let popup_changed = combo_popup != self.active_combo_popup;
        if owner_changed || popup_changed {
            if self.enabled {
                if root_owner
                    && owner_changed
                    && (candidate.is_supported()
                        || (supported_dialog_visibility_changed
                            && (self.settings_open
                                || self.notes_open
                                || self.todo_open
                                || self.timer_window_title.is_some()
                                || self.paste_sequence_open
                                || self.mkmacro_open
                                || self.observed_dialogs.iter().any(|dialog| dialog.open))))
                    && !preserve_query_focus
                {
                    // A supported native dialog deliberately takes ownership
                    // from the launcher query. `surrender_focus` is conditional
                    // on this exact ID still owning focus.
                    ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("query_input")));
                }
                if owner_changed && let Some(focus) = self.last_completed_focus_handoff.as_ref() {
                    Self::surrender_focus_unless_show_query(
                        ctx,
                        focus.focus_owner_id(),
                        preserve_query_focus,
                    );
                }
                if owner_changed && let Some(focus) = self.frame_consumed_focus_intent.as_ref() {
                    Self::surrender_focus_unless_show_query(
                        ctx,
                        focus.focus_owner_id(),
                        preserve_query_focus,
                    );
                }
            }
            self.cancel_all();
            self.last_completed_focus_handoff = None;
            self.navigation_owner = candidate;
            self.active_combo_popup = combo_popup;
            self.navigation_owner_transitioned = true;
            self.root_input_frame.owner_blocked = true;
            self.held_captured_keys.clear();
        }
        self.frame_navigation_owner = if self.navigation_owner_transitioned {
            NavigationOwner::Blocked
        } else {
            self.navigation_owner
        };
        self.frame_navigation_owner
    }

    pub(crate) fn combo_popup_transition_pending(
        &self,
        candidate: Option<&ComboPopupOwner>,
    ) -> bool {
        self.active_combo_popup.as_ref() != candidate
    }

    pub(crate) fn resolve_combo_popup_owner(
        &self,
        ctx: &egui::Context,
        parent_owner: NavigationOwner,
    ) -> Option<ComboPopupOwner> {
        if let Some(active) = self.active_combo_popup.as_ref()
            && (active.is_current(ctx, parent_owner) || active.is_sizing_pass(ctx, parent_owner))
        {
            return Some(active.clone());
        }
        let registry = self
            .pass_registry
            .as_ref()
            .unwrap_or(&self.retained_registry);
        registry.open_combo_popup(ctx, parent_owner).ok().flatten()
    }

    pub(crate) fn resolve_combo_popup_owner_for_viewport(
        &self,
        ctx: &egui::Context,
        binding: NavigationViewportBinding,
    ) -> Option<ComboPopupOwner> {
        if ctx.viewport_id() != binding.viewport {
            return None;
        }
        if let Some(active) = self.active_combo_popup.as_ref()
            && active.binding == Some(binding)
            && (active.is_current(ctx, binding.owner) || active.is_sizing_pass(ctx, binding.owner))
        {
            return Some(active.clone());
        }
        let registry = self
            .pass_registry
            .as_ref()
            .unwrap_or(&self.retained_registry);
        registry
            .open_combo_popup_for_binding(ctx, binding.owner, Some(binding))
            .ok()
            .flatten()
    }

    pub(crate) fn frame_navigation_owner(&self) -> NavigationOwner {
        self.frame_navigation_owner
    }

    /// Resolve the root's first visible-frame input owner before any menu or
    /// widget can observe the events. The first pass performs transitions;
    /// later passes reapply the same event removals to egui's restored input.
    pub(crate) fn handle_root_input(
        &mut self,
        ctx: &egui::Context,
        navigation_input_allowed: bool,
        first_ui_pass: bool,
    ) {
        self.handle_viewport_input(
            ctx,
            navigation_input_allowed,
            first_ui_pass,
            egui::ViewportId::ROOT,
        );
    }

    pub(crate) fn handle_viewport_input(
        &mut self,
        ctx: &egui::Context,
        navigation_input_allowed: bool,
        first_ui_pass: bool,
        viewport: egui::ViewportId,
    ) {
        if ctx.viewport_id() != viewport {
            // A viewport coordinator is never allowed to inspect or consume a
            // different viewport's event stream. Quarantine only its own
            // stale navigation state; do not surrender focus through this
            // mismatched context.
            self.quarantine_mismatched_viewport();
            return;
        }
        if first_ui_pass {
            self.root_input_frame.viewport = viewport;
        }
        if !navigation_input_allowed {
            self.capture_invalidated_session_key_events(ctx);
            self.root_input_frame.owner_blocked = true;
            if !ctx.input(|input| input.focused) && self.active_session.is_some() {
                self.capture_unfocused_owner_loss_events(ctx);
            }
        }

        if first_ui_pass {
            if !navigation_input_allowed {
                self.capture_root_input_remainder(ctx);
                self.cancel_for_owner_loss();
            } else if self.enabled && !self.root_input_frame.owner_blocked {
                self.process_root_input_events(ctx);
            }
        }

        self.apply_root_input_capture(ctx);
    }

    pub(crate) fn active_session(&self) -> Option<&HintSession> {
        self.active_session
            .as_ref()
            .filter(|session| matches!(session.lifecycle(), SessionLifecycle::Active))
    }

    #[cfg(test)]
    pub(crate) fn test_has_focus_intent_or_handoff(&self) -> bool {
        self.pending_focus_intent.is_some()
            || self.frame_consumed_focus_intent.is_some()
            || self.last_completed_focus_handoff.is_some()
    }

    pub(crate) fn last_outcome(&self) -> Option<&MatchOutcome> {
        self.last_outcome.as_ref()
    }

    /// Start a new pass collector. Calling this again replaces all geometry
    /// collected by a pass that egui later discards.
    pub(crate) fn begin_ui_pass(&mut self) {
        self.pass_registry = None;
        self.collector = self.enabled.then(|| {
            Arc::new(Mutex::new(PassCollector {
                pending_focus_intent: self
                    .frame_consumed_focus_intent
                    .is_none()
                    .then(|| self.pending_focus_intent.clone())
                    .flatten(),
                ..PassCollector::default()
            }))
        });
    }

    pub(crate) fn scope(
        &self,
        surface: SurfaceId,
        generation: TargetGeneration,
        identity_namespace: impl Into<String>,
        viewport: egui::ViewportId,
        root_owner_allowed: bool,
    ) -> Option<JumpModeScope> {
        self.make_scope(
            surface,
            generation,
            identity_namespace,
            viewport,
            root_owner_allowed,
            None,
            None,
        )
    }

    pub(crate) fn scope_for_navigation_owner(
        &self,
        surface: SurfaceId,
        generation: TargetGeneration,
        identity_namespace: impl Into<String>,
        viewport: egui::ViewportId,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
    ) -> Option<JumpModeScope> {
        self.make_scope(
            surface,
            generation,
            identity_namespace,
            viewport,
            root_targets_allowed,
            Some(navigation_owner),
            None,
        )
    }

    pub(crate) fn scope_for_viewport_binding(
        &self,
        surface: SurfaceId,
        generation: TargetGeneration,
        identity_namespace: impl Into<String>,
        binding: NavigationViewportBinding,
    ) -> Option<JumpModeScope> {
        if !binding.owner_is_admissible() || binding.viewport == egui::ViewportId::ROOT {
            return None;
        }
        self.make_scope(
            surface,
            generation,
            identity_namespace,
            binding.viewport,
            false,
            Some(binding.owner),
            Some(binding),
        )
    }

    fn make_scope(
        &self,
        surface: SurfaceId,
        generation: TargetGeneration,
        identity_namespace: impl Into<String>,
        viewport: egui::ViewportId,
        root_owner_allowed: bool,
        navigation_owner: Option<NavigationOwner>,
        viewport_binding: Option<NavigationViewportBinding>,
    ) -> Option<JumpModeScope> {
        Some(JumpModeScope {
            collector: Arc::clone(self.collector.as_ref()?),
            surface,
            generation,
            identity_namespace: identity_namespace.into(),
            viewport,
            viewport_binding,
            epoch: self.navigation_epoch,
            root_owner_allowed,
            navigation_owner,
        })
    }

    pub(crate) fn finish_ui_pass(&mut self) {
        let Some(collector) = self.collector.take() else {
            self.pass_registry = None;
            return;
        };
        let Ok(mut collector) = collector.lock() else {
            self.pass_registry = None;
            return;
        };
        collector.closed = true;
        collector.registry.prefer_active_surface();
        if let Some(consumed) = collector.consumed_focus_intent.clone() {
            self.frame_consumed_focus_intent = Some(consumed);
        }
        self.pass_registry = Some(collector.registry.clone());
    }

    /// Revalidate and reconcile the just-finished pass before its geometry is
    /// used for visual feedback. Retained-frame ownership remains in
    /// `complete_frame_for_navigation_owner`, after egui has finished the whole frame.
    /// This root-only adapter is used by focused tests.
    #[cfg(test)]
    pub(crate) fn prepare_pass_for_paint(
        &mut self,
        ctx: &egui::Context,
        root_owner_allowed: bool,
        root_visible: bool,
    ) -> bool {
        self.prepare_pass_for_navigation_owner(
            ctx,
            if root_owner_allowed {
                NavigationOwner::Root
            } else {
                NavigationOwner::Blocked
            },
            root_owner_allowed,
            root_visible,
        )
    }

    pub(crate) fn prepare_pass_for_navigation_owner(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
    ) -> bool {
        self.prepare_pass_with_viewport_binding(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            None,
            false,
            false,
        )
    }

    pub(crate) fn prepare_pass_for_navigation_owner_with_query_focus(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        preserve_query_focus: bool,
    ) -> bool {
        self.prepare_pass_for_navigation_owner_with_query_focus_and_notes_bridge(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            preserve_query_focus,
            false,
        )
    }

    pub(crate) fn prepare_pass_for_navigation_owner_with_query_focus_and_notes_bridge(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        preserve_query_focus: bool,
        notes_query_bridge_allowed: bool,
    ) -> bool {
        self.prepare_pass_with_viewport_binding(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            None,
            preserve_query_focus,
            notes_query_bridge_allowed,
        )
    }

    pub(crate) fn prepare_pass_for_viewport(
        &mut self,
        ctx: &egui::Context,
        binding: NavigationViewportBinding,
        navigation_owner: NavigationOwner,
        root_visible: bool,
    ) -> bool {
        if ctx.viewport_id() != binding.viewport {
            self.quarantine_mismatched_viewport();
            return false;
        }
        self.prepare_pass_with_viewport_binding(
            ctx,
            navigation_owner,
            false,
            root_visible,
            Some(binding),
            false,
            false,
        )
    }

    fn prepare_pass_with_viewport_binding(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        viewport_binding: Option<NavigationViewportBinding>,
        preserve_query_focus: bool,
        notes_query_bridge_allowed: bool,
    ) -> bool {
        let viewport_is_current = match viewport_binding {
            Some(binding) => {
                binding.base_is_current(ctx)
                    && (navigation_owner == NavigationOwner::Blocked
                        || binding.owner == navigation_owner)
            }
            None => ctx.viewport_id() == egui::ViewportId::ROOT,
        };
        if !self.enabled || !root_visible || !viewport_is_current {
            return false;
        }
        if navigation_owner == NavigationOwner::Blocked {
            self.cancel_for_owner_loss();
            return false;
        }
        if navigation_owner == NavigationOwner::Root && !root_targets_allowed {
            self.cancel_for_owner_loss();
            return false;
        }

        let Some(mut completed) = self.pass_registry.take() else {
            return true;
        };
        self.reconcile_current_registry(
            ctx,
            &mut completed,
            navigation_owner,
            root_targets_allowed,
            viewport_binding,
            preserve_query_focus,
            notes_query_bridge_allowed,
        );
        self.pass_registry = Some(completed);
        true
    }

    fn reconcile_current_registry(
        &mut self,
        ctx: &egui::Context,
        registry: &mut TargetRegistry,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        viewport_binding: Option<NavigationViewportBinding>,
        preserve_query_focus: bool,
        notes_query_bridge_allowed: bool,
    ) {
        registry.revalidate(
            ctx,
            navigation_owner,
            root_targets_allowed,
            notes_query_bridge_allowed,
            self.active_combo_popup.as_ref(),
            viewport_binding,
        );
        let replacement_focus_target = self
            .last_completed_focus_handoff
            .as_ref()
            .filter(|handoff| registry.get(&handoff.intent.target).is_none())
            .and_then(|handoff| {
                registry.targets().iter().find(|target| {
                    let kind = target.metadata().kind();
                    let can_rebind = matches!(kind, TargetKind::TextEntry | TargetKind::Slider)
                        || (kind == TargetKind::Action && handoff.native_focus_owner_id.is_some());
                    can_rebind
                        && target.id().surface() == handoff.intent.target.surface()
                        && target.id().semantic() == handoff.intent.target.semantic()
                        && target.viewport() == handoff.intent.viewport
                        && target.navigation_epoch() == handoff.intent.epoch
                        && target.response_id() == handoff.target_response_id
                })
            })
            .map(|target| target.id().clone());
        if let Some(target) = replacement_focus_target {
            // A native editor can commit a value and advance its owner
            // generation while keeping the exact same editor widget. Keep
            // that native focus continuity, but rebind the handoff to the
            // current target identity so owner changes can still surrender it.
            if let Some(handoff) = self.last_completed_focus_handoff.as_mut() {
                handoff.intent.target = target;
            }
        } else if self
            .last_completed_focus_handoff
            .as_ref()
            .is_some_and(|handoff| registry.get(&handoff.intent.target).is_none())
            && let Some(handoff) = self.last_completed_focus_handoff.take()
        {
            Self::surrender_focus_unless_show_query(
                ctx,
                handoff.focus_owner_id(),
                preserve_query_focus,
            );
        }
        let navigation_allowed = match navigation_owner {
            NavigationOwner::Root => root_targets_allowed,
            NavigationOwner::Settings { .. }
            | NavigationOwner::Notes { .. }
            | NavigationOwner::Todo { .. }
            | NavigationOwner::Timer { .. }
            | NavigationOwner::PasteSequence { .. }
            | NavigationOwner::MkMacro { .. }
            | NavigationOwner::Dialog { .. }
            | NavigationOwner::SnippetRemoval { .. }
            | NavigationOwner::ClipboardRemoval { .. }
            | NavigationOwner::MouseGestureBinding { .. }
            | NavigationOwner::MouseGestureDelete { .. }
            | NavigationOwner::XmlOverwriteConfirmation { .. }
            | NavigationOwner::BatchRenameConfirmation { .. }
            | NavigationOwner::SearchReplaceConfirmation { .. }
            | NavigationOwner::DashboardEditorConfirmation { .. }
            | NavigationOwner::SnippetPrompt { .. }
            | NavigationOwner::RadialDesignerChild { .. } => true,
            NavigationOwner::Blocked => false,
        };
        let population_changed = navigation_allowed
            && self.active_session.as_mut().is_some_and(|session| {
                session
                    .reconcile_targets(registry.targets().iter().map(|target| target.id().clone()))
                    .map(|change| change.requires_new_session())
                    .unwrap_or(true)
            });
        if population_changed {
            self.cancel_active_session();
        }
    }

    /// Root-only test adapter for `complete_frame_for_navigation_owner`.
    #[cfg(test)]
    pub(crate) fn complete_frame(
        &mut self,
        ctx: &egui::Context,
        root_owner_allowed: bool,
        root_visible: bool,
    ) {
        self.complete_frame_for_navigation_owner(
            ctx,
            if root_owner_allowed {
                NavigationOwner::Root
            } else {
                NavigationOwner::Blocked
            },
            root_owner_allowed,
            root_visible,
        );
    }

    pub(crate) fn complete_frame_for_navigation_owner(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
    ) {
        self.complete_frame_with_viewport_binding(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            None,
            false,
            false,
        );
    }

    pub(crate) fn complete_frame_for_navigation_owner_with_query_focus(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        preserve_query_focus: bool,
    ) {
        self.complete_frame_for_navigation_owner_with_query_focus_and_notes_bridge(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            preserve_query_focus,
            false,
        );
    }

    pub(crate) fn complete_frame_for_navigation_owner_with_query_focus_and_notes_bridge(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        preserve_query_focus: bool,
        notes_query_bridge_allowed: bool,
    ) {
        self.complete_frame_with_viewport_binding(
            ctx,
            navigation_owner,
            root_targets_allowed,
            root_visible,
            None,
            preserve_query_focus,
            notes_query_bridge_allowed,
        );
    }

    pub(crate) fn complete_frame_for_viewport(
        &mut self,
        ctx: &egui::Context,
        binding: NavigationViewportBinding,
        navigation_owner: NavigationOwner,
        root_visible: bool,
    ) {
        self.complete_frame_with_viewport_binding(
            ctx,
            navigation_owner,
            false,
            root_visible,
            Some(binding),
            false,
            false,
        );
    }

    fn complete_frame_with_viewport_binding(
        &mut self,
        ctx: &egui::Context,
        navigation_owner: NavigationOwner,
        root_targets_allowed: bool,
        root_visible: bool,
        viewport_binding: Option<NavigationViewportBinding>,
        preserve_query_focus: bool,
        notes_query_bridge_allowed: bool,
    ) {
        if viewport_binding.is_some_and(|binding| ctx.viewport_id() != binding.viewport) {
            // A stale or misrouted callback must not surrender focus through a
            // different viewport's Context. Retire only this coordinator's
            // local intents and registry; correct-viewport owner loss below
            // still uses the usual focus cleanup.
            self.quarantine_mismatched_viewport();
            return;
        }
        let viewport_is_current = match viewport_binding {
            Some(binding) => {
                binding.base_is_current(ctx)
                    && (navigation_owner == NavigationOwner::Blocked
                        || binding.owner == navigation_owner)
            }
            None => ctx.viewport_id() == egui::ViewportId::ROOT,
        };
        if !self.enabled || !root_visible || !viewport_is_current {
            if let Some(consumed) = self.frame_consumed_focus_intent.as_ref() {
                Self::surrender_focus_unless_show_query(
                    ctx,
                    consumed.focus_owner_id(),
                    preserve_query_focus,
                );
            }
            self.clear();
            return;
        }

        if navigation_owner == NavigationOwner::Blocked
            || (navigation_owner == NavigationOwner::Root && !root_targets_allowed)
        {
            self.cancel_for_owner_loss();
        }

        let mut completed = self.pass_registry.take().unwrap_or_default();
        self.reconcile_current_registry(
            ctx,
            &mut completed,
            navigation_owner,
            root_targets_allowed,
            viewport_binding,
            preserve_query_focus,
            notes_query_bridge_allowed,
        );
        let pending_intent = self.pending_focus_intent.take();
        let mut completed_focus_handoff = None;
        if let Some(consumed) = self.frame_consumed_focus_intent.as_ref() {
            let still_matches = pending_intent.as_ref() == Some(&consumed.intent)
                && completed.targets.iter().any(|target| {
                    target.id() == &consumed.intent.target
                        && target.viewport == consumed.intent.viewport
                        && target.epoch == consumed.intent.epoch
                });
            if !still_matches {
                Self::surrender_focus_unless_show_query(
                    ctx,
                    consumed.focus_owner_id(),
                    preserve_query_focus,
                );
            } else {
                completed_focus_handoff = Some(consumed.clone());
            }
        }
        if let Some(completed_focus_handoff) = completed_focus_handoff {
            self.last_completed_focus_handoff = Some(completed_focus_handoff);
        }
        self.retained_registry = completed;
        self.collector = None;
        self.frame_consumed_focus_intent = None;
    }

    pub(crate) fn retained_registry(&self) -> &TargetRegistry {
        &self.retained_registry
    }

    pub(crate) fn retained_notes_query_bridge_target(&self) -> Option<&RegisteredTarget> {
        self.retained_registry
            .targets()
            .iter()
            .find(|target| target.notes_query_bridge_provenance().is_some())
    }

    pub(crate) fn current_notes_query_bridge_target(&self) -> Option<&RegisteredTarget> {
        self.pass_registry
            .as_ref()
            .and_then(|registry| {
                registry
                    .targets()
                    .iter()
                    .find(|target| target.notes_query_bridge_provenance().is_some())
            })
            .or_else(|| self.retained_notes_query_bridge_target())
    }

    pub(crate) fn retained_query_response_target(&self) -> Option<&RegisteredTarget> {
        let query_id = egui::Id::new("query_input");
        self.retained_registry.targets().iter().find(|target| {
            target.response_id() == query_id && target.viewport() == egui::ViewportId::ROOT
        })
    }

    /// Remove a stale query bridge before this frame's hint keys are matched.
    /// Notes targets keep their ordinary owner and behavior.
    pub(crate) fn prune_retained_notes_query_bridge(&mut self, eligible: bool) {
        if eligible {
            return;
        }
        self.retained_registry
            .targets
            .retain(|target| target.notes_query_bridge_provenance().is_none());
        let population_changed = self.active_session.as_mut().is_some_and(|session| {
            session
                .reconcile_targets(
                    self.retained_registry
                        .targets()
                        .iter()
                        .map(|target| target.id().clone()),
                )
                .map(|change| change.requires_new_session())
                .unwrap_or(true)
        });
        if population_changed {
            self.invalidated_session_key_fence
                .extend(self.held_captured_keys.iter().copied());
            self.cancel_active_session();
            // This prune runs before the first root input pass. Keep the
            // invalidating frame's remaining key/text events and held keys in
            // Jump's existing capture path so a stale hint cannot spill into
            // Notes or a newly opened owner.
            self.root_input_frame.capture_text_remainder = true;
        }
    }

    pub(crate) fn pass_registry(&self) -> Option<&TargetRegistry> {
        self.pass_registry.as_ref()
    }

    pub(crate) fn navigation_epoch(&self) -> NavigationEpoch {
        self.navigation_epoch
    }

    pub(crate) fn advance_navigation_epoch(&mut self) -> NavigationEpoch {
        self.navigation_epoch = self.navigation_epoch.next();
        self.pending_focus_intent = None;
        self.navigation_epoch
    }

    /// Queue a one-shot focus/selection intent for the next UI pass. Call this
    /// after `begin_frame` and before `begin_ui_pass`; resolution stays with
    /// the target owner and this bridge never invokes an action.
    pub(crate) fn queue_focus_intent(
        &mut self,
        target: TargetId,
        viewport: egui::ViewportId,
        epoch: NavigationEpoch,
    ) -> bool {
        if !self.enabled || self.collector.is_some() || self.pass_registry.is_some() {
            return false;
        }
        self.pending_focus_intent = Some(FocusIntent {
            target,
            viewport,
            epoch,
        });
        true
    }

    pub(crate) fn cancel_focus_intent(&mut self) {
        self.pending_focus_intent = None;
    }

    pub(crate) fn cancel_all(&mut self) {
        self.navigation_epoch = self.navigation_epoch.next();
        self.cancel_active_session();
        self.pending_focus_intent = None;
        self.collector = None;
        self.pass_registry = None;
        self.retained_registry = TargetRegistry::default();
        self.frame_consumed_focus_intent = None;
        self.last_completed_focus_handoff = None;
        self.active_combo_popup = None;
    }

    fn quarantine_mismatched_viewport(&mut self) {
        self.cancel_all();
        self.navigation_owner = NavigationOwner::Blocked;
        self.frame_navigation_owner = NavigationOwner::Blocked;
        self.navigation_owner_transitioned = true;
        self.root_input_frame.owner_blocked = true;
    }

    fn clear(&mut self) {
        self.active_session = None;
        self.last_outcome = None;
        self.root_input_frame = RootInputFrame::default();
        self.held_captured_keys.clear();
        self.invalidated_session_key_fence.clear();
        self.pending_focus_intent = None;
        self.collector = None;
        self.pass_registry = None;
        self.retained_registry = TargetRegistry::default();
        self.frame_consumed_focus_intent = None;
        self.last_completed_focus_handoff = None;
        self.active_combo_popup = None;
    }

    pub(crate) fn root_input_owner_allowed(&self, root_owner_allowed: bool) -> bool {
        root_owner_allowed
            && self.frame_navigation_owner == NavigationOwner::Root
            && !self.root_input_frame.owner_blocked
    }

    pub(crate) fn root_combo_popup_is_current(&self, ctx: &egui::Context) -> bool {
        self.frame_navigation_owner == NavigationOwner::Root
            && self
                .active_combo_popup
                .as_ref()
                .is_some_and(|popup| popup.is_current(ctx, NavigationOwner::Root))
    }

    fn process_root_input_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        let input_modifiers = ctx.input(|input| input.modifiers);
        let paired_text_indices = paired_text_indices(&events);
        let mut captured_text_indices = HashSet::new();
        let mut captured_text_only_indices = HashSet::new();
        let mut shortcut_text_indices = HashSet::new();

        for (event_index, event) in events.iter().enumerate() {
            if matches!(
                event,
                egui::Event::Cut | egui::Event::Paste(_) | egui::Event::Ime(_)
            ) {
                if self.active_session.is_some()
                    || self.root_input_frame.resolved
                    || self.root_input_frame.capture_text_remainder
                {
                    self.capture_event(event);
                }
                continue;
            }

            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            {
                if *pressed && self.root_input_frame.capture_text_remainder {
                    self.invalidated_session_key_fence.insert(*key);
                }
                let paired_text_index = paired_text_indices.get(&event_index).copied();
                if !pressed {
                    let was_held_by_session = self.held_captured_keys.remove(key);
                    let was_fenced_by_invalidated_session =
                        self.invalidated_session_key_fence.remove(key);
                    if was_held_by_session || was_fenced_by_invalidated_session {
                        self.capture_event(event);
                    }
                    continue;
                }
                if self.held_captured_keys.contains(key)
                    || self.invalidated_session_key_fence.contains(key)
                {
                    self.capture_event(event);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    continue;
                }

                let leader = is_leader_key(*key, *modifiers);
                if leader && !self.root_input_frame.resolved {
                    self.capture_event(event);
                    self.held_captured_keys.insert(*key);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    if !*repeat
                        && !self.root_input_frame.capture_text_remainder
                        && self.toggle_session()
                    {
                        self.root_input_frame.capture_text_remainder = true;
                    }
                    continue;
                }
                if matches!(*key, egui::Key::Enter | egui::Key::Space)
                    && (self.active_session.is_some()
                        || self.root_input_frame.resolved
                        || self.root_input_frame.capture_text_remainder)
                {
                    self.capture_event(event);
                    self.held_captured_keys.insert(*key);
                    if let Some(text_index) = paired_text_index {
                        captured_text_indices.insert(text_index);
                        captured_text_only_indices.insert(text_index);
                    }
                    continue;
                }

                if has_shortcut_modifier(*modifiers) {
                    if let Some(text_index) = paired_text_index {
                        shortcut_text_indices.insert(text_index);
                    }
                    continue;
                }

                if self.active_session.is_none()
                    && !self.root_input_frame.resolved
                    && !self.root_input_frame.capture_text_remainder
                {
                    continue;
                }

                // While Jump Mode owns the root, all plain key presses are
                // isolated from egui controls. Only the documented keys below
                // alter the hint session.
                self.capture_event(event);
                self.held_captured_keys.insert(*key);
                if let Some(text_index) = paired_text_index {
                    captured_text_indices.insert(text_index);
                    if *repeat {
                        captured_text_only_indices.insert(text_index);
                    }
                }
                if (self.root_input_frame.resolved || self.root_input_frame.capture_text_remainder)
                    && matches!(*key, egui::Key::Enter | egui::Key::Space)
                {
                    continue;
                }
                if *repeat {
                    continue;
                }

                match key {
                    egui::Key::Escape if self.active_session.is_some() => {
                        self.cancel_active_session();
                        self.root_input_frame.capture_text_remainder = true;
                    }
                    egui::Key::Backspace if self.active_session.is_some() => {
                        self.update_session(HintSession::backspace);
                    }
                    egui::Key::Enter | egui::Key::Space => {}
                    _ => {
                        if paired_text_index.is_none()
                            && let Some(letter) = key_letter(*key)
                            && self.active_session.is_some()
                        {
                            self.push_hint_character(letter);
                        }
                    }
                }
                continue;
            }

            let egui::Event::Text(text) = event else {
                continue;
            };
            if shortcut_text_indices.contains(&event_index) {
                continue;
            }
            if captured_text_indices.contains(&event_index) {
                self.capture_event(event);
                if !captured_text_only_indices.contains(&event_index)
                    && self.active_session.is_some()
                    && !has_shortcut_modifier(input_modifiers)
                {
                    for character in text.chars() {
                        self.push_hint_character(character);
                    }
                }
                continue;
            }
            let active = self.active_session.is_some();
            if !active
                && !self.root_input_frame.resolved
                && !self.root_input_frame.capture_text_remainder
            {
                continue;
            }
            if has_shortcut_modifier(input_modifiers) {
                continue;
            }

            self.capture_event(event);
            if !active {
                continue;
            }

            for character in text.chars() {
                self.push_hint_character(character);
                if self.root_input_frame.resolved {
                    self.root_input_frame.capture_text_remainder = true;
                }
            }
        }
    }

    /// Returns true when the leader canceled an existing session.
    fn toggle_session(&mut self) -> bool {
        if let Some(mut session) = self.active_session.take() {
            self.last_outcome = Some(session.cancel());
            return true;
        }

        let map = HintMap::assign(self.retained_registry.metadata().cloned()).unwrap_or_default();
        self.last_outcome = Some(if map.is_empty() {
            MatchOutcome::NoTargets
        } else {
            MatchOutcome::Partial {
                matching_targets: map.len(),
            }
        });
        self.active_session = Some(HintSession::new(map));
        false
    }

    fn cancel_active_session(&mut self) {
        if let Some(mut session) = self.active_session.take() {
            self.last_outcome = Some(session.cancel());
        }
    }

    fn cancel_for_owner_loss(&mut self) {
        self.cancel_active_session();
        self.held_captured_keys.clear();
    }

    fn capture_unfocused_owner_loss_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            self.capture_event(&event);
        }
    }

    fn capture_root_input_remainder(&mut self, ctx: &egui::Context) {
        if !self.root_input_frame.capture_text_remainder {
            return;
        }
        self.invalidated_session_key_fence
            .extend(self.held_captured_keys.iter().copied());
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            if let egui::Event::Key { key, pressed, .. } = &event {
                if *pressed {
                    self.invalidated_session_key_fence.insert(*key);
                } else {
                    self.invalidated_session_key_fence.remove(key);
                }
            }
            self.capture_event(&event);
        }
    }

    /// Keep only keys captured from the invalidated hint isolated until their
    /// physical release, including while another navigation owner is active.
    fn capture_invalidated_session_key_events(&mut self, ctx: &egui::Context) {
        if self.invalidated_session_key_fence.is_empty() {
            return;
        }

        let events = ctx.input(|input| input.events.clone());
        let paired_text_indices = paired_text_indices(&events);
        let mut captured_text_indices = HashSet::new();
        for (event_index, event) in events.iter().enumerate() {
            match event {
                egui::Event::Key { key, pressed, .. }
                    if self.invalidated_session_key_fence.contains(key) =>
                {
                    self.capture_event(event);
                    if *pressed {
                        if let Some(text_index) = paired_text_indices.get(&event_index) {
                            captured_text_indices.insert(*text_index);
                        }
                    } else {
                        self.invalidated_session_key_fence.remove(key);
                    }
                }
                egui::Event::Text(_) if captured_text_indices.contains(&event_index) => {
                    self.capture_event(event);
                }
                _ => {}
            }
        }
    }

    fn update_session(&mut self, update: impl FnOnce(&mut HintSession) -> MatchOutcome) {
        let Some(session) = self.active_session.as_mut() else {
            return;
        };
        let outcome = update(session);
        self.last_outcome = Some(outcome);
    }

    fn push_hint_character(&mut self, character: char) {
        let Some(session) = self.active_session.as_mut() else {
            return;
        };
        let outcome = session.push_char(character);
        let completed = match &outcome {
            MatchOutcome::Completed(target) => Some(target.clone()),
            _ => None,
        };
        self.last_outcome = Some(outcome);
        if let Some(target) = completed {
            self.active_session = None;
            self.root_input_frame.resolved = true;
            let epoch = self.advance_navigation_epoch();
            if !self.queue_focus_intent(target, self.root_input_frame.viewport, epoch) {
                self.last_outcome = Some(MatchOutcome::NoTarget);
            }
        }
    }

    fn capture_event(&mut self, event: &egui::Event) {
        if is_capturable_input_event(event) {
            self.root_input_frame.captured_events.push(event.clone());
        }
    }

    fn apply_root_input_capture(&self, ctx: &egui::Context) {
        if self.root_input_frame.captured_events.is_empty() {
            return;
        }
        ctx.input_mut(|input| {
            let mut remaining = self.root_input_frame.captured_events.clone();
            input.events.retain(|event| {
                if let Some(index) = remaining
                    .iter()
                    .position(|captured| same_keyboard_event(captured, event))
                {
                    remaining.remove(index);
                    false
                } else {
                    true
                }
            });
        });
    }
}

fn has_shortcut_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd
}

fn is_leader_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    key == egui::Key::Space && modifiers.shift && !has_shortcut_modifier(modifiers)
}

fn paired_text_indices(events: &[egui::Event]) -> std::collections::HashMap<usize, usize> {
    let mut pairs = std::collections::HashMap::new();
    for (key_index, event) in events.iter().enumerate() {
        if !matches!(event, egui::Event::Key { pressed: true, .. }) {
            continue;
        }
        for (index, following) in events.iter().enumerate().skip(key_index + 1) {
            match following {
                egui::Event::Text(_) => {
                    pairs.insert(key_index, index);
                    break;
                }
                egui::Event::Key { .. } => break,
                _ => {}
            }
        }
    }
    pairs
}

fn key_letter(key: egui::Key) -> Option<char> {
    Some(match key {
        egui::Key::A => 'A',
        egui::Key::B => 'B',
        egui::Key::C => 'C',
        egui::Key::D => 'D',
        egui::Key::E => 'E',
        egui::Key::F => 'F',
        egui::Key::G => 'G',
        egui::Key::H => 'H',
        egui::Key::I => 'I',
        egui::Key::J => 'J',
        egui::Key::K => 'K',
        egui::Key::L => 'L',
        egui::Key::M => 'M',
        egui::Key::N => 'N',
        egui::Key::O => 'O',
        egui::Key::P => 'P',
        egui::Key::Q => 'Q',
        egui::Key::R => 'R',
        egui::Key::S => 'S',
        egui::Key::T => 'T',
        egui::Key::U => 'U',
        egui::Key::V => 'V',
        egui::Key::W => 'W',
        egui::Key::X => 'X',
        egui::Key::Y => 'Y',
        egui::Key::Z => 'Z',
        _ => return None,
    })
}

fn same_keyboard_event(left: &egui::Event, right: &egui::Event) -> bool {
    match (left, right) {
        (
            egui::Event::Key {
                key: left_key,
                physical_key: left_physical,
                pressed: left_pressed,
                repeat: left_repeat,
                modifiers: left_modifiers,
                ..
            },
            egui::Event::Key {
                key: right_key,
                physical_key: right_physical,
                pressed: right_pressed,
                repeat: right_repeat,
                modifiers: right_modifiers,
                ..
            },
        ) => {
            left_key == right_key
                && left_physical == right_physical
                && left_pressed == right_pressed
                && left_repeat == right_repeat
                && left_modifiers == right_modifiers
        }
        (egui::Event::Text(left_text), egui::Event::Text(right_text)) => left_text == right_text,
        (egui::Event::Paste(left_text), egui::Event::Paste(right_text)) => left_text == right_text,
        (egui::Event::Ime(left_ime), egui::Event::Ime(right_ime)) => left_ime == right_ime,
        (egui::Event::Cut, egui::Event::Cut) => true,
        _ => false,
    }
}

fn is_capturable_input_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Cut
            | egui::Event::Paste(_)
            | egui::Event::Ime(_)
    )
}

fn rect_is_usable(rect: egui::Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.width() > 0.0
        && rect.height() > 0.0
}

fn is_root_surface(surface: &SurfaceId) -> bool {
    matches!(surface, SurfaceId::Dashboard | SurfaceId::LauncherResults)
}

fn is_popup_surface(surface: &SurfaceId) -> bool {
    matches!(surface, SurfaceId::Popup { .. })
}

fn surface_priority(surface: &SurfaceId) -> u8 {
    match surface {
        SurfaceId::Popup { owner, .. } => surface_priority(owner).saturating_add(1).max(3),
        SurfaceId::Dialog { .. } | SurfaceId::Settings { .. } => 2,
        SurfaceId::Dashboard | SurfaceId::LauncherResults => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_context(run: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), run)
            .drop_without_applying_deltas();
    }

    fn register_and_apply_focus(
        ui: &egui::Ui,
        scope: &JumpModeScope,
        response: &egui::Response,
        identity: &str,
        label: &str,
        kind: TargetKind,
        order: u64,
    ) -> RegistrationResult {
        let result = scope.register(ui, response, identity, label, kind, order);
        if let RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = &result
        {
            directive.apply_to_response(response);
        }
        result
    }

    fn render_jump_root_frame(
        ctx: &egui::Context,
        jump: &mut JumpModeUi,
        events: Vec<egui::Event>,
        focus_identity: Option<&str>,
        mut render: impl FnMut(&mut egui::Ui, &JumpModeScope),
    ) -> Vec<RegisteredTarget> {
        jump.begin_frame();
        assert_eq!(
            jump.update_navigation_owner(ctx, NavigationOwner::Root, false),
            NavigationOwner::Root
        );
        if let Some(identity) = focus_identity {
            let target = named_scope_target(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "native",
                identity,
            );
            assert!(jump.queue_focus_intent(
                target,
                egui::ViewportId::ROOT,
                jump.navigation_epoch(),
            ));
        }
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "native",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled Jump Mode creates a root scope");
        let output = ctx.run_ui(root_input_raw(events), |root| {
            egui::CentralPanel::default().show(root, |ui| render(ui, &scope));
        });
        jump.finish_ui_pass();
        assert!(jump.prepare_pass_for_navigation_owner(ctx, NavigationOwner::Root, true, true,));
        jump.complete_frame_for_navigation_owner(ctx, NavigationOwner::Root, true, true);
        let targets = jump.retained_registry().targets().to_vec();
        output.drop_without_applying_deltas();
        targets
    }

    fn render_settings_combo_frame(
        ctx: &egui::Context,
        jump: &mut JumpModeUi,
        selected: &mut usize,
        events: Vec<egui::Event>,
        focus_target: Option<TargetId>,
    ) -> (
        egui::Id,
        Vec<RegisteredTarget>,
        Option<ComboPopupOwner>,
        NavigationOwner,
    ) {
        let owner = NavigationOwner::Settings { instance: 1 };
        let windows = NavigationWindows {
            settings: Some(1),
            ..NavigationWindows::default()
        };
        jump.begin_frame();
        let pre_popup = jump.resolve_combo_popup_owner(ctx, owner);
        let pre_candidate =
            resolve_layer_navigation_owner_with_combo_popup(ctx, windows, pre_popup.as_ref());
        let pre_owner =
            jump.update_navigation_owner_with_combo_popup(ctx, pre_candidate, false, pre_popup);
        if let Some(target) = focus_target {
            assert_eq!(pre_owner, owner, "focus only resolves for the stable owner");
            assert!(jump.queue_focus_intent(
                target,
                egui::ViewportId::ROOT,
                jump.navigation_epoch(),
            ));
        }

        jump.begin_ui_pass();
        let parent_scope = jump
            .scope_for_navigation_owner(
                SurfaceId::Settings { instance: 1 },
                TargetGeneration(1),
                "settings",
                egui::ViewportId::ROOT,
                owner,
                false,
            )
            .expect("enabled Settings owner creates a scope");
        let popup_scope = parent_scope.combo_popup_scope("choice");
        let mut anchor_id = None;
        let output = ctx.run_ui(root_input_raw(events), |ctx| {
            egui::Window::new("Settings").show(ctx, |ui| {
                let combo = egui::ComboBox::from_id_salt("jump-mode-native-choice")
                    .selected_text(if *selected == 0 { "First" } else { "Second" })
                    .show_ui(ui, |ui| {
                        for (index, label) in ["First", "Second"].into_iter().enumerate() {
                            let response = ui.selectable_value(selected, index, label);
                            register_and_apply_focus(
                                ui,
                                &popup_scope,
                                &response,
                                if index == 0 {
                                    "option/first"
                                } else {
                                    "option/second"
                                },
                                label,
                                TargetKind::Selectable,
                                index as u64,
                            );
                        }
                    });
                anchor_id = Some(combo.response.id);
                register_and_apply_focus(
                    ui,
                    &parent_scope,
                    &combo.response,
                    "choice",
                    "Choice",
                    TargetKind::ComboBox,
                    0,
                );
            });
        });
        jump.finish_ui_pass();
        let combo_popup = jump.resolve_combo_popup_owner(ctx, owner);
        let candidate =
            resolve_layer_navigation_owner_with_combo_popup(ctx, windows, combo_popup.as_ref());
        let routed_owner = jump.update_navigation_owner_with_combo_popup(
            ctx,
            candidate,
            false,
            combo_popup.clone(),
        );
        let _ = jump.prepare_pass_for_navigation_owner(ctx, routed_owner, false, true);
        jump.complete_frame_for_navigation_owner(ctx, routed_owner, false, true);
        let targets = jump.retained_registry().targets().to_vec();
        output.drop_without_applying_deltas();
        (
            anchor_id.expect("Settings renders the actual ComboBox response"),
            targets,
            combo_popup,
            routed_owner,
        )
    }

    fn render_navigation_windows(
        ctx: &egui::Context,
        settings_open: bool,
        note_open: bool,
        focus_note: bool,
        move_to_top: Option<egui::LayerId>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            if settings_open {
                egui::Window::new("Settings").show(ui.ctx(), |_| {});
            }
            if note_open {
                egui::Window::new("Unsupported note")
                    .id(egui::Id::new("unsupported_note_window"))
                    .show(ui.ctx(), |ui| {
                        let response = ui.add(egui::TextEdit::singleline(&mut String::new()));
                        if focus_note {
                            response.request_focus();
                        }
                    });
            }
            if let Some(layer) = move_to_top {
                ui.ctx().move_to_top(layer);
            }
        })
        .drop_without_applying_deltas();
    }

    fn render_supported_dialog_windows(
        ctx: &egui::Context,
        settings_open: bool,
        notes_open: bool,
        todo_open: bool,
        move_to_top: Option<egui::LayerId>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            if settings_open {
                egui::Window::new("Settings").show(ui.ctx(), |ui| {
                    let _ = ui.button("Settings body");
                });
            }
            if notes_open {
                egui::Window::new("Quick Notes").show(ui.ctx(), |ui| {
                    let _ = ui.button("Notes body");
                });
            }
            if todo_open {
                egui::Window::new("Todos").show(ui.ctx(), |ui| {
                    let _ = ui.button("Todos body");
                });
            }
            if let Some(layer) = move_to_top {
                ui.ctx().move_to_top(layer);
            }
        })
        .drop_without_applying_deltas();
    }

    fn render_owned_dialog_windows(
        ctx: &egui::Context,
        dialogs: &[NavigationDialogKind],
        move_to_top: Option<egui::LayerId>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            for kind in dialogs {
                match kind {
                    NavigationDialogKind::FileSearch => {
                        egui::Window::new("File Search").show(ctx, |_| {});
                    }
                    NavigationDialogKind::Snippets => {
                        egui::Window::new("Snippets").show(ctx, |_| {});
                    }
                    NavigationDialogKind::ClipboardHistory => {
                        egui::Window::new("Clipboard History").show(ctx, |_| {});
                    }
                    NavigationDialogKind::JsonUtility => {
                        egui::Window::new("JSON Utility")
                            .id(egui::Id::new("json_utility_dialog"))
                            .show(ctx, |_| {});
                    }
                    NavigationDialogKind::RegexTester => {
                        egui::Window::new("Regex Tester")
                            .id(egui::Id::new("regex_tester_embedded_window"))
                            .show(ctx, |_| {});
                    }
                    NavigationDialogKind::XmlToolkit => {
                        egui::Window::new("XML Toolkit")
                            .id(egui::Id::new("xml_toolkit_embedded_window"))
                            .show(ctx, |_| {});
                    }
                    NavigationDialogKind::TableUtility => {
                        egui::Window::new("Table Utility").show(ctx, |_| {});
                    }
                    NavigationDialogKind::ImageCompare => {
                        egui::Window::new("Image Comparison").show(ctx, |_| {});
                    }
                    NavigationDialogKind::MouseSettings => {
                        egui::Window::new("Mouse Settings").show(ctx, |_| {});
                    }
                    NavigationDialogKind::MouseGestureSettings => {
                        egui::Window::new("Mouse Gesture Settings").show(ctx, |_| {});
                    }
                    NavigationDialogKind::MouseGestures => {
                        egui::Window::new("Mouse Gestures").show(ctx, |_| {});
                    }
                    NavigationDialogKind::BatchRename => {
                        egui::Window::new("Batch Renamer")
                            .id(egui::Id::new("batch_rename_preview_dialog"))
                            .show(ctx, |_| {});
                    }
                    NavigationDialogKind::SearchReplace => {
                        egui::Window::new("Search and Replace")
                            .id(egui::Id::new("search_replace_preview_dialog"))
                            .show(ctx, |_| {});
                    }
                    NavigationDialogKind::DashboardEditor => {
                        egui::Window::new("Dashboard Editor").show(ctx, |_| {});
                    }
                    NavigationDialogKind::RadialDesigner => {}
                    NavigationDialogKind::HotkeyDirectory => {
                        egui::Window::new("Hotkey Directory")
                            .id(egui::Id::new(Some("Hotkey Directory")))
                            .show(ctx, |_| {});
                    }
                }
            }
            if let Some(layer) = move_to_top {
                ctx.move_to_top(layer);
            }
        })
        .drop_without_applying_deltas();
    }

    fn windows_with_dialogs(
        dialogs: impl IntoIterator<Item = (NavigationDialogKind, u64)>,
    ) -> NavigationWindows {
        let mut windows = NavigationWindows::default();
        for (kind, instance) in dialogs {
            windows.dialogs[kind.index()] = Some(instance);
        }
        windows
    }

    fn setup(enabled: bool) -> (JumpModeUi, JumpModeScope) {
        let mut jump = JumpModeUi::new(enabled);
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled bridge creates a render scope");
        (jump, scope)
    }

    #[test]
    fn settings_owner_tracks_actual_top_window_order() {
        let ctx = egui::Context::default();
        let settings_layer = settings_layer_id();
        let note_layer = egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("unsupported_note_window"),
        );

        render_navigation_windows(&ctx, true, true, false, None);
        assert_eq!(visible_top_middle_layer(&ctx), Some(note_layer));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(3),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Blocked,
            "a newly retained unsupported window above Settings blocks it"
        );

        render_navigation_windows(&ctx, true, true, false, Some(settings_layer));
        assert_eq!(visible_top_middle_layer(&ctx), Some(settings_layer));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(3),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Settings { instance: 3 }
        );

        render_navigation_windows(&ctx, true, true, false, Some(note_layer));
        assert_eq!(visible_top_middle_layer(&ctx), Some(note_layer));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(3),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Blocked
        );
    }

    #[test]
    fn notes_and_todo_owners_follow_real_window_z_order_and_exact_instances() {
        let ctx = egui::Context::default();
        let windows = NavigationWindows {
            settings: Some(11),
            notes: Some(22),
            todo: Some(33),
            timer: None,
            paste_sequence: None,
            mkmacro: None,
            dialogs: [None; NavigationDialogKind::ALL.len()],
        };
        render_supported_dialog_windows(&ctx, true, true, true, None);
        assert_eq!(visible_top_middle_layer(&ctx), Some(todo_dialog_layer_id()));
        assert_eq!(
            resolve_layer_navigation_owner(&ctx, windows),
            NavigationOwner::Todo { instance: 33 }
        );

        render_supported_dialog_windows(&ctx, true, true, true, Some(notes_dialog_layer_id()));
        assert_eq!(
            visible_top_middle_layer(&ctx),
            Some(notes_dialog_layer_id())
        );
        assert_eq!(
            resolve_layer_navigation_owner(&ctx, windows),
            NavigationOwner::Notes { instance: 22 }
        );

        render_supported_dialog_windows(&ctx, true, true, true, Some(settings_layer_id()));
        assert_eq!(visible_top_middle_layer(&ctx), Some(settings_layer_id()));
        assert_eq!(
            resolve_layer_navigation_owner(&ctx, windows),
            NavigationOwner::Settings { instance: 11 }
        );

        render_supported_dialog_windows(&ctx, true, true, true, Some(todo_dialog_layer_id()));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    todo: None,
                    ..windows
                }
            ),
            NavigationOwner::Blocked,
            "a visible dialog without its exact live instance is unsupported"
        );
    }

    #[test]
    fn expanded_dialog_layer_inventory_matches_actual_window_ids() {
        for kind in NavigationDialogKind::ALL {
            let Some(expected_layer) = kind.layer_id() else {
                assert_eq!(kind, NavigationDialogKind::RadialDesigner);
                continue;
            };
            let ctx = egui::Context::default();
            render_owned_dialog_windows(&ctx, &[kind], None);
            assert_eq!(
                visible_top_middle_layer(&ctx),
                Some(expected_layer),
                "{kind:?} maps to the layer created by its current Window ID"
            );
            assert_eq!(
                resolve_layer_navigation_owner(&ctx, windows_with_dialogs([(kind, 41)])),
                NavigationOwner::Dialog { kind, instance: 41 },
                "{kind:?} requires the matching live instance"
            );
            assert_eq!(
                resolve_layer_navigation_owner(&ctx, NavigationWindows::default()),
                NavigationOwner::Blocked,
                "a supported-looking window without an open instance remains blocked"
            );
        }

        let ctx = egui::Context::default();
        assert_eq!(NavigationDialogKind::RadialDesigner.layer_id(), None);
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                windows_with_dialogs([(NavigationDialogKind::RadialDesigner, 1)])
            ),
            NavigationOwner::Blocked,
            "a viewport-only designer blocks root fallback until its viewport is integrated"
        );
    }

    #[test]
    fn expanded_dialog_instances_change_on_reopen_but_not_reorder() {
        let file_search = NavigationDialogKind::FileSearch;
        let snippets = NavigationDialogKind::Snippets;
        let mut jump = JumpModeUi::new(true);
        assert!(jump.observe_dialog_visibility([(file_search, true), (snippets, true)]));
        let first_file_instance = jump.dialog_instance(file_search).unwrap();
        let first_snippets_instance = jump.dialog_instance(snippets).unwrap();
        assert_eq!((first_file_instance, first_snippets_instance), (1, 1));

        assert!(!jump.observe_dialog_visibility([(snippets, true), (file_search, true)]));
        let windows = windows_with_dialogs([
            (file_search, first_file_instance),
            (snippets, first_snippets_instance),
        ]);
        let ctx = egui::Context::default();
        render_owned_dialog_windows(&ctx, &[file_search, snippets], None);
        assert_eq!(
            resolve_layer_navigation_owner(&ctx, windows),
            NavigationOwner::Dialog {
                kind: snippets,
                instance: first_snippets_instance
            }
        );
        render_owned_dialog_windows(&ctx, &[file_search, snippets], file_search.layer_id());
        assert_eq!(
            resolve_layer_navigation_owner(&ctx, windows),
            NavigationOwner::Dialog {
                kind: file_search,
                instance: first_file_instance
            },
            "raising another open dialog changes only the foreground owner"
        );

        assert!(jump.observe_dialog_visibility([(file_search, false), (snippets, true)]));
        assert_eq!(jump.dialog_instance(file_search), None);
        assert!(jump.observe_dialog_visibility([(file_search, true), (snippets, true)]));
        assert_eq!(
            jump.dialog_instance(file_search),
            Some(first_file_instance + 1)
        );
        assert_eq!(
            jump.dialog_instance(snippets),
            Some(first_snippets_instance)
        );
    }

    #[test]
    fn navigation_scopes_reject_other_dialog_kinds_and_instances() {
        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let owner = NavigationOwner::Notes { instance: 7 };
        let notes = jump
            .scope_for_navigation_owner(
                SurfaceId::dialog("notes", 7),
                TargetGeneration(1),
                "notes",
                egui::ViewportId::ROOT,
                owner,
                false,
            )
            .unwrap();
        let wrong_kind = notes.with_surface(SurfaceId::dialog("todo", 7), TargetGeneration(1));
        let wrong_instance = notes.with_surface(SurfaceId::dialog("notes", 8), TargetGeneration(1));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.button("target");
            for (scope, identity) in [(&wrong_kind, "todo"), (&wrong_instance, "other-notes")] {
                assert_eq!(
                    scope.register(ui, &response, identity, identity, TargetKind::Button, 0),
                    RegistrationResult::Rejected(RegistrationRejection::InactiveOwner)
                );
            }
            let popup = notes.child_popup_scope("notes-menu");
            assert!(matches!(
                popup.register(
                    ui,
                    &response,
                    "menu-item",
                    "Menu item",
                    TargetKind::Button,
                    1
                ),
                RegistrationResult::Registered { .. }
            ));
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn settings_does_not_take_foreign_focused_window_and_stale_layers_are_ignored() {
        let ctx = egui::Context::default();
        let settings_layer = settings_layer_id();
        render_navigation_windows(&ctx, true, true, false, None);
        render_navigation_windows(&ctx, true, true, true, Some(settings_layer));
        let note_layer = egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("unsupported_note_window"),
        );
        assert_eq!(focused_response_layer(&ctx), Some(note_layer));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(1),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Blocked
        );

        let stale_ctx = egui::Context::default();
        render_navigation_windows(&stale_ctx, true, false, false, None);
        render_navigation_windows(&stale_ctx, false, false, false, None);
        assert_eq!(visible_top_middle_layer(&stale_ctx), None);
        assert_eq!(
            resolve_layer_navigation_owner(&stale_ctx, NavigationWindows::default()),
            NavigationOwner::Root
        );
    }

    #[test]
    fn settings_with_no_retained_window_layer_fails_closed_until_it_is_visible() {
        let ctx = egui::Context::default();
        render_navigation_windows(&ctx, false, false, false, None);
        assert_eq!(visible_top_middle_layer(&ctx), None);
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(1),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Blocked
        );
    }

    #[test]
    fn popup_takeover_blocks_layer_owner_resolution() {
        let ctx = egui::Context::default();
        render_navigation_windows(&ctx, true, false, false, None);
        egui::Popup::open_id(&ctx, egui::Id::new("jump_mode_owner_popup"));
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(
            resolve_layer_navigation_owner(
                &ctx,
                NavigationWindows {
                    settings: Some(1),
                    ..NavigationWindows::default()
                },
            ),
            NavigationOwner::Blocked
        );
    }

    #[test]
    fn combo_popup_uses_exact_foreground_owner_and_later_native_option_activation() {
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let ctx = egui::Context::default();
        let owner = NavigationOwner::Settings { instance: 1 };
        let windows = NavigationWindows {
            settings: Some(1),
            ..NavigationWindows::default()
        };
        let mut jump = JumpModeUi::new(true);
        let mut selected = 0;

        // The first visible Settings pass establishes the real owner layer.
        let (_, _, _, first_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(first_owner, NavigationOwner::Blocked);

        let combo_target = named_scope_target(
            SurfaceId::Settings { instance: 1 },
            TargetGeneration(1),
            "settings",
            "choice",
        );
        let (combo_response, _, _, focus_owner) = render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            Vec::new(),
            Some(combo_target.clone()),
        );
        assert_eq!(focus_owner, owner);
        assert_eq!(selected, 0, "focusing the real ComboBox does not select");
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(combo_response));

        let (combo_response, _, opened_popup, open_transition) = render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            vec![key(egui::Key::Enter)],
            None,
        );
        assert_eq!(
            selected, 0,
            "Enter opens the ComboBox without choosing an option"
        );
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(open_transition, NavigationOwner::Blocked);
        let combo_popup = opened_popup.expect("the current ComboBox owns its popup");
        assert_eq!(combo_popup.response_id, combo_response);
        assert_eq!(
            resolve_layer_navigation_owner_with_combo_popup(&ctx, windows, Some(&combo_popup),),
            NavigationOwner::Blocked,
            "the non-interactive sizing pass cannot own navigation"
        );
        assert!(
            egui::containers::AreaState::load(&ctx, combo_response.with("popup"))
                .is_some_and(|state| !state.interactable),
            "the first popup pass is sizing-only"
        );
        assert_eq!(
            resolve_layer_navigation_owner_with_combo_popup(&ctx, windows, None),
            NavigationOwner::Blocked,
            "an open popup is not attributed to a surface without its exact ComboBox owner"
        );

        let (_, transition_targets, recognized_popup, transition_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(transition_owner, NavigationOwner::Blocked);
        assert!(
            transition_targets.is_empty(),
            "the owner transition stays target-free"
        );
        assert!(
            egui::containers::AreaState::load(&ctx, combo_response.with("popup"))
                .is_some_and(|state| state.interactable),
            "the next actual render makes the popup interactive"
        );
        let recognized_popup = recognized_popup.expect("the exact live ComboBox is retained");
        assert_eq!(
            resolve_layer_navigation_owner_with_combo_popup(&ctx, windows, Some(&recognized_popup)),
            owner,
            "the interactive popup recovers only through its retained exact anchor"
        );

        let (_, open_targets, recognized_popup, stable_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(stable_owner, owner);
        let recognized_popup = recognized_popup.expect("the popup remains recognized");
        assert!(open_targets.len() >= 2);
        assert!(open_targets.iter().all(|target| {
            target.id().surface() == &recognized_popup.popup_surface
                && target.metadata().kind() == TargetKind::Selectable
        }));
        assert!(
            open_targets
                .iter()
                .any(|target| target.metadata().label() == "Second")
        );

        let mut ambiguous = jump.retained_registry().clone();
        let mut duplicate_anchor = ambiguous
            .combo_anchors
            .first()
            .expect("the registry keeps the visible ComboBox anchor")
            .clone();
        duplicate_anchor.metadata = TargetMetadata::new(
            named_scope_target(
                SurfaceId::Settings { instance: 1 },
                TargetGeneration(1),
                "settings",
                "duplicate-choice",
            ),
            "Duplicate Choice",
            TargetKind::ComboBox,
            1,
        );
        ambiguous.combo_anchors.push(duplicate_anchor);
        assert!(
            ambiguous.open_combo_popup(&ctx, owner).is_err(),
            "two live anchors matching an open popup are ambiguous and fail closed"
        );

        let option_target = named_scope_target(
            combo_popup_surface(&combo_target),
            TargetGeneration(1),
            "settings",
            "option/second",
        );
        let (_, focused_options, _, focused_owner) = render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            Vec::new(),
            Some(option_target),
        );
        assert_eq!(focused_owner, owner);
        assert_eq!(selected, 0, "focusing a popup option does not select it");
        let focused_option = focused_options
            .iter()
            .find(|target| target.metadata().label() == "Second")
            .expect("the visible second option is registered");
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(focused_option.response_id())
        );

        render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            vec![key(egui::Key::Enter)],
            None,
        );
        assert_eq!(
            selected, 1,
            "a fresh native Enter selects the focused option"
        );

        egui::Popup::open_id(&ctx, egui::Id::new("unrecognized-native-popup"));
        assert!(egui::Popup::is_any_open(&ctx));
        let unknown_popup = jump.resolve_combo_popup_owner(&ctx, owner);
        assert!(unknown_popup.is_none());
        assert_eq!(
            resolve_layer_navigation_owner_with_combo_popup(&ctx, windows, unknown_popup.as_ref(),),
            NavigationOwner::Blocked,
            "an unrelated popup cannot inherit the Settings navigation owner"
        );
    }

    #[test]
    fn combo_popup_with_foreign_foreground_editor_blocks_hints_and_preserves_text_input() {
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let ctx = egui::Context::default();
        let owner = NavigationOwner::Settings { instance: 1 };
        let windows = NavigationWindows {
            settings: Some(1),
            ..NavigationWindows::default()
        };
        let mut jump = JumpModeUi::new(true);
        let mut selected = 0;

        let (_, _, _, first_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(first_owner, NavigationOwner::Blocked);
        let combo_target = named_scope_target(
            SurfaceId::Settings { instance: 1 },
            TargetGeneration(1),
            "settings",
            "choice",
        );
        let (combo_response, _, _, focus_owner) = render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            Vec::new(),
            Some(combo_target),
        );
        assert_eq!(focus_owner, owner);

        render_settings_combo_frame(
            &ctx,
            &mut jump,
            &mut selected,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            None,
        );
        let (_, _, _, sizing_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(sizing_owner, NavigationOwner::Blocked);
        let (_, _, popup_owner, stable_owner) =
            render_settings_combo_frame(&ctx, &mut jump, &mut selected, Vec::new(), None);
        assert_eq!(stable_owner, owner);
        let popup_owner = popup_owner.expect("the real Settings ComboBox owns its popup");

        let editor_id = egui::Id::new("jump-mode-foreign-foreground-editor");
        let mut text = String::new();
        let mut editor_layer = None;
        let mut editor_rect = None;
        let popup_rect = ctx
            .memory(|memory| memory.area_rect(combo_response.with("popup")))
            .expect("the live ComboBox popup has geometry");
        let focus_output = ctx.run_ui(root_input_raw(Vec::new()), |ctx| {
            egui::Window::new("Settings").show(ctx, |ui| {
                egui::ComboBox::from_id_salt("jump-mode-native-choice")
                    .selected_text(if selected == 0 { "First" } else { "Second" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "First");
                        ui.selectable_value(&mut selected, 1, "Second");
                    });
            });
            egui::Area::new(egui::Id::new("jump-mode-foreign-foreground-area"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(20.0, 560.0))
                .show(ctx, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .id(editor_id)
                            .desired_width(180.0),
                    );
                    response.request_focus();
                    editor_layer = Some(response.layer_id);
                    editor_rect = Some(response.rect);
                });
        });
        focus_output.drop_without_applying_deltas();

        let editor_layer = editor_layer.expect("the foreground TextEdit was rendered");
        let editor_rect = editor_rect.expect("the foreground TextEdit has a real response rect");
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(editor_id));
        assert_eq!(focused_response_layer(&ctx), Some(editor_layer));
        assert_eq!(editor_layer.order, egui::Order::Foreground);
        assert_ne!(editor_layer, popup_owner.popup_layer);
        assert!(!editor_rect.contains(popup_rect.center()));
        assert_eq!(
            ctx.layer_id_at(popup_rect.center()),
            Some(popup_owner.popup_layer),
            "the foreign editor is focused away from the live popup"
        );
        assert!(egui::Popup::is_any_open(&ctx));
        assert!(jump.resolve_combo_popup_owner(&ctx, owner).is_none());
        assert_eq!(
            resolve_layer_navigation_owner_with_combo_popup(&ctx, windows, None),
            NavigationOwner::Blocked,
            "foreign foreground focus cannot inherit the ComboBox owner"
        );

        let events = vec![
            key(egui::Key::Space, egui::Modifiers::SHIFT),
            egui::Event::Text(" ".into()),
            key(egui::Key::A, egui::Modifiers::NONE),
            egui::Event::Text("a".into()),
        ];
        let mut remaining_after_router = Vec::new();
        let routed_output = ctx.run_ui(root_input_raw(events.clone()), |ctx| {
            jump.begin_frame();
            let popup_candidate = jump.resolve_combo_popup_owner(ctx, owner);
            let candidate = resolve_layer_navigation_owner_with_combo_popup(
                ctx,
                windows,
                popup_candidate.as_ref(),
            );
            assert_eq!(candidate, NavigationOwner::Blocked);
            let transition_pending = jump.combo_popup_transition_pending(popup_candidate.as_ref());
            let input_allowed = !transition_pending && candidate != NavigationOwner::Blocked;
            jump.handle_root_input(ctx, input_allowed, true);
            remaining_after_router = ctx.input(|input| input.events.clone());
            let routed_owner = jump.update_navigation_owner_with_combo_popup(
                ctx,
                candidate,
                false,
                popup_candidate,
            );
            assert_eq!(routed_owner, NavigationOwner::Blocked);

            jump.begin_ui_pass();
            egui::Window::new("Settings").show(ctx, |ui| {
                egui::ComboBox::from_id_salt("jump-mode-native-choice")
                    .selected_text(if selected == 0 { "First" } else { "Second" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "First");
                        ui.selectable_value(&mut selected, 1, "Second");
                    });
            });
            egui::Area::new(egui::Id::new("jump-mode-foreign-foreground-area"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(20.0, 560.0))
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .id(editor_id)
                            .desired_width(180.0),
                    );
                });
            jump.finish_ui_pass();
            jump.prepare_pass_for_navigation_owner(ctx, routed_owner, false, true);
            jump.complete_frame_for_navigation_owner(ctx, routed_owner, false, true);
        });
        routed_output.drop_without_applying_deltas();

        assert_eq!(
            remaining_after_router, events,
            "Jump Mode captures no input"
        );
        assert_eq!(
            text, " a",
            "the focused native editor receives the leader text"
        );
        assert_eq!(
            selected, 0,
            "foreign focus does not activate a ComboBox option"
        );
        assert!(jump.active_session().is_none());
        assert!(jump.retained_registry().targets().is_empty());
    }

    #[test]
    fn native_combo_escape_closes_without_changing_the_selected_value() {
        let ctx = egui::Context::default();
        let mut selected = 0;
        let mut response_id = None;
        ctx.run_ui(root_input_raw(Vec::new()), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                let combo = egui::ComboBox::from_id_salt("jump-mode-native-escape")
                    .selected_text(if selected == 0 { "First" } else { "Second" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "First");
                        ui.selectable_value(&mut selected, 1, "Second");
                    });
                response_id = Some(combo.response.id);
            });
        })
        .drop_without_applying_deltas();

        egui::Popup::open_id(
            &ctx,
            response_id
                .expect("the real ComboBox response is rendered")
                .with("popup"),
        );
        ctx.run_ui(root_input_raw(Vec::new()), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                egui::ComboBox::from_id_salt("jump-mode-native-escape")
                    .selected_text("First")
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, 0, "First");
                        ui.selectable_value(&mut selected, 1, "Second");
                    });
            });
        })
        .drop_without_applying_deltas();
        assert!(egui::Popup::is_any_open(&ctx));

        ctx.run_ui(
            root_input_raw(vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]),
            |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    egui::ComboBox::from_id_salt("jump-mode-native-escape")
                        .selected_text("First")
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected, 0, "First");
                            ui.selectable_value(&mut selected, 1, "Second");
                        });
                });
            },
        )
        .drop_without_applying_deltas();
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(selected, 0, "Escape only dismisses the native popup");
    }

    #[test]
    fn owner_transition_blocks_all_discarded_passes_then_stabilizes() {
        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        assert!(jump.observe_settings_visibility(true));
        let owner =
            jump.update_navigation_owner(&ctx, NavigationOwner::Settings { instance: 1 }, true);
        assert_eq!(owner, NavigationOwner::Blocked);
        jump.handle_root_input(&ctx, false, true);
        assert!(jump.active_session().is_none());

        jump.begin_ui_pass();
        jump.finish_ui_pass();
        assert_eq!(
            jump.update_navigation_owner(&ctx, NavigationOwner::Settings { instance: 1 }, false,),
            NavigationOwner::Blocked
        );
        assert!(!jump.prepare_pass_for_navigation_owner(
            &ctx,
            NavigationOwner::Blocked,
            false,
            true,
        ));

        jump.begin_frame();
        assert_eq!(
            jump.update_navigation_owner(&ctx, NavigationOwner::Settings { instance: 1 }, false,),
            NavigationOwner::Settings { instance: 1 }
        );
        assert_eq!(jump.settings_instance(), 1);
        assert!(jump.observe_settings_visibility(false));
        assert!(!jump.observe_settings_visibility(false));
        assert!(jump.observe_settings_visibility(true));
        assert_eq!(jump.settings_instance(), 2);
    }

    #[test]
    fn settings_owner_keeps_the_shared_hint_input_session_alive() {
        let ctx = egui::Context::default();
        let settings_owner = NavigationOwner::Settings { instance: 1 };
        let settings_surface = SurfaceId::Settings { instance: 1 };
        let mut jump = JumpModeUi::new(true);

        ctx.run_ui(root_input_raw(Vec::new()), |_| {
            jump.begin_frame();
            assert!(jump.observe_settings_visibility(true));
            assert_eq!(
                jump.update_navigation_owner(&ctx, settings_owner, true),
                NavigationOwner::Blocked,
                "the opening transition cannot route input"
            );
        })
        .drop_without_applying_deltas();

        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            jump.begin_frame();
            assert_eq!(
                jump.update_navigation_owner(&ctx, settings_owner, false),
                settings_owner
            );
            jump.begin_ui_pass();
            let scope = jump
                .scope_for_navigation_owner(
                    settings_surface.clone(),
                    TargetGeneration(1),
                    "settings",
                    ctx.viewport_id(),
                    settings_owner,
                    false,
                )
                .expect("stable Settings owner receives its local collection scope");
            let response = ui.button("Settings target");
            assert!(matches!(
                scope.register(
                    ui,
                    &response,
                    "settings-target",
                    "Settings target",
                    TargetKind::Button,
                    1,
                ),
                RegistrationResult::Registered { .. }
            ));
            jump.finish_ui_pass();
        })
        .drop_without_applying_deltas();
        assert!(jump.prepare_pass_for_navigation_owner(&ctx, settings_owner, false, true));
        jump.complete_frame_for_navigation_owner(&ctx, settings_owner, false, true);
        assert_eq!(jump.retained_registry().len(), 1);

        ctx.run_ui(
            root_input_raw(vec![
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::Text(" ".into()),
            ]),
            |ui| {
                jump.begin_frame();
                assert_eq!(
                    jump.update_navigation_owner(&ctx, settings_owner, false),
                    settings_owner
                );
                jump.handle_root_input(&ctx, true, true);
                assert!(
                    jump.active_session().is_some(),
                    "the shared input router starts from retained Settings targets"
                );

                jump.begin_ui_pass();
                let scope = jump
                    .scope_for_navigation_owner(
                        settings_surface.clone(),
                        TargetGeneration(1),
                        "settings",
                        ctx.viewport_id(),
                        settings_owner,
                        false,
                    )
                    .expect("Settings pass remains registered while hints are active");
                let response = ui.button("Settings target");
                assert!(matches!(
                    scope.register(
                        ui,
                        &response,
                        "settings-target",
                        "Settings target",
                        TargetKind::Button,
                        1,
                    ),
                    RegistrationResult::Registered { .. }
                ));
                jump.finish_ui_pass();
            },
        )
        .drop_without_applying_deltas();
        assert!(jump.prepare_pass_for_navigation_owner(&ctx, settings_owner, false, true));
        jump.complete_frame_for_navigation_owner(&ctx, settings_owner, false, true);
        assert!(
            jump.active_session().is_some(),
            "Settings pass reconciliation preserves its active shared-router session"
        );
    }

    #[test]
    fn entering_settings_surrenders_only_the_background_query_focus() {
        let ctx = egui::Context::default();
        let query_id = egui::Id::new("query_input");
        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut String::new()).id(query_id));
            response.request_focus();
        })
        .drop_without_applying_deltas();
        assert!(ctx.memory(|memory| memory.has_focus(query_id)));

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let settings_opened = jump.observe_settings_visibility(true);
        assert_eq!(
            jump.update_navigation_owner(
                &ctx,
                NavigationOwner::Settings { instance: 1 },
                settings_opened,
            ),
            NavigationOwner::Blocked
        );
        assert!(!ctx.memory(|memory| memory.has_focus(query_id)));

        let note_ctx = egui::Context::default();
        render_navigation_windows(&note_ctx, false, true, true, None);
        let note_layer = egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("unsupported_note_window"),
        );
        let focused = note_ctx.memory(|memory| memory.focused()).unwrap();
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.update_navigation_owner(&note_ctx, NavigationOwner::Blocked, false);
        assert_eq!(note_ctx.memory(|memory| memory.focused()), Some(focused));
        assert_eq!(focused_response_layer(&note_ctx), Some(note_layer));
    }

    #[test]
    fn disabled_settings_transition_preserves_native_focus_and_clears_old_handoff() {
        let query_ctx = egui::Context::default();
        let query_id = egui::Id::new("query_input");
        query_ctx
            .run_ui(root_input_raw(Vec::new()), |ui| {
                ui.add(egui::TextEdit::singleline(&mut String::new()).id(query_id))
                    .request_focus();
            })
            .drop_without_applying_deltas();
        assert!(query_ctx.memory(|memory| memory.has_focus(query_id)));
        let mut disabled_jump = JumpModeUi::new(false);
        disabled_jump.begin_frame();
        assert!(disabled_jump.observe_settings_visibility(true));
        disabled_jump.update_navigation_owner(&query_ctx, NavigationOwner::Blocked, true);
        assert!(
            query_ctx.memory(|memory| memory.has_focus(query_id)),
            "the default-off feature does not surrender ordinary query focus"
        );

        let ctx = egui::Context::default();
        let foreign_id = egui::Id::new("foreign_window_editor");
        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::Window::new("Foreign editor")
                .id(egui::Id::new("foreign_editor_window"))
                .show(ui.ctx(), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(foreign_id))
                        .request_focus();
                });
        })
        .drop_without_applying_deltas();
        assert!(ctx.memory(|memory| memory.has_focus(foreign_id)));

        let mut jump = JumpModeUi::new(true);
        jump.last_completed_focus_handoff = Some(ConsumedFocusIntent {
            intent: FocusIntent {
                target: TargetId::root_result(TargetGeneration(1), 0),
                viewport: egui::ViewportId::ROOT,
                epoch: NavigationEpoch::default(),
            },
            target_response_id: foreign_id,
            native_focus_owner_id: None,
        });
        jump.set_enabled(false);
        jump.begin_frame();
        assert!(jump.observe_settings_visibility(true));
        jump.update_navigation_owner(&ctx, NavigationOwner::Blocked, true);
        assert!(ctx.memory(|memory| memory.has_focus(foreign_id)));

        jump.set_enabled(true);
        jump.begin_frame();
        jump.update_navigation_owner(&ctx, NavigationOwner::Settings { instance: 1 }, false);
        assert!(
            ctx.memory(|memory| memory.has_focus(foreign_id)),
            "disabling Jump Mode clears any stored focus ID before later owner changes"
        );
    }

    fn root_input_raw(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn register_root_input_targets(jump: &mut JumpModeUi, ctx: &egui::Context, count: usize) {
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "input-test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled bridge creates a render scope");
        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.columns(3, |columns| {
                    for index in 0..count {
                        let label = format!("Target {index}");
                        let column = &mut columns[index % 3];
                        let response = column.button(&label);
                        let registration = scope.register(
                            column,
                            &response,
                            format!("target-{index}"),
                            label,
                            TargetKind::Button,
                            index as u64,
                        );
                        assert!(
                            matches!(registration, RegistrationResult::Registered { .. }),
                            "target {index} registration rejected: {registration:?}"
                        );
                    }
                });
            });
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        jump.complete_frame(ctx, true, true);
    }

    fn run_root_input_frame(
        jump: &mut JumpModeUi,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<egui::Event> {
        let mut remaining = Vec::new();
        ctx.run_ui(root_input_raw(events), |ui| {
            jump.begin_frame();
            jump.handle_root_input(ctx, true, true);
            remaining = ctx.input(|input| input.events.clone());
            let _ = ui;
        })
        .drop_without_applying_deltas();
        remaining
    }

    fn key_event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    fn key_for_letter(character: char) -> egui::Key {
        match character.to_ascii_uppercase() {
            'A' => egui::Key::A,
            'B' => egui::Key::B,
            'C' => egui::Key::C,
            'D' => egui::Key::D,
            'E' => egui::Key::E,
            'F' => egui::Key::F,
            'G' => egui::Key::G,
            'H' => egui::Key::H,
            'I' => egui::Key::I,
            'J' => egui::Key::J,
            'K' => egui::Key::K,
            'L' => egui::Key::L,
            'M' => egui::Key::M,
            'N' => egui::Key::N,
            'O' => egui::Key::O,
            'P' => egui::Key::P,
            'Q' => egui::Key::Q,
            'R' => egui::Key::R,
            'S' => egui::Key::S,
            'T' => egui::Key::T,
            'U' => egui::Key::U,
            'V' => egui::Key::V,
            'W' => egui::Key::W,
            'X' => egui::Key::X,
            'Y' => egui::Key::Y,
            'Z' => egui::Key::Z,
            _ => panic!("test key must be an ASCII letter"),
        }
    }

    #[test]
    fn jump_mode_root_input_deduplicates_text_repeats_and_cancels_without_leaks() {
        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        register_root_input_targets(&mut jump, &ctx, 27);
        let leader = egui::Modifiers::SHIFT;

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        let active = jump.active_session().expect("leader starts a session");
        assert_eq!(active.hint_map().len(), 27);
        let two_letter_hint = active
            .hint_map()
            .assignments()
            .iter()
            .find(|assignment| assignment.hint.as_str().len() == 2)
            .expect("27 targets allocate one two-letter hint")
            .hint
            .as_str()
            .to_owned();
        let first_letter = two_letter_hint.chars().next().unwrap();

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(egui::Key::Space, false, false, leader)],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        false,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text(first_letter.to_ascii_lowercase().to_string()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "paired Key and Text input contributes one hint character"
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        true,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text(first_letter.to_ascii_lowercase().to_string()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "autorepeat does not advance the prefix"
        );

        let control_a = egui::Modifiers::CTRL;
        let remaining = run_root_input_frame(
            &mut jump,
            &ctx,
            vec![
                key_event(egui::Key::A, true, false, control_a),
                egui::Event::Text("a".into()),
                key_event(egui::Key::A, false, false, egui::Modifiers::NONE),
                key_event(egui::Key::ControlLeft, false, false, egui::Modifiers::NONE),
            ],
        );
        assert_eq!(
            remaining.len(),
            4,
            "shortcut-owned key, text, and unowned releases remain visible"
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string()
        );

        assert!(
            run_root_input_frame(&mut jump, &ctx, vec![egui::Event::Text("1".into())],).is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string(),
            "invalid input leaves the prior valid prefix unchanged"
        );
        assert_eq!(jump.last_outcome(), Some(&MatchOutcome::Invalid));

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        // Exercise Backspace after building a prefix again.
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    true,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            first_letter.to_string()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Backspace,
                    true,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert_eq!(jump.active_session().unwrap().prefix(), "");

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    key_for_letter(first_letter),
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(
                        key_for_letter(first_letter),
                        true,
                        false,
                        egui::Modifiers::NONE,
                    ),
                    egui::Event::Text("\u{00e5}".into()),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            "",
            "a composed non-ASCII Text event cannot resolve from its physical key"
        );

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Escape, true, false, egui::Modifiers::NONE,),
                    egui::Event::Text("z".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_none());
        assert_eq!(jump.last_outcome(), Some(&MatchOutcome::Cancelled));

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Escape,
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_some());
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, true, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(
            jump.active_session().is_some(),
            "leader repeat does not toggle"
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(egui::Key::Space, false, false, leader)],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Space, true, false, leader),
                    egui::Event::Text(" ".into()),
                ],
            )
            .is_empty()
        );
        assert!(jump.active_session().is_none(), "second leader cancels");

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(egui::Key::Space, false, false, leader)],
            )
            .is_empty()
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Backspace,
                    false,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        let long_hint_map = HintMap::assign((0..1000).map(|index| {
            TargetMetadata::new(
                TargetId::root_result(TargetGeneration(1), index),
                format!("Target {index}"),
                TargetKind::Button,
                index as u64,
            )
        }))
        .expect("the synthetic target identities are unique");
        let three_letter_hint = long_hint_map
            .assignments()
            .iter()
            .find(|assignment| assignment.hint.as_str().len() >= 3)
            .expect("the large map includes multi-character prefixes")
            .hint
            .as_str()
            .to_owned();
        let prefix: String = three_letter_hint.chars().take(2).collect();
        jump.active_session = Some(HintSession::new(long_hint_map));
        assert!(
            run_root_input_frame(&mut jump, &ctx, vec![egui::Event::Text(prefix.clone())],)
                .is_empty()
        );
        assert_eq!(jump.active_session().unwrap().prefix(), prefix);

        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![key_event(
                    egui::Key::Backspace,
                    true,
                    false,
                    egui::Modifiers::NONE,
                )],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            &prefix[..1],
            "the first rapid Backspace erases one character"
        );
        assert!(
            run_root_input_frame(
                &mut jump,
                &ctx,
                vec![
                    key_event(egui::Key::Backspace, false, false, egui::Modifiers::NONE,),
                    key_event(egui::Key::Backspace, true, false, egui::Modifiers::NONE,),
                ],
            )
            .is_empty()
        );
        assert_eq!(
            jump.active_session().unwrap().prefix(),
            "",
            "a second Backspace after a release erases the remaining prefix"
        );
    }

    fn named_scope_target(
        surface: SurfaceId,
        generation: TargetGeneration,
        namespace: &str,
        local_identity: &str,
    ) -> TargetId {
        TargetId::new(
            surface,
            SemanticTargetId::named(format!(
                "{namespace}/{}:{local_identity}",
                local_identity.len()
            )),
            generation,
        )
    }

    fn register_button(
        ui: &mut egui::Ui,
        scope: &JumpModeScope,
        local_id: &str,
        label: &str,
        kind: TargetKind,
    ) -> RegistrationResult {
        let response = ui.button(label);
        scope.register(ui, &response, local_id, label, kind, 0)
    }

    fn viewport_raw_input(viewport: egui::ViewportId) -> egui::RawInput {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut raw = egui::RawInput {
            viewport_id: viewport,
            focused: true,
            screen_rect: Some(rect),
            ..Default::default()
        };
        raw.viewports.insert(
            viewport,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                focused: Some(true),
                inner_rect: Some(rect),
                ..Default::default()
            },
        );
        raw
    }

    #[test]
    fn jump_mode_owned_viewport_binding_is_exact_and_mismatched_context_cannot_steal_input() {
        let viewport = egui::ViewportId::from_hash_of("regex owned viewport test");
        let owner = NavigationOwner::Dialog {
            kind: NavigationDialogKind::RegexTester,
            instance: 7,
        };
        let binding = NavigationViewportBinding {
            viewport,
            owner,
            base_layer: egui::LayerId::background(),
        };
        let ctx = egui::Context::default();

        ctx.run_ui(viewport_raw_input(viewport), |ctx| {
            assert_eq!(ctx.viewport_id(), viewport);
            assert!(binding.base_is_current(ctx));
            assert_eq!(
                resolve_bound_viewport_navigation_owner(ctx, binding, None),
                owner
            );
            let wrong_owner = NavigationViewportBinding {
                owner: NavigationOwner::Root,
                ..binding
            };
            assert_eq!(
                resolve_bound_viewport_navigation_owner(ctx, wrong_owner, None),
                NavigationOwner::Blocked
            );
        })
        .drop_without_applying_deltas();

        let root_focus = egui::Id::new("root-focus-must-survive-foreign-callback");
        let target = named_scope_target(
            SurfaceId::dialog("regex-tester", 7),
            TargetGeneration(7),
            "regex/form",
            "pattern",
        );
        let mut jump = JumpModeUi::new(true);
        let seed_focus_handoffs = |jump: &mut JumpModeUi| {
            let intent = FocusIntent {
                target: target.clone(),
                viewport,
                epoch: jump.navigation_epoch,
            };
            let consumed = ConsumedFocusIntent {
                intent,
                target_response_id: root_focus,
                native_focus_owner_id: Some(root_focus),
            };
            jump.last_completed_focus_handoff = Some(consumed.clone());
            jump.frame_consumed_focus_intent = Some(consumed);
        };

        let root_input = egui::RawInput {
            events: vec![egui::Event::Text("R".into())],
            ..Default::default()
        };
        ctx.run_ui(root_input, |ctx| {
            assert_eq!(ctx.viewport_id(), egui::ViewportId::ROOT);
            ctx.memory_mut(|memory| memory.request_focus(root_focus));
            seed_focus_handoffs(&mut jump);
            assert_eq!(
                jump.update_navigation_owner_for_viewport(ctx, binding, true),
                NavigationOwner::Blocked
            );
            assert!(ctx.memory(|memory| memory.has_focus(root_focus)));

            seed_focus_handoffs(&mut jump);
            jump.handle_viewport_input(ctx, true, true, viewport);
            assert!(ctx.memory(|memory| memory.has_focus(root_focus)));
            assert!(ctx.input(|input| input.events.iter().any(|event| matches!(
                event,
                egui::Event::Text(text) if text == "R"
            ))));

            seed_focus_handoffs(&mut jump);
            assert!(!jump.prepare_pass_for_viewport(ctx, binding, owner, true));
            assert!(ctx.memory(|memory| memory.has_focus(root_focus)));

            seed_focus_handoffs(&mut jump);
            jump.complete_frame_for_viewport(ctx, binding, owner, true);
            assert!(ctx.memory(|memory| memory.has_focus(root_focus)));
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn registry_retains_the_parent_ui_clip_separately_from_target_geometry() {
        let (mut jump, scope) = setup(true);
        let ctx = egui::Context::default();
        let mut response_rect = None;
        let mut parent_clip = None;
        ctx.run_ui(root_input_raw(Vec::new()), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                let response = ui.small_button("Refresh");
                response_rect = Some(response.rect);
                parent_clip = Some(ui.clip_rect());
                assert!(matches!(
                    scope.register(ui, &response, "refresh", "Refresh", TargetKind::Button, 0,),
                    RegistrationResult::Registered { .. }
                ));
            });
        })
        .drop_without_applying_deltas();

        jump.finish_ui_pass();
        let target = &jump.pass_registry().unwrap().targets()[0];
        let response_rect = response_rect.expect("the test renders its button");
        let parent_clip = parent_clip.expect("the test records the parent clip");
        assert_eq!(target.rect(), response_rect);
        assert_eq!(target.clipped_rect(), response_rect.intersect(parent_clip));
        assert_eq!(target.ui_clip_rect(), parent_clip);
        assert!(parent_clip.width() > response_rect.width());
    }

    #[test]
    fn registry_keeps_equal_labels_distinct_and_rejects_duplicate_ids() {
        let (mut jump, scope) = setup(true);
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(matches!(
                    register_button(ui, &scope, "first", "Same label", TargetKind::Button),
                    RegistrationResult::Registered { .. }
                ));
                assert!(matches!(
                    register_button(ui, &scope, "second", "Same label", TargetKind::Button),
                    RegistrationResult::Registered { .. }
                ));
                assert_eq!(
                    register_button(ui, &scope, "first", "Duplicate", TargetKind::Button),
                    RegistrationResult::Rejected(RegistrationRejection::DuplicateIdentity)
                );
            });
        });
        jump.finish_ui_pass();
        let registry = jump.pass_registry().expect("pass was collected");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.targets()[0].metadata().label(), "Same label");
        assert!(registry.targets()[0].eligible());
    }

    #[test]
    fn registry_excludes_disabled_clipped_zero_area_blocked_and_wrong_viewport_targets() {
        let (mut jump, scope) = setup(true);
        let wrong_viewport_scope =
            scope.with_surface(SurfaceId::dialog("wrong-viewport", 1), TargetGeneration(1));
        let wrong_viewport_scope = JumpModeScope {
            viewport: egui::ViewportId::from_hash_of("other viewport"),
            ..wrong_viewport_scope
        };
        let context = egui::Context::default();
        context
            .run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(false, |ui| {
                        assert_eq!(
                            register_button(ui, &scope, "disabled", "Disabled", TargetKind::Button),
                            RegistrationResult::Rejected(RegistrationRejection::Disabled)
                        );
                    });

                    let mut clipped_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                    clipped_ui.set_clip_rect(egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(0.0, 0.0),
                    ));
                    assert_eq!(
                        register_button(
                            &mut clipped_ui,
                            &scope,
                            "clipped",
                            "Clipped",
                            TargetKind::Button,
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped)
                    );

                    let zero = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::click());
                    assert_eq!(
                        scope.register(ui, &zero, "zero", "Zero", TargetKind::Button, 2),
                        RegistrationResult::Rejected(RegistrationRejection::EmptyOrClipped)
                    );

                    let hover_only = ui.add(
                        egui::Label::new("Hover only")
                            .selectable(false)
                            .sense(egui::Sense::hover()),
                    );
                    assert_eq!(
                        scope.register(
                            ui,
                            &hover_only,
                            "hover-only",
                            "Hover only",
                            TargetKind::Button,
                            3,
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::NonInteractive)
                    );

                    let response = ui.button("Wrong viewport");
                    assert_eq!(
                        wrong_viewport_scope.register(
                            ui,
                            &response,
                            "other",
                            "Other",
                            TargetKind::Button,
                            4
                        ),
                        RegistrationResult::Rejected(RegistrationRejection::WrongViewport)
                    );
                });
            })
            .drop_without_applying_deltas();

        let modal_layer =
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("active modal"));
        context
            .run_ui(egui::RawInput::default(), |ui| {
                ui.ctx()
                    .memory_mut(|memory| memory.set_modal_layer(modal_layer));
            })
            .drop_without_applying_deltas();
        let blocked_layer =
            egui::LayerId::new(egui::Order::Background, egui::Id::new("background target"));
        context
            .run_ui(egui::RawInput::default(), |ui| {
                let mut blocked_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(ui.max_rect())
                        .layer_id(blocked_layer),
                );
                let response = blocked_ui.button("Behind modal");
                assert_eq!(
                    scope.register(
                        &blocked_ui,
                        &response,
                        "blocked",
                        "Behind modal",
                        TargetKind::Button,
                        4,
                    ),
                    RegistrationResult::Rejected(RegistrationRejection::BlockedLayer)
                );
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());
    }

    #[test]
    fn popup_surface_precedes_dialog_and_root_targets() {
        let (mut jump, root) = setup(true);
        let dialog = root.with_surface(SurfaceId::dialog("settings", 3), TargetGeneration(7));
        let popup = dialog.with_surface(
            SurfaceId::popup("choice", 4, SurfaceId::dialog("settings", 3)),
            TargetGeneration(9),
        );
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Equal", TargetKind::Button);
                register_button(ui, &dialog, "dialog", "Equal", TargetKind::Button);
                register_button(ui, &popup, "popup", "Equal", TargetKind::Button);
            });
        });
        jump.finish_ui_pass();
        let targets = jump.pass_registry().unwrap().targets();
        assert_eq!(targets.len(), 1);
        assert!(matches!(targets[0].id().surface(), SurfaceId::Popup { .. }));
    }

    #[test]
    fn dashboard_and_launcher_results_share_the_root_owner() {
        let (mut jump, root) = setup(true);
        let dashboard = root.with_surface(SurfaceId::Dashboard, TargetGeneration(4));
        input_context(|ui| {
            register_button(ui, &root, "result", "Root result", TargetKind::Button);
            register_button(
                ui,
                &dashboard,
                "dashboard",
                "Dashboard widget",
                TargetKind::Button,
            );
        });
        jump.finish_ui_pass();

        let targets = jump.pass_registry().unwrap().targets();
        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|target| matches!(target.id().surface(), SurfaceId::LauncherResults))
        );
        assert!(
            targets
                .iter()
                .any(|target| matches!(target.id().surface(), SurfaceId::Dashboard))
        );
    }

    #[test]
    fn ambiguous_same_priority_dialogs_and_nested_popups_fail_closed() {
        let (mut jump, root) = setup(true);
        let first_dialog = root.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let second_dialog =
            root.with_surface(SurfaceId::dialog("settings", 2), TargetGeneration(1));
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Root", TargetKind::Button);
                register_button(
                    ui,
                    &first_dialog,
                    "first",
                    "First dialog",
                    TargetKind::Button,
                );
                register_button(
                    ui,
                    &second_dialog,
                    "second",
                    "Second dialog",
                    TargetKind::Button,
                );
            });
        });
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());

        let (mut jump, root) = setup(true);
        let dialog = root.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let parent_popup = dialog.with_surface(
            SurfaceId::popup("parent", 1, SurfaceId::dialog("notes", 1)),
            TargetGeneration(1),
        );
        let nested_popup = parent_popup.with_surface(
            SurfaceId::popup(
                "nested",
                2,
                SurfaceId::popup("parent", 1, SurfaceId::dialog("notes", 1)),
            ),
            TargetGeneration(1),
        );
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &root, "root", "Root", TargetKind::Button);
                register_button(
                    ui,
                    &parent_popup,
                    "parent",
                    "Parent popup",
                    TargetKind::Button,
                );
                register_button(
                    ui,
                    &nested_popup,
                    "nested",
                    "Nested popup",
                    TargetKind::Button,
                );
            });
        });
        jump.finish_ui_pass();
        let targets = jump.pass_registry().unwrap();
        let nested_target = nested_popup.target_id("nested");
        assert_eq!(targets.len(), 1);
        assert!(targets.get(&nested_target).is_some());

        let (mut jump, root) = setup(true);
        let dialog = root.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let first_popup = dialog.with_surface(
            SurfaceId::popup("first", 1, SurfaceId::dialog("notes", 1)),
            TargetGeneration(1),
        );
        let second_popup = dialog.with_surface(
            SurfaceId::popup("second", 2, SurfaceId::dialog("notes", 1)),
            TargetGeneration(1),
        );
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                register_button(ui, &first_popup, "first", "First popup", TargetKind::Button);
                register_button(
                    ui,
                    &second_popup,
                    "second",
                    "Second popup",
                    TargetKind::Button,
                );
            });
        });
        jump.finish_ui_pass();
        assert!(jump.pass_registry().unwrap().is_empty());
    }

    #[test]
    fn a_real_response_receives_focus_once_without_clicking_or_activating() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let target_id = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "button",
        );
        let epoch = jump.navigation_epoch();
        assert!(jump.queue_focus_intent(target_id.clone(), egui::ViewportId::ROOT, epoch));
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let ctx = egui::Context::default();
        let mut clicked = false;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.button("Focus target");
            let result = scope.register(
                ui,
                &response,
                "button",
                "Focus target",
                TargetKind::Button,
                0,
            );
            assert_eq!(
                result,
                RegistrationResult::Registered {
                    id: target_id.clone(),
                    directive: Some(FocusDirective::Focus(TargetKind::Button)),
                }
            );
            if let RegistrationResult::Registered {
                directive: Some(directive),
                ..
            } = result
            {
                directive.apply_to_response(&response);
            }
            clicked = response.clicked();
        });
        assert!(!clicked);
        jump.finish_ui_pass();
        jump.complete_frame(&ctx, true, true);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            jump.retained_registry()
                .targets()
                .first()
                .map(RegisteredTarget::response_id)
        );
        assert!(jump.pending_focus_intent.is_none());
        let has_shapes = !output.shapes.is_empty();
        output.drop_without_applying_deltas();
        assert!(has_shapes);
    }

    #[test]
    fn expanded_dialog_scope_bridge_focuses_only_the_real_current_response() {
        let ctx = egui::Context::default();
        let kind = NavigationDialogKind::FileSearch;
        let instance = 17;
        let owner = NavigationOwner::Dialog { kind, instance };
        let surface = SurfaceId::dialog(kind.surface_kind(), instance);
        let generation = TargetGeneration(4);
        let namespace = "file-search/window";
        let local_identity = "query";
        let target_id = named_scope_target(surface.clone(), generation, namespace, local_identity);
        let mut query = "unchanged".to_owned();
        ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::Window::new("File Search").show(ui.ctx(), |window_ui| {
                window_ui.add(egui::TextEdit::singleline(&mut query));
            });
        })
        .drop_without_applying_deltas();

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        assert_eq!(
            jump.update_navigation_owner(&ctx, owner, false),
            NavigationOwner::Blocked,
            "the transition frame cannot route a stale root intent into a new dialog"
        );
        jump.begin_frame();
        assert_eq!(jump.update_navigation_owner(&ctx, owner, false), owner);
        assert!(jump.queue_focus_intent(
            target_id.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let scope = jump
            .scope_for_navigation_owner(
                surface,
                generation,
                namespace,
                egui::ViewportId::ROOT,
                owner,
                false,
            )
            .expect("an enabled, current Dialog owner receives a scope");

        let mut response_id = None;
        let mut changed = false;
        let mut visual_order = 0;
        let output = ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::Window::new("File Search").show(ui.ctx(), |window_ui| {
                let response = window_ui.add(egui::TextEdit::singleline(&mut query));
                assert_eq!(response.layer_id, kind.layer_id().unwrap());
                response_id = Some(response.id);
                changed = response.changed();
                assert_eq!(
                    register_and_apply_target(
                        Some(&scope),
                        window_ui,
                        &response,
                        local_identity,
                        "Search query",
                        TargetKind::TextEntry,
                        &mut visual_order,
                    ),
                    Some(FocusDirective::Focus(TargetKind::TextEntry))
                );
            });
        });

        jump.finish_ui_pass();
        assert!(jump.prepare_pass_for_navigation_owner(&ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(&ctx, owner, false, true);
        assert_eq!(visual_order, 1);
        assert_eq!(
            query, "unchanged",
            "hint completion does not edit the field"
        );
        assert!(!changed);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            response_id,
            "the exact current dialog response receives native focus"
        );
        assert_eq!(jump.retained_registry().targets().len(), 1);
        assert_eq!(jump.retained_registry().targets()[0].id(), &target_id);
        output.drop_without_applying_deltas();
    }

    #[test]
    fn jump_mode_json_namespace_discard_preserves_form_targets_and_cleans_consumed_focus() {
        let ctx = egui::Context::default();
        let kind = NavigationDialogKind::JsonUtility;
        let instance = 23;
        let owner = NavigationOwner::Dialog { kind, instance };
        let surface = SurfaceId::dialog(kind.surface_kind(), instance);
        let form_generation = TargetGeneration(instance);
        let tree_generation = TargetGeneration(8);
        let window = |ctx: &egui::Context| {
            ctx.run_ui(root_input_raw(Vec::new()), |ui| {
                egui::Window::new("JSON Utility")
                    .id(egui::Id::new("json_utility_dialog"))
                    .show(ui.ctx(), |_| {});
            })
            .drop_without_applying_deltas();
        };
        window(&ctx);

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        assert_eq!(
            jump.update_navigation_owner(&ctx, owner, false),
            NavigationOwner::Blocked
        );
        jump.begin_frame();
        assert_eq!(jump.update_navigation_owner(&ctx, owner, false), owner);
        jump.begin_ui_pass();
        let form_scope = jump
            .scope_for_navigation_owner(
                surface.clone(),
                form_generation,
                "json-utility/form",
                egui::ViewportId::ROOT,
                owner,
                false,
            )
            .expect("the current JSON Utility owner receives a scope");
        let tree_scope = form_scope
            .child_namespace("tree")
            .with_surface(surface.clone(), tree_generation);
        let tree_target = tree_scope.target_id("node/4");
        jump.finish_ui_pass();
        jump.begin_frame();
        assert_eq!(jump.update_navigation_owner(&ctx, owner, false), owner);
        assert!(jump.queue_focus_intent(
            tree_target.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let form_scope = jump
            .scope_for_navigation_owner(
                surface.clone(),
                form_generation,
                "json-utility/form",
                egui::ViewportId::ROOT,
                owner,
                false,
            )
            .expect("the current JSON Utility owner receives a scope");
        let tree_scope = form_scope
            .child_namespace("tree")
            .with_surface(surface.clone(), tree_generation);
        let sibling_scope = form_scope.child_namespace("document/7");

        let mut form_id = None;
        let mut sibling_id = None;
        let mut row_id = None;
        let mut form_order = 0;
        let mut sibling_order = 0;
        let mut tree_order = 0;
        let output = ctx.run_ui(root_input_raw(Vec::new()), |ui| {
            egui::Window::new("JSON Utility")
                .id(egui::Id::new("json_utility_dialog"))
                .show(ui.ctx(), |window_ui| {
                    let form_response = window_ui.button("Text");
                    form_id = Some(form_response.id);
                    let _ = register_and_apply_target(
                        Some(&form_scope),
                        window_ui,
                        &form_response,
                        "view/text",
                        "Text view",
                        TargetKind::Tab,
                        &mut form_order,
                    );

                    let sibling_response = window_ui.button("Format");
                    sibling_id = Some(sibling_response.id);
                    let _ = register_and_apply_target(
                        Some(&sibling_scope),
                        window_ui,
                        &sibling_response,
                        "format",
                        "Format JSON",
                        TargetKind::Button,
                        &mut sibling_order,
                    );

                    let row_response = window_ui.button("Node");
                    row_id = Some(row_response.id);
                    assert_eq!(
                        register_and_apply_target(
                            Some(&tree_scope),
                            window_ui,
                            &row_response,
                            "node/4",
                            "JSON tree node",
                            TargetKind::Action,
                            &mut tree_order,
                        ),
                        Some(FocusDirective::Select)
                    );
                    row_response.request_focus();
                    assert!(tree_scope.record_select_focus_owner(&tree_target, row_response.id));
                    tree_scope.discard_registered_namespace();
                });
        });

        jump.finish_ui_pass();
        let pass_registry = jump.pass_registry().expect("the pass has a registry");
        assert_eq!(pass_registry.targets().len(), 2);
        assert!(
            pass_registry
                .targets()
                .iter()
                .any(|target| target.id() == &form_scope.target_id("view/text"))
        );
        assert!(
            pass_registry
                .targets()
                .iter()
                .any(|target| target.id() == &sibling_scope.target_id("format"))
        );
        assert!(
            pass_registry
                .targets()
                .iter()
                .all(|target| target.id() != &tree_target)
        );
        assert!(jump.prepare_pass_for_navigation_owner(&ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(&ctx, owner, false, true);
        assert_ne!(ctx.memory(|memory| memory.focused()), row_id);
        assert!(form_id.is_some());
        assert!(sibling_id.is_some());
        output.drop_without_applying_deltas();
    }

    #[test]
    fn jump_mode_native_complex_widgets_keep_their_normal_keyboard_semantics() {
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut slider_value = 1.0_f32;
        let slider_targets =
            render_jump_root_frame(&ctx, &mut jump, Vec::new(), Some("slider"), |ui, scope| {
                let response =
                    ui.add(egui::Slider::new(&mut slider_value, 0.0..=2.0).show_value(false));
                assert!(matches!(
                    register_and_apply_focus(
                        ui,
                        scope,
                        &response,
                        "slider",
                        "Slider",
                        TargetKind::Slider,
                        0,
                    ),
                    RegistrationResult::Registered {
                        directive: Some(FocusDirective::Focus(TargetKind::Slider)),
                        ..
                    }
                ));
            });
        assert_eq!(slider_value, 1.0, "focus does not adjust a slider");
        let slider_id = slider_targets[0].response_id();
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(slider_id));
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowRight)],
            None,
            |ui, scope| {
                let response =
                    ui.add(egui::Slider::new(&mut slider_value, 0.0..=2.0).show_value(false));
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "slider",
                    "Slider",
                    TargetKind::Slider,
                    0,
                );
            },
        );
        assert!(slider_value > 1.0 && slider_value <= 2.0);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider_id),
            "the first native slider arrow stays with the jumped-to control"
        );
        slider_value = 2.0;
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowRight)],
            None,
            |ui, scope| {
                let response =
                    ui.add(egui::Slider::new(&mut slider_value, 0.0..=2.0).show_value(false));
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "slider",
                    "Slider",
                    TargetKind::Slider,
                    0,
                );
            },
        );
        assert_eq!(
            slider_value, 2.0,
            "native slider keys retain the upper bound"
        );

        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut number = 2.0_f32;
        let number_targets =
            render_jump_root_frame(&ctx, &mut jump, Vec::new(), Some("number"), |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut number)
                        .range(0.0..=3.0)
                        .speed(0.5),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "number",
                    "Number",
                    TargetKind::TextEntry,
                    0,
                );
            });
        assert_eq!(number, 2.0, "focus does not change a DragValue");
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(number_targets[0].response_id())
        );
        let number_arrow_targets = render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowUp)],
            None,
            |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut number)
                        .range(0.0..=3.0)
                        .speed(0.5),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "number",
                    "Number",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(number_arrow_targets[0].response_id())
        );
        assert!(number > 2.0 && number <= 3.0);
        number = 3.0;
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowUp)],
            None,
            |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut number)
                        .range(0.0..=3.0)
                        .speed(0.5),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "number",
                    "Number",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(number, 3.0, "native DragValue keys retain the upper bound");
        number = 0.0;
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowDown)],
            None,
            |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut number)
                        .range(0.0..=3.0)
                        .speed(0.5),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "number",
                    "Number",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(number, 0.0, "native DragValue keys retain the lower bound");

        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut unclamped_existing = 4.0_f32;
        render_jump_root_frame(
            &ctx,
            &mut jump,
            Vec::new(),
            Some("unclamped"),
            |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut unclamped_existing)
                        .range(0.0..=3.0)
                        .clamp_existing_to_range(false),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "unclamped",
                    "Unclamped existing value",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(
            unclamped_existing, 4.0,
            "clamp_existing_to_range(false) preserves untouched values"
        );
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::ArrowUp)],
            None,
            |ui, scope| {
                let response = ui.add(
                    egui::DragValue::new(&mut unclamped_existing)
                        .range(0.0..=3.0)
                        .clamp_existing_to_range(false),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "unclamped",
                    "Unclamped existing value",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(
            unclamped_existing, 3.0,
            "native keyboard edits still clamp an out-of-range existing value"
        );

        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut selected = false;
        render_jump_root_frame(
            &ctx,
            &mut jump,
            Vec::new(),
            Some("radio/second"),
            |ui, scope| {
                let first = ui.radio_value(&mut selected, false, "First");
                register_and_apply_focus(
                    ui,
                    scope,
                    &first,
                    "radio/first",
                    "First",
                    TargetKind::Selectable,
                    0,
                );
                let second = ui.radio_value(&mut selected, true, "Second");
                register_and_apply_focus(
                    ui,
                    scope,
                    &second,
                    "radio/second",
                    "Second",
                    TargetKind::Selectable,
                    1,
                );
            },
        );
        assert!(!selected, "the radio focus handoff never selects");
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![key(egui::Key::Space)],
            None,
            |ui, scope| {
                let first = ui.radio_value(&mut selected, false, "First");
                register_and_apply_focus(
                    ui,
                    scope,
                    &first,
                    "radio/first",
                    "First",
                    TargetKind::Selectable,
                    0,
                );
                let second = ui.radio_value(&mut selected, true, "Second");
                register_and_apply_focus(
                    ui,
                    scope,
                    &second,
                    "radio/second",
                    "Second",
                    TargetKind::Selectable,
                    1,
                );
            },
        );
        assert!(selected, "a later native Space activates the focused radio");

        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut text = String::new();
        let editor_id = egui::Id::new("jump-mode-native-multiline-editor");
        render_jump_root_frame(&ctx, &mut jump, Vec::new(), Some("editor"), |ui, scope| {
            let response = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(editor_id)
                    .desired_rows(3),
            );
            register_and_apply_focus(
                ui,
                scope,
                &response,
                "editor",
                "Editor",
                TargetKind::TextEntry,
                0,
            );
        });
        assert!(
            text.is_empty(),
            "the multiline focus handoff does not edit text"
        );
        render_jump_root_frame(
            &ctx,
            &mut jump,
            vec![
                egui::Event::Text("a".into()),
                key(egui::Key::Enter),
                egui::Event::Text("b".into()),
            ],
            None,
            |ui, scope| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(editor_id)
                        .desired_rows(3),
                );
                register_and_apply_focus(
                    ui,
                    scope,
                    &response,
                    "editor",
                    "Editor",
                    TargetKind::TextEntry,
                    0,
                );
            },
        );
        assert_eq!(
            text, "a\nb",
            "native text and Enter preserve the editor caret"
        );
    }

    #[test]
    fn action_targets_request_logical_selection_without_running_actions() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let intent = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "card",
        );
        jump.queue_focus_intent(intent, egui::ViewportId::ROOT, jump.navigation_epoch());
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut actions_run = 0;
        let mut visual_order = 0;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = ui.button("Action card");
                let directive = register_and_apply_target(
                    Some(&scope),
                    ui,
                    &response,
                    "card",
                    "Action card",
                    TargetKind::Action,
                    &mut visual_order,
                );
                assert_eq!(
                    directive,
                    Some(FocusDirective::Select),
                    "the renderer receives Select rather than the adapter activating the response"
                );
                assert!(!response.clicked());
                if directive == Some(FocusDirective::Select) {
                    // The owning surface will update selection in a later milestone.
                    actions_run += 0;
                }
            });
        });
        jump.finish_ui_pass();
        assert_eq!(visual_order, 1);
        assert_eq!(actions_run, 0);
    }

    #[test]
    fn generation_owner_and_epoch_mismatches_fail_closed_and_consume_once() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let old_id = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        let epoch = jump.navigation_epoch();
        jump.queue_focus_intent(old_id.clone(), egui::ViewportId::ROOT, epoch);
        jump.begin_ui_pass();
        let root_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        assert_eq!(root_scope.target_id("target"), old_id);

        // A newer pass replaces the old pass geometry without inheriting it.
        jump.finish_ui_pass();
        jump.begin_ui_pass();
        let new_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(2),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut stale_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } = register_button(
                    ui,
                    &new_scope,
                    "target",
                    "New generation",
                    TargetKind::Button,
                ) {
                    stale_directive = directive;
                }
            });
        });
        assert_eq!(stale_directive, None);
        jump.finish_ui_pass();
        assert_eq!(jump.pass_registry().unwrap().len(), 1);
        let ctx = egui::Context::default();
        jump.complete_frame(&ctx, true, true);
        assert!(jump.pending_focus_intent.is_none());

        // A target on another owner and a target from an older navigation epoch
        // cannot consume a newly queued intent.
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let epoch = jump.navigation_epoch();
        let expected = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        jump.queue_focus_intent(expected, egui::ViewportId::ROOT, epoch);
        jump.begin_ui_pass();
        let root_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let wrong_owner =
            root_scope.with_surface(SurfaceId::dialog("notes", 1), TargetGeneration(1));
        let mut wrong_owner_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } = register_button(
                    ui,
                    &wrong_owner,
                    "target",
                    "Same local ID",
                    TargetKind::Button,
                ) {
                    wrong_owner_directive = directive;
                }
            });
        });
        assert_eq!(wrong_owner_directive, None);
        jump.finish_ui_pass();
        jump.complete_frame(&egui::Context::default(), true, true);
        assert!(jump.pending_focus_intent.is_none());

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        let stale_epoch = jump.navigation_epoch();
        jump.advance_navigation_epoch();
        let stale_target = named_scope_target(
            SurfaceId::LauncherResults,
            TargetGeneration(1),
            "test",
            "target",
        );
        assert!(jump.queue_focus_intent(stale_target, egui::ViewportId::ROOT, stale_epoch));
        jump.begin_ui_pass();
        let current_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let mut old_epoch_directive = None;
        input_context(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let RegistrationResult::Registered { directive, .. } =
                    register_button(ui, &current_scope, "target", "Target", TargetKind::Button)
                {
                    old_epoch_directive = directive;
                }
            });
        });
        assert_eq!(old_epoch_directive, None);
        jump.finish_ui_pass();
        jump.complete_frame(&egui::Context::default(), true, true);
        assert!(jump.pending_focus_intent.is_none());
    }

    #[test]
    fn a_new_ui_pass_replaces_geometry_and_only_the_completed_pass_is_retained() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let first_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "results",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            register_button(
                ui,
                &first_scope,
                "old",
                "Discarded pass",
                TargetKind::Button,
            );
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        assert_eq!(jump.pass_registry().unwrap().len(), 1);

        jump.begin_ui_pass();
        let final_scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(2),
                "results",
                egui::ViewportId::ROOT,
                true,
            )
            .unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            register_button(ui, &final_scope, "new", "Final pass", TargetKind::Button);
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();

        let final_registry = jump.pass_registry().unwrap();
        assert_eq!(final_registry.len(), 1);
        assert_eq!(final_registry.targets()[0].metadata().label(), "Final pass");
        assert_eq!(
            final_registry.targets()[0].id().generation(),
            TargetGeneration(2)
        );
        let expected_retained = final_registry.clone();

        jump.complete_frame(&ctx, true, true);
        assert_eq!(jump.retained_registry(), &expected_retained);
    }

    #[test]
    fn disabled_bridge_has_no_scope_or_registry_and_clears_retained_state() {
        let mut jump = JumpModeUi::new(false);
        jump.begin_frame();
        jump.begin_ui_pass();
        assert!(
            jump.scope(
                SurfaceId::LauncherResults,
                TargetGeneration(1),
                "test",
                egui::ViewportId::ROOT,
                true,
            )
            .is_none()
        );
        assert!(jump.pass_registry().is_none());
        assert!(jump.retained_registry().is_empty());
        jump.queue_focus_intent(
            TargetId::root_result(TargetGeneration(1), 0),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        );
        assert!(jump.pending_focus_intent.is_none());
    }

    #[test]
    fn focus_intents_are_rejected_after_a_ui_pass_starts() {
        let (mut jump, scope) = setup(true);
        assert!(!jump.queue_focus_intent(
            scope.target_id("too-late"),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        assert!(jump.pending_focus_intent.is_none());
    }
}
