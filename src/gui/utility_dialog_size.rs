//! Presentation lifecycle for persisted ordinary utility-window sizes.
//!
//! Settings remains the durable owner. This module applies the accepted
//! preference when a window opens or resets, keeps the body sized to egui's
//! current offer while it is visible, and submits only a settled resize-grip
//! gesture to the existing settings-save worker.

use crate::settings::utility_dialog_sizes::UtilitySizeMutation;
use crate::settings::{
    PreferredWindowSize, Settings, UtilitySizeAcknowledgement, UtilitySizePreference,
    UtilitySizeRevision, UtilityToolKey,
};
use crate::settings_save::SettingsSaveWorkerRegistry;
use eframe::egui::{self, Context, Id, InnerResponse, Rect, Ui, Vec2, Window};
use std::collections::HashMap;
use std::time::Duration;

const VIEWPORT_MARGIN: f32 = 16.0;
const ACK_POLL_INTERVAL: Duration = Duration::from_millis(16);

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
        }
    }
}

/// One GUI-context owner for size acknowledgements and per-window presentation
/// lifecycle. Store one alongside the app's other GUI context state.
pub(crate) struct UtilityDialogSizeManager {
    settings_path: String,
    workers: SettingsSaveWorkerRegistry,
    frame: u64,
    instances: HashMap<Id, WindowInstance>,
    accepted_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    waiting_for_ack: HashMap<UtilityToolKey, UtilitySizeRevision>,
    local_resize_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
    reset_revisions: HashMap<UtilityToolKey, UtilitySizeRevision>,
}

impl UtilityDialogSizeManager {
    pub(crate) fn new(settings_path: String, workers: SettingsSaveWorkerRegistry) -> Self {
        Self {
            settings_path,
            workers,
            frame: 0,
            instances: HashMap::new(),
            accepted_revisions: HashMap::new(),
            waiting_for_ack: HashMap::new(),
            local_resize_revisions: HashMap::new(),
            reset_revisions: HashMap::new(),
        }
    }

    /// Advance one GUI frame, consume the registry's acknowledgements once,
    /// and publish only revisions that are still current into Settings.
    pub(crate) fn begin_frame(
        &mut self,
        ctx: &Context,
        settings: &mut Settings,
    ) -> Result<(), String> {
        self.frame = self.frame.wrapping_add(1).max(1);

        for acknowledgement in self.workers.drain_utility_size_acknowledgements() {
            self.publish_acknowledgement(settings, acknowledgement)?;
        }

        let waiting = std::mem::take(&mut self.waiting_for_ack);
        for (key, revision) in waiting {
            let latest = self.workers.utility_size_latest_revision(key);
            let revision = latest.unwrap_or(revision);
            if self.accepted_revisions.get(&key) == Some(&revision)
                || self.workers.utility_size_write_error(key).is_some()
            {
                continue;
            }
            self.waiting_for_ack.insert(key, revision);
            ctx.request_repaint_after(ACK_POLL_INTERVAL);
        }
        Ok(())
    }

