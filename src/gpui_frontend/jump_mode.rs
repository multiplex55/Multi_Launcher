use super::{PhysicalKeydown, appearance};
use gpui_kit::{component::label::Label, *};
use multi_launcher::launcher::jump_mode::{
    Hint, HintMap, HintSession, MatchOutcome, TargetId, TargetMetadata,
};
use multi_launcher::settings::{HintAlphabet, JumpModeChord};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct JumpTargetGeometry {
    pub(super) metadata: TargetMetadata,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) visible_bounds: Bounds<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct JumpGeometryFrame {
    pub(super) generation: u64,
    pub(super) targets: Vec<JumpTargetGeometry>,
    pub(super) target_ids: Vec<TargetId>,
    pub(super) target_indices: HashMap<TargetId, usize>,
    pub(super) matches_active_session: bool,
}

impl JumpGeometryFrame {
    pub(super) fn target_ids(&self) -> &[TargetId] {
        &self.target_ids
    }

    pub(super) fn has_target(&self, id: &TargetId) -> bool {
        self.target_indices.contains_key(id)
    }

    pub(super) fn same_population(&self, other: &Self) -> bool {
        self.generation == other.generation && self.target_ids == other.target_ids
    }

    pub(super) fn same_geometry(&self, other: &Self) -> bool {
        self.same_population(other)
            && self.target_ids.iter().all(|target_id| {
                self.target_geometry(target_id)
                    .zip(other.target_geometry(target_id))
                    .is_some_and(|(left, right)| {
                        left.bounds == right.bounds && left.visible_bounds == right.visible_bounds
                    })
            })
    }

    pub(super) fn target_geometry(&self, id: &TargetId) -> Option<&JumpTargetGeometry> {
        self.target_indices
            .get(id)
            .and_then(|index| self.targets.get(*index))
    }
}

pub(super) fn clipped_jump_geometry(
    metadata: TargetMetadata,
    bounds: Bounds<Pixels>,
    content_mask: Bounds<Pixels>,
) -> Option<JumpTargetGeometry> {
    let visible_bounds = bounds.intersect(&content_mask);
    (visible_bounds.size.width.as_f32() > 0.0 && visible_bounds.size.height.as_f32() > 0.0)
        .then_some(JumpTargetGeometry {
            metadata,
            bounds,
            visible_bounds,
        })
}

#[derive(Debug)]
pub(super) struct JumpGeometryDraft {
    pub(super) generation: u64,
    pub(super) targets: Vec<JumpTargetGeometry>,
}

impl JumpGeometryDraft {
    pub(super) fn new(generation: u64) -> Self {
        Self {
            generation,
            targets: Vec::new(),
        }
    }

