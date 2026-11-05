//! Presentation lifecycle for persisted ordinary utility-window sizes.
//!
//! Settings remains the durable owner. This module applies the accepted
//! preference when a window opens or resets, keeps the body sized to egui's
//! current offer while it is visible, and submits only a settled resize-grip
//! gesture to the existing settings-save worker.

use crate::settings::utility_dialog_sizes::UtilitySizeMutation;
use crate::settings::{
    PreferredWindowSize, UtilityDialogSizes, UtilitySizeAcknowledgement, UtilitySizePreference,
    UtilitySizeRevision, UtilityToolKey,
};
use crate::settings_save::SettingsSaveWorkerRegistry;
use eframe::egui::{self, Context, Id, InnerResponse, Rect, Ui, Vec2, Window};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const VIEWPORT_MARGIN: f32 = 16.0;
const ACK_POLL_INTERVAL: Duration = Duration::from_millis(16);
const CONTEXT_CAPABILITY_ID: &str = "multi_launcher.utility_dialog_size_capability";

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResizeRelease {
    after_frame: u64,
    starting_size: Vec2,
}

#[derive(Debug)]
struct WindowInstance {
    key: UtilityToolKey,
    is_open: bool,
    last_seen_frame: Option<u64>,
    last_preference: Option<UtilitySizePreference>,
    last_revision: Option<UtilitySizeRevision>,
    has_applied_restore: bool,
    restore_pending: bool,
    last_outer_rect: Option<Rect>,
    last_body_rect: Option<Rect>,
    last_body_clip: Option<Rect>,
    resize_start: Option<Vec2>,
    resize_release: Option<ResizeRelease>,
    locally_resized: bool,
    restore_after_size_ack: bool,
}

impl WindowInstance {
    fn new(key: UtilityToolKey) -> Self {
        Self {
            key,
            is_open: false,
            last_seen_frame: None,
            last_preference: None,
            last_revision: None,
            has_applied_restore: false,
            restore_pending: true,
            last_outer_rect: None,
            last_body_rect: None,
            last_body_clip: None,
            resize_start: None,
            resize_release: None,
            locally_resized: false,
            restore_after_size_ack: false,
        }
    }
}

#[derive(Debug)]
struct NativeWindowInstance {
    key: UtilityToolKey,
    initial_restore_applied: bool,
    last_reset_revision: Option<UtilitySizeRevision>,
}

impl NativeWindowInstance {
    fn new(key: UtilityToolKey) -> Self {
        Self {
            key,
            initial_restore_applied: false,
            last_reset_revision: None,
        }
    }
}

/// Cloneable access to the one app-owned utility-size lifecycle and preference
/// cache. The short state locks are never held while a Window renders content,
/// so a nested utility can use the same capability from its content closure.
#[derive(Clone)]
pub(crate) struct UtilityDialogSizeManager {
    state: Arc<Mutex<UtilityDialogSizeState>>,
}

struct UtilityDialogSizeState {
    settings_path: String,
    workers: SettingsSaveWorkerRegistry,
    sizes: UtilityDialogSizes,
    frame: u64,
    instances: HashMap<Id, WindowInstance>,
    native_instances: HashMap<Id, NativeWindowInstance>,
    accepted_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    waiting_for_ack: HashMap<UtilityToolKey, UtilitySizeRevision>,
    local_resize_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    reset_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    known_keys: HashSet<UtilityToolKey>,
    reported_failures: HashMap<UtilityToolKey, UtilitySizeRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UtilityDialogSizeWriteFailure {
    pub(crate) key: UtilityToolKey,
    pub(crate) message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UtilityDialogSizeStatus {
    pub(crate) available: bool,
    pub(crate) preference: UtilitySizePreference,
    pub(crate) acknowledged_revision: Option<UtilitySizeRevision>,
    pub(crate) pending_revision: Option<UtilitySizeRevision>,
    pub(crate) failure: Option<(UtilitySizeRevision, String)>,
}

impl Default for UtilityDialogSizeStatus {
    fn default() -> Self {
        Self {
            available: false,
            preference: UtilitySizePreference::Missing,
            acknowledged_revision: None,
            pending_revision: None,
            failure: None,
        }
    }
}

impl UtilityDialogSizeManager {
    pub(crate) fn new(
        settings_path: String,
        workers: SettingsSaveWorkerRegistry,
        sizes: UtilityDialogSizes,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(UtilityDialogSizeState {
                settings_path,
                workers,
                sizes,
                frame: 0,
                instances: HashMap::new(),
                native_instances: HashMap::new(),
                accepted_revisions: HashMap::new(),
                waiting_for_ack: HashMap::new(),
                local_resize_revisions: HashMap::new(),
                reset_revisions: HashMap::new(),
                known_keys: HashSet::new(),
                reported_failures: HashMap::new(),
            })),
        }
    }

    pub(crate) fn install_in_context(&self, ctx: &Context) {
        ctx.data_mut(|data| {
            data.insert_temp(Id::new(CONTEXT_CAPABILITY_ID), self.clone());
        });
    }

    pub(crate) fn for_context(ctx: &Context) -> UtilityDialogSizeAccess {
        UtilityDialogSizeAccess(
            ctx.data(|data| data.get_temp::<Self>(Id::new(CONTEXT_CAPABILITY_ID))),
        )
    }

    pub(crate) fn rebind(&self, settings_path: String, workers: SettingsSaveWorkerRegistry) {
        let mut state = self.lock();
        state.settings_path = settings_path;
        state.workers = workers;
    }