    fn publish_acknowledgement(
        &mut self,
        settings: &mut Settings,
        acknowledgement: UtilitySizeAcknowledgement,
    ) -> Result<(), String> {
        let key = acknowledgement.key;
        let revision = acknowledgement.revision;
        if self.workers.utility_size_latest_revision(key) != Some(revision) {
            return Ok(());
        }

        match acknowledgement.preference {
            UtilitySizePreference::Preferred(size) => settings
                .utility_dialog_sizes
                .set(key, size)
                .map(|_| ())
                .map_err(|error| error.to_string())?,
            UtilitySizePreference::Reset => settings
                .utility_dialog_sizes
                .reset(key)
                .map(|_| ())
                .map_err(|error| error.to_string())?,
            UtilitySizePreference::Missing | UtilitySizePreference::Unsupported => return Ok(()),
        }

        self.accepted_revisions.insert(key, revision);
        self.waiting_for_ack.remove(&key);
        let locally_resized = self.local_resize_revisions.get(&key) == Some(&revision);
        if locally_resized {
            self.local_resize_revisions.remove(&key);
        }
        let explicitly_reset = self.reset_revisions.get(&key) == Some(&revision);
        if explicitly_reset {
            self.reset_revisions.remove(&key);
        }

        for instance in self
            .instances
            .values_mut()
            .filter(|instance| instance.key == key)
        {
            if !instance.is_open || !instance.has_applied_restore || instance.restore_pending {
                continue;
            }
            if explicitly_reset || !locally_resized {
                instance.restore_pending = true;
            } else {
                // A completed grip drag changes the shared value for future
                // openings without moving other currently open instances.
                instance.last_preference = Some(acknowledgement.preference);
                instance.last_revision = Some(revision);
            }
        }
        Ok(())
    }

    /// Queue a reset through the canonical settings transaction. The reset
    /// takes effect in open windows after its successful acknowledgement.
    pub(crate) fn reset(&mut self, key: UtilityToolKey) -> Result<UtilitySizeRevision, String> {
        self.workers.queue_utility_size_mutation(
            self.settings_path.clone(),
            key,
            UtilitySizeMutation::Reset,
        )?;
        let revision = self
            .workers
            .utility_size_latest_revision(key)
            .ok_or_else(|| "utility size reset was queued without a revision".to_owned())?;
        self.reset_revisions.insert(key, revision);
        self.local_resize_revisions.remove(&key);
        self.waiting_for_ack.insert(key, revision);
        Ok(revision)
    }

    /// Retry a retained failure only from an explicit controlled event.
    pub(crate) fn retry_failed(&self) -> Result<(), String> {
        self.workers
            .retry_utility_size_mutations(self.settings_path.clone())
    }

    pub(crate) fn write_error(&self, key: UtilityToolKey) -> Option<String> {
        self.workers.utility_size_write_error(key)
    }

    /// Show one ordinary resizable utility window. The supplied builder keeps
    /// its title, id, position, flags and other caller choices; this wrapper
    /// adds only its one-shot opening default and body-space lifecycle.
    pub(crate) fn show_window<R>(
        &mut self,
        ctx: &Context,
        settings: &Settings,
        key: UtilityToolKey,
        id: Id,
        open: &mut bool,
        default_outer_size: Vec2,
        window: Window<'_>,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        if !*open {
            let instance = self
                .instances
                .entry(id)
                .or_insert_with(|| WindowInstance::new(key));
            if instance.key != key {
                *instance = WindowInstance::new(key);
            }
            instance.is_open = false;
            instance.last_seen_frame = Some(self.frame);
            instance.restore_pending = true;
            instance.resize_start = None;
            instance.resize_release = None;
            return None;
        }

        let preference = settings.utility_dialog_sizes.preference(key);
        let revision = self.accepted_revisions.get(&key).copied();
        let instance = self
            .instances
            .entry(id)
            .or_insert_with(|| WindowInstance::new(key));
        if instance.key != key {
            *instance = WindowInstance::new(key);
        }

        let omitted = instance
            .last_seen_frame
            .is_some_and(|last| last.saturating_add(1) < self.frame);
        if !instance.is_open
            || omitted
            || !instance.has_applied_restore
            || instance.last_preference != Some(preference)
            || instance.last_revision != revision
        {
            instance.restore_pending = true;
        }
        let restore_pending = instance.restore_pending;
        let previous_size = instance.last_outer_rect.map(|rect| rect.size());
        instance.is_open = true;
        instance.last_seen_frame = Some(self.frame);

