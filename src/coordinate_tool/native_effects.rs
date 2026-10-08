//! Lazy native ownership for the cursor-centered magnifier effects.
//!
//! The runtime owns one Magnification session and the native resources for
//! each requested effect. HALO presentation uses only the successful live
//! cursor sample supplied by the worker; zoom remains prepared for M4.

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
    fn configure_halo_outline(
        &mut self,
        preferences: HaloPreferences,
        fallback: bool,
    ) -> Result<(), String>;
    fn host_window(&self, kind: EffectKind) -> Option<usize>;
    fn auxiliary_window_ids(&self, kind: EffectKind) -> Vec<usize>;
    fn set_filter_list(
        &mut self,
        kind: EffectKind,
        excluded_windows: &[usize],
    ) -> Result<(), String>;
    fn is_visible(&self, kind: EffectKind) -> bool;
    fn is_halo_fallback_visible(&self) -> bool;
    fn hide_surface(&mut self, kind: EffectKind) -> Result<(), String>;
    fn refresh_visible_source(
        &mut self,
        kind: EffectKind,
        current_point: PhysicalPoint,
    ) -> Result<(), String>;
    fn present_live_source(
        &mut self,
        kind: EffectKind,
        current_point: PhysicalPoint,
    ) -> Result<(), String>;
    fn destroy_surface(&mut self, kind: EffectKind) -> Result<(), String>;
    fn release_auxiliary_surface(&mut self, kind: EffectKind) -> Result<(), String>;
    fn uninitialize_session(&mut self) -> Result<(), String>;
}

/// Reconciles requested effect state against native resources. This is owned
/// by the passive worker's surface backend and never runs on the GUI thread.
pub(crate) struct CursorEffectsRuntime<O: EffectNativeOperations> {
    operations: O,
    session_initialized: bool,
    session_initialization_failed: bool,
    session_cleanup_failed: bool,
    session_cleanup_reason: Option<String>,
    failures: [EffectFailureLatch; 2],
    configurations: [Option<EffectConfiguration>; 2],
    halo_outline_configuration: Option<(HaloPreferences, bool)>,
    halo_preferences: Option<HaloPreferences>,
    cached_live_points: [Option<PhysicalPoint>; 2],
    requested: EffectRequests,
    status: CoordinateEffectsStatus,
    filters_dirty: bool,
    shutdown_complete: bool,
}

/// Retry suppression is kept separate from the public presentation state.
/// Paused and Fallback are display states and must never erase a known native
/// failure; presentation failure records that the fallback itself failed.
#[derive(Clone, Debug, Default)]
struct EffectFailureLatch {
    native: Option<String>,
    presentation: Option<String>,
}

impl EffectFailureLatch {
    fn is_latched(&self) -> bool {
        self.native.is_some() || self.presentation.is_some()
    }
}

