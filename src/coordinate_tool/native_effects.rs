//! Lazy native ownership for the cursor-centered magnifier effects.
//!
//! The runtime owns one Magnification session and at most one hidden host per
//! requested effect. It deliberately contains no halo compositing or lens
//! presentation logic; later effect renderers can activate these prepared
//! surfaces without changing resource ownership.

use super::model::{
    CoordinateEffectsStatus, CoordinateToolRuntimeState, CursorEffectStatus, PhysicalPoint,
};
use super::settings::{CoordinateToolPreferences, HaloPreferences, ZoomPreferences};

const EFFECT_KINDS: [EffectKind; 2] = [EffectKind::Halo, EffectKind::Zoom];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectKind {
    Halo,
    Zoom,
}

impl EffectKind {
    const fn index(self) -> usize {
        match self {
            Self::Halo => 0,
            Self::Zoom => 1,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Halo => "halo",
            Self::Zoom => "zoom",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EffectConfiguration {
    Halo(HaloPreferences),
    Zoom(ZoomPreferences),
}

impl EffectConfiguration {
    fn from_preferences(kind: EffectKind, preferences: &CoordinateToolPreferences) -> Self {
        match kind {
            EffectKind::Halo => Self::Halo(preferences.halo),
            EffectKind::Zoom => Self::Zoom(preferences.zoom),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EffectRequests {
    pub halo: bool,
    pub zoom: bool,
}

impl EffectRequests {
    pub(crate) const fn from_runtime(runtime: &CoordinateToolRuntimeState) -> Self {
        Self {
            halo: runtime.halo_enabled(),
            zoom: runtime.zoom_enabled(),
        }
    }

    const fn enabled(self, kind: EffectKind) -> bool {
        match kind {
            EffectKind::Halo => self.halo,
            EffectKind::Zoom => self.zoom,
        }
    }
}

/// The narrow native seam used by both the production Win32 implementation
/// and lifecycle tests. Surface handles remain owned by the implementation.
pub(crate) trait EffectNativeOperations {
    fn initialize_session(&mut self) -> Result<(), String>;
    fn has_surface(&self, kind: EffectKind) -> bool;
    fn create_surface(
        &mut self,
        kind: EffectKind,
        configuration: &EffectConfiguration,
    ) -> Result<(), String>;
    fn configure_surface(
        &mut self,
        kind: EffectKind,
        configuration: &EffectConfiguration,
    ) -> Result<(), String>;
    fn host_window(&self, kind: EffectKind) -> Option<usize>;
    fn set_filter_list(
        &mut self,
        kind: EffectKind,
        excluded_windows: &[usize],
    ) -> Result<(), String>;
    fn is_visible(&self, kind: EffectKind) -> bool;
    fn hide_surface(&mut self, kind: EffectKind) -> Result<(), String>;
    fn refresh_visible_source(
        &mut self,
        kind: EffectKind,
        current_point: PhysicalPoint,
    ) -> Result<(), String>;
    fn destroy_surface(&mut self, kind: EffectKind) -> Result<(), String>;
    fn uninitialize_session(&mut self) -> Result<(), String>;
}

/// Reconciles requested effect state against native resources. This is owned
/// by the passive worker's surface backend and never runs on the GUI thread.
pub(crate) struct CursorEffectsRuntime<O: EffectNativeOperations> {
    operations: O,
    session_initialized: bool,
    session_initialization_failed: bool,
    session_cleanup_failed: bool,
    configurations: [Option<EffectConfiguration>; 2],
    cached_live_points: [Option<PhysicalPoint>; 2],
    requested: EffectRequests,
    status: CoordinateEffectsStatus,
    filters_dirty: bool,
    shutdown_complete: bool,
}

impl<O: EffectNativeOperations> CursorEffectsRuntime<O> {
    pub(crate) fn new(operations: O) -> Self {
        Self {
            operations,
            session_initialized: false,
            session_initialization_failed: false,
            session_cleanup_failed: false,
            configurations: [None, None],
            cached_live_points: [None, None],
            requested: EffectRequests::default(),
            status: CoordinateEffectsStatus::default(),
            filters_dirty: false,
            shutdown_complete: false,
        }
    }

    pub(crate) fn status(&self) -> CoordinateEffectsStatus {
        self.status.clone()
    }

    /// Reconcile one frame's request and preferences. `current_point` is only
    /// the successful live sample; frozen HUD and fallback placement samples
    /// are intentionally not accepted by this boundary.
    pub(crate) fn reconcile(
        &mut self,
        requests: EffectRequests,
        preferences: &CoordinateToolPreferences,
        current_point: Option<PhysicalPoint>,
        cheap_window_ids: &[usize],
        topology_invalidated: bool,
    ) {
        if self.shutdown_complete {
            return;
        }

        let reenabled = EFFECT_KINDS
            .into_iter()
            .any(|kind| requests.enabled(kind) && !self.requested.enabled(kind));
        if reenabled {
            self.session_initialization_failed = false;
            self.session_cleanup_failed = false;
            self.filters_dirty = true;
            for kind in EFFECT_KINDS {
                if requests.enabled(kind) && self.is_unavailable(kind) {
                    self.set_status(kind, CursorEffectStatus::Disabled);
                }
            }
        }
        if topology_invalidated {
            self.filters_dirty = true;
            self.session_initialization_failed = false;
            for kind in EFFECT_KINDS {
                if requests.enabled(kind) && self.is_unavailable(kind) {
                    self.set_status(kind, CursorEffectStatus::Disabled);
                }
            }
            self.session_cleanup_failed = false;
        }

        // Clear disabled modes before shared-session setup. A failed session
        // initialization must never prevent an independently disabled effect
        // from releasing its host and status.
        for kind in EFFECT_KINDS {
            let requested = requests.enabled(kind);
            let was_requested = self.requested.enabled(kind);
            let has_surface = self.operations.has_surface(kind);
            if requested {
                continue;
            }
            self.cached_live_points[kind.index()] = None;
            if has_surface && (was_requested || topology_invalidated) {
                match self.destroy_one(kind) {
                    Ok(()) => self.set_status(kind, CursorEffectStatus::Disabled),
                    Err(error) => self.set_status(
                        kind,
                        CursorEffectStatus::Unavailable(format!(
                            "Could not release disabled {} effect: {error}",
                            kind.label()
                        )),
                    ),
                }
            } else if !has_surface && was_requested {
                self.configurations[kind.index()] = None;
                self.set_status(kind, CursorEffectStatus::Disabled);
            }
        }

        if !requests.halo && !requests.zoom {
            self.session_initialization_failed = false;
        }

        // A shared init failure is stable until a requested off/on transition
        // or display/DPI invalidation. Likewise, if every requested effect is
        // already unavailable, do not spin up an otherwise empty session just
        // because the cursor sample changed.
        let any_requested_effect_eligible = EFFECT_KINDS
            .into_iter()
            .any(|kind| requests.enabled(kind) && !self.is_unavailable(kind));
        if !self.session_initialized
            && (self.session_initialization_failed || !any_requested_effect_eligible)
        {
            self.cache_live_points(requests, current_point);
            self.requested = requests;
            return;
        }

        if (requests.halo || requests.zoom) && !self.session_initialized {
            match self.operations.initialize_session() {
                Ok(()) => {
                    self.session_initialized = true;
                    self.session_initialization_failed = false;
                }
                Err(error) => {
                    self.session_initialization_failed = true;
                    for kind in EFFECT_KINDS {
                        if requests.enabled(kind) {
                            self.set_status(
                                kind,
                                CursorEffectStatus::Unavailable(format!(
                                    "Could not initialize Magnification for {}: {error}",
                                    kind.label()
                                )),
                            );
                        }
                    }
                    self.cache_live_points(requests, current_point);
                    self.requested = requests;
                    return;
                }
            }
        }

        for kind in EFFECT_KINDS {
            let requested = requests.enabled(kind);
            let was_requested = self.requested.enabled(kind);
            if !requested {
                continue;
            }

            let configuration = EffectConfiguration::from_preferences(kind, preferences);
            let was_reenabled = requested && !was_requested;
            if self.is_unavailable(kind) && !topology_invalidated && !was_reenabled {
                continue;
            }

            let mut needs_create = !self.operations.has_surface(kind);
            if !needs_create && self.configurations[kind.index()].is_none() {
                if let Err(error) = self.destroy_one(kind) {
                    self.set_status(
                        kind,
                        CursorEffectStatus::Unavailable(format!(
                            "Could not replace incomplete {} effect resources: {error}",
                            kind.label()
                        )),
                    );
                    continue;
                }
                needs_create = true;
            }

            if !needs_create {
                if self.configurations[kind.index()].as_ref() != Some(&configuration) {
                    if let Err(error) = self.operations.configure_surface(kind, &configuration) {
                        self.fail_surface(
                            kind,
                            format!("Could not update {} effect: {error}", kind.label()),
                        );
                        continue;
                    }
                    self.configurations[kind.index()] = Some(configuration);
                    self.filters_dirty = true;
                }
            } else {
                self.filters_dirty = true;
                match self.operations.create_surface(kind, &configuration) {
                    Ok(()) => {
                        self.configurations[kind.index()] = Some(configuration);
                        self.set_status(kind, CursorEffectStatus::Prepared);
                        self.filters_dirty = true;
                    }
                    Err(error) => {
                        self.fail_surface(
                            kind,
                            format!("Could not prepare {} effect: {error}", kind.label()),
                        );
                    }
                }
            }
        }

        if topology_invalidated {
            self.filters_dirty = true;
        }
        self.apply_filter_lists(cheap_window_ids, requests);
        self.update_live_status(requests, current_point);
        self.cache_live_points(requests, current_point);
        self.maybe_uninitialize(requests, topology_invalidated, false);
        self.requested = requests;
    }

    /// Refresh currently visible native children without asking the backend to
    /// redraw the cheap HUD/crosshair surfaces. This runs only on the worker's
    /// existing message-pump cadence and consumes a cached successful live
    /// point; hidden and disabled effects are no-ops.
    pub(crate) fn poll_visible_sources(&mut self, cheap_window_ids: &[usize]) {
        if self.shutdown_complete {
            return;
        }
        let mut failed_surface = false;
        for kind in EFFECT_KINDS {
            if !self.requested.enabled(kind)
                || !self.operations.has_surface(kind)
                || self.is_unavailable(kind)
            {
                continue;
            }
            if !self.operations.is_visible(kind) {
                continue;
            }
            let Some(point) = self.cached_live_points[kind.index()] else {
                if let Err(error) = self.operations.hide_surface(kind) {
                    failed_surface = true;
                    self.fail_surface(
                        kind,
                        format!(
                            "Could not pause {} effect without a live sample: {error}",
                            kind.label()
                        ),
                    );
                } else {
                    self.set_status(kind, CursorEffectStatus::Paused);
                }
                continue;
            };
            if let Err(error) = self.operations.refresh_visible_source(kind, point) {
                failed_surface = true;
                self.fail_surface(
                    kind,
                    format!("Could not refresh visible {} effect: {error}", kind.label()),
                );
            } else {
                self.set_status(kind, CursorEffectStatus::Active);
            }
        }
        if failed_surface {
            let requests = self.requested;
            self.apply_filter_lists(cheap_window_ids, requests);
            self.maybe_uninitialize(requests, false, false);
        }
    }

    /// Release effect hosts before the shared Magnification session. The
    /// controller calls this synchronously during worker shutdown.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        if self.shutdown_complete {
            return Ok(());
        }
        let mut errors = Vec::new();
        for kind in [EffectKind::Zoom, EffectKind::Halo] {
            if self.operations.has_surface(kind) {
                if let Err(error) = self.destroy_one(kind) {
                    self.set_status(
                        kind,
                        CursorEffectStatus::Unavailable(format!(
                            "Could not shut down {} effect: {error}",
                            kind.label()
                        )),
                    );
                    errors.push(error);
                } else {
                    self.set_status(kind, CursorEffectStatus::Disabled);
                }
            } else {
                self.configurations[kind.index()] = None;
                self.set_status(kind, CursorEffectStatus::Disabled);
            }
        }

        self.requested = EffectRequests::default();
        self.cached_live_points = [None, None];
        if !EFFECT_KINDS
            .into_iter()
            .any(|kind| self.operations.has_surface(kind))
        {
            if self.session_initialized {
                match self.operations.uninitialize_session() {
                    Ok(()) => {
                        self.session_initialized = false;
                        self.session_cleanup_failed = false;
                    }
                    Err(error) => {
                        self.session_cleanup_failed = true;
                        self.set_status(
                            EffectKind::Halo,
                            CursorEffectStatus::Unavailable(format!(
                                "Could not uninitialize Magnification: {error}"
                            )),
                        );
                        errors.push(error);
                    }
                }
            }
        } else if errors.is_empty() {
            errors.push("Magnifier windows remain after shutdown cleanup".into());
        }

        self.filters_dirty = false;
        self.shutdown_complete = !self.session_initialized
            && !EFFECT_KINDS
                .into_iter()
                .any(|kind| self.operations.has_surface(kind));
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn apply_filter_lists(&mut self, cheap_window_ids: &[usize], requests: EffectRequests) {
        if !self.filters_dirty {
            return;
        }

        // There are only two magnifiers. A failure disables that target and a
        // second pass refreshes the surviving target with the final host list.
        let mut failed = [false; 2];
        for _ in 0..EFFECT_KINDS.len() {
            let mut failures = Vec::new();
            let excluded = self.complete_exclusion_list(cheap_window_ids);
            for kind in EFFECT_KINDS {
                let index = kind.index();
                if failed[index]
                    || !requests.enabled(kind)
                    || !self.operations.has_surface(kind)
                    || self.is_unavailable(kind)
                {
                    continue;
                }
                if self.operations.host_window(kind).is_none() {
                    failures.push((kind, "effect host has no HWND".to_string()));
                    continue;
                }
                if let Err(error) = self.operations.set_filter_list(kind, &excluded) {
                    failures.push((kind, error));
                }
            }
            if failures.is_empty() {
                self.filters_dirty = false;
                return;
            }
            for (kind, error) in failures {
                failed[kind.index()] = true;
                self.fail_surface(
                    kind,
                    format!("Could not exclude recursive effect windows: {error}"),
                );
            }
        }
        self.filters_dirty = false;
    }

    fn complete_exclusion_list(&self, cheap_window_ids: &[usize]) -> Vec<usize> {
        let mut excluded = Vec::with_capacity(cheap_window_ids.len() + 2);
        for &window in cheap_window_ids {
            if window != 0 && !excluded.contains(&window) {
                excluded.push(window);
            }
        }
        for kind in EFFECT_KINDS {
            if let Some(window) = self.operations.host_window(kind)
                && window != 0
                && !excluded.contains(&window)
            {
                excluded.push(window);
            }
        }
        excluded
    }

    fn update_live_status(
        &mut self,
        requests: EffectRequests,
        current_point: Option<PhysicalPoint>,
    ) {
        for kind in EFFECT_KINDS {
            if !requests.enabled(kind)
                || !self.operations.has_surface(kind)
                || self.is_unavailable(kind)
            {
                continue;
            }
            let Some(_point) = current_point else {
                if self.operations.is_visible(kind)
                    && let Err(error) = self.operations.hide_surface(kind)
                {
                    self.fail_surface(
                        kind,
                        format!(
                            "Could not pause {} effect without a live sample: {error}",
                            kind.label()
                        ),
                    );
                    continue;
                }
                self.set_status(kind, CursorEffectStatus::Paused);
                continue;
            };

            if self.operations.is_visible(kind) {
                // The worker's next poll refreshes the cached live source.
                // Keeping that operation out of frame reconciliation avoids
                // duplicate magnifier work during cursor movement while still
                // allowing stationary desktop content to stay live.
                self.set_status(kind, CursorEffectStatus::Active);
            } else {
                self.set_status(kind, CursorEffectStatus::Prepared);
            }
        }
    }

    fn destroy_one(&mut self, kind: EffectKind) -> Result<(), String> {
        if !self.operations.has_surface(kind) {
            self.configurations[kind.index()] = None;
            return Ok(());
        }

        let hide_error = self.operations.hide_surface(kind).err();
        match self.operations.destroy_surface(kind) {
            Ok(()) => {
                self.configurations[kind.index()] = None;
                self.cached_live_points[kind.index()] = None;
                self.filters_dirty = true;
                Ok(())
            }
            Err(destroy) => {
                if let Some(hide) = hide_error {
                    Err(format!(
                        "hide failed: {hide}; DestroyWindow failed: {destroy}"
                    ))
                } else {
                    Err(format!("DestroyWindow failed: {destroy}"))
                }
            }
        }
    }

    fn fail_surface(&mut self, kind: EffectKind, reason: String) {
        self.cached_live_points[kind.index()] = None;
        self.filters_dirty = true;
        let reason = match self.destroy_one(kind) {
            Ok(()) => reason,
            Err(cleanup) => format!("{reason}; cleanup also failed: {cleanup}"),
        };
        self.set_status(kind, CursorEffectStatus::Unavailable(reason));
    }

    fn cache_live_points(
        &mut self,
        requests: EffectRequests,
        current_point: Option<PhysicalPoint>,
    ) {
        for kind in EFFECT_KINDS {
            let point = if requests.enabled(kind) && !self.is_unavailable(kind) {
                current_point
            } else {
                None
            };
            self.cached_live_points[kind.index()] = point;
        }
    }

    fn maybe_uninitialize(
        &mut self,
        requests: EffectRequests,
        topology_invalidated: bool,
        force: bool,
    ) {
        if !self.session_initialized
            || EFFECT_KINDS
                .into_iter()
                .any(|kind| self.operations.has_surface(kind))
            || (self.session_cleanup_failed && !topology_invalidated && !force)
        {
            return;
        }

        match self.operations.uninitialize_session() {
            Ok(()) => {
                self.session_initialized = false;
                self.session_cleanup_failed = false;
                for kind in EFFECT_KINDS {
                    if !requests.enabled(kind)
                        && matches!(self.effect_status(kind), CursorEffectStatus::Unavailable(_))
                        && !self.operations.has_surface(kind)
                    {
                        self.set_status(kind, CursorEffectStatus::Disabled);
                    }
                }
            }
            Err(error) => {
                self.session_cleanup_failed = true;
                let affected = EFFECT_KINDS
                    .into_iter()
                    .find(|kind| requests.enabled(*kind))
                    .or_else(|| {
                        EFFECT_KINDS
                            .into_iter()
                            .find(|kind| self.requested.enabled(*kind))
                    })
                    .unwrap_or(EffectKind::Halo);
                let prior = match self.effect_status(affected) {
                    CursorEffectStatus::Unavailable(reason) => format!("{reason}; "),
                    _ => String::new(),
                };
                self.set_status(
                    affected,
                    CursorEffectStatus::Unavailable(format!(
                        "{prior}Could not uninitialize Magnification: {error}"
                    )),
                );
            }
        }
    }

    fn is_unavailable(&self, kind: EffectKind) -> bool {
        matches!(self.effect_status(kind), CursorEffectStatus::Unavailable(_))
    }

    fn effect_status(&self, kind: EffectKind) -> &CursorEffectStatus {
        match kind {
            EffectKind::Halo => self.status.halo(),
            EffectKind::Zoom => self.status.zoom(),
        }
    }

    fn set_status(&mut self, kind: EffectKind, status: CursorEffectStatus) {
        match kind {
            EffectKind::Halo => self.status.set_halo(status),
            EffectKind::Zoom => self.status.set_zoom(status),
        }
    }
}

impl<O: EffectNativeOperations> Drop for CursorEffectsRuntime<O> {
    fn drop(&mut self) {
        if !self.shutdown_complete {
            let _ = self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        CursorEffectsRuntime, EffectConfiguration, EffectKind, EffectNativeOperations,
        EffectRequests,
    };
    use crate::coordinate_tool::model::{
        CoordinateEffectsStatus, CoordinateToolRuntimeState, CursorEffectStatus, PhysicalPoint,
    };
    use crate::coordinate_tool::settings::CoordinateToolPreferences;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailurePoint {
        Initialize,
        CreateHost(EffectKind),
        CreateRegion(EffectKind),
        AttachRegion(EffectKind),
        CreateChild(EffectKind),
        Configure(EffectKind),
        Filter(EffectKind),
        Hide(EffectKind),
        Refresh(EffectKind),
        Destroy(EffectKind),
        Uninitialize,
    }

    #[derive(Default)]
    struct FakeOperations {
        session: bool,
        surfaces: [bool; 2],
        child_ready: [bool; 2],
        visible: [bool; 2],
        failures: VecDeque<FailurePoint>,
        events: Vec<String>,
        create_attempts: [usize; 2],
        resource_counts: [usize; 2],
        configure_calls: usize,
        filter_calls: usize,
        refresh_calls: usize,
        partial_resources: usize,
        filter_lists: [Vec<usize>; 2],
    }

    impl FakeOperations {
        fn fail_next(&mut self, point: FailurePoint) {
            self.failures.push_back(point);
        }

        fn fails(&mut self, point: FailurePoint) -> bool {
            if let Some(index) = self.failures.iter().position(|failure| *failure == point) {
                self.failures.remove(index);
                true
            } else {
                false
            }
        }

        fn host_id(kind: EffectKind) -> usize {
            match kind {
                EffectKind::Halo => 100,
                EffectKind::Zoom => 101,
            }
        }
    }

    impl EffectNativeOperations for FakeOperations {
        fn initialize_session(&mut self) -> Result<(), String> {
            self.events.push("initialize".into());
            if self.fails(FailurePoint::Initialize) {
                return Err("injected initialize failure".into());
            }
            self.session = true;
            Ok(())
        }

        fn has_surface(&self, kind: EffectKind) -> bool {
            self.surfaces[kind.index()]
        }

        fn create_surface(
            &mut self,
            kind: EffectKind,
            _configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            self.events.push(format!("create-host:{}", kind.label()));
            self.create_attempts[kind.index()] += 1;
            if self.fails(FailurePoint::CreateHost(kind)) {
                return Err("injected host creation failure".into());
            }
            self.partial_resources += 1;
            self.resource_counts[kind.index()] = 1;
            self.surfaces[kind.index()] = true;
            self.events.push(format!("create-region:{}", kind.label()));
            if self.fails(FailurePoint::CreateRegion(kind)) {
                return Err("injected region creation failure".into());
            }
            self.partial_resources += 1;
            self.resource_counts[kind.index()] += 1;
            self.events.push(format!("attach-region:{}", kind.label()));
            if self.fails(FailurePoint::AttachRegion(kind)) {
                return Err("injected region ownership failure".into());
            }
            self.events.push(format!("create-child:{}", kind.label()));
            if self.fails(FailurePoint::CreateChild(kind)) {
                return Err("injected magnifier child creation failure".into());
            }
            self.partial_resources += 1;
            self.resource_counts[kind.index()] += 1;
            self.child_ready[kind.index()] = true;
            Ok(())
        }

        fn configure_surface(
            &mut self,
            kind: EffectKind,
            _configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            self.configure_calls += 1;
            self.events.push(format!("configure:{}", kind.label()));
            if self.fails(FailurePoint::Configure(kind)) {
                Err("injected configuration failure".into())
            } else {
                Ok(())
            }
        }

        fn host_window(&self, kind: EffectKind) -> Option<usize> {
            self.has_surface(kind).then(|| Self::host_id(kind))
        }

        fn set_filter_list(
            &mut self,
            kind: EffectKind,
            excluded_windows: &[usize],
        ) -> Result<(), String> {
            self.filter_calls += 1;
            self.events.push(format!("filter:{}", kind.label()));
            if !self.child_ready[kind.index()] {
                return Err("injected missing magnifier child".into());
            }
            if self.fails(FailurePoint::Filter(kind)) {
                return Err("injected filter failure".into());
            }
            self.filter_lists[kind.index()] = excluded_windows.to_vec();
            Ok(())
        }

        fn is_visible(&self, kind: EffectKind) -> bool {
            self.visible[kind.index()]
        }

        fn hide_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            self.events.push(format!("hide:{}", kind.label()));
            if self.fails(FailurePoint::Hide(kind)) {
                return Err("injected hide failure".into());
            }
            self.visible[kind.index()] = false;
            Ok(())
        }

        fn refresh_visible_source(
            &mut self,
            kind: EffectKind,
            _current_point: PhysicalPoint,
        ) -> Result<(), String> {
            self.events.push(format!("refresh:{}", kind.label()));
            self.refresh_calls += 1;
            if self.fails(FailurePoint::Refresh(kind)) {
                Err("injected live-source refresh failure".into())
            } else {
                Ok(())
            }
        }

        fn destroy_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            self.events.push(format!("destroy:{}", kind.label()));
            if self.fails(FailurePoint::Destroy(kind)) {
                return Err("injected surface destruction failure".into());
            }
            if self.surfaces[kind.index()] {
                self.surfaces[kind.index()] = false;
                self.child_ready[kind.index()] = false;
                self.visible[kind.index()] = false;
                self.partial_resources -= self.resource_counts[kind.index()];
                self.resource_counts[kind.index()] = 0;
            }
            Ok(())
        }

        fn uninitialize_session(&mut self) -> Result<(), String> {
            assert!(!self.surfaces.into_iter().any(|surface| surface));
            self.events.push("uninitialize".into());
            if self.fails(FailurePoint::Uninitialize) {
                return Err("injected uninitialize failure".into());
            }
            self.session = false;
            Ok(())
        }
    }

    fn requests(halo: bool, zoom: bool) -> EffectRequests {
        EffectRequests { halo, zoom }
    }

    fn reconcile(
        runtime: &mut CursorEffectsRuntime<FakeOperations>,
        requests: EffectRequests,
        preferences: &CoordinateToolPreferences,
        point: Option<PhysicalPoint>,
        topology_invalidated: bool,
    ) {
        runtime.reconcile(
            requests,
            preferences,
            point,
            &[1, 2, 3, 4],
            topology_invalidated,
        );
    }

    fn status(runtime: &CursorEffectsRuntime<FakeOperations>) -> CoordinateEffectsStatus {
        runtime.status()
    }

    fn poll(runtime: &mut CursorEffectsRuntime<FakeOperations>) {
        runtime.poll_visible_sources(&[1, 2, 3, 4]);
    }

    #[test]
    fn idle_and_cheap_only_leave_magnification_resources_uninitialized() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        reconcile(
            &mut runtime,
            requests(false, false),
            &CoordinateToolPreferences::default(),
            None,
            false,
        );
        assert!(runtime.operations.events.is_empty());
        assert_eq!(status(&runtime), CoordinateEffectsStatus::default());
        runtime.shutdown().unwrap();
    }

    #[test]
    fn effects_share_session_disable_independently_and_dispose_before_uninitialize() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let preferences = CoordinateToolPreferences::default();
        let live = Some(PhysicalPoint::new(-80, 150));
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            live,
            false,
        );
        assert!(runtime.operations.session);
        assert!(runtime.operations.has_surface(EffectKind::Halo));
        assert!(!runtime.operations.has_surface(EffectKind::Zoom));
        assert_eq!(runtime.operations.create_attempts, [1, 0]);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Prepared);

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            live,
            false,
        );
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(runtime.operations.filter_lists[0], [1, 2, 3, 4, 100, 101]);
        assert_eq!(runtime.operations.filter_lists[1], [1, 2, 3, 4, 100, 101]);
        let initialize_count = runtime
            .operations
            .events
            .iter()
            .filter(|event| *event == "initialize")
            .count();
        assert_eq!(initialize_count, 1);