        let preferred = match preference {
            UtilitySizePreference::Preferred(size) => size,
            UtilitySizePreference::Missing
            | UtilitySizePreference::Reset
            | UtilitySizePreference::Unsupported => preferred_default(default_outer_size),
        };
        let viewport = ctx.content_rect().size();
        let available = Vec2::new(
            (viewport.x - 2.0 * VIEWPORT_MARGIN).max(1.0),
            (viewport.y - 2.0 * VIEWPORT_MARGIN).max(1.0),
        );
        let applied = preferred.apply_within(available.x, available.y);
        let applied_outer = Vec2::new(applied.width, applied.height);

        let mut body_rect = None;
        let mut body_clip = None;
        let mut accepted_body = false;

        let mut window = window;
        if restore_pending {
            window = window.default_size(applied_outer);
            let (minimum, maximum) = restoration_bounds(applied_outer, available, previous_size);
            window = window.min_size(minimum).max_size(maximum);
        }
        let response = window.open(open).show(ctx, |ui| {
            let accepted = ui.is_visible() && ui.is_enabled() && !ui.is_sizing_pass();
            accepted_body = accepted;

            let result = contents(ui);
            if accepted && !ctx.will_discard() {
                ui.take_available_space();
                body_rect = Some(ui.min_rect());
                body_clip = Some(ui.clip_rect());
            }
            result
        });

        let visible_body = accepted_body
            && response
                .as_ref()
                .is_some_and(|response| response.inner.is_some())
            && *open
            && !ctx.will_discard();
        let rect = response.as_ref().map(|response| response.response.rect);
        let grip = resize_grip_state(ctx, id);
        let mut mutation = None;

        if let Some(instance) = self.instances.get_mut(&id) {
            if !*open {
                instance.is_open = false;
                instance.restore_pending = true;
                instance.resize_start = None;
                instance.resize_release = None;
            } else if visible_body {
                if restore_pending {
                    instance.restore_pending = false;
                    instance.has_applied_restore = true;
                    instance.last_preference = Some(preference);
                    instance.last_revision = revision;
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
                            after_frame: self.frame.saturating_add(1),
                            starting_size: instance.resize_start.unwrap_or(rect.size()),
                        });
                    }

                    if let Some(release) = instance.resize_release
                        && self.frame >= release.after_frame
                        && !grip.dragging
                        && !grip.stopped
                    {
                        let changed = (rect.width() - release.starting_size.x).abs() > 1.0
                            || (rect.height() - release.starting_size.y).abs() > 1.0;
                        if changed {
                            mutation = PreferredWindowSize::new(rect.width(), rect.height()).ok();
                        }
                        instance.resize_start = None;
                        instance.resize_release = None;
                    }
                }
            }
        }

        if let Some(size) = mutation {
            match self.workers.queue_utility_size_mutation(
                self.settings_path.clone(),
                key,
                UtilitySizeMutation::Set(size),
            ) {
                Ok(()) => {
                    if let Some(revision) = self.workers.utility_size_latest_revision(key) {
                        self.local_resize_revisions.insert(key, revision);
                        self.reset_revisions.remove(&key);
                        self.waiting_for_ack.insert(key, revision);
                        ctx.request_repaint_after(ACK_POLL_INTERVAL);
                    }
                }
                Err(error) => {
                    tracing::warn!(tool = key.as_str(), %error, "could not queue utility window size");
                }
            }
        }

        response
    }
}

fn preferred_default(default_outer: Vec2) -> PreferredWindowSize {
    PreferredWindowSize::new(default_outer.x, default_outer.y).unwrap_or(PreferredWindowSize {
        width: 400.0,
        height: 300.0,
    })
}