    fn submit_resize(
        &self,
        key: UtilityToolKey,
        size: PreferredWindowSize,
        ctx: Option<&Context>,
        restore_after_ack: Option<Id>,
    ) -> Result<(), String> {
        let (workers, path) = {
            let state = self.lock();
            (state.workers.clone(), state.settings_path.clone())
        };
        if let Err(error) =
            workers.queue_utility_size_mutation(path, key, UtilitySizeMutation::Set(size))
        {
            tracing::warn!(tool = key.as_str(), %error, "could not queue utility window size");
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
            return Err(error);
        }

        let revision = workers
            .utility_size_latest_revision(key)
            .ok_or_else(|| "utility size write was queued without a revision".to_owned())?;
        let mut state = self.lock();
        state.known_keys.insert(key);
        state.local_resize_revisions.insert(key, revision);
        state.reset_revisions.remove(&key);
        state.waiting_for_ack.insert(key, revision);
        if let Some(id) = restore_after_ack
            && let Some(instance) = state.instances.get_mut(&id)
        {
            instance.restore_after_size_ack = true;
        }
        if let Some(ctx) = ctx {
            ctx.request_repaint_after(ACK_POLL_INTERVAL);
        }
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, UtilityDialogSizeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Advance one GUI frame, consume the registry's acknowledgements once,
    /// and publish only revisions that are still current into the app-owned
    /// acknowledged size cache.
    pub(crate) fn begin_frame(&self, ctx: &Context) -> Result<(), String> {
        let mut state = self.lock();
        state.frame = state.frame.wrapping_add(1).max(1);

        for acknowledgement in state.workers.drain_utility_size_acknowledgements() {
            state.publish_acknowledgement(acknowledgement)?;
        }

        let waiting = std::mem::take(&mut state.waiting_for_ack);
        for (key, revision) in waiting {
            let latest = state.workers.utility_size_latest_revision(key);
            let revision = latest.unwrap_or(revision);
            if state.accepted_revisions.get(&key) == Some(&revision) {
                continue;
            }
            if state.workers.utility_size_write_failure(key).is_some() {
                ctx.request_repaint();
                continue;
            }
            state.waiting_for_ack.insert(key, revision);
            ctx.request_repaint_after(ACK_POLL_INTERVAL);
        }
        Ok(())
    }

    /// Return each current persistence failure once. A newer genuine size
    /// mutation clears the old failure; a later failure is reported for its
    /// new revision.
    pub(crate) fn take_new_write_failures(&self) -> Vec<UtilityDialogSizeWriteFailure> {
        let keys = {
            let state = self.lock();
            state.known_keys.iter().copied().collect::<Vec<_>>()
        };
        let mut failures = Vec::new();
        for key in keys {
            let mut state = self.lock();
            match state.workers.utility_size_write_failure(key) {
                Some((revision, message)) => {
                    if state.reported_failures.get(&key) != Some(&revision) {
                        state.reported_failures.insert(key, revision);
                        failures.push(UtilityDialogSizeWriteFailure { key, message });
                    }
                }
                None => {
                    state.reported_failures.remove(&key);
                }
            }
        }
        failures.sort_by_key(|failure| failure.key);
        failures
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> UtilityDialogSizes {
        self.lock().sizes.clone()
    }

    #[cfg(test)]
    pub(crate) fn tracked_windows(&self) -> Vec<(Id, UtilityToolKey)> {
        self.lock()
            .instances
            .iter()
            .map(|(id, instance)| (*id, instance.key))
            .collect()
    }

    fn preference(&self, key: UtilityToolKey) -> UtilitySizePreference {
        self.lock().sizes.preference(key)
    }

    fn status(&self, key: UtilityToolKey) -> UtilityDialogSizeStatus {
        let state = self.lock();
        let latest_revision = state.workers.utility_size_latest_revision(key);
        let failure = state.workers.utility_size_write_failure(key);
        let pending_revision = if failure.is_none() {
            state.waiting_for_ack.get(&key).copied().filter(|revision| {
                Some(*revision) == latest_revision
                    && state.accepted_revisions.get(&key) != Some(revision)
            })
        } else {
            None
        };
        UtilityDialogSizeStatus {
            available: true,
            preference: state.sizes.preference(key),
            acknowledged_revision: state.accepted_revisions.get(&key).copied(),
            pending_revision,
            failure,
        }
    }

    fn restore_native_viewport(
        &self,
        key: UtilityToolKey,
        viewport_info: &egui::ViewportInfo,
        factory_inner_size: Vec2,
        minimum_inner_size: Option<Vec2>,
        maximum_inner_size: Option<Vec2>,
    ) -> Option<Vec2> {
        let mut state = self.lock();
        let preference = state.sizes.preference(key);
        let accepted_revision = state.accepted_revisions.get(&key).copied();
        let instance_id = native_instance_id(key);
        let instance = state
            .native_instances
            .entry(instance_id)
            .or_insert_with(|| NativeWindowInstance::new(key));
        if instance.key != key {
            *instance = NativeWindowInstance::new(key);
        }
        if matches!(preference, UtilitySizePreference::Unsupported) {
            instance.initial_restore_applied = true;
            return None;
        }
        let reset_revision = matches!(preference, UtilitySizePreference::Reset)
            .then_some(accepted_revision)
            .flatten();
        let reset_acknowledged =
            reset_revision.is_some() && reset_revision != instance.last_reset_revision;
        if instance.initial_restore_applied && !reset_acknowledged {
            return None;
        }

        let applied = native_restore_inner_size(
            viewport_info,
            preference,
            factory_inner_size,
            minimum_inner_size,
            maximum_inner_size,
        )?;
        instance.initial_restore_applied = true;
        if matches!(preference, UtilitySizePreference::Reset) {
            instance.last_reset_revision = reset_revision;
        }
        Some(applied)
    }

    fn close_native_viewport(&self, key: UtilityToolKey) {
        let mut state = self.lock();
        if let Some(instance) = state.native_instances.get_mut(&native_instance_id(key))
            && instance.key == key
        {
            instance.initial_restore_applied = false;
        }
    }

    /// Retry a retained failure only from an explicit controlled event.
    pub(crate) fn retry_failed(&self, ctx: &Context) -> Result<(), String> {
        let (workers, path, known_keys) = {
            let state = self.lock();
            (
                state.workers.clone(),
                state.settings_path.clone(),
                state.known_keys.iter().copied().collect::<Vec<_>>(),
            )
        };
        let failed = known_keys
            .into_iter()
            .filter_map(|key| {
                workers
                    .utility_size_write_failure(key)
                    .map(|(revision, _)| (key, revision))
            })
            .collect::<Vec<_>>();
        workers.retry_utility_size_mutations(path)?;

        let mut state = self.lock();
        for (key, failed_revision) in failed {
            let latest_revision = state.workers.utility_size_latest_revision(key);
            if latest_revision == Some(failed_revision)
                && state.accepted_revisions.get(&key) != Some(&failed_revision)
            {
                state.waiting_for_ack.insert(key, failed_revision);
            }
        }
        drop(state);
        ctx.request_repaint();
        ctx.request_repaint_after(ACK_POLL_INTERVAL);
        Ok(())
    }

    /// Admit a final write for any real grip release that was observed on the
    /// last visible frame but did not get its normal settling frame before app
    /// shutdown. The registry is still accepting work at this point.
    pub(crate) fn flush_released_resizes_for_shutdown(&self) -> Vec<(UtilityToolKey, String)> {
        let candidates = {
            let state = self.lock();
            state
                .instances
                .iter()
                .filter_map(|(id, instance)| {
                    completed_resize_size(instance).map(|size| (*id, instance.key, size))
                })
                .collect::<Vec<_>>()
        };

        let mut failures = Vec::new();
        for (id, key, size) in candidates {
            match self.submit_resize(key, size, None, None) {
                Ok(()) => {
                    if let Some(instance) = self.lock().instances.get_mut(&id) {
                        instance.resize_start = None;
                        instance.resize_release = None;
                    }
                }
                Err(error) => failures.push((key, error)),
            }
        }
        failures
    }

    fn prepare_window(
        &self,
        ctx: &Context,
        key: UtilityToolKey,
        instance_id: Id,
        open: bool,
        recipe: UtilityWindowSizeRecipe,
    ) -> Option<WindowPlan> {
        let viewport = ctx.content_rect().size();
        let existing_area_size = ctx
            .memory(|memory| memory.area_rect(instance_id))
            .map(|rect| rect.size());
        let available = Vec2::new(
            (viewport.x - 2.0 * VIEWPORT_MARGIN).max(1.0),
            (viewport.y - 2.0 * VIEWPORT_MARGIN).max(1.0),
        );
        let mut state = self.lock();
        let frame = state.frame;
        state.known_keys.insert(key);
        let mut completed_release = None;
        let mut previous_size_after_kind_change = None;

        if !open {
            let instance = state
                .instances
                .entry(instance_id)
                .or_insert_with(|| WindowInstance::new(key));
            if instance.key != key {
                completed_release =
                    completed_resize_size(instance).map(|size| (instance.key, size));
                *instance = WindowInstance::new(key);
            } else {
                completed_release = completed_resize_size(instance).map(|size| (key, size));
            }
            instance.is_open = false;
            instance.last_seen_frame = Some(frame);
            instance.restore_pending = true;
            instance.resize_start = None;
            instance.resize_release = None;
            drop(state);
            if let Some((release_key, size)) = completed_release {
                let _ = self.submit_resize(release_key, size, Some(ctx), None);
            }
            return None;
        }

        let preference = state.sizes.preference(key);
        let revision = state.accepted_revisions.get(&key).copied();
        let pending_local_resize = state.local_resize_revisions.contains_key(&key);
        let instance = state
            .instances
            .entry(instance_id)
            .or_insert_with(|| WindowInstance::new(key));
        if instance.key != key {
            completed_release = completed_resize_size(instance).map(|size| (instance.key, size));
            previous_size_after_kind_change = instance
                .last_outer_rect
                .map(|rect| rect.size())
                .or(existing_area_size);
            *instance = WindowInstance::new(key);
        }

        let omitted = instance
            .last_seen_frame
            .is_some_and(|last| last.saturating_add(1) < frame);
        if omitted {
            if completed_release.is_none() {
                completed_release = completed_resize_size(instance).map(|size| (key, size));
            }
            instance.resize_start = None;
            instance.resize_release = None;
        }
        if !instance.is_open
            || omitted
            || !instance.has_applied_restore
            || instance.last_preference != Some(preference)
            || instance.last_revision != revision
        {
            instance.restore_pending = true;
        }
        let restore_pending = instance.restore_pending;
        let previous_size = instance
            .last_outer_rect
            .map(|rect| rect.size())
            .or(previous_size_after_kind_change)
            .or(existing_area_size);
        instance.is_open = true;
        instance.last_seen_frame = Some(frame);

        let maximum = Vec2::new(
            recipe.maximum_outer[0].unwrap_or(available.x),
            recipe.maximum_outer[1].unwrap_or(available.y),
        )
        .min(available)
        .max(Vec2::ZERO);
        // `Window::new` starts with egui's own 96x32 minimum. Most callers in
        // the inventory have no explicit minimum, so their recipe uses ZERO
        // as the "keep the builder default" marker. A size restore must keep
        // that original minimum instead of replacing it with ZERO.
        let configured_minimum = if recipe.minimum_outer.x == 0.0 && recipe.minimum_outer.y == 0.0 {
            Vec2::new(96.0, 32.0)
        } else {
            recipe.minimum_outer
        };
        let minimum = configured_minimum.min(maximum).max(Vec2::ZERO);
        let preferred = match preference {
            UtilitySizePreference::Preferred(size) => [Some(size.width), Some(size.height)],
            UtilitySizePreference::Missing
            | UtilitySizePreference::Reset
            | UtilitySizePreference::Unsupported => recipe.default.dimensions(),
        };
        let target_outer = [
            preferred[0].map(|value| value.min(maximum.x).max(minimum.x)),
            preferred[1].map(|value| value.min(maximum.y).max(minimum.y)),
        ];
        let completed_local_release =
            completed_release.is_some_and(|(released_key, _)| released_key == key);
        let active_resize = instance.resize_start.is_some()
            || instance.resize_release.is_some()
            || instance.locally_resized;
        // Intrinsic and one-axis factory defaults keep their absent axis
        // content-sized during idle frames. Only an explicit two-axis size or
        // actual user resize state opts into filling the offered body.
        let fill_body = preferred.iter().all(Option::is_some)
            || pending_local_resize
            || completed_local_release
            || active_resize;
        let plan = WindowPlan {
            key,
            instance_id,
            frame,
            preference,
            revision,
            restore_pending,
            previous_size,
            target_outer,
            available,
            minimum,
            maximum,
            fill_body,
        };
        drop(state);
        if let Some((release_key, size)) = completed_release {
            let _ = self.submit_resize(release_key, size, Some(ctx), None);
        }
        Some(plan)
    }

    fn observe_window(
        &self,
        plan: WindowPlan,
        open: bool,
        visible_body: bool,
        rect: Option<Rect>,
        body_rect: Option<Rect>,
        body_clip: Option<Rect>,
        grip: ResizeGripState,
        ctx: &Context,
    ) {
        let mut mutation = None;
        let mut completed_release = None;
        {
            let mut state = self.lock();
            state.known_keys.insert(plan.key);
            if let Some(instance) = state.instances.get_mut(&plan.instance_id) {
                if !open {
                    if grip.stopped
                        && let Some(starting_size) = instance.resize_start
                    {
                        instance.resize_release = Some(ResizeRelease {
                            after_frame: plan.frame,
                            starting_size,
                        });
                    }
                    completed_release = completed_resize_size(instance);
                    instance.is_open = false;
                    instance.restore_pending = true;
                    instance.resize_start = None;
                    instance.resize_release = None;
                } else if visible_body {
                    if plan.restore_pending {
                        instance.restore_pending = false;
                        instance.has_applied_restore = true;
                        instance.last_preference = Some(plan.preference);
                        instance.last_revision = plan.revision;
                    }

                    if let Some(rect) = rect {
                        let previous_size = instance
                            .last_outer_rect
                            .map_or(rect.size(), |previous| previous.size());
                        instance.last_outer_rect = Some(rect);
                        instance.last_body_rect = body_rect;
                        instance.last_body_clip = body_clip;

                        if grip.dragging {
                            instance.resize_start.get_or_insert(previous_size);
                            instance.resize_release = None;
                        } else if grip.stopped && instance.resize_start.is_some() {
                            instance.resize_release = Some(ResizeRelease {
                                after_frame: plan.frame.saturating_add(1),
                                starting_size: instance.resize_start.unwrap_or(rect.size()),
                            });
                        }

                        if let Some(release) = instance.resize_release
                            && plan.frame >= release.after_frame
                            && !grip.dragging
                            && !grip.stopped
                        {
                            let changed = (rect.width() - release.starting_size.x).abs() > 1.0
                                || (rect.height() - release.starting_size.y).abs() > 1.0;
                            if changed {
                                mutation =
                                    PreferredWindowSize::new(rect.width(), rect.height()).ok();
                                instance.locally_resized = mutation.is_some();
                            }
                            instance.resize_start = None;
                            instance.resize_release = None;
                        }
                    }
                }
            }
        }

        if let Some(size) = completed_release {
            let _ = self.submit_resize(plan.key, size, Some(ctx), None);
        }
        if let Some(size) = mutation {
            let _ = self.submit_resize(plan.key, size, Some(ctx), None);
        }
    }

    pub(crate) fn reset(&self, key: UtilityToolKey) -> Result<UtilitySizeRevision, String> {
        let (workers, path) = {
            let mut state = self.lock();
            state.known_keys.insert(key);
            (state.workers.clone(), state.settings_path.clone())
        };
        workers.queue_utility_size_mutation(path, key, UtilitySizeMutation::Reset)?;
        let revision = workers
            .utility_size_latest_revision(key)
            .ok_or_else(|| "utility size reset was queued without a revision".to_owned())?;
        let mut state = self.lock();
        state.reset_revisions.insert(key, revision);
        state.local_resize_revisions.remove(&key);
        state.waiting_for_ack.insert(key, revision);
        Ok(revision)
    }
}

struct WindowPlan {
    key: UtilityToolKey,
    instance_id: Id,
    frame: u64,
    preference: UtilitySizePreference,
    revision: Option<UtilitySizeRevision>,
    restore_pending: bool,
    previous_size: Option<Vec2>,
    target_outer: [Option<f32>; 2],
    available: Vec2,
    minimum: Vec2,
    maximum: Vec2,
    fill_body: bool,
}

/// The caller's normal first-open size. Missing axes preserve egui's
/// content-sized default.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UtilityWindowSizeDefault {
    width: Option<f32>,
    height: Option<f32>,
}

impl UtilityWindowSizeDefault {
    pub(crate) const fn intrinsic() -> Self {
        Self {
            width: None,
            height: None,
        }
    }

    pub(crate) const fn outer(size: Vec2) -> Self {
        Self {
            width: Some(size.x),
            height: Some(size.y),
        }
    }

    pub(crate) const fn width(width: f32) -> Self {
        Self {
            width: Some(width),
            height: None,
        }
    }

