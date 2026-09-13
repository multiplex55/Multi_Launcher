use super::geometry::{LogicalPoint, PhysicalPoint};
use super::model::{CellId, ConfigRevision, InteractionMode, InvocationId, MenuId, SessionId};
use std::collections::BTreeMap;

const ARMING_DISTANCE_SQUARED: f32 = 16.0;
const DRAG_DISTANCE_SQUARED: f32 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardOwnership {
    MenuNavigation,
    ExternalApplication,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellRole {
    Action,
    Submenu,
    Spacer,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct MenuFrame {
    pub frame_id: FrameId,
    pub menu_id: MenuId,
    pub origin: PhysicalPoint,
    pub geometry_generation: u64,
    pub page: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingPress {
    pub cell_id: CellId,
    pub button: PointerButton,
    pub session_generation: u64,
    pub geometry_generation: u64,
    pub origin: LogicalPoint,
    pub dragged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DynamicCacheKey {
    pub frame_id: FrameId,
    pub menu_id: MenuId,
    pub cell_id: CellId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmingBaseline {
    pub point: LogicalPoint,
    pub geometry_generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    pub session_id: SessionId,
    pub definition_revision: ConfigRevision,
    pub stack: Vec<MenuFrame>,
    pub frozen_dynamic_results: BTreeMap<DynamicCacheKey, Vec<String>>,
    pub hovered: Option<CellId>,
    pub selected: Option<CellId>,
    pub keyboard_ownership: KeyboardOwnership,
    pub pending_press: Option<PendingPress>,
    pub interaction: InteractionMode,
    pub consumed_invocation: Option<InvocationId>,
    pub session_generation: u64,
    pub armed: bool,
    pub arming_baseline: ArmingBaseline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    PointerMoved {
        point: LogicalPoint,
        hovered: Option<CellId>,
        geometry_generation: u64,
    },
    PointerDown {
        point: LogicalPoint,
        cell: Option<CellId>,
        role: CellRole,
        button: PointerButton,
        geometry_generation: u64,
    },
    PointerUp {
        point: LogicalPoint,
        cell: Option<CellId>,
        role: CellRole,
        button: PointerButton,
        geometry_generation: u64,
    },
    TriggerReleased {
        point: LogicalPoint,
        cell: Option<CellId>,
        role: CellRole,
        geometry_generation: u64,
    },
    OpenChild {
        menu_id: MenuId,
        origin: PhysicalPoint,
        geometry_generation: u64,
        pointer_baseline: LogicalPoint,
    },
    Back {
        geometry_generation: u64,
        pointer_baseline: LogicalPoint,
    },
    PageChanged {
        page: usize,
        geometry_generation: u64,
        pointer_baseline: LogicalPoint,
    },
    DisplayRelayout {
        geometry_generation: u64,
        pointer_baseline: LogicalPoint,
    },
    OutsideInteraction,
    MenuInteraction,
    SelectKeyboard {
        cell: CellId,
    },
    FreezeDynamic {
        frame_id: FrameId,
        source_cell: CellId,
        results: Vec<String>,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionIntent {
    Dispatch {
        cell_id: CellId,
        token: DispatchToken,
    },
    OpenSubmenu {
        cell_id: CellId,
    },
    CloseTree,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DispatchToken {
    pub session_generation: u64,
    pub ordinal: u64,
}

pub struct SessionReducer {
    pub state: SessionState,
    invocation_id: InvocationId,
    next_dispatch: u64,
    next_frame: u64,
    closed: bool,
}

impl SessionReducer {
    pub fn new(
        session_id: SessionId,
        definition_revision: ConfigRevision,
        root_menu: MenuId,
        origin: PhysicalPoint,
        geometry_generation: u64,
        interaction: InteractionMode,
        invocation: InvocationId,
        open_pointer: LogicalPoint,
    ) -> Self {
        Self {
            state: SessionState {
                session_id,
                definition_revision,
                stack: vec![MenuFrame {
                    frame_id: FrameId(1),
                    menu_id: root_menu,
                    origin,
                    geometry_generation,
                    page: 0,
                }],
                frozen_dynamic_results: BTreeMap::new(),
                hovered: None,
                selected: None,
                keyboard_ownership: KeyboardOwnership::MenuNavigation,
                pending_press: None,
                interaction,
                consumed_invocation: None,
                session_generation: 1,
                armed: false,
                arming_baseline: ArmingBaseline {
                    point: open_pointer,
                    geometry_generation,
                },
            },
            invocation_id: invocation,
            next_dispatch: 1,
            next_frame: 2,
            closed: false,
        }
    }

    pub fn reduce(&mut self, event: SessionEvent) -> Vec<SessionIntent> {
        if self.closed {
            return vec![];
        }
        match event {
            SessionEvent::PointerMoved {
                point,
                hovered,
                geometry_generation,
            } => {
                if geometry_generation != self.current_geometry() {
                    return vec![];
                }
                self.state.hovered = hovered;
                if let Some(press) = self.state.pending_press.as_mut() {
                    if press.geometry_generation == geometry_generation
                        && distance2(point, press.origin) > DRAG_DISTANCE_SQUARED
                    {
                        press.dragged = true;
                    }
                }
                if self.state.arming_baseline.geometry_generation == geometry_generation
                    && distance2(point, self.state.arming_baseline.point) >= ARMING_DISTANCE_SQUARED
                {
                    self.state.armed = true;
                }
                vec![]
            }
            SessionEvent::PointerDown {
                point,
                cell: Some(cell_id),
                role,
                button,
                geometry_generation,
            } if actionable(role) && self.current_geometry() == geometry_generation => {
                self.state.armed = true;
                self.state.pending_press = Some(PendingPress {
                    cell_id,
                    button,
                    session_generation: self.state.session_generation,
                    geometry_generation,
                    origin: point,
                    dragged: false,
                });
                vec![]
            }
            SessionEvent::PointerDown { .. } => {
                self.state.pending_press = None;
                vec![]
            }
            SessionEvent::PointerUp {
                point,
                cell,
                role,
                button,
                geometry_generation,
            } => {
                let Some(mut press) = self.state.pending_press.take() else {
                    return vec![];
                };
                if distance2(point, press.origin) > DRAG_DISTANCE_SQUARED {
                    press.dragged = true;
                }
                if press.dragged
                    || press.button != button
                    || press.session_generation != self.state.session_generation
                    || press.geometry_generation != geometry_generation
                    || Some(&press.cell_id) != cell.as_ref()
                    || !actionable(role)
                {
                    return vec![];
                }
                self.activate(press.cell_id, role, point, geometry_generation)
            }
            SessionEvent::TriggerReleased {
                point,
                cell,
                role,
                geometry_generation,
            } => {
                if self.state.interaction != InteractionMode::ReleaseToSelect
                    || self.state.consumed_invocation == Some(self.invocation_id)
                {
                    return vec![];
                }
                if !self.state.armed
                    || self.current_geometry() != geometry_generation
                    || !actionable(role)
                {
                    return self.cancel_tree();
                }
                let Some(cell_id) = cell else {
                    return self.cancel_tree();
                };
                self.activate(cell_id, role, point, geometry_generation)
            }
            SessionEvent::OpenChild {
                menu_id,
                origin,
                geometry_generation,
                pointer_baseline,
            } => {
                self.bump_generation();
                let frame_id = FrameId(self.next_frame);
                self.next_frame = self.next_frame.saturating_add(1);
                self.state.stack.push(MenuFrame {
                    frame_id,
                    menu_id,
                    origin,
                    geometry_generation,
                    page: 0,
                });
                self.disarm(pointer_baseline, geometry_generation);
                vec![]
            }
            SessionEvent::Back {
                geometry_generation,
                pointer_baseline,
            } => {
                let popped = self.state.stack.len() > 1;
                if popped {
                    self.state.stack.pop();
                    self.bump_generation();
                }
                if let Some(frame) = self.state.stack.last_mut() {
                    frame.geometry_generation = geometry_generation;
                }
                self.disarm(pointer_baseline, geometry_generation);
                if popped {
                    vec![SessionIntent::Back]
                } else {
                    vec![]
                }
            }
            SessionEvent::PageChanged {
                page,
                geometry_generation,
                pointer_baseline,
            } => {
                if let Some(frame) = self.state.stack.last_mut() {
                    frame.page = page;
                    frame.geometry_generation = geometry_generation;
                }
                self.bump_generation();
                self.disarm(pointer_baseline, geometry_generation);
                vec![]
            }
            SessionEvent::DisplayRelayout {
                geometry_generation,
                pointer_baseline,
            } => {
                if let Some(frame) = self.state.stack.last_mut() {
                    frame.geometry_generation = geometry_generation;
                }
                self.bump_generation();
                self.disarm(pointer_baseline, geometry_generation);
                vec![]
            }
            SessionEvent::OutsideInteraction => {
                self.state.keyboard_ownership = KeyboardOwnership::ExternalApplication;
                self.state.pending_press = None;
                vec![]
            }
            SessionEvent::MenuInteraction => {
                self.state.keyboard_ownership = KeyboardOwnership::MenuNavigation;
                vec![]
            }
            SessionEvent::SelectKeyboard { cell } => {
                if self.state.keyboard_ownership == KeyboardOwnership::MenuNavigation {
                    self.state.selected = Some(cell);
                    self.state.armed = true;
                }
                vec![]
            }
            SessionEvent::FreezeDynamic {
                frame_id,
                source_cell,
                results,
            } => {
                let Some(frame) = self.state.stack.last() else {
                    return vec![];
                };
                if frame.frame_id != frame_id {
                    return vec![];
                }
                let key = DynamicCacheKey {
                    frame_id,
                    menu_id: frame.menu_id.clone(),
                    cell_id: source_cell,
                };
                self.state
                    .frozen_dynamic_results
                    .entry(key)
                    .or_insert(results);
                vec![]
            }
            SessionEvent::Close => self.cancel_tree(),
        }
    }

    fn activate(
        &mut self,
        cell_id: CellId,
        role: CellRole,
        point: LogicalPoint,
        geometry_generation: u64,
    ) -> Vec<SessionIntent> {
        if role == CellRole::Submenu {
            if self.state.interaction == InteractionMode::ReleaseToSelect {
                self.state.interaction = InteractionMode::StickyClick;
                self.state.consumed_invocation = Some(self.invocation_id);
            }
            self.disarm(point, geometry_generation);
            return vec![SessionIntent::OpenSubmenu { cell_id }];
        }
        if role != CellRole::Action {
            return vec![];
        }
        if self.state.interaction == InteractionMode::ReleaseToSelect {
            if self.state.consumed_invocation == Some(self.invocation_id) {
                return vec![];
            }
            self.state.consumed_invocation = Some(self.invocation_id);
        }
        let token = DispatchToken {
            session_generation: self.state.session_generation,
            ordinal: self.next_dispatch,
        };
        self.next_dispatch = self.next_dispatch.saturating_add(1);
        vec![SessionIntent::Dispatch { cell_id, token }]
    }
    fn cancel_tree(&mut self) -> Vec<SessionIntent> {
        self.closed = true;
        self.state.pending_press = None;
        vec![SessionIntent::CloseTree]
    }
    fn current_geometry(&self) -> u64 {
        self.state
            .stack
            .last()
            .map_or(0, |frame| frame.geometry_generation)
    }
    fn bump_generation(&mut self) {
        self.state.session_generation = self.state.session_generation.saturating_add(1);
    }
    fn disarm(&mut self, point: LogicalPoint, geometry_generation: u64) {
        self.state.armed = false;
        self.state.hovered = None;
        self.state.selected = None;
        self.state.pending_press = None;
        self.state.arming_baseline = ArmingBaseline {
            point,
            geometry_generation,
        };
    }
}

fn actionable(role: CellRole) -> bool {
    matches!(role, CellRole::Action | CellRole::Submenu)
}
fn distance2(a: LogicalPoint, b: LogicalPoint) -> f32 {
    let x = a.x - b.x;
    let y = a.y - b.y;
    x * x + y * y
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reducer(mode: InteractionMode) -> SessionReducer {
        SessionReducer::new(
            SessionId::new("s"),
            ConfigRevision(4),
            MenuId::new("root"),
            PhysicalPoint::default(),
            10,
            mode,
            InvocationId(1),
            p(0.0, 0.0),
        )
    }
    fn p(x: f32, y: f32) -> LogicalPoint {
        LogicalPoint { x, y }
    }
    fn move_to(
        r: &mut SessionReducer,
        point: LogicalPoint,
        hovered: Option<CellId>,
        generation: u64,
    ) {
        r.reduce(SessionEvent::PointerMoved {
            point,
            hovered,
            geometry_generation: generation,
        });
    }

    #[test]
    fn release_to_select_cancellations_close_tree() {
        for (cell, role, generation) in [
            (None, CellRole::Spacer, 10),
            (Some(CellId::new("x")), CellRole::Unavailable, 10),
            (Some(CellId::new("x")), CellRole::Action, 9),
        ] {
            let mut r = reducer(InteractionMode::ReleaseToSelect);
            move_to(&mut r, p(5.0, 0.0), cell.clone(), 10);
            assert_eq!(
                r.reduce(SessionEvent::TriggerReleased {
                    point: p(5.0, 0.0),
                    cell,
                    role,
                    geometry_generation: generation
                }),
                vec![SessionIntent::CloseTree]
            );
        }
        let mut unarmed = reducer(InteractionMode::ReleaseToSelect);
        assert_eq!(
            unarmed.reduce(SessionEvent::TriggerReleased {
                point: p(0.0, 0.0),
                cell: Some(CellId::new("x")),
                role: CellRole::Action,
                geometry_generation: 10
            }),
            vec![SessionIntent::CloseTree]
        );
    }
    #[test]
    fn deliberate_generation_current_movement_arms_release_selection() {
        let mut r = reducer(InteractionMode::ReleaseToSelect);
        move_to(&mut r, p(5.0, 0.0), Some(CellId::new("a")), 9);
        assert!(!r.state.armed);
        move_to(&mut r, p(5.0, 0.0), Some(CellId::new("a")), 10);
        assert!(r.state.armed);
        assert!(matches!(
            r.reduce(SessionEvent::TriggerReleased {
                point: p(5.0, 0.0),
                cell: Some(CellId::new("a")),
                role: CellRole::Action,
                geometry_generation: 10
            })
            .as_slice(),
            [SessionIntent::Dispatch { .. }]
        ));
    }
    #[test]
    fn child_back_page_and_relayout_reset_baseline_and_reject_stale_moves() {
        let mut r = reducer(InteractionMode::ReleaseToSelect);
        move_to(&mut r, p(5.0, 0.0), None, 10);
        r.reduce(SessionEvent::OpenChild {
            menu_id: MenuId::new("child"),
            origin: PhysicalPoint::default(),
            geometry_generation: 11,
            pointer_baseline: p(5.0, 0.0),
        });
        move_to(&mut r, p(6.0, 0.0), None, 10);
        assert!(!r.state.armed);
        move_to(&mut r, p(10.0, 0.0), None, 11);
        assert!(r.state.armed);
        r.reduce(SessionEvent::Back {
            geometry_generation: 12,
            pointer_baseline: p(10.0, 0.0),
        });
        assert!(!r.state.armed);
        r.reduce(SessionEvent::PageChanged {
            page: 1,
            geometry_generation: 13,
            pointer_baseline: p(10.0, 0.0),
        });
        assert!(!r.state.armed);
        r.reduce(SessionEvent::DisplayRelayout {
            geometry_generation: 14,
            pointer_baseline: p(10.0, 0.0),
        });
        assert!(!r.state.armed);
        move_to(&mut r, p(20.0, 0.0), None, 13);
        assert!(!r.state.armed);
    }
    #[test]
    fn drag_out_and_return_still_cancels_click() {
        let mut r = reducer(InteractionMode::StickyClick);
        r.reduce(SessionEvent::PointerDown {
            point: p(0.0, 0.0),
            cell: Some(CellId::new("a")),
            role: CellRole::Action,
            button: PointerButton::Primary,
            geometry_generation: 10,
        });
        move_to(&mut r, p(20.0, 0.0), None, 10);
        move_to(&mut r, p(0.0, 0.0), Some(CellId::new("a")), 10);
        assert!(
            r.reduce(SessionEvent::PointerUp {
                point: p(0.0, 0.0),
                cell: Some(CellId::new("a")),
                role: CellRole::Action,
                button: PointerButton::Primary,
                geometry_generation: 10
            })
            .is_empty()
        );
    }
    #[test]
    fn click_requires_same_cell_button_and_generation() {
        let mut r = reducer(InteractionMode::StickyClick);
        r.reduce(SessionEvent::PointerDown {
            point: p(1.0, 1.0),
            cell: Some(CellId::new("a")),
            role: CellRole::Action,
            button: PointerButton::Primary,
            geometry_generation: 10,
        });
        assert!(
            r.reduce(SessionEvent::PointerUp {
                point: p(1.0, 1.0),
                cell: Some(CellId::new("b")),
                role: CellRole::Action,
                button: PointerButton::Primary,
                geometry_generation: 10
            })
            .is_empty()
        );
    }
    #[test]
    fn release_submenu_converts_to_sticky_and_cannot_double_dispatch() {
        let mut r = reducer(InteractionMode::ReleaseToSelect);
        move_to(&mut r, p(5.0, 0.0), Some(CellId::new("sub")), 10);
        assert!(matches!(
            r.reduce(SessionEvent::TriggerReleased {
                point: p(5.0, 0.0),
                cell: Some(CellId::new("sub")),
                role: CellRole::Submenu,
                geometry_generation: 10
            })
            .as_slice(),
            [SessionIntent::OpenSubmenu { .. }]
        ));
        assert_eq!(r.state.interaction, InteractionMode::StickyClick);
    }
    #[test]
    fn frozen_dynamic_results_are_frame_and_menu_scoped() {
        let mut r = reducer(InteractionMode::StickyClick);
        let cell = CellId::new("dynamic");
        r.reduce(SessionEvent::FreezeDynamic {
            frame_id: FrameId(1),
            source_cell: cell.clone(),
            results: vec!["root".into()],
        });
        r.reduce(SessionEvent::OpenChild {
            menu_id: MenuId::new("child"),
            origin: PhysicalPoint::default(),
            geometry_generation: 11,
            pointer_baseline: p(0.0, 0.0),
        });
        r.reduce(SessionEvent::FreezeDynamic {
            frame_id: FrameId(1),
            source_cell: cell.clone(),
            results: vec!["stale".into()],
        });
        r.reduce(SessionEvent::FreezeDynamic {
            frame_id: FrameId(2),
            source_cell: cell.clone(),
            results: vec!["child".into()],
        });
        assert_eq!(r.state.frozen_dynamic_results.len(), 2);
        assert!(
            r.state
                .frozen_dynamic_results
                .iter()
                .any(|(key, value)| key.menu_id == MenuId::new("root") && value == &vec!["root"])
        );
        assert!(
            r.state
                .frozen_dynamic_results
                .iter()
                .any(|(key, value)| key.menu_id == MenuId::new("child") && value == &vec!["child"])
        );
    }
    #[test]
    fn outside_interaction_releases_only_keyboard_navigation() {
        let mut r = reducer(InteractionMode::StickyClick);
        r.reduce(SessionEvent::OutsideInteraction);
        assert_eq!(
            r.state.keyboard_ownership,
            KeyboardOwnership::ExternalApplication
        );
        assert_eq!(r.state.stack.len(), 1);
        r.reduce(SessionEvent::SelectKeyboard {
            cell: CellId::new("a"),
        });
        assert!(r.state.selected.is_none());
    }
    #[test]
    fn stale_click_up_after_relayout_cannot_dispatch() {
        let mut r = reducer(InteractionMode::StickyClick);
        r.reduce(SessionEvent::PointerDown {
            point: p(0.0, 0.0),
            cell: Some(CellId::new("a")),
            role: CellRole::Action,
            button: PointerButton::Primary,
            geometry_generation: 10,
        });
        r.reduce(SessionEvent::DisplayRelayout {
            geometry_generation: 11,
            pointer_baseline: p(0.0, 0.0),
        });
        assert!(
            r.reduce(SessionEvent::PointerUp {
                point: p(0.0, 0.0),
                cell: Some(CellId::new("a")),
                role: CellRole::Action,
                button: PointerButton::Primary,
                geometry_generation: 10
            })
            .is_empty()
        );
    }
    #[test]
    fn click_then_trigger_release_cannot_double_dispatch() {
        let mut r = reducer(InteractionMode::ReleaseToSelect);
        r.reduce(SessionEvent::PointerDown {
            point: p(0.0, 0.0),
            cell: Some(CellId::new("a")),
            role: CellRole::Action,
            button: PointerButton::Primary,
            geometry_generation: 10,
        });
        assert_eq!(
            r.reduce(SessionEvent::PointerUp {
                point: p(0.0, 0.0),
                cell: Some(CellId::new("a")),
                role: CellRole::Action,
                button: PointerButton::Primary,
                geometry_generation: 10
            })
            .len(),
            1
        );
        assert!(
            r.reduce(SessionEvent::TriggerReleased {
                point: p(0.0, 0.0),
                cell: Some(CellId::new("a")),
                role: CellRole::Action,
                geometry_generation: 10
            })
            .is_empty()
        );
    }
}