fn restoration_bounds(target: Vec2, available: Vec2, previous: Option<Vec2>) -> (Vec2, Vec2) {
    let mut minimum = Vec2::splat(64.0).min((available - Vec2::splat(0.5)).max(Vec2::ZERO));
    let mut maximum = available;
    if let Some(previous) = previous {
        for axis in 0..2 {
            if target[axis] + 0.5 < previous[axis] {
                maximum[axis] = target[axis];
                minimum[axis] = minimum[axis].min((target[axis] - 0.5).max(0.0));
            } else if target[axis] > previous[axis] + 0.5 {
                minimum[axis] = (target[axis] - 0.5).max(0.0);
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

fn resize_grip_state(ctx: &Context, window_id: Id) -> ResizeGripState {
    let edge_drag = Id::new(egui::LayerId::new(egui::Order::Middle, window_id)).with("edge_drag");
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
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        if let Some(size) = size {
            settings.utility_dialog_sizes.set(key(), size).unwrap();
        }
        settings.save(path.to_str().unwrap()).unwrap();
        let path = path.to_string_lossy().into_owned();
        let workers = SettingsSaveWorkerRegistry::default();
        let manager = UtilityDialogSizeManager::new(path.clone(), workers.clone());
        (
            directory,
            path,
            settings,
            Context::default(),
            workers,
            manager,
        )
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

    fn render_window(
        ctx: &Context,
        manager: &mut UtilityDialogSizeManager,
        settings: &mut Settings,
        id: Id,
        screen: Vec2,
        events: Vec<Event>,
        pixels_per_point: f32,
        default_open: bool,
        body_rows: usize,
        discard_first_pass: bool,
    ) -> Vec<Id> {
        let mut open = true;
        let mut dragged_ids = Vec::new();
        let output = ctx.run_ui(raw_input(screen, events, pixels_per_point), |root| {
            let ctx = root.ctx().clone();
            manager.begin_frame(&ctx, settings).unwrap();
            if discard_first_pass && ctx.current_pass_index() == 0 {
                ctx.request_discard("exercise one-shot utility size restore");
            }
            let window = Window::new(TITLE)
                .id(id)
                .resizable(true)
                .default_open(default_open);
            let _ = manager.show_window(
                &ctx,
                settings,
                key(),
                id,
                &mut open,
                DEFAULT_OUTER,
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
        let (_directory, path, mut settings, ctx, workers, mut manager) =
            setup(Some(PreferredWindowSize::new(400.0, 300.0).unwrap()));
        let id = Id::new("utility-real-resize-grip");
        for _ in 0..3 {
            render_window(
                &ctx,
                &mut manager,
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
        let instance = manager.instances.get(&id).unwrap();
        let body_before = instance.last_body_rect.unwrap();
        let body_clip = instance.last_body_clip.unwrap();
        assert!(body_before.width() > 300.0 && body_before.height() > 200.0);
        assert!(body_clip.width() > 300.0 && body_clip.height() > 200.0);
        assert_body_and_clip_within_frame(&ctx, before, body_before, body_clip);

        let mut observed_grip_ids = Vec::new();
        for events in pointer_press_and_drag(before, egui::vec2(75.0, 55.0)) {
            observed_grip_ids.extend(render_window(
                &ctx,
                &mut manager,
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
                &mut manager,
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
        let instance = manager.instances.get(&id).unwrap();
        let body_after = instance.last_body_rect.unwrap();
        let clip_after = instance.last_body_clip.unwrap();
        assert!(body_after.width() > body_before.width() + 30.0);
        assert!(body_after.height() > body_before.height() + 20.0);
        assert!(clip_after.width() > body_clip.width() + 30.0);
        assert!(clip_after.height() > body_clip.height() + 20.0);
        assert_body_and_clip_within_frame(&ctx, after, body_after, clip_after);

        workers.join_all();
        // Drain the one context-owned acknowledgement before checking that the
        // current in-memory settings and durable file agree.
        ctx.run_ui(raw_input(egui::vec2(1000.0, 800.0), vec![], 1.0), |root| {
            manager.begin_frame(root.ctx(), &mut settings).unwrap();
        })
        .drop_without_applying_deltas();
        render_window(
            &ctx,
            &mut manager,
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
        assert_eq!(
            settings.utility_dialog_sizes.preferred(key()),
            Some(persisted)
        );
    }

    #[test]
    fn omitted_window_reopens_with_latest_size_from_another_instance() {
        let (_directory, path, mut settings, ctx, workers, mut manager) =
            setup(Some(PreferredWindowSize::new(400.0, 300.0).unwrap()));
        let first = Id::new("utility-size-first-instance");
        let second = Id::new("utility-size-second-instance");

        for _ in 0..3 {
            render_window(
                &ctx,
                &mut manager,
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
                &mut manager,
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
        let second_body_before = manager.instances[&second].last_body_rect.unwrap();
        let second_clip_before = manager.instances[&second].last_body_clip.unwrap();
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
                &mut manager,
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
                &mut manager,
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
        let second_body_after = manager.instances[&second].last_body_rect.unwrap();
        let second_clip_after = manager.instances[&second].last_body_clip.unwrap();
        assert!(second_body_after.width() > second_body_before.width() + 30.0);
        assert!(second_body_after.height() > second_body_before.height() + 20.0);
        assert_body_and_clip_within_frame(&ctx, resized, second_body_after, second_clip_after);

        workers.join_all();
        render_window(
            &ctx,
            &mut manager,
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
            &mut manager,
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
    fn viewport_clamp_and_discarded_pass_preserve_the_durable_preference() {
        let preferred = PreferredWindowSize::new(1200.0, 900.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, mut manager) = setup(Some(preferred));
        let id = Id::new("utility-size-clamp-and-discard");
        render_window(
            &ctx,
            &mut manager,
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
        assert_eq!(
            settings.utility_dialog_sizes.preferred(key()),
            Some(preferred)
        );
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preferred(key()),
            Some(preferred)
        );
        workers.join_all();
    }

    #[test]
    fn smaller_reset_reopens_over_stale_egui_size() {
        let large = PreferredWindowSize::new(720.0, 520.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, mut manager) = setup(Some(large));
        let id = Id::new("utility-size-smaller-reset");
        for _ in 0..3 {
            render_window(
                &ctx,
                &mut manager,
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
        while settings.utility_dialog_sizes.preference(key()) != UtilitySizePreference::Reset {
            assert!(
                Instant::now() < reset_deadline,
                "timed out waiting for reset revision {reset_revision:?}"
            );
            render_window(
                &ctx,
                &mut manager,
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
                &mut manager,
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
                &mut manager,
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
                &mut manager,
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
            manager.begin_frame(root.ctx(), &mut settings).unwrap();
        })
        .drop_without_applying_deltas();
        let persisted_size = PreferredWindowSize::new(resized.width(), resized.height()).unwrap();
        assert_eq!(
            settings.utility_dialog_sizes.preferred(key()),
            Some(persisted_size)
        );
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preference(key()),
            UtilitySizePreference::Preferred(persisted_size)
        );
    }

    #[test]
    fn title_drag_content_reflow_and_collapsed_window_do_not_save_size() {
        let preferred = PreferredWindowSize::new(400.0, 300.0).unwrap();
        let (_directory, path, mut settings, ctx, workers, mut manager) = setup(Some(preferred));
        let id = Id::new("utility-size-non-grip-control");
        for _ in 0..3 {
            render_window(
                &ctx,
                &mut manager,
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
                &mut manager,
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
                &mut manager,
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
            &mut manager,
            &mut settings,
            collapsed_id,
            egui::vec2(1000.0, 800.0),
            vec![],
            1.0,
            false,
            1,
            false,
        );
        assert!(manager.instances[&collapsed_id].restore_pending);
        workers.join_all();
        assert_eq!(
            settings_file(&path).utility_dialog_sizes.preferred(key()),
            Some(preferred)
        );
        assert_eq!(workers.utility_size_latest_revision(key()), None);
        assert_eq!(workers.utility_size_latest_revision(key()), None);
    }
}