    pub(crate) const fn height(height: f32) -> Self {
        Self {
            width: None,
            height: Some(height),
        }
    }

    fn dimensions(self) -> [Option<f32>; 2] {
        [self.width, self.height]
    }
}

/// Bounds that belong to the original Window builder.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UtilityWindowSizeRecipe {
    default: UtilityWindowSizeDefault,
    minimum_outer: Vec2,
    maximum_outer: [Option<f32>; 2],
}

impl UtilityWindowSizeRecipe {
    pub(crate) const fn new(
        default: UtilityWindowSizeDefault,
        minimum_outer: Vec2,
        maximum_outer: [Option<f32>; 2],
    ) -> Self {
        Self {
            default,
            minimum_outer,
            maximum_outer,
        }
    }

    pub(crate) const fn intrinsic() -> Self {
        Self::new(
            UtilityWindowSizeDefault::intrinsic(),
            Vec2::ZERO,
            [None, None],
        )
    }

    pub(crate) const fn outer(default: Vec2) -> Self {
        Self::new(
            UtilityWindowSizeDefault::outer(default),
            Vec2::ZERO,
            [None, None],
        )
    }

    pub(crate) const fn width(default: f32) -> Self {
        Self::new(
            UtilityWindowSizeDefault::width(default),
            Vec2::ZERO,
            [None, None],
        )
    }

    pub(crate) const fn with_minimum(mut self, minimum: Vec2) -> Self {
        self.minimum_outer = minimum;
        self
    }

    pub(crate) fn with_min_width(mut self, minimum: f32) -> Self {
        if self.minimum_outer == Vec2::ZERO {
            self.minimum_outer = Vec2::new(96.0, 32.0);
        }
        self.minimum_outer.x = minimum;
        self
    }

    pub(crate) fn with_min_height(mut self, minimum: f32) -> Self {
        if self.minimum_outer == Vec2::ZERO {
            self.minimum_outer = Vec2::new(96.0, 32.0);
        }
        self.minimum_outer.y = minimum;
        self
    }

    pub(crate) const fn with_maximum(mut self, maximum: Vec2) -> Self {
        self.maximum_outer = [Some(maximum.x), Some(maximum.y)];
        self
    }

    pub(crate) const fn with_max_width(mut self, maximum: f32) -> Self {
        self.maximum_outer[0] = Some(maximum);
        self
    }

    pub(crate) const fn with_max_height(mut self, maximum: f32) -> Self {
        self.maximum_outer[1] = Some(maximum);
        self
    }
}

pub(crate) trait UtilityDialogSizeWindowExt {
    fn show_utility<R>(
        self,
        ctx: &Context,
        stable_key: &'static str,
        instance_id: Id,
        open: Option<&mut bool>,
        recipe: UtilityWindowSizeRecipe,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>>;
}

impl UtilityDialogSizeWindowExt for Window<'_> {
    fn show_utility<R>(
        self,
        ctx: &Context,
        stable_key: &'static str,
        instance_id: Id,
        open: Option<&mut bool>,
        recipe: UtilityWindowSizeRecipe,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        show_utility_window(ctx, stable_key, instance_id, open, recipe, self, contents)
    }
}

