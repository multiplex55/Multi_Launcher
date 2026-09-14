use super::dynamic::FrozenRadialEntry;
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavigationModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub alt_gr: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationCommand {
    Next,
    Previous,
    ActivatePrimary,
    ActivateSecondary,
    Back,
    NextPage,
    PreviousPage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellRole {
    Action,
    Submenu,
    Back,
    Close,
    NextPage,
    PreviousPage,
    Spacer,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct MenuFrame {
    pub frame_id: FrameId,
    pub parent_frame_id: Option<FrameId>,
    pub menu_id: MenuId,
    pub origin: PhysicalPoint,
    pub geometry_generation: u64,
    pub scale_factor: f64,
    pub page: usize,
    pub page_count: usize,
    pub selected: Option<CellId>,
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
    pub frozen_dynamic_results: BTreeMap<DynamicCacheKey, Vec<FrozenRadialEntry>>,
    pub hovered: Option<CellId>,
    pub selected: Option<CellId>,
    pub keyboard_ownership: KeyboardOwnership,
    pub pending_press: Option<PendingPress>,
    pub interaction: InteractionMode,
    pub consumed_invocation: Option<InvocationId>,
    pub session_generation: u64,
    pub armed: bool,
    pub arming_baseline: ArmingBaseline,
    pub modifiers: NavigationModifiers,
    pub dwell_candidate: Option<(CellId, u64)>,
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
    Navigate {
        command: NavigationCommand,
        cells: Vec<(CellId, CellRole)>,
        pointer_baseline: LogicalPoint,
        geometry_generation: u64,
    },
    ModifiersChanged(NavigationModifiers),
    StartDwell {
        cell: CellId,
        role: CellRole,
        deadline: u64,
    },
    DwellExpired {
        cell: CellId,
        role: CellRole,
        at: u64,
        point: LogicalPoint,
        geometry_generation: u64,
    },
    FreezeDynamic {
        frame_id: FrameId,
        source_cell: CellId,
        results: Vec<FrozenRadialEntry>,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionIntent {
    Dispatch {
        cell_id: CellId,
        token: DispatchToken,
        button: PointerButton,
        modifiers: NavigationModifiers,
        source: crate::commands::ActivationSource,
    },
    OpenSubmenu {
        cell_id: CellId,
    },
    CloseTree,
    Back,
    PageChanged {
        page: usize,
    },
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
                    parent_frame_id: None,
                    menu_id: root_menu,
                    origin,
                    geometry_generation,
                    scale_factor: 1.0,
                    page: 0,
                    page_count: 1,
                    selected: None,
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
                modifiers: NavigationModifiers::default(),
                dwell_candidate: None,
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
                if self
                    .state
                    .dwell_candidate
                    .as_ref()
                    .is_some_and(|(cell, _)| Some(cell) != self.state.hovered.as_ref())
                {
                    self.state.dwell_candidate = None;
                }
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
                self.state.dwell_candidate = None;
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
                self.state.dwell_candidate = None;
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
                self.activate(
                    press.cell_id,
                    role,
                    point,
                    geometry_generation,
                    button,
                    crate::commands::ActivationSource::Click,
                )
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
                self.activate(
                    cell_id,
                    role,
                    point,
                    geometry_generation,
                    PointerButton::Primary,
                    crate::commands::ActivationSource::RadialRelease,
                )
            }
            SessionEvent::OpenChild {
                menu_id,
                origin,
                geometry_generation,
                pointer_baseline,
            } => {
                if !self.bump_generation() {
                    return self.cancel_tree();
                }
                let frame_id = FrameId(self.next_frame);
                let Some(next_frame) = self.next_frame.checked_add(1) else {
                    return self.cancel_tree();
                };
                self.next_frame = next_frame;
                let parent_frame_id = self.state.stack.last().map(|frame| frame.frame_id);
                let scale_factor = self
                    .state
                    .stack
                    .last()
                    .map_or(1.0, |frame| frame.scale_factor);
                if let Some(parent) = self.state.stack.last_mut() {
                    parent.selected = self.state.selected.clone();
                }
                self.state.stack.push(MenuFrame {
                    frame_id,
                    parent_frame_id,
                    menu_id,
                    origin,
                    geometry_generation,
                    scale_factor,
                    page: 0,
                    page_count: 1,
                    selected: None,
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
                    if !self.bump_generation() {
                        return self.cancel_tree();
                    }
                }
                if let Some(frame) = self.state.stack.last_mut() {
                    frame.geometry_generation = geometry_generation;
                }
                let restored = self
                    .state
                    .stack
                    .last()
                    .and_then(|frame| frame.selected.clone());
                self.disarm(pointer_baseline, geometry_generation);
                self.state.selected = restored;
                if popped {
                    vec![SessionIntent::Back]
                } else {
                    vec![]
                }
            }
            SessionEvent::PageChanged {
                mut page,
                geometry_generation,
                pointer_baseline,
            } => {
                if let Some(frame) = self.state.stack.last_mut() {
                    page = page.min(frame.page_count.saturating_sub(1));
                    frame.page = page;
                    frame.geometry_generation = geometry_generation;
                }
                if !self.bump_generation() {
                    return self.cancel_tree();
                }
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
                if !self.bump_generation() {
                    return self.cancel_tree();
                }
                self.disarm(pointer_baseline, geometry_generation);
                vec![]
            }
            SessionEvent::OutsideInteraction => {
                self.state.keyboard_ownership = KeyboardOwnership::ExternalApplication;
                self.state.pending_press = None;
                self.state.dwell_candidate = None;
                vec![]
            }
            SessionEvent::MenuInteraction => {
                self.state.keyboard_ownership = KeyboardOwnership::MenuNavigation;
                vec![]
            }
            SessionEvent::SelectKeyboard { cell } => {
                if self.state.keyboard_ownership == KeyboardOwnership::MenuNavigation {
                    self.state.selected = Some(cell.clone());
                    if let Some(frame) = self.state.stack.last_mut() {
                        frame.selected = Some(cell);
                    }
                    self.state.armed = true;
                }
                vec![]
            }
            SessionEvent::ModifiersChanged(modifiers) => {
                self.state.modifiers = modifiers;
                vec![]
            }
            SessionEvent::Navigate {
                command,
                cells,
                pointer_baseline,
                geometry_generation,
            } => self.navigate(command, cells, pointer_baseline, geometry_generation),
            SessionEvent::StartDwell {
                cell,
                role,
                deadline,
            } => {
                if role != CellRole::Submenu {
                    self.state.dwell_candidate = None;
                    return vec![];
                }
                if self
                    .state
                    .dwell_candidate
                    .as_ref()
                    .is_none_or(|(current, _)| current != &cell)
                {
                    self.state.dwell_candidate = Some((cell, deadline));
                }
                vec![]
            }
            SessionEvent::DwellExpired {
                cell,
                role,
                at,
                point,
                geometry_generation,
            } => {
                if self.state.dwell_candidate.as_ref() != Some(&(cell.clone(), at))
                    || self.current_geometry() != geometry_generation
                {
                    return vec![];
                }
                self.state.dwell_candidate = None;
                if role != CellRole::Submenu {
                    return vec![];
                }
                self.activate(
                    cell,
                    role,
                    point,
                    geometry_generation,
                    PointerButton::Primary,
                    crate::commands::ActivationSource::Click,
                )
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
        button: PointerButton,
        source: crate::commands::ActivationSource,
    ) -> Vec<SessionIntent> {
        if role == CellRole::Back {
            return self.reduce(SessionEvent::Back {
                geometry_generation,
                pointer_baseline: point,
            });
        }
        if role == CellRole::Close {
            return self.cancel_tree();
        }
        if matches!(role, CellRole::NextPage | CellRole::PreviousPage) {
            let page = self.state.stack.last().map_or(0, |frame| frame.page);
            let page = if role == CellRole::NextPage {
                page.saturating_add(1)
            } else {
                page.saturating_sub(1)
            };
            self.reduce(SessionEvent::PageChanged {
                page,
                geometry_generation,
                pointer_baseline: point,
            });
            let page = self.state.stack.last().map_or(0, |frame| frame.page);
            return vec![SessionIntent::PageChanged { page }];
        }
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
        let Some(next_dispatch) = self.next_dispatch.checked_add(1) else {
            return self.cancel_tree();
        };
        self.next_dispatch = next_dispatch;
        vec![SessionIntent::Dispatch {
            cell_id,
            token,
            button,
            modifiers: self.state.modifiers,
            source,
        }]
    }
    fn navigate(
        &mut self,
        command: NavigationCommand,
        cells: Vec<(CellId, CellRole)>,
        point: LogicalPoint,
        geometry_generation: u64,
    ) -> Vec<SessionIntent> {
        if self.state.keyboard_ownership != KeyboardOwnership::MenuNavigation
            || self.current_geometry() != geometry_generation
        {
            return vec![];
        }
        let actionable: Vec<_> = cells
            .into_iter()
            .filter(|(_, role)| actionable(*role))
            .collect();
        match command {
            NavigationCommand::Next | NavigationCommand::Previous => {
                if actionable.is_empty() {
                    return vec![];
                }
                let current = self
                    .state
                    .selected
                    .as_ref()
                    .and_then(|selected| actionable.iter().position(|(id, _)| id == selected));
                let index = match (command, current) {
                    (NavigationCommand::Next, Some(i)) => (i + 1) % actionable.len(),
                    (NavigationCommand::Previous, Some(0)) => actionable.len() - 1,
                    (NavigationCommand::Previous, Some(i)) => i - 1,
                    (NavigationCommand::Previous, None) => actionable.len() - 1,
                    _ => 0,
                };
                self.state.selected = Some(actionable[index].0.clone());
                if let Some(frame) = self.state.stack.last_mut() {
                    frame.selected = self.state.selected.clone();
                }
                self.state.armed = true;
                vec![]
            }
            NavigationCommand::ActivatePrimary | NavigationCommand::ActivateSecondary => {
                let Some(selected) = self.state.selected.clone() else {
                    return vec![];
                };
                let Some((_, role)) = actionable.into_iter().find(|(id, _)| id == &selected) else {
                    return vec![];
                };
                self.activate(
                    selected,
                    role,
                    point,
                    geometry_generation,
                    if command == NavigationCommand::ActivateSecondary {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    },
                    crate::commands::ActivationSource::Enter,
                )
            }
            NavigationCommand::Back => self.reduce(SessionEvent::Back {
                geometry_generation,
                pointer_baseline: point,
            }),
            NavigationCommand::NextPage | NavigationCommand::PreviousPage => {
                let current = self.state.stack.last().map_or(0, |frame| frame.page);
                let page = if command == NavigationCommand::NextPage {
                    current.saturating_add(1)
                } else {
                    current.saturating_sub(1)
                };
                self.reduce(SessionEvent::PageChanged {
                    page,
                    geometry_generation,
                    pointer_baseline: point,
                });
                let page = self.state.stack.last().map_or(0, |frame| frame.page);
                vec![SessionIntent::PageChanged { page }]
            }
        }
    }
    fn cancel_tree(&mut self) -> Vec<SessionIntent> {
        self.closed = true;
        self.state.pending_press = None;
        self.state.dwell_candidate = None;
        vec![SessionIntent::CloseTree]
    }
    fn current_geometry(&self) -> u64 {
        self.state
            .stack
            .last()
            .map_or(0, |frame| frame.geometry_generation)
    }
    fn bump_generation(&mut self) -> bool {
        let Some(next) = self.state.session_generation.checked_add(1) else {
            return false;
        };
        self.state.session_generation = next;
        true
    }
    fn disarm(&mut self, point: LogicalPoint, geometry_generation: u64) {
        self.state.armed = false;
        self.state.hovered = None;
        self.state.selected = None;
        self.state.pending_press = None;
        self.state.dwell_candidate = None;
        self.state.arming_baseline = ArmingBaseline {
            point,
            geometry_generation,
        };
    }
}

fn actionable(role: CellRole) -> bool {
    matches!(
        role,
        CellRole::Action
            | CellRole::Submenu
            | CellRole::Back
            | CellRole::Close
            | CellRole::NextPage
            | CellRole::PreviousPage
    )
}
fn distance2(a: LogicalPoint, b: LogicalPoint) -> f32 {
    let x = a.x - b.x;
    let y = a.y - b.y;
    x * x + y * y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::dynamic::{FrozenAvailability, FrozenEntryId};
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
    fn frozen(id: &str) -> FrozenRadialEntry {
        FrozenRadialEntry {
            id: FrozenEntryId(id.into()),
            label: id.into(),
            binding: None,
            availability: FrozenAvailability::Available,
            history_query: String::new(),
            requirement: super::super::handoff::InteractionRequirement::None,
        }
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
            results: vec![frozen("root")],
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
            results: vec![frozen("stale")],
        });
        r.reduce(SessionEvent::FreezeDynamic {
            frame_id: FrameId(2),
            source_cell: cell.clone(),
            results: vec![frozen("child")],
        });
        assert_eq!(r.state.frozen_dynamic_results.len(), 2);
        assert!(
            r.state
                .frozen_dynamic_results
                .iter()
                .any(|(key, value)| key.menu_id == MenuId::new("root")
                    && value == &vec![frozen("root")])
        );
        assert!(
            r.state
                .frozen_dynamic_results
                .iter()
                .any(|(key, value)| key.menu_id == MenuId::new("child")
                    && value == &vec![frozen("child")])
        );
    }

    #[test]
    fn keyboard_navigation_wraps_and_dispatches_selected_cell() {
        let mut reducer = reducer(InteractionMode::StickyClick);
        let cells = vec![
            (CellId::new("a"), CellRole::Action),
            (CellId::new("gap"), CellRole::Spacer),
            (CellId::new("b"), CellRole::Action),
        ];
        reducer.reduce(SessionEvent::Navigate {
            command: NavigationCommand::Previous,
            cells: cells.clone(),
            pointer_baseline: LogicalPoint::default(),
            geometry_generation: 10,
        });
        assert_eq!(reducer.state.selected, Some(CellId::new("b")));
        assert!(matches!(
            reducer
                .reduce(SessionEvent::Navigate {
                    command: NavigationCommand::ActivatePrimary,
                    cells,
                    pointer_baseline: LogicalPoint::default(),
                    geometry_generation: 10,
                })
                .as_slice(),
            [SessionIntent::Dispatch { cell_id, .. }] if cell_id == &CellId::new("b")
        ));
    }

    #[test]
    fn back_restores_parent_selection_and_frozen_frame_metadata() {
        let mut reducer = reducer(InteractionMode::StickyClick);
        reducer.reduce(SessionEvent::SelectKeyboard {
            cell: CellId::new("parent"),
        });
        reducer.state.stack[0].scale_factor = 1.5;
        reducer.reduce(SessionEvent::OpenChild {
            menu_id: MenuId::new("child"),
            origin: PhysicalPoint { x: -30.0, y: 40.0 },
            geometry_generation: 11,
            pointer_baseline: LogicalPoint::default(),
        });
        assert_eq!(reducer.state.stack[1].parent_frame_id, Some(FrameId(1)));
        assert_eq!(reducer.state.stack[1].scale_factor, 1.5);
        reducer.reduce(SessionEvent::SelectKeyboard {
            cell: CellId::new("child-cell"),
        });
        reducer.reduce(SessionEvent::Back {
            geometry_generation: 12,
            pointer_baseline: LogicalPoint::default(),
        });
        assert_eq!(reducer.state.selected, Some(CellId::new("parent")));
        assert_eq!(reducer.state.stack[0].selected, Some(CellId::new("parent")));
        assert_eq!(reducer.state.stack[0].scale_factor, 1.5);
    }

    #[test]
    fn stale_dwell_cannot_dispatch_after_navigation_generation_changes() {
        let mut reducer = reducer(InteractionMode::StickyClick);
        reducer.reduce(SessionEvent::StartDwell {
            cell: CellId::new("a"),
            role: CellRole::Submenu,
            deadline: 50,
        });
        reducer.reduce(SessionEvent::PageChanged {
            page: 1,
            geometry_generation: 11,
            pointer_baseline: LogicalPoint::default(),
        });
        assert!(
            reducer
                .reduce(SessionEvent::DwellExpired {
                    cell: CellId::new("a"),
                    role: CellRole::Action,
                    at: 50,
                    point: LogicalPoint::default(),
                    geometry_generation: 10,
                })
                .is_empty()
        );
    }
    #[test]
    fn page_state_is_clamped_to_prepared_frame_count() {
        let mut reducer = reducer(InteractionMode::StickyClick);
        reducer.state.stack[0].page_count = 3;
        reducer.reduce(SessionEvent::PageChanged {
            page: usize::MAX,
            geometry_generation: 11,
            pointer_baseline: LogicalPoint::default(),
        });
        assert_eq!(reducer.state.stack[0].page, 2);
    }
    #[test]
    fn outside_interaction_releases_only_keyboard_navigation() {
        let mut r = reducer(InteractionMode::StickyClick);
        r.reduce(SessionEvent::StartDwell {
            cell: CellId::new("submenu"),
            role: CellRole::Submenu,
            deadline: 10,
        });
        r.reduce(SessionEvent::OutsideInteraction);
        assert_eq!(
            r.state.keyboard_ownership,
            KeyboardOwnership::ExternalApplication
        );
        assert_eq!(r.state.stack.len(), 1);
        assert!(r.state.dwell_candidate.is_none());
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