        reconcile(
            &mut runtime,
            requests(false, true),
            &preferences,
            live,
            false,
        );
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(runtime.operations.session);
        assert_eq!(runtime.operations.filter_lists[1], [1, 2, 3, 4, 101]);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Disabled);
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);

        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            live,
            false,
        );
        assert!(!runtime.operations.session);
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Disabled);
        let destroy_zoom = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "destroy:zoom")
            .unwrap();
        let uninitialize = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "uninitialize")
            .unwrap();
        assert!(destroy_zoom < uninitialize);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn repeated_requests_preference_updates_and_shutdown_do_not_duplicate_surfaces() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let initial = CoordinateToolPreferences::default();
        let live = Some(PhysicalPoint::new(20, 30));
        reconcile(&mut runtime, requests(true, true), &initial, live, false);
        reconcile(&mut runtime, requests(true, true), &initial, live, false);
        assert_eq!(runtime.operations.create_attempts, [1, 1]);

        let mut changed = initial.clone();
        changed.halo.radius = 72;
        changed.zoom.zoom_factor = 3.0;
        reconcile(&mut runtime, requests(true, true), &changed, live, false);
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(runtime.operations.configure_calls, 2);

        runtime.shutdown().unwrap();
        runtime.shutdown().unwrap();
        assert_eq!(runtime.operations.partial_resources, 0);
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert!(!runtime.operations.session);
        assert_eq!(runtime.status(), CoordinateEffectsStatus::default());
    }

    #[test]
    fn partial_host_region_and_child_failures_release_only_the_affected_effect() {
        for failure in [
            FailurePoint::CreateHost(EffectKind::Zoom),
            FailurePoint::CreateRegion(EffectKind::Zoom),
            FailurePoint::AttachRegion(EffectKind::Zoom),
            FailurePoint::CreateChild(EffectKind::Zoom),
        ] {
            let mut operations = FakeOperations::default();
            operations.fail_next(failure);
            let mut runtime = CursorEffectsRuntime::new(operations);
            reconcile(
                &mut runtime,
                requests(true, true),
                &CoordinateToolPreferences::default(),
                Some(PhysicalPoint::new(-30, 45)),
                false,
            );

            assert!(runtime.operations.has_surface(EffectKind::Halo));
            assert!(!runtime.operations.has_surface(EffectKind::Zoom));
            assert_eq!(runtime.operations.partial_resources, 3);
            if failure != FailurePoint::CreateHost(EffectKind::Zoom) {
                assert!(
                    runtime
                        .operations
                        .events
                        .contains(&"destroy:zoom".to_string())
                );
            }
            assert!(matches!(
                runtime.status().zoom(),
                CursorEffectStatus::Unavailable(_)
            ));
            assert_eq!(*runtime.status().halo(), CursorEffectStatus::Prepared);
            runtime.shutdown().unwrap();
            assert_eq!(runtime.operations.partial_resources, 0);
        }
    }

    #[test]
    fn failed_partial_host_cleanup_is_retried_before_recreating_on_topology_change() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateChild(EffectKind::Halo));
        operations.fail_next(FailurePoint::Destroy(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-100, 12)),
            false,
        );
        assert!(runtime.operations.has_surface(EffectKind::Halo));
        assert_eq!(runtime.operations.partial_resources, 2);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(reason) if reason.contains("cleanup also failed")
        ));

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-101, 12)),
            true,
        );
        assert!(runtime.operations.has_surface(EffectKind::Halo));
        assert_eq!(runtime.operations.partial_resources, 3);
        assert_eq!(
            runtime.operations.create_attempts[EffectKind::Halo.index()],
            2
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Prepared);
        runtime.shutdown().unwrap();
        assert_eq!(runtime.operations.partial_resources, 0);
        assert!(!runtime.operations.session);
    }

    #[test]
    fn filter_failure_releases_its_target_and_refreshes_the_survivor_filter_list() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Filter(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        reconcile(
            &mut runtime,
            requests(true, true),
            &CoordinateToolPreferences::default(),
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        assert_eq!(runtime.operations.filter_lists[1], [1, 2, 3, 4, 101]);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn stable_failures_retry_only_after_toggle_or_display_invalidation() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateChild(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert_eq!(runtime.operations.create_attempts[0], 1);
        for x in 1..8 {
            reconcile(
                &mut runtime,
                requests(true, false),
                &preferences,
                Some(PhysicalPoint::new(x, 0)),
                false,
            );
        }
        assert_eq!(runtime.operations.create_attempts[0], 1);

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(8, 0)),
            true,
        );
        assert_eq!(runtime.operations.create_attempts[0], 2);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Prepared);

        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Disabled);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(9, 0)),
            false,
        );
        assert_eq!(runtime.operations.create_attempts[0], 3);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn topology_refresh_rebuilds_complete_filter_lists_and_live_only_refreshes_visible_surfaces() {
        let operations = FakeOperations::default();
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        let point = Some(PhysicalPoint::new(-1440, 300));
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            false,
        );
        assert_eq!(runtime.operations.refresh_calls, 0);
        let filters_before = runtime.operations.filter_calls;

        runtime.operations.visible[EffectKind::Halo.index()] = true;
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            true,
        );
        assert_eq!(runtime.operations.refresh_calls, 0);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);
        assert!(runtime.operations.filter_calls >= filters_before + 2);
        assert_eq!(runtime.operations.filter_lists[0], [1, 2, 3, 4, 100, 101]);
        assert_eq!(runtime.operations.filter_lists[1], [1, 2, 3, 4, 100, 101]);
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 1);

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            None,
            false,
        );
        assert_eq!(runtime.operations.refresh_calls, 1);
        assert!(!runtime.operations.visible[EffectKind::Halo.index()]);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Paused);
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            point,
            false,
        );
        assert_eq!(runtime.operations.refresh_calls, 1);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn stationary_poll_refreshes_only_visible_requested_effects_without_reconciling() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-1920, 240)),
            false,
        );
        let filters_before_poll = runtime.operations.filter_calls;
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 0);

        runtime.operations.visible[EffectKind::Halo.index()] = true;
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 1);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 2);
        assert_eq!(runtime.operations.filter_calls, filters_before_poll);

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            None,
            false,
        );
        assert!(!runtime.operations.visible[EffectKind::Halo.index()]);
        runtime.operations.visible[EffectKind::Halo.index()] = true;
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 2);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Paused);

        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, 2);
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Disabled);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn stationary_refresh_failure_is_latched_and_releases_only_its_effect() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            Some(PhysicalPoint::new(-250, 70)),
            false,
        );
        let filter_calls_before_poll = runtime.operations.filter_calls;
        runtime.operations.visible[EffectKind::Halo.index()] = true;
        runtime
            .operations
            .fail_next(FailurePoint::Refresh(EffectKind::Halo));
        poll(&mut runtime);
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(reason) if reason.contains("live-source refresh failure")
        ));
        assert_eq!(
            runtime.operations.filter_calls,
            filter_calls_before_poll + 1
        );
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Zoom.index()],
            [1, 2, 3, 4, 101]
        );
        let refresh_count = runtime.operations.refresh_calls;
        let filter_calls = runtime.operations.filter_calls;
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, refresh_count);
        assert_eq!(runtime.operations.filter_calls, filter_calls);
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);
        runtime.shutdown().unwrap();

        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-11, 32)),
            false,
        );
        runtime.operations.visible[EffectKind::Halo.index()] = true;
        runtime
            .operations
            .fail_next(FailurePoint::Refresh(EffectKind::Halo));
        poll(&mut runtime);
        assert!(!runtime.operations.session);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "uninitialize")
                .count(),
            1
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        runtime.shutdown().unwrap();
    }

    #[test]
    fn initialization_and_surface_release_failures_remain_status_and_cleanup_retries_on_shutdown() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        reconcile(
            &mut runtime,
            requests(true, true),
            &CoordinateToolPreferences::default(),
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        assert!(matches!(
            runtime.status().zoom(),
            CursorEffectStatus::Unavailable(_)
        ));
        let unavailable = runtime.status();
        for coordinate in 1..8 {
            reconcile(
                &mut runtime,
                requests(true, true),
                &CoordinateToolPreferences::default(),
                Some(PhysicalPoint::new(coordinate, coordinate)),
                false,
            );
        }
        assert_eq!(runtime.operations.create_attempts, [0, 0]);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            1
        );
        assert_eq!(runtime.status(), unavailable);

        reconcile(
            &mut runtime,
            requests(false, true),
            &CoordinateToolPreferences::default(),
            Some(PhysicalPoint::new(8, 7)),
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Disabled);
        assert!(matches!(
            runtime.status().zoom(),
            CursorEffectStatus::Unavailable(_)
        ));
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            1
        );

        reconcile(
            &mut runtime,
            requests(false, false),
            &CoordinateToolPreferences::default(),
            None,
            false,
        );
        reconcile(
            &mut runtime,
            requests(true, true),
            &CoordinateToolPreferences::default(),
            Some(PhysicalPoint::new(8, 8)),
            false,
        );
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            2
        );
        runtime.shutdown().unwrap();

        let operations = FakeOperations::default();
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        runtime
            .operations
            .fail_next(FailurePoint::Destroy(EffectKind::Halo));
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        assert!(runtime.operations.session);
        runtime.shutdown().unwrap();
        // The one-shot destroy failure is retried during synchronous shutdown;
        // the session is released only after its host is gone.
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(!runtime.operations.session);
        assert_eq!(runtime.operations.partial_resources, 0);
    }

    #[test]
    fn empty_failed_session_cleanup_is_latched_across_live_samples() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Halo));
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Zoom));
        operations.fail_next(FailurePoint::Uninitialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        let unavailable = runtime.status();
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(reason) if reason.contains("Could not uninitialize Magnification")
        ));

        for coordinate in 1..8 {
            reconcile(
                &mut runtime,
                requests(true, true),
                &preferences,
                Some(PhysicalPoint::new(coordinate, -coordinate)),
                false,
            );
        }

        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            1
        );
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "uninitialize")
                .count(),
            1
        );
        assert_eq!(runtime.status(), unavailable);

        runtime.shutdown().unwrap();
        assert!(!runtime.operations.session);
    }

    #[test]
    fn all_surface_failures_do_not_recreate_an_empty_session_for_each_sample() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Halo));
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Zoom));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert!(!runtime.operations.session);
        let unavailable = runtime.status();

        for coordinate in 1..8 {
            reconcile(
                &mut runtime,
                requests(true, true),
                &preferences,
                Some(PhysicalPoint::new(-coordinate, coordinate)),
                false,
            );
        }
        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(runtime.status(), unavailable);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            1
        );
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "uninitialize")
                .count(),
            1
        );

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            Some(PhysicalPoint::new(-8, 8)),
            true,
        );
        assert_eq!(runtime.operations.create_attempts, [2, 2]);
        assert!(runtime.operations.session);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn preference_update_failure_releases_only_that_effect() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let original = CoordinateToolPreferences::default();
        let point = Some(PhysicalPoint::new(12, 34));
        reconcile(&mut runtime, requests(true, true), &original, point, false);
        runtime
            .operations
            .fail_next(FailurePoint::Configure(EffectKind::Halo));

        let mut changed = original;
        changed.halo.radius = 91;
        reconcile(&mut runtime, requests(true, true), &changed, point, false);
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn uninitialize_failure_is_reported_once_then_retried_during_shutdown() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(2, 3)),
            false,
        );
        runtime.operations.fail_next(FailurePoint::Uninitialize);
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.session);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(reason)
                if reason.contains("uninitialize")
        ));
        let uninitializations = runtime
            .operations
            .events
            .iter()
            .filter(|event| *event == "uninitialize")
            .count();
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        let uninitializations_after_steady_off = runtime
            .operations
            .events
            .iter()
            .filter(|event| *event == "uninitialize")
            .count();
        assert_eq!(uninitializations_after_steady_off, uninitializations);

        runtime.shutdown().unwrap();
        assert!(!runtime.operations.session);
        assert_eq!(runtime.status(), CoordinateEffectsStatus::default());
    }

    #[test]
    fn legacy_runtime_flags_translate_to_independent_effect_requests() {
        let mut runtime_state = CoordinateToolRuntimeState::default();
        runtime_state.set_halo_enabled(true);
        assert_eq!(
            EffectRequests::from_runtime(&runtime_state),
            requests(true, false)
        );
        runtime_state.set_zoom_enabled(true);
        assert_eq!(
            EffectRequests::from_runtime(&runtime_state),
            requests(true, true)
        );
    }
}