/// Render an ordinary embedded utility window through the app's one shared
/// capability. The helper falls back to the caller's original default recipe
/// when used by headless or standalone contexts.
pub(crate) fn show_utility_window<R>(
    ctx: &Context,
    stable_key: &'static str,
    instance_id: Id,
    open: Option<&mut bool>,
    recipe: UtilityWindowSizeRecipe,
    window: Window<'_>,
    contents: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<Option<R>>> {
    let Ok(key) = UtilityToolKey::parse(stable_key) else {
        tracing::error!(tool = stable_key, "unknown utility window size key");
        return show_unmanaged_window(ctx, open, recipe.default, window, contents);
    };
    UtilityDialogSizeManager::for_context(ctx).show_window_with_recipe(
        ctx,
        key,
        instance_id,
        open,
        recipe,
        window,
        contents,
    )
}

fn show_unmanaged_window<R>(
    ctx: &Context,
    open: Option<&mut bool>,
    default: UtilityWindowSizeDefault,
    window: Window<'_>,
    contents: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<Option<R>>> {
    let window = match (default.width, default.height) {
        (Some(width), Some(height)) => window.default_size(Vec2::new(width, height)),
        (Some(width), None) => window.default_width(width),
        (None, Some(height)) => window.default_height(height),
        (None, None) => window,
    };
    match open {
        Some(open) if *open => window.open(open).show(ctx, contents),
        Some(_) => None,
        None => window.show(ctx, contents),
    }
}

/// A frame-local handle fetched from egui Context data by every eligible
/// window. It owns a clone of the same capability or falls back to plain egui.
pub(crate) struct UtilityDialogSizeAccess(Option<UtilityDialogSizeManager>);

impl UtilityDialogSizeAccess {
    pub(crate) fn is_available(&self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn preference(&self, key: UtilityToolKey) -> UtilitySizePreference {
        self.0
            .as_ref()
            .map_or(UtilitySizePreference::Missing, |capability| {
                capability.preference(key)
            })
    }

    pub(crate) fn status(&self, key: UtilityToolKey) -> UtilityDialogSizeStatus {
        self.0
            .as_ref()
            .map_or_else(UtilityDialogSizeStatus::default, |manager| {
                manager.status(key)
            })
    }

    pub(crate) fn reset(&self, key: UtilityToolKey) -> Result<UtilitySizeRevision, String> {
        self.0
            .as_ref()
            .ok_or_else(|| "utility size settings are unavailable in this context".to_owned())?
            .reset(key)
    }

    pub(crate) fn retry_failed(&self, ctx: &Context) -> Result<(), String> {
        self.0
            .as_ref()
            .ok_or_else(|| "utility size settings are unavailable in this context".to_owned())?
            .retry_failed(ctx)
    }

    fn restore_native_viewport(
        &self,
        key: UtilityToolKey,
        viewport_info: &egui::ViewportInfo,
        factory_inner_size: Vec2,
        minimum_inner_size: Option<Vec2>,
        maximum_inner_size: Option<Vec2>,
    ) -> Option<Vec2> {
        self.0.as_ref()?.restore_native_viewport(
            key,
            viewport_info,
            factory_inner_size,
            minimum_inner_size,
            maximum_inner_size,
        )
    }

    fn close_native_viewport(&self, key: UtilityToolKey) {
        if let Some(manager) = &self.0 {
            manager.close_native_viewport(key);
        }
    }

    pub(crate) fn show_window<R>(
        &self,
        ctx: &Context,
        key: UtilityToolKey,
        id: Id,
        open: &mut bool,
        default_outer_size: Vec2,
        minimum_outer_size: Vec2,
        window: Window<'_>,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        self.show_window_with_recipe(
            ctx,
            key,
            id,
            Some(open),
            UtilityWindowSizeRecipe::new(
                UtilityWindowSizeDefault::outer(default_outer_size),
                minimum_outer_size,
                [None, None],
            ),
            window,
            contents,
        )
    }

    pub(crate) fn show_window_with_recipe<R>(
        &self,
        ctx: &Context,
        key: UtilityToolKey,
        instance_id: Id,
        open: Option<&mut bool>,
        recipe: UtilityWindowSizeRecipe,
        window: Window<'_>,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        let is_open = open.as_deref().copied().unwrap_or(true);
        let Some(capability) = self.0.as_ref() else {
            return show_unmanaged_window(ctx, open, recipe.default, window, contents);
        };
        let mut open = open;
        let plan = capability.prepare_window(ctx, key, instance_id, is_open, recipe)?;
        let mut window = window;
        if plan.restore_pending && plan.target_outer.iter().any(Option::is_some) {
            if matches!(plan.preference, UtilitySizePreference::Preferred(_)) {
                window = window.default_size(Vec2::new(
                    plan.target_outer[0].unwrap_or_default(),
                    plan.target_outer[1].unwrap_or_default(),
                ));
            } else {
                if let Some(width) = plan.target_outer[0] {
                    window = window.default_width(width);
                }
                if let Some(height) = plan.target_outer[1] {
                    window = window.default_height(height);
                }
            }
            let (minimum, maximum) = restoration_bounds(
                plan.target_outer,
                plan.available,
                plan.previous_size,
                plan.minimum,
                plan.maximum,
            );
            window = window.min_size(minimum).max_size(maximum);
        }

        let mut body_rect = None;
        let mut body_clip = None;
        let mut accepted_body = false;
        let fill_body = plan.fill_body;
        let response = match open.as_deref_mut() {
            Some(open) => window.open(open).show(ctx, |ui| {
                let accepted = ui.is_visible() && ui.is_enabled() && !ui.is_sizing_pass();
                accepted_body = accepted;

                let result = contents(ui);
                if accepted && fill_body && !ctx.will_discard() {
                    ui.take_available_space();
                    body_rect = Some(ui.min_rect());
                    body_clip = Some(ui.clip_rect());
                }
                result
            }),
            None => window.show(ctx, |ui| {
                let accepted = ui.is_visible() && ui.is_enabled() && !ui.is_sizing_pass();
                accepted_body = accepted;

                let result = contents(ui);
                if accepted && fill_body && !ctx.will_discard() {
                    ui.take_available_space();
                    body_rect = Some(ui.min_rect());
                    body_clip = Some(ui.clip_rect());
                }
                result
            }),
        };

        let is_still_open = open.as_deref().copied().unwrap_or(true);
        let visible_body = accepted_body
            && response
                .as_ref()
                .is_some_and(|response| response.inner.is_some())
            && is_still_open
            && !ctx.will_discard();
        let rect = response.as_ref().map(|response| response.response.rect);
        let window_layer = response.as_ref().map_or(
            egui::LayerId::new(egui::Order::Middle, instance_id),
            |response| response.response.layer_id,
        );
        let grip = resize_grip_state(ctx, window_layer);
        capability.observe_window(
            plan,
            is_still_open,
            visible_body,
            rect,
            body_rect,
            body_clip,
            grip,
            ctx,
        );
        response
    }
}

/// Apply one child-viewport restore command from the same acknowledged utility
/// size capability used by embedded Windows. Native observed geometry never
/// becomes a preference; this adapter only restores on open/reset.
pub(crate) fn apply_native_utility_size_restore(
    child: &Ui,
    class: egui::ViewportClass,
    key: UtilityToolKey,
    factory_inner_size: Vec2,
    minimum_inner_size: Option<Vec2>,
    maximum_inner_size: Option<Vec2>,
) {
    if class != egui::ViewportClass::Immediate {
        return;
    }
    let ctx = child.ctx();
    let viewport_info = child.input(|input| input.viewport().clone());
    let access = UtilityDialogSizeManager::for_context(ctx);
    if let Some(size) = access.restore_native_viewport(
        key,
        &viewport_info,
        factory_inner_size,
        minimum_inner_size,
        maximum_inner_size,
    ) {
        child.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
    }
}

pub(crate) fn close_native_utility_size_window(ctx: &Context, key: UtilityToolKey) {
    UtilityDialogSizeManager::for_context(ctx).close_native_viewport(key);
}

fn native_instance_id(key: UtilityToolKey) -> Id {
    Id::new(("multi_launcher.native_utility_size", key))
}

fn native_restore_inner_size(
    viewport_info: &egui::ViewportInfo,
    preference: UtilitySizePreference,
    factory_inner_size: Vec2,
    minimum_inner_size: Option<Vec2>,
    maximum_inner_size: Option<Vec2>,
) -> Option<Vec2> {
    if matches!(preference, UtilitySizePreference::Unsupported) {
        return None;
    }
    let inner_rect = viewport_info.inner_rect?;
    let outer_rect = viewport_info.outer_rect?;
    let monitor_size = viewport_info.monitor_size?;
    let inner_size = inner_rect.size();
    let outer_size = outer_rect.size();
    if !finite_rect(inner_rect)
        || !finite_rect(outer_rect)
        || !finite_positive_size(inner_size)
        || !finite_positive_size(outer_size)
        || !finite_positive_size(monitor_size)
        || !finite_positive_size(factory_inner_size)
    {
        return None;
    }

    // ViewportInfo rectangles and monitor_size are already egui points. Do
    // not apply native DPI or zoom a second time.
    let chrome = (outer_size - inner_size).max(Vec2::ZERO);
    let monitor_inner = (monitor_size - chrome).max(Vec2::ZERO);
    if !finite_positive_size(monitor_inner) {
        return None;
    }
    let maximum = maximum_inner_size
        .unwrap_or(monitor_inner)
        .min(monitor_inner)
        .max(Vec2::ZERO);
    let requested_minimum = minimum_inner_size.unwrap_or(Vec2::ZERO);
    if !finite_size(requested_minimum) || !finite_size(maximum) {
        return None;
    }
    let minimum = requested_minimum.max(Vec2::ZERO).min(maximum);
    let requested_inner = match preference {
        UtilitySizePreference::Preferred(size) => {
            (Vec2::new(size.width, size.height) - chrome).max(Vec2::ZERO)
        }
        UtilitySizePreference::Missing | UtilitySizePreference::Reset => factory_inner_size,
        UtilitySizePreference::Unsupported => return None,
    };
    Some(requested_inner.min(maximum).max(minimum))
}

fn finite_size(size: Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite()
}

fn finite_positive_size(size: Vec2) -> bool {
    finite_size(size) && size.x > 0.0 && size.y > 0.0
}

fn finite_rect(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
}

impl UtilityDialogSizeState {
    fn publish_acknowledgement(
        &mut self,
        acknowledgement: UtilitySizeAcknowledgement,
    ) -> Result<(), String> {
        let key = acknowledgement.key;
        let revision = acknowledgement.revision;
        if self.workers.utility_size_latest_revision(key) != Some(revision) {
            return Ok(());
        }

        self.sizes
            .replace_committed(
                key,
                acknowledgement.preference,
                acknowledgement.native_geometry,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())?;

        self.accepted_revisions.insert(key, revision);
        self.waiting_for_ack.remove(&key);
        // A later native-only revision can be the first current acknowledgement
        // after an in-flight resize/reset committed. Carry that older accepted
        // intent forward before retiring its per-window bookkeeping.
        let locally_resized = self
            .local_resize_revisions
            .get(&key)
            .is_some_and(|resize_revision| *resize_revision <= revision);
        if locally_resized {
            self.local_resize_revisions.remove(&key);
        }
        let explicitly_reset = self
            .reset_revisions
            .get(&key)
            .is_some_and(|reset_revision| *reset_revision <= revision);
        if explicitly_reset {
            self.reset_revisions.remove(&key);
        }

        for instance in self
            .instances
            .values_mut()
            .filter(|instance| instance.key == key)
        {
            instance.locally_resized = false;
            if !instance.is_open || !instance.has_applied_restore || instance.restore_pending {
                continue;
            }
            if explicitly_reset
                || (acknowledgement.affects_size && !locally_resized)
                || instance.restore_after_size_ack
            {
                instance.restore_pending = true;
                instance.restore_after_size_ack = false;
            } else {
                // A grip resize or native-only update changes durable size
                // bookkeeping without moving existing docked instances.
                instance.last_preference = Some(acknowledgement.preference);
                instance.last_revision = Some(revision);
            }
        }
        Ok(())
    }
}
fn completed_resize_size(instance: &WindowInstance) -> Option<PreferredWindowSize> {
    let release = instance.resize_release?;
    let rect = instance.last_outer_rect?;
    let changed = (rect.width() - release.starting_size.x).abs() > 1.0
        || (rect.height() - release.starting_size.y).abs() > 1.0;
    changed
        .then(|| PreferredWindowSize::new(rect.width(), rect.height()).ok())
        .flatten()
}

fn restoration_bounds(
    target: [Option<f32>; 2],
    available: Vec2,
    previous: Option<Vec2>,
    caller_minimum: Vec2,
    caller_maximum: Vec2,
) -> (Vec2, Vec2) {
    let mut minimum = caller_minimum
        .min(caller_maximum)
        .min((available - Vec2::splat(0.5)).max(Vec2::ZERO));
    let mut maximum = caller_maximum.min(available);
    if let Some(previous) = previous {
        for axis in 0..2 {
            if let Some(target) = target[axis] {
                if target + 0.5 < previous[axis] {
                    maximum[axis] = target;
                    minimum[axis] = minimum[axis].min((target - 0.5).max(0.0));
                } else if target > previous[axis] + 0.5 {
                    minimum[axis] = (target - 0.5).max(0.0);
                }
            }
        }
    }
    (minimum, maximum)
}

#[derive(Clone, Copy, Debug, Default)]
struct ResizeGripState {
    dragging: bool,
    stopped: bool,
}

fn resize_grip_state(ctx: &Context, window_layer: egui::LayerId) -> ResizeGripState {
    let edge_drag = Id::new(window_layer).with("edge_drag");
    let edge_ids = [
        "left",
        "right",
        "top",
        "bottom",
        "left_top",
        "left_bottom",
        "right_top",
        "right_bottom",
    ]
    .map(|edge| edge_drag.with(edge));
    ResizeGripState {
        dragging: ctx
            .dragged_id()
            .is_some_and(|dragged| edge_ids.contains(&dragged)),
        stopped: ctx
            .drag_stopped_id()
            .is_some_and(|stopped| edge_ids.contains(&stopped)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use eframe::egui::{Event, PointerButton, Pos2, RawInput};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    const KEY: &str = "hash-generator";
    const TITLE: &str = "Utility size real window test";
    const DEFAULT_OUTER: Vec2 = egui::vec2(400.0, 300.0);

    fn key() -> UtilityToolKey {
        UtilityToolKey::parse(KEY).unwrap()
    }

    fn setup(
        size: Option<PreferredWindowSize>,
    ) -> (
        TempDir,
        String,
        Settings,
        Context,
        SettingsSaveWorkerRegistry,
        UtilityDialogSizeManager,
    ) {
        setup_with_preferences(&[(key(), size)])
    }

    fn setup_with_preferences(
        preferences: &[(UtilityToolKey, Option<PreferredWindowSize>)],
    ) -> (
        TempDir,
        String,
        Settings,
        Context,
        SettingsSaveWorkerRegistry,
        UtilityDialogSizeManager,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        for (key, size) in preferences {
            if let Some(size) = size {
                settings.utility_dialog_sizes.set(*key, *size).unwrap();
            }
        }
        settings.save(path.to_str().unwrap()).unwrap();
        let path = path.to_string_lossy().into_owned();
        let workers = SettingsSaveWorkerRegistry::default();
        let ctx = Context::default();
        let manager = UtilityDialogSizeManager::new(
            path.clone(),
            workers.clone(),
            settings.utility_dialog_sizes.clone(),
        );
        manager.install_in_context(&ctx);
        (directory, path, settings, ctx, workers, manager)
    }

    #[test]
    fn latest_geometry_ack_replaces_complete_cache_without_size_side_effects() {
        use crate::settings::utility_dialog_sizes::{
            NativeWindowGeometry, SavedWindowPosition, UtilitySizeMutation,
        };
        use std::sync::mpsc;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let path_text = path.to_string_lossy().into_owned();
        let key = key();
        let initial_size = PreferredWindowSize::new(700.0, 500.0).unwrap();
        let initial_geometry = NativeWindowGeometry::new(
            SavedWindowPosition::new(-900.0, -80.0).unwrap(),
            PreferredWindowSize::new(800.0, 600.0).unwrap(),
            1.0,
        )
        .unwrap();
        let first_size = PreferredWindowSize::new(720.0, 480.0).unwrap();
        let first_geometry = NativeWindowGeometry::new(
            SavedWindowPosition::new(-1280.0, -100.0).unwrap(),
            PreferredWindowSize::new(920.0, 700.0).unwrap(),
            1.5,
        )
        .unwrap();
        let final_size = PreferredWindowSize::new(840.0, 620.0).unwrap();
        let mut settings = Settings::default();
        settings
            .utility_dialog_sizes
            .set(key, initial_size)
            .unwrap();
        settings
            .utility_dialog_sizes
            .set_native_geometry(key, initial_geometry)
            .unwrap();
        settings.save(&path_text).unwrap();

        let (first_started_sender, first_started_receiver) = mpsc::channel();
        let (first_release_sender, first_release_receiver) = mpsc::channel();
        let (reset_started_sender, reset_started_receiver) = mpsc::channel();
        let (reset_release_sender, reset_release_receiver) = mpsc::channel();
        let (superseded_reset_started_sender, superseded_reset_started_receiver) = mpsc::channel();
        let (superseded_reset_release_sender, superseded_reset_release_receiver) = mpsc::channel();
        let first_release_receiver = Arc::new(Mutex::new(first_release_receiver));
        let reset_release_receiver = Arc::new(Mutex::new(reset_release_receiver));
        let superseded_reset_release_receiver =
            Arc::new(Mutex::new(superseded_reset_release_receiver));
        let before_write = {
            let first_release_receiver = Arc::clone(&first_release_receiver);
            let reset_release_receiver = Arc::clone(&reset_release_receiver);
            let superseded_reset_release_receiver = Arc::clone(&superseded_reset_release_receiver);
            Arc::new(move |_key: UtilityToolKey, revision: UtilitySizeRevision| {
                let (sender, receiver) = match revision.0 {
                    1 => (&first_started_sender, &first_release_receiver),
                    3 => (&reset_started_sender, &reset_release_receiver),
                    5 => (
                        &superseded_reset_started_sender,
                        &superseded_reset_release_receiver,
                    ),
                    _ => return,
                };
                let _ = sender.send(revision);
                let _ = receiver
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5));
            })
        };
        let workers = SettingsSaveWorkerRegistry::default();
        workers.set_utility_size_before_write_hook(before_write);
        let ctx = Context::default();
        let manager = UtilityDialogSizeManager::new(
            path_text.clone(),
            workers.clone(),
            settings.utility_dialog_sizes.clone(),
        );
        manager.install_in_context(&ctx);
        let instance_id = Id::new("native-geometry-cache-test");
        manager.lock().instances.insert(
            instance_id,
            WindowInstance {
                key,
                is_open: true,
                last_seen_frame: Some(0),
                last_preference: Some(UtilitySizePreference::Preferred(initial_size)),
                last_revision: None,
                has_applied_restore: true,
                restore_pending: false,
                last_outer_rect: None,
                last_body_rect: None,
                last_body_clip: None,
                resize_start: None,
                resize_release: None,
                locally_resized: false,
                restore_after_size_ack: false,
            },
        );
        let run_window_frame = || {
            let mut restore_pending = None;
            ctx.run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
                let frame_ctx = root.ctx().clone();
                manager.begin_frame(&frame_ctx).unwrap();
                let plan = manager
                    .prepare_window(
                        &frame_ctx,
                        key,
                        instance_id,
                        true,
                        UtilityWindowSizeRecipe::outer(DEFAULT_OUTER),
                    )
                    .unwrap();
                restore_pending = Some(plan.restore_pending);
                manager.observe_window(
                    plan,
                    true,
                    true,
                    Some(Rect::from_min_size(Pos2::ZERO, DEFAULT_OUTER)),
                    None,
                    None,
                    ResizeGripState::default(),
                    &frame_ctx,
                );
            })
            .drop_without_applying_deltas();
            restore_pending.unwrap()
        };

        workers
            .queue_utility_size_mutation(
                path_text.clone(),
                key,
                UtilitySizeMutation::Set(first_size),
            )
            .unwrap();
        assert_eq!(
            first_started_receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
            UtilitySizeRevision(1)
        );
        workers
            .queue_utility_size_mutation(
                path_text.clone(),
                key,
                UtilitySizeMutation::SetNative(first_geometry),
            )
            .unwrap();
        first_release_sender.send(()).unwrap();
        let first_ack_deadline = Instant::now() + Duration::from_secs(5);
        let first_ack_restore;
        loop {
            let restore_pending = run_window_frame();
            if manager.status(key).acknowledged_revision == Some(UtilitySizeRevision(2)) {
                first_ack_restore = restore_pending;
                break;
            }
            assert!(
                Instant::now() < first_ack_deadline,
                "timed out waiting for the size/native acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(4));
        }
        let sizes = manager.snapshot();
        assert_eq!(sizes.preferred(key), Some(first_size));
        assert_eq!(sizes.native_geometry(key), Some(first_geometry));
        assert_eq!(
            manager.status(key).acknowledged_revision,
            Some(UtilitySizeRevision(2))
        );
        {
            let state = manager.lock();
            let instance = state.instances.get(&instance_id).unwrap();
            assert!(!instance.restore_pending);
            assert_eq!(
                instance.last_preference,
                Some(UtilitySizePreference::Preferred(first_size)),
                "a native-only acknowledgement cannot change an open docked presentation"
            );
            assert_eq!(instance.last_revision, Some(UtilitySizeRevision(2)));
        }
        assert_eq!(
            first_ack_restore, false,
            "the next actual window frame must not schedule a size restore"
        );

        // A reset succeeds in flight, then the latest size-only update
        // commits without native geometry. Its acknowledgement must clear
        // the stale geometry retained in the previous in-memory snapshot.
        workers
            .queue_utility_size_mutation(path_text.clone(), key, UtilitySizeMutation::Reset)
            .unwrap();
        assert_eq!(
            reset_started_receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
            UtilitySizeRevision(3)
        );
        workers
            .queue_utility_size_mutation(
                path_text.clone(),
                key,
                UtilitySizeMutation::Set(final_size),
            )
            .unwrap();
        reset_release_sender.send(()).unwrap();
        let final_ack_deadline = Instant::now() + Duration::from_secs(5);
        let final_ack_restore;
        loop {
            let restore_pending = run_window_frame();
            if manager.status(key).acknowledged_revision == Some(UtilitySizeRevision(4)) {
                final_ack_restore = restore_pending;
                break;
            }
            assert!(
                Instant::now() < final_ack_deadline,
                "timed out waiting for the reset/size acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(4));
        }
        let sizes = manager.snapshot();
        assert_eq!(sizes.preferred(key), Some(final_size));
        assert_eq!(sizes.native_geometry(key), None);
        assert_eq!(
            manager.status(key).acknowledged_revision,
            Some(UtilitySizeRevision(4))
        );
        assert!(final_ack_restore);

        // Apply the size restore in an actual window frame, then supersede a
        // pending explicit reset with a native-only write. The accepted native
        // revision must still carry the reset's docked restore effect.
        assert!(
            !manager
                .lock()
                .instances
                .get(&instance_id)
                .unwrap()
                .restore_pending
        );

        assert_eq!(manager.reset(key).unwrap(), UtilitySizeRevision(5));
        assert_eq!(
            superseded_reset_started_receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap(),
            UtilitySizeRevision(5)
        );
        let latest_geometry = NativeWindowGeometry::new(
            SavedWindowPosition::new(-2560.0, -200.0).unwrap(),
            PreferredWindowSize::new(1000.0, 760.0).unwrap(),
            2.0,
        )
        .unwrap();
        workers
            .queue_utility_size_mutation(
                path_text,
                key,
                UtilitySizeMutation::SetNative(latest_geometry),
            )
            .unwrap();
        superseded_reset_release_sender.send(()).unwrap();
        let superseded_reset_deadline = Instant::now() + Duration::from_secs(5);
        let superseded_reset_restore;
        loop {
            let restore_pending = run_window_frame();
            if manager.status(key).acknowledged_revision == Some(UtilitySizeRevision(6)) {
                superseded_reset_restore = restore_pending;
                break;
            }
            assert!(
                Instant::now() < superseded_reset_deadline,
                "timed out waiting for the reset/native acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(4));
        }
        assert_eq!(
            manager.snapshot().preference(key),
            UtilitySizePreference::Reset
        );
        assert_eq!(
            manager.snapshot().native_geometry(key),
            Some(latest_geometry)
        );
        assert_eq!(
            superseded_reset_restore, true,
            "a native-only acknowledgement must not retire an older explicit reset"
        );
        workers.join_all();
    }

    fn render_recipe_window(
        ctx: &Context,
        manager: &UtilityDialogSizeManager,
        tool: UtilityToolKey,
        id: Id,
        title: &'static str,
        recipe: UtilityWindowSizeRecipe,
        open: &mut bool,
        screen: Vec2,
        events: Vec<Event>,
    ) -> Vec<Id> {
        let mut dragged_ids = Vec::new();
        let output = ctx.run_ui(raw_input(screen, events, 1.0), |root| {
            let ctx = root.ctx().clone();
            manager.begin_frame(&ctx).unwrap();
            let window = Window::new(title).id(id).resizable(true);
            let _ = UtilityDialogSizeManager::for_context(&ctx).show_window_with_recipe(
                &ctx,
                tool,
                id,
                Some(open),
                recipe,
                window,
                |ui| {
                    ui.label("Stable utility size test content");
                    ui.label("Second intrinsic content row");
                },
            );
            if let Some(id) = ctx.dragged_id() {
                dragged_ids.push(id);
            }
        });
        output.drop_without_applying_deltas();
        dragged_ids
    }

    fn wait_for_preference(
        ctx: &Context,
        manager: &UtilityDialogSizeManager,
        key: UtilityToolKey,
        preference: UtilitySizePreference,
        screen: Vec2,
    ) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if manager.snapshot().preference(key) == preference {
                return;
            }
            ctx.run_ui(raw_input(screen, vec![], 1.0), |root| {
                manager.begin_frame(root.ctx()).unwrap();
            })
            .drop_without_applying_deltas();
            assert!(
                Instant::now() < deadline,
                "timed out waiting for utility preference acknowledgement: {preference:?}"
            );
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    fn raw_input(screen: Vec2, events: Vec<Event>, pixels_per_point: f32) -> RawInput {
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport exists in default RawInput")
            .native_pixels_per_point = Some(pixels_per_point);
        input
    }

    fn native_viewport_info(
        inner_size: Vec2,
        chrome: Vec2,
        monitor_size: Vec2,
    ) -> egui::ViewportInfo {
        let inner_rect = Rect::from_min_size(Pos2::ZERO, inner_size);
        let outer_rect = Rect::from_min_size(Pos2::ZERO, inner_size + chrome);
        egui::ViewportInfo {
            inner_rect: Some(inner_rect),
            outer_rect: Some(outer_rect),
            monitor_size: Some(monitor_size),
            native_pixels_per_point: Some(2.0),
            ..Default::default()
        }
    }

    fn raw_input_with_native_viewport(info: egui::ViewportInfo) -> RawInput {
        let mut input = raw_input(egui::vec2(800.0, 600.0), Vec::new(), 1.0);
        *input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport exists in default RawInput") = info;
        input
    }

    fn render_window(
        ctx: &Context,
        manager: &UtilityDialogSizeManager,
        _settings: &mut Settings,
        id: Id,
        screen: Vec2,
        events: Vec<Event>,
        pixels_per_point: f32,
        default_open: bool,
        body_rows: usize,
        discard_first_pass: bool,
    ) -> Vec<Id> {
        let mut open = true;
        render_window_with_open(
            ctx,
            manager,
            _settings,
            id,
            screen,
            events,
            pixels_per_point,
            default_open,
            body_rows,
            discard_first_pass,
            &mut open,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_window_with_open(
        ctx: &Context,
        manager: &UtilityDialogSizeManager,
        _settings: &mut Settings,
        id: Id,
        screen: Vec2,
        events: Vec<Event>,
        pixels_per_point: f32,
        default_open: bool,
        body_rows: usize,
        discard_first_pass: bool,
        open: &mut bool,
    ) -> Vec<Id> {
        let mut dragged_ids = Vec::new();
        let output = ctx.run_ui(raw_input(screen, events, pixels_per_point), |root| {
            let ctx = root.ctx().clone();
            manager.begin_frame(&ctx).unwrap();
            if discard_first_pass && ctx.current_pass_index() == 0 {
                ctx.request_discard("exercise one-shot utility size restore");
            }
            let window = Window::new(TITLE)
                .id(id)
                .resizable(true)
                .default_open(default_open);
            let _ = UtilityDialogSizeManager::for_context(&ctx).show_window(
                &ctx,
                key(),
                id,
                open,
                DEFAULT_OUTER,
                Vec2::splat(64.0),
                window,
                |ui| {
                    for _ in 0..body_rows {
                        ui.label("Stable fixture body");
                    }
                },
            );
            if let Some(id) = ctx.dragged_id() {
                dragged_ids.push(id);
            }
        });
        output.drop_without_applying_deltas();
        dragged_ids
    }

    fn outer_rect(ctx: &Context, id: Id) -> Rect {
        ctx.memory(|memory| memory.area_rect(id))
            .expect("fixture Window should have an egui area rectangle")
    }

    fn body_and_clip(manager: &UtilityDialogSizeManager, id: Id) -> (Rect, Rect) {
        let state = manager.lock();
        let instance = state.instances.get(&id).unwrap();
        (
            instance.last_body_rect.unwrap(),
            instance.last_body_clip.unwrap(),
        )
    }

    fn settings_file(path: &str) -> Settings {
        Settings::load(path).unwrap()
    }

    fn pointer_press_and_drag(before: Rect, delta: Vec2) -> [Vec<Event>; 3] {
        let press = before.right_bottom() - egui::vec2(3.0, 3.0);
        let release = press + delta;
        [
            vec![
                Event::PointerMoved(press),
                Event::PointerButton {
                    pos: press,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![Event::PointerMoved(release)],
            vec![Event::PointerButton {
                pos: release,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ]
    }

    fn assert_actual_resize_grip_was_dragged(window_id: Id, observed_ids: &[Id]) {
        let edge_drag =
            Id::new(egui::LayerId::new(egui::Order::Middle, window_id)).with("edge_drag");
        let edge_ids = [
            "left",
            "right",
            "top",
            "bottom",
            "left_top",
            "left_bottom",
            "right_top",
            "right_bottom",
        ]
        .map(|edge| edge_drag.with(edge));
        assert!(
            observed_ids.iter().any(|id| edge_ids.contains(id)),
            "no real Window edge/corner id was dragged: {observed_ids:?}"
        );
    }

    fn assert_body_and_clip_within_frame(ctx: &Context, frame: Rect, body: Rect, clip: Rect) {
        assert!(
            frame.expand(1.0).contains_rect(body),
            "frame={frame:?}, body={body:?}"
        );
        assert!(
            frame.expand(1.0).contains_rect(clip),
            "frame={frame:?}, clip={clip:?}"
        );
        assert!(
            ctx.content_rect().contains_rect(clip),
            "viewport={:?}, clip={clip:?}",
            ctx.content_rect()
        );
    }

    #[test]
    fn real_window_resize_grip_records_candidate_after_one_shot_restore() {
        let (_directory, path, mut settings, ctx, workers, manager) =
            setup(Some(PreferredWindowSize::new(400.0, 300.0).unwrap()));
        let id = Id::new("utility-real-resize-grip");
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }

        let before = outer_rect(&ctx, id);
        assert!((before.width() - 400.0).abs() < 1.0, "{before:?}");
        assert!((before.height() - 300.0).abs() < 1.0, "{before:?}");
        let (body_before, body_clip) = body_and_clip(&manager, id);
        assert!(body_before.width() > 300.0 && body_before.height() > 200.0);
        assert!(body_clip.width() > 300.0 && body_clip.height() > 200.0);
        assert_body_and_clip_within_frame(&ctx, before, body_before, body_clip);

        let mut observed_grip_ids = Vec::new();
        for events in pointer_press_and_drag(before, egui::vec2(75.0, 55.0)) {
            observed_grip_ids.extend(render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                events,
                1.0,
                true,
                1,
                false,
            ));
        }
        assert_actual_resize_grip_was_dragged(id, &observed_grip_ids);
        for _ in 0..2 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        let after = outer_rect(&ctx, id);
        assert!(
            after.width() > before.width() + 30.0,
            "before={before:?}, after={after:?}"
        );
        assert!(
            after.height() > before.height() + 20.0,
            "before={before:?}, after={after:?}"
        );
        let (body_after, clip_after) = body_and_clip(&manager, id);
        assert!(body_after.width() > body_before.width() + 30.0);
        assert!(body_after.height() > body_before.height() + 20.0);
        assert!(clip_after.width() > body_clip.width() + 30.0);
        assert!(clip_after.height() > body_clip.height() + 20.0);
        assert_body_and_clip_within_frame(&ctx, after, body_after, clip_after);

        workers.join_all();
        // Drain the one context-owned acknowledgement before checking that the
        // current in-memory settings and durable file agree.
        ctx.run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
            manager.begin_frame(root.ctx()).unwrap();
        })
        .drop_without_applying_deltas();
        render_window(
            &ctx,
            &manager,
            &mut settings,
            id,
            egui::vec2(1000.0, 800.0),
            vec![],
            1.0,
            true,
            1,
            false,
        );
        let after_ack = outer_rect(&ctx, id);
        assert!((after_ack.width() - after.width()).abs() < 1.0);
        assert!((after_ack.height() - after.height()).abs() < 1.0);
        let persisted = settings_file(&path)
            .utility_dialog_sizes
            .preferred(key())
            .expect("grip drag should persist an outer-size preference");
        assert!((persisted.width - after.width()).abs() < 1.0);
        assert!((persisted.height - after.height()).abs() < 1.0);
        assert_eq!(manager.snapshot().preferred(key()), Some(persisted));
    }

    #[test]
    fn omitted_window_reopens_with_latest_size_from_another_instance() {
        let (_directory, path, mut settings, ctx, workers, manager) =
            setup(Some(PreferredWindowSize::new(400.0, 300.0).unwrap()));
        let first = Id::new("utility-size-first-instance");
        let second = Id::new("utility-size-second-instance");

        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                first,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        let first_initial = outer_rect(&ctx, first);
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                second,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        let second_before = outer_rect(&ctx, second);
        let (second_body_before, second_clip_before) = body_and_clip(&manager, second);
        assert_body_and_clip_within_frame(
            &ctx,
            second_before,
            second_body_before,
            second_clip_before,
        );
        let mut observed_grip_ids = Vec::new();
        for events in pointer_press_and_drag(second_before, egui::vec2(75.0, 55.0)) {
            observed_grip_ids.extend(render_window(
                &ctx,
                &manager,
                &mut settings,
                second,
                egui::vec2(1000.0, 800.0),
                events,
                1.0,
                true,
                1,
                false,
            ));
        }
        assert_actual_resize_grip_was_dragged(second, &observed_grip_ids);
        for _ in 0..2 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                second,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        let resized = outer_rect(&ctx, second);
        let (second_body_after, second_clip_after) = body_and_clip(&manager, second);
        assert!(second_body_after.width() > second_body_before.width() + 30.0);
        assert!(second_body_after.height() > second_body_before.height() + 20.0);
        assert_body_and_clip_within_frame(&ctx, resized, second_body_after, second_clip_after);

        workers.join_all();
        render_window(
            &ctx,
            &manager,
            &mut settings,
            second,
            egui::vec2(1000.0, 800.0),
            vec![],
            1.0,
            true,
            1,
            false,
        );
        render_window(
            &ctx,
            &manager,
            &mut settings,
            first,
            egui::vec2(1000.0, 800.0),
            vec![],
            1.0,
            true,
            1,
            false,
        );
        let reopened = outer_rect(&ctx, first);
        assert!(reopened.width() > first_initial.width() + 30.0);
        assert!(reopened.height() > first_initial.height() + 20.0);
        assert!((reopened.width() - resized.width()).abs() < 1.0);
        assert!((reopened.height() - resized.height()).abs() < 1.0);
        let persisted = settings_file(&path)
            .utility_dialog_sizes
            .preferred(key())
            .unwrap();
        assert!((persisted.width - resized.width()).abs() < 1.0);
        assert!((persisted.height - resized.height()).abs() < 1.0);
    }

    #[test]
    fn utility_dialog_sizes_shutdown_flushes_a_real_unsettled_grip_release() {
        let (_directory, path, mut settings, ctx, workers, manager) =
            setup(Some(PreferredWindowSize::new(400.0, 300.0).unwrap()));
        let id = Id::new("utility-size-shutdown-release");
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        let before = outer_rect(&ctx, id);
        let mut observed_grip_ids = Vec::new();
        for events in pointer_press_and_drag(before, egui::vec2(75.0, 55.0)) {
            observed_grip_ids.extend(render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                events,
                1.0,
                true,
                1,
                false,
            ));
        }
        assert_actual_resize_grip_was_dragged(id, &observed_grip_ids);
        let released = outer_rect(&ctx, id);
        assert!(released.width() > before.width() + 30.0, "{released:?}");
        assert!(released.height() > before.height() + 20.0, "{released:?}");
        assert!(manager.lock().instances[&id].resize_release.is_some());

        assert!(manager.flush_released_resizes_for_shutdown().is_empty());
        workers.begin_shutdown();
        workers.join_all();

        let persisted = settings_file(&path)
            .utility_dialog_sizes
            .preferred(key())
            .expect("shutdown flush persists the accepted grip result");
        assert!((persisted.width - released.width()).abs() < 1.0);
        assert!((persisted.height - released.height()).abs() < 1.0);
    }

    #[test]
    fn completed_resize_survives_immediate_close_and_omission() {
        let preferred = PreferredWindowSize::new(400.0, 300.0).unwrap();
        for omitted in [false, true] {
            let (_directory, path, mut settings, ctx, workers, manager) = setup(Some(preferred));
            let id = Id::new(if omitted {
                "utility-size-omitted-after-resize"
            } else {
                "utility-size-closed-after-resize"
            });
            for _ in 0..3 {
                render_window(
                    &ctx,
                    &manager,
                    &mut settings,
                    id,
                    egui::vec2(1000.0, 800.0),
                    vec![],
                    1.0,
                    true,
                    1,
                    false,
                );
            }
            let before = outer_rect(&ctx, id);
            let mut dragged_ids = Vec::new();
            for events in pointer_press_and_drag(before, egui::vec2(80.0, 60.0)) {
                dragged_ids.extend(render_window(
                    &ctx,
                    &manager,
                    &mut settings,
                    id,
                    egui::vec2(1000.0, 800.0),
                    events,
                    1.0,
                    true,
                    1,
                    false,
                ));
            }
            assert_actual_resize_grip_was_dragged(id, &dragged_ids);
            let resized = outer_rect(&ctx, id);
            assert!(resized.width() > before.width() + 30.0, "{resized:?}");
            assert!(resized.height() > before.height() + 20.0, "{resized:?}");
            assert!(manager.lock().instances[&id].resize_release.is_some());

            let mut open = true;
            if omitted {
                let output = ctx
                    .run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
                        manager.begin_frame(root.ctx()).unwrap()
                    });
                output.drop_without_applying_deltas();
                render_window_with_open(
                    &ctx,
                    &manager,
                    &mut settings,
                    id,
                    egui::vec2(1000.0, 800.0),
                    vec![],
                    1.0,
                    true,
                    1,
                    false,
                    &mut open,
                );
            } else {
                let close = Pos2::new(resized.right() - 12.0, resized.top() + 12.0);
                render_window_with_open(
                    &ctx,
                    &manager,
                    &mut settings,
                    id,
                    egui::vec2(1000.0, 800.0),
                    vec![
                        Event::PointerMoved(close),
                        Event::PointerButton {
                            pos: close,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        Event::PointerButton {
                            pos: close,
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    1.0,
                    true,
                    1,
                    false,
                    &mut open,
                );
                assert!(
                    !open,
                    "the same-pass pointer sequence must click the real Window X"
                );
            }

            workers.join_all();
            let output = ctx.run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
                manager.begin_frame(root.ctx()).unwrap()
            });
            output.drop_without_applying_deltas();
            let persisted = settings_file(&path)
                .utility_dialog_sizes
                .preferred(key())
                .expect("the completed grip resize was saved before the close/omission");
            assert!(
                (persisted.width - resized.width()).abs() < 1.0,
                "{persisted:?}"
            );
            assert!(
                (persisted.height - resized.height()).abs() < 1.0,
                "{persisted:?}"
            );

            open = true;
            for _ in 0..3 {
                render_window_with_open(
                    &ctx,
                    &manager,
                    &mut settings,
                    id,
                    egui::vec2(1000.0, 800.0),
                    vec![],
                    1.0,
                    true,
                    1,
                    false,
                    &mut open,
                );
            }
            let reopened = outer_rect(&ctx, id);
            assert!(
                (reopened.width() - persisted.width).abs() < 1.0,
                "{reopened:?}"
            );
            assert!(
                (reopened.height() - persisted.height).abs() < 1.0,
                "{reopened:?}"
            );
            assert_eq!(manager.snapshot().preferred(key()), Some(persisted));
        }
    }

    #[test]
    fn viewport_clamp_and_discarded_pass_preserve_the_durable_preference() {
        let preferred = PreferredWindowSize::new(1200.0, 900.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, manager) = setup(Some(preferred));
        let id = Id::new("utility-size-clamp-and-discard");
        render_window(
            &ctx,
            &manager,
            &mut settings,
            id,
            egui::vec2(640.0, 480.0),
            vec![],
            2.0,
            true,
            1,
            true,
        );
        let applied = outer_rect(&ctx, id);
        assert!(applied.width() <= 608.0 + 1.0, "{applied:?}");
        assert!(applied.height() <= 448.0 + 1.0, "{applied:?}");
        assert!(
            applied.width() > 500.0 && applied.height() > 350.0,
            "{applied:?}"
        );
        assert_eq!(manager.snapshot().preferred(key()), Some(preferred));
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preferred(key()),
            Some(preferred)
        );
        workers.join_all();
    }

    #[test]
    fn smaller_reset_reopens_over_stale_egui_size() {
        let large = PreferredWindowSize::new(720.0, 520.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, manager) = setup(Some(large));
        let id = Id::new("utility-size-smaller-reset");
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        assert!((outer_rect(&ctx, id).width() - 720.0).abs() < 1.0);
        assert!((outer_rect(&ctx, id).height() - 520.0).abs() < 1.0);

        let reset_revision = manager.reset(key()).unwrap();
        let reset_deadline = Instant::now() + Duration::from_secs(3);
        while manager.snapshot().preference(key()) != UtilitySizePreference::Reset {
            assert!(
                Instant::now() < reset_deadline,
                "timed out waiting for reset revision {reset_revision:?}"
            );
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
            std::thread::yield_now();
        }
        assert_eq!(
            workers.utility_size_latest_revision(key()),
            Some(reset_revision)
        );
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Reset
        );
        let mut reset_outer = outer_rect(&ctx, id);
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
            reset_outer = outer_rect(&ctx, id);
            assert!(
                (reset_outer.width() - DEFAULT_OUTER.x).abs() < 1.0,
                "{reset_outer:?}"
            );
            assert!(
                (reset_outer.height() - DEFAULT_OUTER.y).abs() < 1.0,
                "{reset_outer:?}"
            );
        }

        let mut observed_grip_ids = Vec::new();
        for events in pointer_press_and_drag(reset_outer, egui::vec2(75.0, 55.0)) {
            observed_grip_ids.extend(render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                events,
                1.0,
                true,
                1,
                false,
            ));
        }
        for _ in 0..2 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                1,
                false,
            );
        }
        assert_actual_resize_grip_was_dragged(id, &observed_grip_ids);
        let resized = outer_rect(&ctx, id);
        assert!(resized.width() > reset_outer.width() + 30.0, "{resized:?}");
        assert!(
            resized.height() > reset_outer.height() + 20.0,
            "{resized:?}"
        );
        workers.join_all();
        ctx.run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
            manager.begin_frame(root.ctx()).unwrap();
        })
        .drop_without_applying_deltas();
        let persisted_size = PreferredWindowSize::new(resized.width(), resized.height()).unwrap();
        assert_eq!(manager.snapshot().preferred(key()), Some(persisted_size));
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Preferred(persisted_size)
        );
    }

    #[test]
    fn title_drag_content_reflow_and_collapsed_window_do_not_save_size() {
        let preferred = PreferredWindowSize::new(400.0, 300.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, manager) = setup(Some(preferred));
        let id = Id::new("utility-size-non-grip-control");
        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                2,
                false,
            );
        }
        let before = outer_rect(&ctx, id);
        assert!((before.width() - 400.0).abs() < 1.0, "{before:?}");
        assert!((before.height() - 300.0).abs() < 1.0, "{before:?}");
        let start = before.left_top() + egui::vec2(100.0, 12.0);
        let end = start + egui::vec2(70.0, 35.0);
        for events in [
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![Event::PointerMoved(end)],
            vec![Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                events,
                1.0,
                true,
                2,
                false,
            );
        }
        let moved = outer_rect(&ctx, id);
        assert!(
            (moved.width() - before.width()).abs() < 1.0,
            "title drag changed window width: before={before:?}, moved={moved:?}"
        );
        assert!(
            (moved.height() - before.height()).abs() < 1.0,
            "title drag changed window height: before={before:?}, moved={moved:?}"
        );
        assert!(
            (moved.left() - before.left()).abs() > 10.0
                || (moved.top() - before.top()).abs() > 10.0
        );

        for _ in 0..3 {
            render_window(
                &ctx,
                &manager,
                &mut settings,
                id,
                egui::vec2(1000.0, 800.0),
                vec![],
                1.0,
                true,
                20,
                false,
            );
        }
        let reflowed = outer_rect(&ctx, id);
        assert!(reflowed.height() > moved.height() + 20.0);

        let collapsed_id = Id::new("utility-size-collapsed-control");
        render_window(
            &ctx,
            &manager,
            &mut settings,
            collapsed_id,
            egui::vec2(1000.0, 800.0),
            vec![],
            1.0,
            false,
            1,
            false,
        );
        assert!(manager.lock().instances[&collapsed_id].restore_pending);
        workers.join_all();
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preferred(key()),
            Some(preferred)
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        assert_eq!(workers.utility_size_latest_revision(key()), None);
    }

    #[test]
    fn utility_size_inventory_intrinsic_default_matches_plain_egui_window() {
        let tool = UtilityToolKey::parse("plugin-settings").unwrap();
        let (_directory, _path, _settings, ctx, workers, manager) =
            setup_with_preferences(&[(tool, None)]);
        let screen = egui::vec2(1000.0, 800.0);
        let id = Id::new("Intrinsic baseline production title");
        let mut open = true;
        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                "Intrinsic baseline production title",
                UtilityWindowSizeRecipe::intrinsic(),
                &mut open,
                screen,
                vec![],
            );
        }
        let managed = outer_rect(&ctx, id);

        let plain_ctx = Context::default();
        for _ in 0..3 {
            plain_ctx
                .run_ui(raw_input(screen, vec![], 1.0), |root| {
                    Window::new("Intrinsic baseline production title")
                        .id(id)
                        .resizable(true)
                        .show(root.ctx(), |ui| {
                            ui.label("Stable utility size test content");
                            ui.label("Second intrinsic content row");
                        });
                })
                .drop_without_applying_deltas();
        }
        let plain = outer_rect(&plain_ctx, id);
        assert!(
            (managed.width() - plain.width()).abs() < 1.0
                && (managed.height() - plain.height()).abs() < 1.0,
            "intrinsic managed={managed:?}, plain={plain:?}"
        );
        workers.join_all();
    }

    #[test]
    fn utility_size_inventory_same_id_kind_change_flushes_release_and_restores_new_kind() {
        let export_key = UtilityToolKey::parse("mkmacro-package-export").unwrap();
        let import_key = UtilityToolKey::parse("mkmacro-package-import").unwrap();
        let export_default = PreferredWindowSize::new(480.0, 360.0).unwrap();
        let import_default = PreferredWindowSize::new(300.0, 250.0).unwrap();
        let (_directory, path, _settings, ctx, workers, manager) = setup_with_preferences(&[
            (export_key, Some(export_default)),
            (import_key, Some(import_default)),
        ]);
        let screen = egui::vec2(1000.0, 800.0);
        let shared_id = Id::new("mkmacro_package_modal");
        let recipe = UtilityWindowSizeRecipe::width(520.0);
        let mut open = true;
        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                export_key,
                shared_id,
                "Shared Package Modal",
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let before = outer_rect(&ctx, shared_id);
        assert!((before.width() - 480.0).abs() < 1.0, "{before:?}");
        assert!((before.height() - 360.0).abs() < 1.0, "{before:?}");

        let mut dragged = Vec::new();
        for events in pointer_press_and_drag(before, egui::vec2(75.0, 55.0)) {
            dragged.extend(render_recipe_window(
                &ctx,
                &manager,
                export_key,
                shared_id,
                "Shared Package Modal",
                recipe,
                &mut open,
                screen,
                events,
            ));
        }
        assert_actual_resize_grip_was_dragged(shared_id, &dragged);
        let completed_export_size = outer_rect(&ctx, shared_id);
        assert!(completed_export_size.width() > before.width() + 30.0);
        assert!(completed_export_size.height() > before.height() + 20.0);

        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                import_key,
                shared_id,
                "Shared Package Modal",
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let import_rect = outer_rect(&ctx, shared_id);
        assert!(
            (import_rect.width() - import_default.width).abs() < 1.0
                && (import_rect.height() - import_default.height).abs() < 1.0,
            "new modal kind did not restore over the prior kind's larger desired size: {import_rect:?}"
        );
        let expected_export = PreferredWindowSize::new(
            completed_export_size.width(),
            completed_export_size.height(),
        )
        .unwrap();
        wait_for_preference(
            &ctx,
            &manager,
            export_key,
            UtilitySizePreference::Preferred(expected_export),
            screen,
        );
        workers.join_all();
        let persisted = settings_file(&path).utility_dialog_sizes;
        assert_eq!(persisted.preferred(export_key), Some(expected_export));
        assert_eq!(persisted.preferred(import_key), Some(import_default));
    }

    #[test]
    fn utility_size_inventory_real_resize_reset_and_width_default_reopen() {
        let tool = UtilityToolKey::parse("action-sheet").unwrap();
        let prior = PreferredWindowSize::new(560.0, 380.0).unwrap();
        let (_directory, path, _settings, ctx, workers, manager) =
            setup_with_preferences(&[(tool, Some(prior))]);
        let screen = egui::vec2(1000.0, 800.0);
        let title = "Action Sheet reset default fixture";
        let id = Id::new(title);
        let recipe = UtilityWindowSizeRecipe::width(440.0);
        let mut open = true;

        let plain_ctx = Context::default();
        for _ in 0..3 {
            plain_ctx
                .run_ui(raw_input(screen, vec![], 1.0), |root| {
                    Window::new(title)
                        .id(id)
                        .default_width(440.0)
                        .show(root.ctx(), |ui| {
                            ui.label("Stable utility size test content");
                            ui.label("Second intrinsic content row");
                        });
                })
                .drop_without_applying_deltas();
        }
        let normal_default = outer_rect(&plain_ctx, id);

        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                title,
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let before = outer_rect(&ctx, id);
        assert!((before.width() - prior.width).abs() < 1.0, "{before:?}");
        assert!((before.height() - prior.height).abs() < 1.0, "{before:?}");

        let mut dragged = Vec::new();
        for events in pointer_press_and_drag(before, egui::vec2(80.0, 60.0)) {
            dragged.extend(render_recipe_window(
                &ctx, &manager, tool, id, title, recipe, &mut open, screen, events,
            ));
        }
        assert_actual_resize_grip_was_dragged(id, &dragged);
        for _ in 0..2 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                title,
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let resized = outer_rect(&ctx, id);
        assert!(resized.width() > before.width() + 30.0);
        assert!(resized.height() > before.height() + 20.0);
        let expected_saved = PreferredWindowSize::new(resized.width(), resized.height()).unwrap();
        wait_for_preference(
            &ctx,
            &manager,
            tool,
            UtilitySizePreference::Preferred(expected_saved),
            screen,
        );

        manager.reset(tool).unwrap();
        wait_for_preference(&ctx, &manager, tool, UtilitySizePreference::Reset, screen);
        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                title,
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let reset_rect = outer_rect(&ctx, id);
        assert!(
            (reset_rect.width() - 440.0).abs() < 1.0,
            "one-axis default width was not restored: {reset_rect:?}"
        );
        assert!(
            (reset_rect.height() - normal_default.height()).abs() < 1.0,
            "intrinsic height changed on reset: reset={reset_rect:?}, default={normal_default:?}"
        );
        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                title,
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let idle_rect = outer_rect(&ctx, id);
        assert!((idle_rect.width() - reset_rect.width()).abs() < 1.0);
        assert!((idle_rect.height() - reset_rect.height()).abs() < 1.0);

        open = false;
        render_recipe_window(
            &ctx,
            &manager,
            tool,
            id,
            title,
            recipe,
            &mut open,
            screen,
            vec![],
        );
        open = true;
        for _ in 0..3 {
            render_recipe_window(
                &ctx,
                &manager,
                tool,
                id,
                title,
                recipe,
                &mut open,
                screen,
                vec![],
            );
        }
        let reopened = outer_rect(&ctx, id);
        assert!((reopened.width() - 440.0).abs() < 1.0, "{reopened:?}");
        assert!(
            (reopened.height() - normal_default.height()).abs() < 1.0,
            "reopened intrinsic height differs: reopened={reopened:?}, default={normal_default:?}"
        );
        workers.join_all();
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(tool),
            UtilitySizePreference::Reset
        );
    }

    #[test]
    fn utility_size_integration_native_restore_uses_outer_chrome_and_monitor_points_once() {
        let preferred = PreferredWindowSize::new(900.0, 700.0).unwrap();
        let (_directory, path, _settings, _ctx, workers, manager) = setup(Some(preferred));
        let viewport = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(1000.0, 800.0),
        );

        let applied = manager
            .restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            )
            .expect("finite child metadata permits a restore");
        assert_eq!(
            applied,
            egui::vec2(880.0, 660.0),
            "the saved value is an outer size; chrome is removed once and native DPI is not reapplied"
        );
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            None,
            "normal subsequent callbacks do not reapply the opening size"
        );

        let constrained = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(600.0, 500.0),
        );
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &constrained,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            None,
            "observed resize or monitor changes while open do not issue another restore"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        manager.close_native_viewport(key());
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                Some(egui::vec2(720.0, 720.0)),
            ),
            Some(egui::vec2(720.0, 660.0)),
            "the caller's native maximum clamps applied inner geometry only"
        );
        manager.close_native_viewport(key());
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &constrained,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            Some(egui::vec2(580.0, 460.0)),
            "child monitor bounds clamp only the applied inner size"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Preferred(preferred),
            "native observations and monitor changes never write a preference"
        );
        workers.join_all();
    }

    #[test]
    fn utility_size_integration_native_reset_ack_reapplies_once_and_reopen_uses_default() {
        let preferred = PreferredWindowSize::new(760.0, 540.0).unwrap();
        let (_directory, path, _settings, ctx, workers, manager) = setup(Some(preferred));
        let viewport = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(1200.0, 900.0),
        );
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            Some(egui::vec2(740.0, 500.0))
        );

        let revision = manager.reset(key()).unwrap();
        let pending = manager.status(key());
        assert_eq!(pending.pending_revision, Some(revision));
        wait_for_preference(
            &ctx,
            &manager,
            key(),
            UtilitySizePreference::Reset,
            egui::vec2(1000.0, 800.0),
        );
        let acknowledged = manager.status(key());
        assert_eq!(acknowledged.acknowledged_revision, Some(revision));
        assert_eq!(acknowledged.pending_revision, None);
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            Some(egui::vec2(620.0, 700.0)),
            "only the acknowledged reset revision restores the native factory size"
        );
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            None,
            "a reset revision is applied once while the native window stays open"
        );

        manager.close_native_viewport(key());
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &viewport,
                egui::vec2(620.0, 700.0),
                Some(egui::vec2(320.0, 420.0)),
                None,
            ),
            Some(egui::vec2(620.0, 700.0)),
            "reopening after reset restores the factory size once"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), Some(revision));
        workers.join_all();
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Reset
        );
    }

    #[test]
    fn utility_size_integration_native_restore_defers_missing_metadata_without_mutation() {
        let preferred = PreferredWindowSize::new(700.0, 500.0).unwrap();
        let (_directory, path, _settings, _ctx, workers, manager) = setup(Some(preferred));
        assert_eq!(
            manager.restore_native_viewport(
                key(),
                &egui::ViewportInfo::default(),
                egui::vec2(620.0, 440.0),
                None,
                None,
            ),
            None,
            "incomplete child metadata defers restoration"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        let complete = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(1000.0, 800.0),
        );
        assert_eq!(
            manager
                .restore_native_viewport(key(), &complete, egui::vec2(620.0, 440.0), None, None,),
            Some(egui::vec2(680.0, 460.0)),
            "a later ordinary child callback can apply the still-pending opening restore"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Preferred(preferred)
        );
        workers.join_all();
    }

    #[test]
    fn utility_size_integration_native_restore_leaves_unsupported_preferences_inactive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let opaque: UtilityDialogSizes =
            serde_json::from_str(r#"{"version":99,"entries":{}}"#).unwrap();
        let manager = UtilityDialogSizeManager::new(
            path.to_string_lossy().into_owned(),
            workers.clone(),
            opaque,
        );
        let viewport = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(1000.0, 800.0),
        );

        assert_eq!(
            manager.status(key()).preference,
            UtilitySizePreference::Unsupported
        );
        assert_eq!(
            manager
                .restore_native_viewport(key(), &viewport, egui::vec2(620.0, 440.0), None, None,),
            None,
            "an unsupported subtree does not trigger a factory or saved-size viewport command"
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        workers.join_all();
    }

    #[test]
    fn utility_size_integration_native_adapter_only_commands_immediate_viewports_once() {
        let preferred = PreferredWindowSize::new(700.0, 500.0).unwrap();
        let (_directory, _path, _settings, ctx, workers, _manager) = setup(Some(preferred));
        let viewport = native_viewport_info(
            egui::vec2(400.0, 300.0),
            egui::vec2(20.0, 40.0),
            egui::vec2(1000.0, 800.0),
        );
        let embedded = ctx.run_ui(raw_input_with_native_viewport(viewport.clone()), |root| {
            apply_native_utility_size_restore(
                root,
                egui::ViewportClass::EmbeddedWindow,
                key(),
                egui::vec2(620.0, 440.0),
                None,
                None,
            );
        });
        let embedded_commands = &embedded
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output is present")
            .commands;
        assert!(
            embedded_commands
                .iter()
                .all(|command| !matches!(command, egui::ViewportCommand::InnerSize(_))),
            "native restore adapter does not send child size commands for embedded viewports"
        );
        embedded.drop_without_applying_deltas();

        let immediate = ctx.run_ui(raw_input_with_native_viewport(viewport.clone()), |root| {
            apply_native_utility_size_restore(
                root,
                egui::ViewportClass::Immediate,
                key(),
                egui::vec2(620.0, 440.0),
                None,
                None,
            );
        });
        let immediate_commands = &immediate
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output is present")
            .commands;
        assert!(
            immediate_commands.iter().any(|command| {
                matches!(command, egui::ViewportCommand::InnerSize(size) if *size == egui::vec2(680.0, 460.0))
            }),
            "the Immediate adapter sends the converted outer preference as inner points"
        );
        immediate.drop_without_applying_deltas();

        let repeated = ctx.run_ui(raw_input_with_native_viewport(viewport), |root| {
            apply_native_utility_size_restore(
                root,
                egui::ViewportClass::Immediate,
                key(),
                egui::vec2(620.0, 440.0),
                None,
                None,
            );
        });
        assert!(
            repeated
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .expect("root viewport output is present")
                .commands
                .iter()
                .all(|command| !matches!(command, egui::ViewportCommand::InnerSize(_))),
            "normal native frames do not resend the opening restore"
        );
        repeated.drop_without_applying_deltas();
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        workers.join_all();
    }

    #[test]
    fn utility_size_integration_retry_is_explicit_and_keeps_failure_visible_until_ack() {
        let (directory, path, _settings, ctx, workers, manager) = setup(None);
        let repaint_delays = Arc::new(Mutex::new(Vec::new()));
        let captured_repaints = Arc::clone(&repaint_delays);
        ctx.set_request_repaint_callback(move |info| {
            captured_repaints
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(info.delay);
        });
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "keep this parent path blocked").unwrap();
        manager.rebind(
            blocker.join("settings.json").to_string_lossy().into_owned(),
            workers.clone(),
        );

        let revision = manager.reset(key()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let failure = loop {
            manager.begin_frame(&ctx).unwrap();
            if let Some(failure) = manager.status(key()).failure {
                break failure;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the injected utility-size write failure"
            );
            std::thread::sleep(Duration::from_millis(4));
        };
        assert_eq!(failure.0, revision);
        assert!(failure.1.contains("failed to persist"));
        assert_eq!(manager.status(key()).pending_revision, None);
        assert_eq!(
            manager.snapshot().preference(key()),
            UtilitySizePreference::Missing,
            "a failed write does not publish Reset as committed state"
        );
        assert_eq!(
            manager.take_new_write_failures().len(),
            1,
            "the app notification owner receives a new failure exactly once"
        );
        repaint_delays
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        for _ in 0..3 {
            manager.begin_frame(&ctx).unwrap();
            assert_eq!(manager.status(key()).pending_revision, None);
            assert_eq!(
                manager.status(key()).failure.as_ref().map(|(rev, _)| *rev),
                Some(revision)
            );
            assert!(manager.take_new_write_failures().is_empty());
        }
        assert!(
            repaint_delays
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty(),
            "a retained failure does not create a repaint/retry loop"
        );

        let mut repaint_state_drained = !ctx.has_requested_repaint();
        for _ in 0..8 {
            if repaint_state_drained {
                break;
            }
            let output = ctx.run_ui(
                raw_input(egui::vec2(800.0, 600.0), Vec::new(), 1.0),
                |root| manager.begin_frame(root.ctx()).unwrap(),
            );
            output.drop_without_applying_deltas();
            repaint_state_drained = !ctx.has_requested_repaint();
        }
        assert!(
            repaint_state_drained,
            "bounded real egui passes consume pending repaint requests before explicit retry; causes={:?}",
            ctx.repaint_causes()
        );
        repaint_delays
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        manager.retry_failed(&ctx).unwrap();
        assert!(
            repaint_delays
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&Duration::ZERO),
            "an explicit retry schedules an immediate UI update"
        );
        let retrying = manager.status(key());
        assert!(
            retrying.pending_revision == Some(revision) || retrying.failure.is_some(),
            "an explicit retry is either pending or already has its bounded failure result: {retrying:?}"
        );
        if let Some((failed_revision, _)) = retrying.failure {
            assert_eq!(failed_revision, revision);
            assert_eq!(retrying.pending_revision, None);
        }
        let retry_deadline = Instant::now() + Duration::from_secs(3);
        let retry_failure = loop {
            manager.begin_frame(&ctx).unwrap();
            if let Some(failure) = manager.status(key()).failure {
                break failure;
            }
            assert!(
                Instant::now() < retry_deadline,
                "timed out waiting for the explicit retry result"
            );
            std::thread::sleep(Duration::from_millis(4));
        };
        assert_eq!(retry_failure.0, revision);
        assert_eq!(manager.status(key()).pending_revision, None);
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Missing,
            "the failed retry also leaves persisted settings unchanged"
        );
        workers.join_all();
    }
}