    pub(super) fn finish(
        &self,
        expected_generation: Option<u64>,
        expected_target_ids: &[TargetId],
        alphabet: &HintAlphabet,
        metrics: multi_launcher::gui::theme::ThemeMetrics,
    ) -> JumpGeometryFrame {
        // A target belongs to the eligible population only if its assigned
        // marker fits inside the currently visible part of the element.
        // Assign against the full visible snapshot first so the filter is
        // deterministic and cannot silently retarget a code mid-session.
        let geometry_by_id = self
            .targets
            .iter()
            .map(|target| (target.metadata.id().clone(), target))
            .collect::<HashMap<_, _>>();
        let eligible_target_ids = HintMap::assign_with_alphabet(
            self.targets.iter().map(|target| target.metadata.clone()),
            alphabet,
        )
        .map(|map| {
            map.assignments()
                .iter()
                .filter_map(|assignment| {
                    let target_id = assignment.target().id();
                    let geometry = geometry_by_id.get(target_id)?;
                    jump_badge_placement(geometry, assignment.hint(), metrics)
                        .is_some()
                        .then(|| target_id.to_owned())
                })
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
        let targets = self
            .targets
            .iter()
            .filter(|target| eligible_target_ids.contains(target.metadata.id()))
            .cloned()
            .collect::<Vec<_>>();
        let mut target_ids = targets
            .iter()
            .map(|target| target.metadata.id().clone())
            .collect::<Vec<_>>();
        target_ids.sort();
        let target_indices = targets
            .iter()
            .enumerate()
            .map(|(index, target)| (target.metadata.id().clone(), index))
            .collect();
        JumpGeometryFrame {
            generation: self.generation,
            targets,
            matches_active_session: expected_generation == Some(self.generation)
                && expected_target_ids == target_ids.as_slice(),
            target_ids,
            target_indices,
        }
    }
}

pub(super) struct JumpModeController {
    pub(super) enabled: bool,
    chord_vk: Option<u16>,
    chord_modifiers: Modifiers,
    pub(super) alphabet: HintAlphabet,
    pub(super) session: Option<HintSession>,
    pub(super) generation: Option<u64>,
    pub(super) target_ids: Vec<TargetId>,
    pub(super) held_owned_vkeys: HashSet<u16>,
    revision: u64,
}

#[derive(Default)]
pub(super) struct JumpKeyResult {
    pub(super) consumed: bool,
    pub(super) repaint: bool,
    pub(super) session_transition: bool,
    pub(super) selected_target: Option<TargetId>,
}

impl JumpModeController {
    pub(super) fn new(enabled: bool, chord: JumpModeChord, alphabet: HintAlphabet) -> Self {
        let chord_vk = multi_launcher::hotkey::launcher_invocation::vk_from_key(chord.hotkey_key())
            .and_then(|virtual_key| u16::try_from(virtual_key).ok());
        Self {
            enabled,
            chord_vk,
            chord_modifiers: Modifiers {
                control: chord.ctrl(),
                alt: chord.alt(),
                shift: chord.shift(),
                platform: false,
                function: false,
            },
            alphabet,
            session: None,
            generation: None,
            target_ids: Vec::new(),
            held_owned_vkeys: HashSet::new(),
            revision: 0,
        }
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub(super) fn target_ids(&self) -> &[TargetId] {
        &self.target_ids
    }

    pub(super) fn session_generation(&self) -> Option<u64> {
        self.generation
    }

    pub(super) fn hint_for(&self, target: &TargetId) -> Option<&Hint> {
        self.session
            .as_ref()?
            .matching_assignments()
            .find_map(|assignment| {
                (assignment.target().id() == target).then_some(assignment.hint())
            })
    }

    fn matches_leader(&self, receipt: PhysicalKeydown) -> bool {
        self.enabled
            && self.chord_vk == Some(receipt.virtual_key)
            && self.chord_modifiers == receipt.modifiers
    }

    pub(super) fn should_process_key_down(&self, receipt: PhysicalKeydown) -> bool {
        if !self.enabled {
            return false;
        }
        if Self::is_native_alt_space(receipt) {
            return !receipt.held && self.matches_leader(receipt);
        }
        if self.held_owned_vkeys.contains(&receipt.virtual_key) {
            return true;
        }
        if self.session.is_some() {
            return self.matches_leader(receipt)
                || matches!(receipt.virtual_key, 0x0D | 0x20)
                || !(receipt.modifiers.control
                    || receipt.modifiers.alt
                    || receipt.modifiers.platform
                    || receipt.modifiers.function);
        }
        self.matches_leader(receipt)
    }

    fn is_native_alt_space(receipt: PhysicalKeydown) -> bool {
        receipt.virtual_key == 0x20
            && receipt.modifiers.alt
            && !receipt.modifiers.control
            && !receipt.modifiers.shift
            && !receipt.modifiers.platform
            && !receipt.modifiers.function
    }

    pub(super) fn start(&mut self, frame: &JumpGeometryFrame) -> bool {
        let targets = frame
            .targets
            .iter()
            .map(|target| target.metadata.clone())
            .collect::<Vec<_>>();
        let Ok(map) = HintMap::assign_with_alphabet(targets, &self.alphabet) else {
            return false;
        };
        if map.is_empty() {
            return false;
        }
        self.target_ids = frame.target_ids.clone();
        self.generation = Some(frame.generation);
        self.session = Some(HintSession::new(map));
        self.revision = self.revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn retire(&mut self) -> bool {
        if self.session.take().is_none() {
            return false;
        }
        self.generation = None;
        self.target_ids.clear();
        self.revision = self.revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn reconcile_frame(&mut self, frame: Option<&JumpGeometryFrame>) -> bool {
        if self.session.is_none() {
            return false;
        }
        let matches = frame.is_some_and(|frame| {
            self.generation == Some(frame.generation)
                && frame.target_ids() == self.target_ids.as_slice()
        });
        !matches && self.retire()
    }

    pub(super) fn process_key_down(
        &mut self,
        receipt: PhysicalKeydown,
        frame: Option<&JumpGeometryFrame>,
        target_generation: u64,
    ) -> JumpKeyResult {
        let mut outcome = JumpKeyResult::default();
        let native_alt_space = Self::is_native_alt_space(receipt);
        if !native_alt_space && self.held_owned_vkeys.contains(&receipt.virtual_key) {
            if receipt.held {
                outcome.consumed = true;
                return outcome;
            }
            self.held_owned_vkeys.remove(&receipt.virtual_key);
        }

        if self.session.is_some() {
            if self.matches_leader(receipt) {
                if native_alt_space {
                    if !receipt.held {
                        outcome.session_transition = self.retire();
                        outcome.repaint = outcome.session_transition;
                    }
                    return outcome;
                }
                self.held_owned_vkeys.insert(receipt.virtual_key);
                outcome.consumed = true;
                if !receipt.held {
                    outcome.session_transition = self.retire();
                    outcome.repaint = outcome.session_transition;
                }
                return outcome;
            }

            // Windows reserves Alt+Space for the system menu. It remains
            // native even when it is the configured leader or a hint is live.
            if native_alt_space {
                return outcome;
            }

            if matches!(receipt.virtual_key, 0x0D | 0x20) {
                self.held_owned_vkeys.insert(receipt.virtual_key);
                outcome.consumed = true;
                return outcome;
            }

            // Other Ctrl/Alt/Win/Fn chords remain available to existing
            // launcher bindings while a hint session owns ordinary Search input.
            if receipt.modifiers.control
                || receipt.modifiers.alt
                || receipt.modifiers.platform
                || receipt.modifiers.function
            {
                return outcome;
            }

            if receipt.held {
                self.held_owned_vkeys.insert(receipt.virtual_key);
                outcome.consumed = true;
                return outcome;
            }

            let (previous_prefix, current_prefix, match_outcome) = {
                let Some(session) = self.session.as_mut() else {
                    return outcome;
                };
                let previous_prefix = session.prefix().to_owned();
                let match_outcome = match receipt.virtual_key {
                    0x08 => session.backspace(),
                    0x1B => session.cancel(),
                    0x41..=0x5A => session.push_char(char::from(receipt.virtual_key as u8)),
                    _ => MatchOutcome::Partial {
                        matching_targets: session.matching_assignments().count(),
                    },
                };
                (previous_prefix, session.prefix().to_owned(), match_outcome)
            };
            self.held_owned_vkeys.insert(receipt.virtual_key);
            outcome.consumed = true;
            outcome.repaint = current_prefix != previous_prefix;
            match match_outcome {
                MatchOutcome::Completed(target) => {
                    outcome.selected_target = Some(target);
                    outcome.session_transition = self.retire();
                    outcome.repaint = true;
                }
                MatchOutcome::Cancelled => {
                    outcome.session_transition = self.retire();
                    outcome.repaint = true;
                }
                MatchOutcome::NoTarget | MatchOutcome::SessionClosed => {
                    outcome.session_transition = self.retire();
                    outcome.repaint = true;
                }
                MatchOutcome::NoTargets | MatchOutcome::Partial { .. } | MatchOutcome::Invalid => {}
            }
            return outcome;
        }

        if !self.matches_leader(receipt) {
            return outcome;
        }
        if native_alt_space {
            if !receipt.held
                && let Some(frame) = frame.filter(|frame| frame.generation == target_generation)
                && self.start(frame)
            {
                outcome.repaint = true;
                outcome.session_transition = true;
            }
            return outcome;
        }
        outcome.consumed = true;
        self.held_owned_vkeys.insert(receipt.virtual_key);
        if receipt.held {
            return outcome;
        }
        let Some(frame) = frame.filter(|frame| frame.generation == target_generation) else {
            return outcome;
        };
        if self.start(frame) {
            outcome.repaint = true;
            outcome.session_transition = true;
        }
        outcome
    }
}

pub(super) struct JumpGeometryElement<E> {
    inner: E,
    pub(super) metadata: TargetMetadata,
    draft: Rc<RefCell<JumpGeometryDraft>>,
}

impl<E> JumpGeometryElement<E> {
    pub(super) fn new(
        inner: E,
        metadata: TargetMetadata,
        draft: Rc<RefCell<JumpGeometryDraft>>,
    ) -> Self {
        Self {
            inner,
            metadata,
            draft,
        }
    }
}

impl<E: InteractiveElement> InteractiveElement for JumpGeometryElement<E> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}

impl<E: Element> IntoElement for JumpGeometryElement<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: Element> Element for JumpGeometryElement<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let prepaint = self
            .inner
            .prepaint(id, inspector_id, bounds.clone(), layout, window, cx);
        if let Some(target) =
            clipped_jump_geometry(self.metadata.clone(), bounds, window.content_mask().bounds)
        {
            self.draft.borrow_mut().targets.push(target);
        }
        prepaint
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector_id, bounds, layout, prepaint, window, cx);
    }

    fn a11y_role(&self) -> Option<Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut gpui_kit::accesskit::Node) {
        self.inner.write_a11y_info(node);
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

struct JumpBadgeElement<E> {
    inner: E,
    frame_slot: Rc<RefCell<Option<JumpGeometryFrame>>>,
    pub(super) generation: u64,
    target: TargetId,
}

impl<E> JumpBadgeElement<E> {
    pub(super) fn new(
        inner: E,
        frame_slot: Rc<RefCell<Option<JumpGeometryFrame>>>,
        generation: u64,
        target: TargetId,
    ) -> Self {
        Self {
            inner,
            frame_slot,
            generation,
            target,
        }
    }
}

impl<E: Element> IntoElement for JumpBadgeElement<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: Element> Element for JumpBadgeElement<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector_id, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let frame_is_current = self
            .frame_slot
            .borrow()
            .as_ref()
            .filter(|frame| frame.generation == self.generation && frame.matches_active_session)
            .and_then(|frame| frame.target_geometry(&self.target))
            .is_some_and(|target| bounds_are_contained_by(&bounds, &target.visible_bounds));
        if frame_is_current {
            self.inner
                .paint(id, inspector_id, bounds, layout, prepaint, window, cx);
        }
    }