impl<O: EffectNativeOperations> CursorEffectsRuntime<O> {
    pub(crate) fn new(operations: O) -> Self {
        Self {
            operations,
            session_initialized: false,
            session_initialization_failed: false,
            session_cleanup_failed: false,
            session_cleanup_reason: None,
            failures: std::array::from_fn(|_| EffectFailureLatch::default()),
            configurations: [None, None],
            halo_outline_configuration: None,
            halo_preferences: None,
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
            .filter(|kind| requests.enabled(*kind) && !self.requested.enabled(*kind))
            .collect::<Vec<_>>();
        if !reenabled.is_empty() {
            self.session_initialization_failed = false;
            self.session_cleanup_failed = false;
            self.session_cleanup_reason = None;
            self.filters_dirty = true;
            for kind in reenabled {
                self.failures[kind.index()] = EffectFailureLatch::default();
                self.set_status(kind, CursorEffectStatus::Disabled);
            }
        }
        if topology_invalidated {
            self.filters_dirty = true;
            self.session_initialization_failed = false;
            for kind in EFFECT_KINDS {
                if requests.enabled(kind) && self.failures[kind.index()].is_latched() {
                    self.failures[kind.index()] = EffectFailureLatch::default();
                    self.set_status(kind, CursorEffectStatus::Disabled);
                }
            }
            self.session_cleanup_failed = false;
            self.session_cleanup_reason = None;
        }
        self.halo_preferences = Some(preferences.halo.normalized());

        // Clear disabled modes before shared-session setup. A failed session
        // initialization must never prevent an independently disabled effect
        // from releasing its host and status.
        for kind in EFFECT_KINDS {
            let requested = requests.enabled(kind);
            let was_requested = self.requested.enabled(kind);
            if requested {
                continue;
            }
            self.cached_live_points[kind.index()] = None;
            if was_requested {
                self.session_initialization_failed = false;
                self.session_cleanup_failed = false;
                self.session_cleanup_reason = None;
            }
            if was_requested || (topology_invalidated && self.operations.has_surface(kind)) {
                match self.release_effect(kind) {
                    Ok(()) => {
                        self.failures[kind.index()] = EffectFailureLatch::default();
                        if kind == EffectKind::Halo {
                            self.halo_outline_configuration = None;
                        }
                        self.set_status(kind, CursorEffectStatus::Disabled);
                    }
                    Err(error) => {
                        self.failures[kind.index()].presentation = Some(error.clone());
                        self.set_status(
                            kind,
                            CursorEffectStatus::Unavailable(format!(
                                "Could not release disabled {} effect: {error}",
                                kind.label()
                            )),
                        );
                    }
                }
            }
        }

        // Keep the optional outline independently owned and ready even when
        // Magnification initialization fails, so fallback can be added without
        // coupling it to the native magnifier HWND.
        if requests.halo
            && self.failures[EffectKind::Halo.index()]
                .presentation
                .is_none()
        {
            let fallback = self.failures[EffectKind::Halo.index()].native.is_some()
                && !self.operations.has_surface(EffectKind::Halo);
            if let Err(error) = self.ensure_halo_outline(preferences.halo, fallback) {
                self.fail_halo_presentation(format!("Could not configure halo outline: {error}"));
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
            .any(|kind| requests.enabled(kind) && !self.failures[kind.index()].is_latched());
        if !self.session_initialized
            && !self.session_initialization_failed
            && any_requested_effect_eligible
        {
            match self.operations.initialize_session() {
                Ok(()) => {
                    self.session_initialized = true;
                    self.session_initialization_failed = false;
                }
                Err(error) => {
                    self.session_initialization_failed = true;
                    for kind in EFFECT_KINDS {
                        if requests.enabled(kind) && !self.failures[kind.index()].is_latched() {
                            self.fail_native_surface(
                                kind,
                                format!(
                                    "Could not initialize Magnification for {}: {error}",
                                    kind.label()
                                ),
                            );
                        }
                    }
                }
            }
        }

        for kind in EFFECT_KINDS {
            let requested = requests.enabled(kind);
            if !requested || !self.session_initialized {
                continue;
            }

            let configuration = EffectConfiguration::from_preferences(kind, preferences);
            if self.failures[kind.index()].is_latched() {
                continue;
            }

            let mut needs_create = !self.operations.has_surface(kind);
            if !needs_create && self.configurations[kind.index()].is_none() {
                if let Err(error) = self.destroy_one(kind) {
                    self.fail_native_surface(
                        kind,
                        format!(
                            "Could not replace incomplete {} effect resources: {error}",
                            kind.label()
                        ),
                    );
                    continue;
                }
                needs_create = true;
            }

            if !needs_create {
                if self.configurations[kind.index()].as_ref() != Some(&configuration) {
                    if let Err(error) = self.operations.configure_surface(kind, &configuration) {
                        self.fail_native_surface(
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
                        self.fail_native_surface(
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
        self.update_live_status(requests, current_point, cheap_window_ids);
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
            if !self.requested.enabled(kind) {
                continue;
            }
            if kind == EffectKind::Halo && self.halo_fallback_ready() {
                if !self.operations.is_halo_fallback_visible() {
                    continue;
                }
                let Some(point) = self.cached_live_points[kind.index()] else {
                    if let Err(error) = self.operations.hide_surface(kind) {
                        self.fail_halo_presentation(format!(
                            "Could not pause halo fallback without a live sample: {error}"
                        ));
                        failed_surface = true;
                    } else {
                        self.set_status(kind, CursorEffectStatus::Paused);
                    }
                    continue;
                };
                if let Err(error) = self.operations.refresh_visible_source(kind, point) {
                    self.fail_halo_presentation(format!(
                        "Could not refresh visible halo fallback: {error}"
                    ));
                    failed_surface = true;
                } else {
                    let reason = self.halo_fallback_status_reason();
                    self.set_status(kind, CursorEffectStatus::Fallback(reason));
                }
                continue;
            }
            if !self.operations.has_surface(kind) || self.is_unavailable(kind) {
                continue;
            }
            if !self.operations.is_visible(kind) {
                continue;
            }
            let Some(point) = self.cached_live_points[kind.index()] else {
                if let Err(error) = self.operations.hide_surface(kind) {
                    failed_surface = true;
                    self.fail_native_surface(
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
                self.fail_native_surface(
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
            let point = self.cached_live_points[EffectKind::Halo.index()];
            if self.halo_fallback_ready() && requests.halo {
                self.update_halo_fallback(point);
            }
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
            if let Err(error) = self.release_effect(kind) {
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
        }

        self.requested = EffectRequests::default();
        self.cached_live_points = [None, None];
        if !self.has_magnification_surfaces() {
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
        self.shutdown_complete = !self.session_initialized && !self.has_effect_resources();
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
                self.fail_native_surface(
                    kind,
                    format!("Could not exclude recursive effect windows: {error}"),
                );
            }
        }
        self.filters_dirty = false;
    }

    fn complete_exclusion_list(&self, cheap_window_ids: &[usize]) -> Vec<usize> {
        let mut excluded = Vec::with_capacity(cheap_window_ids.len() + 4);
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
            for window in self.operations.auxiliary_window_ids(kind) {
                if window != 0 && !excluded.contains(&window) {
                    excluded.push(window);
                }
            }
        }
        excluded
    }

    fn update_live_status(
        &mut self,
        requests: EffectRequests,
        current_point: Option<PhysicalPoint>,
        cheap_window_ids: &[usize],
    ) {
        for kind in EFFECT_KINDS {
            if !requests.enabled(kind) {
                continue;
            }
            if kind == EffectKind::Halo && self.halo_fallback_ready() {
                self.update_halo_fallback(current_point);
                continue;
            }
            if !self.operations.has_surface(kind) || self.is_unavailable(kind) {
                continue;
            }
            let Some(point) = current_point else {
                if (kind == EffectKind::Halo || self.operations.is_visible(kind))
                    && let Err(error) = self.operations.hide_surface(kind)
                {
                    self.fail_native_surface(
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

            if kind == EffectKind::Halo {
                if let Err(error) = self.operations.present_live_source(kind, point) {
                    self.fail_native_surface(kind, format!("Could not present live halo: {error}"));
                    if self.halo_fallback_ready() {
                        self.apply_filter_lists(cheap_window_ids, requests);
                        self.update_halo_fallback(current_point);
                    }
                    continue;
                }
                if !self.operations.is_visible(kind) {
                    self.fail_native_surface(
                        kind,
                        "Native halo presentation completed without a visible host".into(),
                    );
                    if self.halo_fallback_ready() {
                        self.apply_filter_lists(cheap_window_ids, requests);
                        self.update_halo_fallback(current_point);
                    }
                    continue;
                }
                self.set_status(kind, CursorEffectStatus::Active);
            } else if self.operations.is_visible(kind) {
                // Zoom remains prepared until M4 supplies its presentation.
                self.set_status(kind, CursorEffectStatus::Active);
            } else {
                self.set_status(kind, CursorEffectStatus::Prepared);
            }
        }
    }

    fn destroy_one(&mut self, kind: EffectKind) -> Result<(), String> {
        let hide_error = self.operations.hide_surface(kind).err();
        if !self.operations.has_surface(kind) {
            self.configurations[kind.index()] = None;
            return hide_error.map_or(Ok(()), Err);
        }

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

    fn release_effect(&mut self, kind: EffectKind) -> Result<(), String> {
        let surface_error = self.destroy_one(kind).err();
        let auxiliary_error = self.operations.release_auxiliary_surface(kind).err();
        self.configurations[kind.index()] = None;
        self.cached_live_points[kind.index()] = None;
        if kind == EffectKind::Halo {
            self.halo_outline_configuration = None;
        }
        self.filters_dirty = true;
        match (surface_error, auxiliary_error) {
            (None, None) => Ok(()),
            (Some(error), None) | (None, Some(error)) => Err(error),
            (Some(surface), Some(auxiliary)) => Err(format!(
                "{surface}; auxiliary cleanup also failed: {auxiliary}"
            )),
        }
    }

    fn has_effect_resources(&self) -> bool {
        EFFECT_KINDS.into_iter().any(|kind| {
            self.operations.has_surface(kind)
                || !self.operations.auxiliary_window_ids(kind).is_empty()
        })
    }

    fn has_magnification_surfaces(&self) -> bool {
        EFFECT_KINDS
            .into_iter()
            .any(|kind| self.operations.has_surface(kind))
    }

    fn ensure_halo_outline(
        &mut self,
        preferences: HaloPreferences,
        fallback: bool,
    ) -> Result<(), String> {
        let preferences = preferences.normalized();
        let configuration = (preferences, fallback);
        if self.halo_outline_configuration == Some(configuration) {
            return Ok(());
        }
        self.operations
            .configure_halo_outline(preferences, fallback)?;
        self.halo_outline_configuration = Some(configuration);
        self.filters_dirty = true;
        Ok(())
    }

    fn halo_fallback_ready(&self) -> bool {
        let failure = &self.failures[EffectKind::Halo.index()];
        failure.native.is_some()
            && failure.presentation.is_none()
            && !self.operations.has_surface(EffectKind::Halo)
    }

    fn fail_native_surface(&mut self, kind: EffectKind, reason: String) {
        let index = kind.index();
        self.failures[index]
            .native
            .get_or_insert_with(|| reason.clone());
        self.filters_dirty = true;
        let cached_point = self.cached_live_points[index];
        let cleanup = self.destroy_one(kind).err();
        if kind == EffectKind::Halo {
            self.cached_live_points[index] = cached_point;
        } else {
            self.cached_live_points[index] = None;
        }
        let mut combined = self.failures[index].native.clone().unwrap_or(reason);
        if let Some(cleanup) = cleanup {
            combined.push_str(&format!("; cleanup also failed: {cleanup}"));
        }
        if kind == EffectKind::Halo && !self.operations.has_surface(kind) {
            self.failures[index].native = Some(combined.clone());
            let preferences = self
                .halo_preferences
                .unwrap_or_else(HaloPreferences::default);
            if let Err(error) = self.ensure_halo_outline(preferences, true) {
                self.failures[index].presentation = Some(error.clone());
                let hide_error = self.operations.hide_surface(kind).err();
                let cleanup = hide_error
                    .map(|hide| format!("; ring hide also failed: {hide}"))
                    .unwrap_or_default();
                self.set_status(
                    kind,
                    CursorEffectStatus::Unavailable(format!(
                        "{combined}; could not prepare contrasting fallback ring: {error}{cleanup}"
                    )),
                );
                return;
            }
            self.set_status(kind, CursorEffectStatus::Paused);
        } else {
            if kind == EffectKind::Halo {
                self.failures[index].presentation.get_or_insert_with(|| {
                    "failed Magnification host remained after cleanup".into()
                });
            }
            self.set_status(kind, CursorEffectStatus::Unavailable(combined));
        }
    }

    fn fail_halo_presentation(&mut self, reason: String) {
        let index = EffectKind::Halo.index();
        self.failures[index]
            .presentation
            .get_or_insert_with(|| reason.clone());
        self.filters_dirty = true;
        let cleanup = self.destroy_one(EffectKind::Halo).err();
        let mut combined = self.failures[index]
            .native
            .clone()
            .unwrap_or_else(|| "Halo presentation failed".into());
        combined.push_str(&format!("; {reason}"));
        if let Some(cleanup) = cleanup {
            combined.push_str(&format!("; cleanup also failed: {cleanup}"));
        }
        self.set_status(EffectKind::Halo, CursorEffectStatus::Unavailable(combined));
    }

    fn update_halo_fallback(&mut self, point: Option<PhysicalPoint>) {
        let index = EffectKind::Halo.index();
        if self.failures[index].presentation.is_some() {
            return;
        }
        let Some(point) = point else {
            if let Err(error) = self.operations.hide_surface(EffectKind::Halo) {
                self.fail_halo_presentation(format!(
                    "Could not pause halo fallback without a live sample: {error}"
                ));
            } else {
                self.set_status(EffectKind::Halo, CursorEffectStatus::Paused);
            }
            return;
        };
        if let Err(error) = self.operations.present_live_source(EffectKind::Halo, point) {
            self.fail_halo_presentation(format!(
                "Could not present contrasting halo fallback: {error}"
            ));
            return;
        }
        if !self.operations.is_halo_fallback_visible() {
            self.fail_halo_presentation(
                "Fallback ring presentation completed without a visible ring".into(),
            );
            return;
        }
        let reason = self.halo_fallback_status_reason();
        self.set_status(EffectKind::Halo, CursorEffectStatus::Fallback(reason));
    }

    fn cache_live_points(
        &mut self,
        requests: EffectRequests,
        current_point: Option<PhysicalPoint>,
    ) {
        for kind in EFFECT_KINDS {
            let failure = &self.failures[kind.index()];
            let point = if requests.enabled(kind)
                && failure.presentation.is_none()
                && (kind == EffectKind::Halo || failure.native.is_none())
            {
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
            || self.has_magnification_surfaces()
            || (self.session_cleanup_failed && !topology_invalidated && !force)
        {
            return;
        }

        match self.operations.uninitialize_session() {
            Ok(()) => {
                self.session_initialized = false;
                self.session_cleanup_failed = false;
                self.session_cleanup_reason = None;
                for kind in EFFECT_KINDS {
                    if !requests.enabled(kind)
                        && matches!(self.effect_status(kind), CursorEffectStatus::Unavailable(_))
                        && !self.operations.has_surface(kind)
                        && self.operations.auxiliary_window_ids(kind).is_empty()
                    {
                        self.set_status(kind, CursorEffectStatus::Disabled);
                    }
                }
            }
            Err(error) => {
                self.session_cleanup_failed = true;
                let cleanup_reason = format!("Could not uninitialize Magnification: {error}");
                self.session_cleanup_reason = Some(cleanup_reason.clone());
                let affected = EFFECT_KINDS
                    .into_iter()
                    .find(|kind| requests.enabled(*kind))
                    .or_else(|| {
                        EFFECT_KINDS
                            .into_iter()
                            .find(|kind| self.requested.enabled(*kind))
                    })
                    .unwrap_or(EffectKind::Halo);
                let status = match self.effect_status(affected) {
                    CursorEffectStatus::Fallback(reason) => {
                        CursorEffectStatus::Fallback(format!("{reason}; {cleanup_reason}"))
                    }
                    CursorEffectStatus::Unavailable(reason) => {
                        CursorEffectStatus::Unavailable(format!("{reason}; {cleanup_reason}"))
                    }
                    _ => CursorEffectStatus::Unavailable(cleanup_reason),
                };
                self.set_status(affected, status);
            }
        }
    }

    fn is_unavailable(&self, kind: EffectKind) -> bool {
        self.failures[kind.index()].is_latched()
    }

    fn halo_fallback_status_reason(&self) -> String {
        let mut reason = self.failures[EffectKind::Halo.index()]
            .native
            .clone()
            .unwrap_or_else(|| "Magnification is unavailable".into());
        if let Some(cleanup) = &self.session_cleanup_reason {
            reason.push_str("; ");
            reason.push_str(cleanup);
        }
        reason
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
    use crate::coordinate_tool::settings::{CoordinateToolPreferences, HaloPreferences};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailurePoint {
        Initialize,
        CreateHost(EffectKind),
        CreateRegion(EffectKind),
        AttachRegion(EffectKind),
        CreateChild(EffectKind),
        Configure(EffectKind),
        ConfigureOutline,
        ConfigureFallback,
        Filter(EffectKind),
        Hide(EffectKind),
        Refresh(EffectKind),
        RefreshFallback,
        Present(EffectKind),
        PresentFallback,
        Destroy(EffectKind),
        ReleaseOutline,
        Uninitialize,
    }

    #[derive(Default)]
    struct FakeOperations {
        session: bool,
        surfaces: [bool; 2],
        child_ready: [bool; 2],
        visible: [bool; 2],
        outline_window_exists: bool,
        outline_visible: bool,
        outline_enabled: bool,
        outline_fallback: bool,
        outline_preferences: Option<HaloPreferences>,
        configurations: [Option<EffectConfiguration>; 2],
        failures: VecDeque<FailurePoint>,
        events: Vec<String>,
        create_attempts: [usize; 2],
        resource_counts: [usize; 2],
        configure_calls: usize,
        filter_calls: usize,
        release_outline_calls: usize,
        refresh_calls: usize,
        present_calls: usize,
        presented_points: Vec<PhysicalPoint>,
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

        const fn outline_id() -> usize {
            102
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
            configuration: &EffectConfiguration,
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
            self.configurations[kind.index()] = Some(configuration.clone());
            Ok(())
        }

        fn configure_surface(
            &mut self,
            kind: EffectKind,
            configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            self.configure_calls += 1;
            self.events.push(format!("configure:{}", kind.label()));
            if self.fails(FailurePoint::Configure(kind)) {
                Err("injected configuration failure".into())
            } else {
                self.configurations[kind.index()] = Some(configuration.clone());
                Ok(())
            }
        }

        fn configure_halo_outline(
            &mut self,
            preferences: HaloPreferences,
            fallback: bool,
        ) -> Result<(), String> {
            let preferences = preferences.normalized();
            if self.outline_preferences == Some(preferences) && self.outline_fallback == fallback {
                return Ok(());
            }
            self.events.push(if fallback {
                "configure-fallback".into()
            } else {
                "configure-outline".into()
            });
            if self.fails(if fallback {
                FailurePoint::ConfigureFallback
            } else {
                FailurePoint::ConfigureOutline
            }) {
                return Err("injected halo outline configuration failure".into());
            }
            if fallback || preferences.outline_enabled {
                self.outline_window_exists = true;
                self.outline_enabled = preferences.outline_enabled;
            } else {
                self.outline_visible = false;
                self.outline_enabled = false;
            }
            self.outline_preferences = Some(preferences);
            self.outline_fallback = fallback;
            Ok(())
        }

        fn host_window(&self, kind: EffectKind) -> Option<usize> {
            self.has_surface(kind).then(|| Self::host_id(kind))
        }

        fn auxiliary_window_ids(&self, kind: EffectKind) -> Vec<usize> {
            if kind == EffectKind::Halo && self.outline_window_exists {
                vec![Self::outline_id()]
            } else {
                Vec::new()
            }
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

        fn is_halo_fallback_visible(&self) -> bool {
            self.outline_fallback && self.outline_visible
        }

        fn hide_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            self.events.push(format!("hide:{}", kind.label()));
            if self.fails(FailurePoint::Hide(kind)) {
                return Err("injected hide failure".into());
            }
            self.visible[kind.index()] = false;
            if kind == EffectKind::Halo {
                self.outline_visible = false;
            }
            Ok(())
        }

        fn refresh_visible_source(
            &mut self,
            kind: EffectKind,
            current_point: PhysicalPoint,
        ) -> Result<(), String> {
            self.events.push(format!("refresh:{}", kind.label()));
            self.refresh_calls += 1;
            if kind == EffectKind::Halo && self.outline_fallback {
                if self.fails(FailurePoint::RefreshFallback) {
                    return Err("injected fallback live-source refresh failure".into());
                }
                self.presented_points.push(current_point);
                return Ok(());
            }
            if self.fails(FailurePoint::Refresh(kind)) {
                Err("injected live-source refresh failure".into())
            } else {
                self.presented_points.push(current_point);
                Ok(())
            }
        }

        fn present_live_source(
            &mut self,
            kind: EffectKind,
            current_point: PhysicalPoint,
        ) -> Result<(), String> {
            self.events.push(format!("present:{}", kind.label()));
            self.present_calls += 1;
            if kind == EffectKind::Halo && self.outline_fallback {
                if self.fails(FailurePoint::PresentFallback) {
                    return Err("injected fallback passive presentation failure".into());
                }
                self.presented_points.push(current_point);
                self.visible[kind.index()] = false;
                self.outline_visible = true;
                return Ok(());
            }
            if self.fails(FailurePoint::Present(kind)) {
                return Err("injected passive presentation failure".into());
            }
            self.presented_points.push(current_point);
            self.visible[kind.index()] = true;
            if kind == EffectKind::Halo {
                self.outline_visible = self.outline_enabled;
            }
            Ok(())
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

        fn release_auxiliary_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            if kind != EffectKind::Halo {
                return Ok(());
            }
            self.release_outline_calls += 1;
            self.events.push("release-outline".into());
            if self.fails(FailurePoint::ReleaseOutline) {
                return Err("injected halo outline cleanup failure".into());
            }
            self.outline_window_exists = false;
            self.outline_visible = false;
            self.outline_enabled = false;
            self.outline_fallback = false;
            self.outline_preferences = None;
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
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);

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
    fn halo_presents_the_current_live_sample_and_updates_preferences_in_place() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let initial = CoordinateToolPreferences::default();
        let frozen_display_point = PhysicalPoint::new(700, -900);
        let live_point = PhysicalPoint::new(-1540, 310);
        reconcile(
            &mut runtime,
            requests(true, true),
            &initial,
            Some(live_point),
            false,
        );

        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);
        assert_eq!(runtime.operations.presented_points, [live_point]);
        assert!(
            !runtime
                .operations
                .presented_points
                .contains(&frozen_display_point)
        );

        runtime.operations.events.clear();
        let mut changed = initial;
        changed.halo.radius = 91;
        changed.halo.inversion_strength = 0.75;
        changed.halo.outline_enabled = true;
        reconcile(
            &mut runtime,
            requests(true, true),
            &changed,
            Some(live_point),
            false,
        );

        assert_eq!(runtime.operations.create_attempts, [1, 1]);
        assert_eq!(runtime.operations.configure_calls, 1);
        assert_eq!(
            runtime.operations.configurations[EffectKind::Halo.index()],
            Some(EffectConfiguration::Halo(changed.halo))
        );
        assert!(runtime.operations.outline_visible);
        let expected_filter = [1, 2, 3, 4, 100, FakeOperations::outline_id(), 101];
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Halo.index()],
            expected_filter
        );
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Zoom.index()],
            expected_filter
        );
        assert_eq!(
            runtime.operations.presented_points,
            [live_point, live_point]
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);

        let outline_configuration = runtime
            .operations
            .events
            .iter()
            .position(|event| event == "configure-outline")
            .unwrap();
        let halo_filter = runtime
            .operations
            .events
            .iter()
            .position(|event| event == "filter:halo")
            .unwrap();
        let zoom_filter = runtime
            .operations
            .events
            .iter()
            .position(|event| event == "filter:zoom")
            .unwrap();
        let presentation = runtime
            .operations
            .events
            .iter()
            .position(|event| event == "present:halo")
            .unwrap();
        assert!(outline_configuration < halo_filter);
        assert!(outline_configuration < zoom_filter);
        assert!(halo_filter < presentation);
        assert!(zoom_filter < presentation);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn halo_missing_live_sample_hides_host_and_outline_until_sampling_recovers() {
        let mut runtime = CursorEffectsRuntime::new(FakeOperations::default());
        let mut preferences = CoordinateToolPreferences::default();
        preferences.halo.outline_enabled = true;
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-250, 88)),
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        assert!(runtime.operations.visible[EffectKind::Halo.index()]);
        assert!(runtime.operations.outline_visible);

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            None,
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Paused);
        assert!(!runtime.operations.visible[EffectKind::Halo.index()]);
        assert!(!runtime.operations.outline_visible);
        let presentations_before_poll = runtime.operations.present_calls;
        poll(&mut runtime);
        assert_eq!(runtime.operations.present_calls, presentations_before_poll);

        let recovered = PhysicalPoint::new(-251, 88);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(recovered),
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        assert_eq!(runtime.operations.presented_points.last(), Some(&recovered));
        assert!(runtime.operations.outline_visible);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn halo_presentation_failure_never_publishes_active_and_keeps_other_effect_prepared() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Present(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let mut preferences = CoordinateToolPreferences::default();
        preferences.halo.outline_enabled = true;
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            Some(PhysicalPoint::new(40, -75)),
            false,
        );

        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(reason) if reason.contains("present live halo")
        ));
        assert_eq!(*runtime.status().zoom(), CursorEffectStatus::Prepared);
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(!runtime.operations.visible[EffectKind::Halo.index()]);
        assert!(runtime.operations.outline_fallback);
        assert!(runtime.operations.outline_visible);
        assert!(runtime.operations.session);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn retained_outline_does_not_keep_an_empty_magnification_session_alive() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let mut preferences = CoordinateToolPreferences::default();
        preferences.halo.outline_enabled = true;

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-40, 72)),
            false,
        );

        assert!(runtime.operations.outline_window_exists);
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
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
            CursorEffectStatus::Fallback(_)
        ));
        assert!(runtime.operations.outline_fallback);
        runtime.shutdown().unwrap();
        assert!(!runtime.operations.outline_window_exists);
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
            assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
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
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
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
            CursorEffectStatus::Fallback(_)
        ));
        assert!(runtime.operations.outline_fallback);
        assert_eq!(
            runtime.operations.filter_lists[1],
            [1, 2, 3, 4, FakeOperations::outline_id(), 101]
        );
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
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));
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
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Active);
        assert!(!runtime.operations.outline_fallback);

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

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            true,
        );
        assert_eq!(runtime.operations.refresh_calls, 0);
        assert_eq!(runtime.operations.present_calls, 2);
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
        assert_eq!(runtime.operations.refresh_calls, 0);
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
            CursorEffectStatus::Fallback(reason) if reason.contains("live-source refresh failure")
        ));
        assert_eq!(
            runtime.operations.filter_calls,
            filter_calls_before_poll + 1
        );
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Zoom.index()],
            [1, 2, 3, 4, FakeOperations::outline_id(), 101]
        );
        let refresh_count = runtime.operations.refresh_calls;
        let filter_calls = runtime.operations.filter_calls;
        poll(&mut runtime);
        assert_eq!(runtime.operations.refresh_calls, refresh_count + 1);
        assert_eq!(runtime.operations.filter_calls, filter_calls);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));
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
            CursorEffectStatus::Fallback(_)
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
            CursorEffectStatus::Fallback(_)
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
            CursorEffectStatus::Fallback(reason) if reason.contains("Could not uninitialize Magnification")
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
            CursorEffectStatus::Fallback(_)
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
    fn halo_initialization_failure_falls_back_without_retrying_across_pause_and_recovery() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        let first = PhysicalPoint::new(-1430, 220);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(first),
            false,
        );

        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(reason) if reason.contains("Could not initialize Magnification")
        ));
        assert!(!runtime.operations.session);
        assert!(!runtime.operations.has_surface(EffectKind::Halo));
        assert!(runtime.operations.outline_window_exists);
        assert!(runtime.operations.outline_fallback);
        assert!(runtime.operations.outline_visible);
        assert_eq!(runtime.operations.create_attempts, [0, 0]);

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            None,
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Paused);
        assert!(!runtime.operations.outline_visible);
        poll(&mut runtime);
        assert_eq!(runtime.operations.create_attempts, [0, 0]);

        let recovered = PhysicalPoint::new(-1429, 221);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(recovered),
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));
        assert!(runtime.operations.outline_visible);
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
        assert_eq!(runtime.operations.presented_points, [first, recovered]);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn halo_fallback_coexists_with_zoom_filters_and_releases_independently() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        let point = Some(PhysicalPoint::new(-900, 310));
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            point,
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            false,
        );
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(runtime.operations.outline_visible);
        assert!(runtime.operations.outline_fallback);
        assert_eq!(runtime.operations.create_attempts, [0, 1]);
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Zoom.index()],
            [
                1,
                2,
                3,
                4,
                FakeOperations::outline_id(),
                FakeOperations::host_id(EffectKind::Zoom)
            ]
        );
        let filter = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "filter:zoom")
            .unwrap();
        let presentation = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "present:halo")
            .unwrap();
        assert!(filter < presentation);

        reconcile(
            &mut runtime,
            requests(false, true),
            &preferences,
            point,
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Disabled);
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(!runtime.operations.outline_window_exists);
        assert_eq!(
            runtime.operations.filter_lists[EffectKind::Zoom.index()],
            [1, 2, 3, 4, FakeOperations::host_id(EffectKind::Zoom)]
        );
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert!(!runtime.operations.session);
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
    fn disabling_last_zoom_uninitializes_magnification_while_halo_fallback_remains() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Halo));
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        let point = Some(PhysicalPoint::new(120, -440));
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            point,
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));

        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            false,
        );
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(runtime.operations.outline_visible);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            point,
            false,
        );

        assert!(!runtime.operations.session);
        assert!(!runtime.operations.has_surface(EffectKind::Zoom));
        assert!(runtime.operations.outline_window_exists);
        assert!(runtime.operations.outline_visible);
        assert!(runtime.operations.outline_fallback);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(_)
        ));
        let release_outline = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "release-outline");
        let uninitialize = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "uninitialize")
            .unwrap();
        assert!(release_outline.is_none_or(|release| release < uninitialize));

        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert!(!runtime.operations.outline_window_exists);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn shutdown_releases_fallback_ring_and_magnifiers_before_uninitializing() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        let point = Some(PhysicalPoint::new(-20, 60));
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            point,
            false,
        );
        reconcile(
            &mut runtime,
            requests(true, true),
            &preferences,
            point,
            false,
        );
        assert!(runtime.operations.has_surface(EffectKind::Zoom));
        assert!(runtime.operations.outline_visible);

        runtime.shutdown().unwrap();
        assert_eq!(runtime.operations.partial_resources, 0);
        assert!(!runtime.operations.outline_window_exists);
        assert!(!runtime.operations.session);
        let destroy_zoom = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "destroy:zoom")
            .unwrap();
        let release_ring = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "release-outline")
            .unwrap();
        let uninitialize = runtime
            .operations
            .events
            .iter()
            .rposition(|event| event == "uninitialize")
            .unwrap();
        assert!(destroy_zoom < release_ring);
        assert!(release_ring < uninitialize);
    }

    #[test]
    fn halo_fallback_presentation_failures_are_latched_until_reenabled() {
        let preferences = CoordinateToolPreferences::default();
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        operations.fail_next(FailurePoint::ConfigureFallback);
        let mut runtime = CursorEffectsRuntime::new(operations);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert!(
            matches!(runtime.status().halo(), CursorEffectStatus::Unavailable(reason) if reason.contains("contrasting fallback ring"))
        );
        for x in 1..5 {
            let mut changed = preferences.clone();
            changed.halo.radius += x;
            reconcile(
                &mut runtime,
                requests(true, false),
                &changed,
                Some(PhysicalPoint::new(x, x)),
                false,
            );
        }
        assert_eq!(runtime.operations.release_outline_calls, 0);
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "configure-fallback")
                .count(),
            1
        );
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            1
        );
        runtime.shutdown().unwrap();

        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        operations.fail_next(FailurePoint::PresentFallback);
        let mut runtime = CursorEffectsRuntime::new(operations);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        for x in 1..5 {
            reconcile(
                &mut runtime,
                requests(true, false),
                &preferences,
                Some(PhysicalPoint::new(x, x)),
                false,
            );
            poll(&mut runtime);
        }
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "present:halo")
                .count(),
            1
        );
        assert_eq!(runtime.operations.create_attempts, [0, 0]);
        runtime.shutdown().unwrap();

        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(0, 0)),
            false,
        );
        runtime.operations.fail_next(FailurePoint::RefreshFallback);
        poll(&mut runtime);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Unavailable(_)
        ));
        let refreshes = runtime.operations.refresh_calls;
        for _ in 0..3 {
            poll(&mut runtime);
        }
        assert_eq!(runtime.operations.refresh_calls, refreshes);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn fallback_reason_survives_stationary_frames_and_paused_sample_recovery() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::CreateHost(EffectKind::Halo));
        operations.fail_next(FailurePoint::Uninitialize);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-50, 80)),
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(reason) if reason.contains("Could not uninitialize Magnification")
        ));
        let initializations = runtime
            .operations
            .events
            .iter()
            .filter(|event| *event == "initialize")
            .count();
        let uninitializations = runtime
            .operations
            .events
            .iter()
            .filter(|event| *event == "uninitialize")
            .count();

        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-49, 80)),
            false,
        );
        poll(&mut runtime);
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(reason) if reason.contains("Could not uninitialize Magnification")
        ));
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            None,
            false,
        );
        assert_eq!(*runtime.status().halo(), CursorEffectStatus::Paused);
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(-48, 80)),
            false,
        );
        assert!(matches!(
            runtime.status().halo(),
            CursorEffectStatus::Fallback(reason) if reason.contains("Could not uninitialize Magnification")
        ));
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "initialize")
                .count(),
            initializations
        );
        assert_eq!(
            runtime
                .operations
                .events
                .iter()
                .filter(|event| *event == "uninitialize")
                .count(),
            uninitializations
        );
        runtime.shutdown().unwrap();
    }

    #[test]
    fn failed_fallback_ring_disposal_is_bounded_and_shutdown_retries_cleanup() {
        let mut operations = FakeOperations::default();
        operations.fail_next(FailurePoint::Initialize);
        operations.fail_next(FailurePoint::ReleaseOutline);
        let mut runtime = CursorEffectsRuntime::new(operations);
        let preferences = CoordinateToolPreferences::default();
        reconcile(
            &mut runtime,
            requests(true, false),
            &preferences,
            Some(PhysicalPoint::new(4, 7)),
            false,
        );
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
        assert!(runtime.operations.outline_window_exists);
        assert_eq!(runtime.operations.release_outline_calls, 1);
        reconcile(
            &mut runtime,
            requests(false, false),
            &preferences,
            None,
            false,
        );
        assert_eq!(runtime.operations.release_outline_calls, 1);
        runtime.shutdown().unwrap();
        assert!(!runtime.operations.outline_window_exists);
        assert_eq!(runtime.operations.release_outline_calls, 2);
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