    fn a11y_role(&self) -> Option<Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut gpui_kit::accesskit::Node) {
        self.inner.write_a11y_info(node);
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

#[derive(Clone, Copy)]
pub(super) struct JumpBadgePlacement {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) fn jump_badge_placement(
    target: &JumpTargetGeometry,
    hint: &Hint,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
) -> Option<JumpBadgePlacement> {
    let target_width = target.bounds.size.width.as_f32();
    let target_height = target.bounds.size.height.as_f32();
    let visible_left = target.visible_bounds.origin.x.as_f32() - target.bounds.origin.x.as_f32();
    let visible_top = target.visible_bounds.origin.y.as_f32() - target.bounds.origin.y.as_f32();
    let visible_right = visible_left + target.visible_bounds.size.width.as_f32();
    let visible_bottom = visible_top + target.visible_bounds.size.height.as_f32();
    let width = (hint.as_str().len() as f32 * metrics.small_text_size * 0.72 + 6.0).max(10.0);
    let height = metrics.line_height.max(metrics.small_text_size) + 4.0;
    if visible_right - visible_left < width || visible_bottom - visible_top < height {
        return None;
    }
    Some(JumpBadgePlacement {
        left: (visible_right - width)
            .min(target_width - width)
            .max(visible_left),
        top: visible_top.max(0.0).min(target_height - height),
        width,
        height,
    })
}

pub(super) fn bounds_are_contained_by(inner: &Bounds<Pixels>, outer: &Bounds<Pixels>) -> bool {
    let inner_right = inner.origin.x.as_f32() + inner.size.width.as_f32();
    let inner_bottom = inner.origin.y.as_f32() + inner.size.height.as_f32();
    let outer_right = outer.origin.x.as_f32() + outer.size.width.as_f32();
    let outer_bottom = outer.origin.y.as_f32() + outer.size.height.as_f32();
    inner.origin.x.as_f32() >= outer.origin.x.as_f32()
        && inner.origin.y.as_f32() >= outer.origin.y.as_f32()
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

pub(super) fn jump_hint_badge(
    hint: Option<&Hint>,
    target: &TargetId,
    generation: u64,
    frame_slot: Rc<RefCell<Option<JumpGeometryFrame>>>,
    palette: multi_launcher::gui::theme::SemanticPalette,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
) -> Option<AnyElement> {
    let hint = hint?;
    let frame = frame_slot.borrow();
    let target_geometry = frame.as_ref()?.target_geometry(target)?;
    let placement = jump_badge_placement(target_geometry, hint, metrics)?;
    drop(frame);
    let badge = div()
        .absolute()
        .top(px(placement.top))
        .left(px(placement.left))
        .w(px(placement.width))
        .h(px(placement.height))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .bg(appearance::to_hsla(palette.accent))
        .text_color(appearance::to_hsla(palette.primary_text))
        .tab_stop(false)
        .child(
            Label::new(hint.as_str().to_owned())
                .text_size(px(metrics.small_text_size))
                .line_height(px(metrics.line_height)),
        );
    Some(
        JumpBadgeElement::new(badge.into_element(), frame_slot, generation, target.clone())
            .into_any_element(),
    )
}

pub(super) fn jump_geometry_publisher<T: 'static>(
    draft: Rc<RefCell<JumpGeometryDraft>>,
    frame_slot: Rc<RefCell<Option<JumpGeometryFrame>>>,
    owner: WeakEntity<T>,
    expected_generation: Option<u64>,
    expected_target_ids: Rc<Vec<TargetId>>,
    alphabet: HintAlphabet,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    on_frame: impl Fn(&mut T, &JumpGeometryFrame, &Window, &mut Context<T>, bool) + 'static,
) -> impl IntoElement {
    let on_frame = Rc::new(on_frame);
    canvas(
        move |_, window, app| {
            let frame = draft.borrow().finish(
                expected_generation,
                expected_target_ids.as_slice(),
                &alphabet,
                metrics,
            );
            let (population_changed, geometry_changed) = {
                let mut slot = frame_slot.borrow_mut();
                let population_changed = slot
                    .as_ref()
                    .is_none_or(|previous| !previous.same_population(&frame));
                let geometry_changed = slot
                    .as_ref()
                    .is_some_and(|previous| !previous.same_geometry(&frame));
                *slot = Some(frame);
                (population_changed, geometry_changed)
            };
            if population_changed || (expected_generation.is_some() && geometry_changed) {
                let owner = owner.clone();
                let on_frame = Rc::clone(&on_frame);
                let latest_frame_slot = Rc::clone(&frame_slot);
                window.defer(app, move |window, app| {
                    let latest_frame = latest_frame_slot.borrow();
                    let Some(frame) = latest_frame.as_ref() else {
                        return;
                    };
                    let _ = owner.update(app, |owner, cx| {
                        on_frame(
                            owner,
                            frame,
                            window,
                            cx,
                            geometry_changed && expected_generation.is_some(),
                        );
                    });
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size(px(0.0))
}
