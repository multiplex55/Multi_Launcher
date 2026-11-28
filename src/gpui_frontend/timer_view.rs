//! Retained timer form and exact session completion presentation.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timer_session::{TimerKind, TimerRow, TimerSession};
mod picker;
pub(super) const WAIT: &str = "Wait for the Timer or Alarm operation to finish.";

pub(super) struct TypedTimerLane {
    busy: Arc<AtomicBool>,
    wake: Arc<HashWakeSignal>,
    released: AtomicBool,
}
impl TypedTimerLane {
    pub fn acquire(busy: Arc<AtomicBool>, wake: Arc<HashWakeSignal>) -> Option<Arc<Self>> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Arc::new(Self {
            busy,
            wake,
            released: AtomicBool::new(false),
        }))
    }
    pub fn release(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            self.busy.store(false, Ordering::Release);
            self.wake.notify();
        }
    }
}
impl Drop for TypedTimerLane {
    fn drop(&mut self) {
        self.release();
    }
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|set| set.contains("timer"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "timer")
        && matches!(
            parse_action(action),
            Ok(Command::Timer(
                multi_launcher::commands::TimerCommand::TimerDialog
                    | multi_launcher::commands::TimerCommand::AlarmDialog
                    | multi_launcher::commands::TimerCommand::Start { .. }
                    | multi_launcher::commands::TimerCommand::AlarmSet { .. }
            ))
        )
        && search_providers::search_routed(
            [&search_providers::TimerBare as &dyn Plugin],
            query,
            enabled,
            caps,
        )
        .into_iter()
        .any(|(name, row)| name == "timer" && row == *action)
}

pub(super) struct TimerView {
    pub open: bool,
    pub session: Arc<TimerSession>,
    busy: Arc<AtomicBool>,
    alarm: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    duration: Entity<InputState>,
    _duration_sub: Subscription,
    picker_hours: Entity<InputState>,
    picker_minutes: Entity<InputState>,
    picker_seconds: Entity<InputState>,
    _picker_subs: Vec<Subscription>,
    picker_parts: picker::Parts,
    picker_projection: [String; 3],
    observed_duration: Option<String>,
    label: Entity<InputState>,
    sound: usize,
    feedback: Option<String>,
    pause_lease: Option<Arc<AtomicBool>>,
    pause_focus_guard: Option<Subscription>,
    pause_active_guard: Option<Subscription>,
    painted_rows: Vec<TimerRow>,
}

impl TimerView {
    fn new(
        root: WeakEntity<RootWindow>,
        session: Arc<TimerSession>,
        busy: Arc<AtomicBool>,
        alarm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let duration = cx.new(|cx| InputState::new(window, cx).placeholder("Duration (for example 5m)"));
        let duration_sub = cx.subscribe_in(&duration, window, |view: &mut Self, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Change) { view.sync_picker(window, cx); cx.notify(); }
        });
        let picker_hours = cx.new(|cx| InputState::new(window, cx).placeholder("Hours"));
        let picker_minutes = cx.new(|cx| InputState::new(window, cx).placeholder("Minutes"));
        let picker_seconds = cx.new(|cx| InputState::new(window, cx).placeholder("Seconds"));
        let picker_subs = [&picker_hours, &picker_minutes, &picker_seconds].into_iter().enumerate().map(|(index, input)|
            cx.subscribe_in(input, window, move |view: &mut Self, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) { return; }
                if (index != 2 || !view.alarm) && input.focus_handle(cx).is_focused(window)
                    && view.picker_changed(cx) {
                    view.sync_picker(window, cx);
                    if view.picker_changed(cx) { view.apply_picker(window, cx, false); }
                }
                cx.notify();
            })).collect();
        Self {
            open: true,
            session,
            busy,
            alarm,
            root,
            host: window.window_handle(),
            owner,
            mount,
            focus: cx.focus_handle(),
            duration,
            _duration_sub: duration_sub,
            picker_hours,
            picker_minutes,
            picker_seconds,
            _picker_subs: picker_subs,
            picker_parts: picker::Parts::default(),
            picker_projection: Default::default(),
            observed_duration: None,
            label: cx.new(|cx| InputState::new(window, cx).placeholder("Label (optional)")),
            sound: default_sound(),
            feedback: None,
            pause_lease: None,
            pause_focus_guard: None,
            pause_active_guard: None,
            painted_rows: Vec::new(),
        }
    }
    fn sync_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.duration.update(cx, |input, cx| input.marked_text_range(window, cx).is_some()) { return; }
        let text = self.duration.read(cx).value().to_string();
        if self.observed_duration.as_ref() == Some(&text) { return; }
        self.observed_duration = Some(text.clone());
        if self.alarm { self.picker_parts.observe_alarm(&text); }
        else { self.picker_parts.observe_timer(&text); }
        self.picker_projection = [self.picker_parts.hours.to_string(), self.picker_parts.minutes.to_string(), self.picker_parts.seconds.to_string()];
        self.picker_hours.update(cx, |input, cx| input.set_value(self.picker_projection[0].clone(), window, cx));
        self.picker_minutes.update(cx, |input, cx| input.set_value(self.picker_projection[1].clone(), window, cx));
        self.picker_seconds.update(cx, |input, cx| input.set_value(self.picker_projection[2].clone(), window, cx));
    }
    fn apply_picker(&mut self, window: &mut Window, cx: &mut Context<Self>, report: bool) -> bool {
        if self.busy.load(Ordering::Acquire) || !self.current(window, cx) || !window.is_window_active()
            || window.has_active_dialog(cx) { return false; }
        if self.duration.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.picker_hours.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.picker_minutes.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || (!self.alarm && self.picker_seconds.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())) { return false; }
        let hours = self.picker_hours.read(cx).value().to_string();
        let minutes = self.picker_minutes.read(cx).value().to_string();
        let seconds = self.picker_seconds.read(cx).value().to_string();
        let Some(parts) = picker::parse_parts(&hours, &minutes, (!self.alarm).then_some(seconds.as_str()), self.alarm) else {
            if report { self.feedback = Some("Enter whole numbers within the shown ranges.".into()); cx.notify(); } return false;
        };
        let current = self.duration.read(cx).value().to_string();
        let text = if self.alarm { picker::alarm_text(parts, &current) } else { Some(picker::timer_text(parts)) };
        let Some(text) = text else { if report { self.feedback = Some("Enter a valid alarm date and time first.".into()); cx.notify(); } return false; };
        self.picker_parts = parts;
        self.picker_projection = [hours, minutes, seconds];
        self.observed_duration = Some(text.clone());
        self.duration.update(cx, |input, cx| input.set_value(text, window, cx));
        self.feedback = None;
        cx.notify();
        true
    }
    fn picker_changed(&self, cx: &App) -> bool {
        picker::fields_changed(&self.picker_projection, [self.picker_hours.read(cx).value().as_str(),
            self.picker_minutes.read(cx).value().as_str(), self.picker_seconds.read(cx).value().as_str()], self.alarm)
    }
    fn current(&self, window: &Window, cx: &App) -> bool {
        self.open
            && self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.action_sheet.is_none()
                    && root.root_visibility.active.load(Ordering::Acquire)
                    && root.root_visibility.visible.load(Ordering::Acquire)
                    && root.native_window.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            })
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.load(Ordering::Acquire) || !self.current(window, cx) {
            return;
        }
        self.retire_form(window, cx);
    }

    fn retire_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.owner.retire();
        let _ = self.root.update(cx, |root, cx| {
            if focused
                && window.is_window_active()
                && !window.has_active_dialog(cx)
                && root
                    .native_window
                    .is_some_and(|native| current_foreground_identity() == Some(native.identity))
            {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) || !window.is_window_active() || window.has_active_dialog(cx) {
            return;
        }
        if self.duration.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.label.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.picker_hours.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.picker_minutes.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || (!self.alarm && self.picker_seconds.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())) {
            self.feedback = Some("Finish composing the current field before starting.".into());
            cx.notify();
            return;
        }
        self.sync_picker(window, cx);
        if self.picker_changed(cx) && !self.apply_picker(window, cx, true) { return; }
        let duration = self.duration.read(cx).value().to_string();
        let label = self.label.read(cx).value().to_string();
        let sound = multi_launcher::sound::SOUND_NAMES[self.sound].to_owned();
        self.run_operation(
            TimerOperation::Start {
                duration,
                label,
                sound,
                alarm: self.alarm,
            },
            cx,
        );
    }

    pub(super) fn revoke_pause(&mut self) {
        if let Some(lease) = &self.pause_lease {
            lease.store(false, Ordering::Release);
        }
    }

    fn set_paused(&mut self, id: u64, generation: u64, paused: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.load(Ordering::Acquire) || !self.current(window, cx)
            || !window.is_window_active() || window.has_active_dialog(cx) {
            return;
        }
        if !self.painted_rows.iter().any(|row| row.id == id && row.generation == generation
            && row.kind == TimerKind::Countdown && row.paused == paused) {
            return;
        }
        let Some(focus) = window.focused(cx) else { return; };
        let Some(root) = self.root.upgrade() else { return; };
        let state = root.read(cx);
        if !state.external_owner_is_current(Some(&focus), window, cx) { return; }
        let Some(native) = state.native_window else { return; };
        let visibility = state.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = state.restart_admission.clone();
        let wake = state.timer_wake.clone();
        if !json_utility::native_host_current(&visibility, revision, Some(native), false) {
            return;
        }
        let Some(permit) = admission.admit() else { return; };
        if self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return;
        }
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.pause_focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| {
            blur.store(false, Ordering::Release);
        }));
        let inactive = lease.clone();
        self.pause_active_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { inactive.store(false, Ordering::Release); }
        }));
        self.pause_lease = Some(lease.clone());
        self.feedback = None;
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let worker_lease = lease.clone();
        let session = self.session.clone();
        let busy = self.busy.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            session.set_countdown_paused_if(id, generation, paused, || {
                worker_mount.is_current() && worker_lease.load(Ordering::Acquire)
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
            })
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            busy.store(false, Ordering::Release);
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    if !mount.is_current() || !mount.matches(&view.mount) { return; }
                    view.pause_lease = None;
                    view.pause_focus_guard = None;
                    view.pause_active_guard = None;
                    if lease.load(Ordering::Acquire) && view.current(window, cx) {
                        match result {
                            Ok(true) => view.feedback = None,
                            Ok(false) => view.feedback = Some("Timer changed; try again.".into()),
                            Err(error) => view.feedback = Some(error),
                        }
                    }
                    cx.notify();
                });
            }
            wake.notify();
        }).detach();
        cx.notify();
    }

    fn run_operation(&mut self, operation: TimerOperation, cx: &mut Context<Self>) {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let permit = self
            .root
            .update(cx, |root, _| root.restart_admission.admit())
            .ok()
            .flatten();
        let Some(permit) = permit else {
            self.busy.store(false, Ordering::Release);
            return;
        };
        let session = self.session.clone();
        let busy = self.busy.clone();
        let wake = self
            .root
            .upgrade()
            .map(|root| root.read(cx).timer_wake.clone());
        let receipt = self.mount.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            match operation {
                TimerOperation::Start {
                    duration,
                    label,
                    sound,
                    alarm,
                } => {
                    if alarm {
                        session.start_alarm(&duration, &label, &sound).map(|_| true)
                    } else {
                        session.start_timer(&duration, &label, &sound).map(|_| true)
                    }
                }
                TimerOperation::Cancel { id, generation } => {
                    session.cancel(id, generation).map(|_| false)
                }
            }
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let outcome = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return;
                    }
                    match outcome {
                        Ok(true) => {
                            view.feedback = None;
                            view.retire_form(window, cx);
                        }
                        Ok(false) => {}
                        Err(error) => view.feedback = Some(error),
                    }
                    cx.notify();
                });
            }
            busy.store(false, Ordering::Release);
            if let Some(wake) = wake {
                wake.notify();
            }
        })
        .detach();
        cx.notify();
    }
}

enum TimerOperation {
    Start {
        duration: String,
        label: String,
        sound: String,
        alarm: bool,
    },
    Cancel {
        id: u64,
        generation: u64,
    },
}

fn default_sound() -> usize {
    multi_launcher::sound::SOUND_NAMES
        .iter()
        .position(|name| *name == "None")
        .unwrap_or(0)
}

fn latest_completion(messages: Vec<String>) -> Option<String> {
    messages.into_iter().last()
}

fn displayed_rows(busy: bool, pause_pending: bool, painted: &mut Vec<TimerRow>, live: impl FnOnce() -> Option<Vec<TimerRow>>) -> Vec<TimerRow> {
    if busy && !pause_pending {
        return Vec::new();
    }
    if !busy {
        if let Some(rows) = live() {
            *painted = rows;
        }
    }
    painted.clone()
}

fn pause_button_id(id: u64) -> SharedString {
    SharedString::from(format!("timer-pause-{id}"))
}

impl Render for TimerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_picker(window, cx);
        let keys = self.mount.clone();
        let input_escape = self.mount.clone();
        let mut panel = div()
            .id("timer-pane")
            .track_focus(&self.focus)
            .capture_action(cx.listener(
                move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if input_escape.is_current()
                        && input_escape.matches(&view.mount)
                        && view.current(window, cx)
                        && !window.has_active_dialog(cx)
                        && (view.focus.is_focused(window)
                            || view.focus.contains_focused(window, cx))
                    {
                        let latched = view
                            .root
                            .update(cx, |root, _| {
                                let was = root.pane_escape_latched;
                                root.pane_escape_latched = true;
                                was
                            })
                            .unwrap_or(true);
                        if !latched {
                            view.close(window, cx);
                        }
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !keys.is_current()
                    || !keys.matches(&view.mount)
                    || !view.current(window, cx)
                    || window.has_active_dialog(cx)
                    || !(view.focus.is_focused(window) || view.focus.contains_focused(window, cx))
                {
                    return;
                }
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        let _ = view
                            .root
                            .update(cx, |root, _| root.pane_escape_latched = true);
                        view.close(window, cx);
                    }
                }
            }))
            .flex()
            .flex_col()
            .gap_2()
            .p_3();
        if self.open {
            let busy = self.busy.load(Ordering::Acquire);
            let start = self.mount.clone();
            let close = self.mount.clone();
            let sound = self.mount.clone();
            let preview = self.mount.clone();
            let picker_mount = self.mount.clone();
            let current_text = self.duration.read(cx).value().to_string();
            let picker_disabled = busy || (self.alarm && !current_text.trim().is_empty()
                && multi_launcher::plugins::timer::parse_alarm_spec(&current_text).is_err());
            panel = panel
                .child(Label::new(if self.alarm { "Alarm" } else { "Timer" }))
                .children(
                    self.alarm
                        .then(|| Label::new("Time: HH:MM, 1d HH:MM, or YYYY-MM-DD HH:MM")),
                )
                .child(
                    Input::new(&self.duration)
                        .readonly(busy)
                        .aria_label(if self.alarm {
                            "Alarm date and time"
                        } else {
                            "Duration"
                        }),
                )
                .child(div().flex().flex_wrap().gap_2()
                    .child(Label::new(if self.alarm { "Hour (0–23)" } else { "Hours" }))
                    .child(div().w(px(100.0)).child(Input::new(&self.picker_hours).readonly(picker_disabled).aria_label(if self.alarm { "Alarm hour" } else { "Timer hours" })))
                    .child(Label::new("Minutes (0–59)"))
                    .child(div().w(px(75.0)).child(Input::new(&self.picker_minutes).readonly(picker_disabled).aria_label(if self.alarm { "Alarm minute" } else { "Timer minutes" })))
                    .children((!self.alarm).then(|| div().flex().gap_2().child(Label::new("Seconds (0–59)"))
                        .child(div().w(px(75.0)).child(Input::new(&self.picker_seconds).readonly(picker_disabled).aria_label("Timer seconds")))))
                    .child(Button::new("timer-apply-picker").label(if self.alarm { "Set time" } else { "Set duration" }).disabled(picker_disabled)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if picker_mount.is_current() && picker_mount.matches(&view.mount) { view.apply_picker(window, cx, true); }
                        }))))
                .children(self.alarm.then(|| {
                    use chrono::TimeZone;
                    Label::new(picker::alarm_preview(&current_text, chrono::Local::now(), |time| chrono::Local.from_local_datetime(&time)))
                }))
                .child(Input::new(&self.label).readonly(busy).aria_label("Label"))
                .child(
                    Button::new("timer-sound")
                        .label(format!(
                            "Sound: {}",
                            multi_launcher::sound::SOUND_NAMES[self.sound]
                        ))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if !view.busy.load(Ordering::Acquire)
                                && sound.is_current()
                                && sound.matches(&view.mount)
                                && view.current(window, cx)
                                && !window.has_active_dialog(cx)
                            {
                                view.sound =
                                    (view.sound + 1) % multi_launcher::sound::SOUND_NAMES.len();
                                cx.notify();
                            }
                        })),
                )
                .child(
                    Button::new("timer-preview")
                        .label("Preview sound")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if !view.busy.load(Ordering::Acquire)
                                && preview.is_current()
                                && preview.matches(&view.mount)
                                && view.current(window, cx)
                                && window.is_window_active()
                                && !window.has_active_dialog(cx)
                            {
                                multi_launcher::sound::play_sound(
                                    multi_launcher::sound::SOUND_NAMES[view.sound],
                                );
                            }
                        })),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("timer-start")
                                .label("Start")
                                .disabled(busy)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if start.is_current() && start.matches(&view.mount) {
                                        view.start(window, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("timer-close")
                                .label("Close")
                                .disabled(busy)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if close.is_current() && close.matches(&view.mount) {
                                        view.close(window, cx);
                                    }
                                })),
                        ),
                );
            if let Some(error) = &self.feedback {
                panel = panel.child(Label::new(error.clone()));
            }
            if busy {
                panel = panel.child(Label::new("Working with timers and alarms…"));
            }
            let session = self.session.clone();
            for row in displayed_rows(busy, self.pause_lease.is_some(), &mut self.painted_rows, || session.try_rows()) {
                let mount = self.mount.clone();
                let pause_mount = mount.clone();
                let id = row.id;
                let generation = row.generation;
                let paused = row.paused;
                let countdown = row.kind == TimerKind::Countdown;
                let mut item = div()
                    .id(SharedString::from(format!("timer-row-{id}")))
                    .flex()
                    .gap_2()
                    .child(Label::new(if paused {
                        format!("{} (Paused · {}s left)", row.label, row.remaining.as_secs())
                    } else {
                        format!("{} ({}s left)", row.label, row.remaining.as_secs())
                    }));
                if countdown {
                    item = item.child(Button::new(pause_button_id(id))
                        .label(if paused { "Resume" } else { "Pause" })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if pause_mount.is_current() && pause_mount.matches(&view.mount) {
                                view.set_paused(id, generation, paused, window, cx);
                            }
                        })));
                }
                panel = panel.child(item.child(
                    Button::new(SharedString::from(format!("timer-cancel-{id}-{generation}")))
                        .label("Cancel")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if mount.is_current() && mount.matches(&view.mount)
                                && view.current(window, cx) && !window.has_active_dialog(cx) {
                                view.run_operation(TimerOperation::Cancel { id, generation }, cx);
                            }
                        })),
                ));
            }
        }
        panel
    }
}

impl RootWindow {
    pub(super) fn start_timer_wake(
        &mut self,
        restore: Arc<Mutex<Option<Result<(), String>>>>,
        start_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = start_error {
            self.restart_status = Some(format!("Could not start Timer: {error}"));
        }
        let Some(session) = self.timer_session.clone() else {
            return;
        };
        let wake = self.timer_wake.clone();
        let busy = self.timer_busy.clone();
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                let (messages, alarm_error) = if busy.load(Ordering::Acquire) {
                    (Vec::new(), None)
                } else {
                    let session = session.clone();
                    cx.background_executor()
                        .spawn(async move { (session.take_finished(), session.take_alarm_error()) })
                        .await
                };
                let result = restore.lock().ok().and_then(|mut value| value.take());
                if root
                    .update(cx, |root, cx| {
                        if let Some(Err(error)) = result {
                            root.restart_status =
                                Some(format!("Could not restore alarms: {error}"));
                        }
                        if let Some(error) = alarm_error {
                            root.restart_status = Some(error);
                        } else if !busy.load(Ordering::Acquire)
                            && root.restart_status.as_deref() == Some(WAIT)
                        {
                            root.restart_status = None;
                        }
                        if let Some(message) = latest_completion(messages) {
                            root.timer_completion_revision =
                                root.timer_completion_revision.wrapping_add(1).max(1);
                            root.timer_completion = Some((root.timer_completion_revision, message));
                        }
                        if let Some(view) = &root.timer_view {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn open_timer(&mut self, alarm: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || (self.timer_view.is_some() && self.timer_busy.load(Ordering::Acquire))
        {
            return;
        }
        if let Some(view) = &self.timer_view {
            view.update(cx, |view, cx| {
                view.mount = view.owner.attach();
                view.open = true;
                view.alarm = alarm;
                view.picker_parts = picker::Parts::default();
                view.observed_duration = None;
                view.duration
                    .update(cx, |input, cx| input.set_value("", window, cx));
                view.label
                    .update(cx, |input, cx| input.set_value("", window, cx));
                view.sound = default_sound();
                view.feedback = None;
                view.duration.focus_handle(cx).focus(window, cx);
                cx.notify();
            });
        } else {
            let Some(session) = self.timer_session.clone() else {
                self.restart_status = Some("Timer notifications are unavailable.".into());
                cx.notify();
                return;
            };
            let root = cx.entity().downgrade();
            let busy = self.timer_busy.clone();
            let view = cx.new(|cx| TimerView::new(root, session, busy, alarm, window, cx));
            view.read(cx).duration.focus_handle(cx).focus(window, cx);
            self.timer_view = Some(view);
        }
        if let Some(view) = &self.timer_view {
            let receipt = view.read(cx).mount.clone();
            let weak = view.downgrade();
            let visibility = self.root_visibility.clone();
            cx.spawn(async move |_, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    let keep = weak
                        .update(cx, |view, cx| {
                            if !view.open || !receipt.is_current() || !receipt.matches(&view.mount)
                            {
                                return false;
                            }
                            if view.busy.load(Ordering::Acquire) {
                                return true;
                            }
                            if visibility.active.load(Ordering::Acquire)
                                && visibility.visible.load(Ordering::Acquire)
                            {
                                cx.notify();
                            }
                            true
                        })
                        .unwrap_or(false);
                    if !keep {
                        break;
                    }
                }
            })
            .detach();
        }
        self.root_scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{current_provider, default_sound, displayed_rows, latest_completion, pause_button_id, TypedTimerLane};
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::timer::TimerPlugin;
    use multi_launcher::plugins::timer_session::{TimerKind, TimerRow};
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    #[test]
    fn typed_timer_rows_require_exact_stock_action_and_current_gates() {
        let enabled = HashSet::from(["timer".to_owned()]);
        for query in ["timer add 5m tea", "alarm 1d 12:00 meeting"] {
            let row = TimerPlugin.search(query).remove(0);
            assert!(current_provider(query, &row, Some(&enabled), None));
            assert!(!current_provider("timer", &row, Some(&enabled), None));
            assert!(!current_provider(query, &row, Some(&HashSet::new()), None));
            assert!(!current_provider(query, &row, Some(&enabled), Some(&HashMap::from([
                ("timer".to_owned(), vec!["completion_dialog".to_owned()]),
            ]))));
            let mut forged = row;
            forged.label.push('!');
            assert!(!current_provider(query, &forged, Some(&enabled), None));
        }
        assert!(!super::search_providers::TimerBare.search("timer list").iter().any(|row| row.action.starts_with("timer:show:")));
        assert!(super::search_providers::TimerBare.commands().iter().any(|row| row.action == "query:timer add "));
    }

    #[test]
    fn typed_timer_lane_is_single_flight_and_releases_on_drop() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
        let busy = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(super::HashWakeSignal::default());
        let lane = TypedTimerLane::acquire(busy.clone(), wake.clone()).unwrap();
        assert!(busy.load(Ordering::Acquire));
        assert!(TypedTimerLane::acquire(busy.clone(), wake.clone()).is_none());
        drop(lane);
        assert!(!busy.load(Ordering::Acquire));
        let lane = TypedTimerLane::acquire(busy.clone(), wake).unwrap();
        lane.release();
        assert!(!busy.load(Ordering::Acquire));
    }

    #[test]
    fn pending_pause_retains_exact_painted_row_and_button_identity() {
        let row = TimerRow { id: 42, generation: 7, label: "Timer".into(),
            remaining: Duration::from_secs(60), kind: TimerKind::Countdown, paused: false };
        let mut painted = vec![row.clone()];
        let pending = displayed_rows(true, true, &mut painted, || panic!("busy must not sample the session"));
        assert_eq!(pending, vec![row.clone()]);
        assert_eq!(pause_button_id(pending[0].id), pause_button_id(row.id));
        assert!(displayed_rows(true, false, &mut painted, || None).is_empty());
        assert_eq!(displayed_rows(false, false, &mut painted, || None), pending);
        let mut resumed = row;
        resumed.generation += 1;
        resumed.paused = true;
        assert_eq!(displayed_rows(false, false, &mut painted, || Some(vec![resumed.clone()])), vec![resumed]);
        assert!(displayed_rows(false, false, &mut painted, || Some(Vec::new())).is_empty());
        assert!(painted.is_empty());
    }

    #[test]
    fn bare_timer_route_requires_exact_current_stock_action_and_search_capability() {
        let action = TimerPlugin.search("TiMeR").pop().unwrap();
        let enabled = HashSet::from(["timer".to_owned()]);
        assert!(current_provider("TiMeR", &action, Some(&enabled), None));
        assert!(!current_provider(
            "timer list",
            &action,
            Some(&enabled),
            None
        ));
        assert!(!current_provider(
            "timer",
            &action,
            Some(&HashSet::new()),
            None
        ));
        assert!(!current_provider(
            "timer",
            &action,
            Some(&enabled),
            Some(&HashMap::from([(
                "timer".to_owned(),
                vec!["completion_dialog".to_owned()]
            )]))
        ));
        let mut forged = action.clone();
        forged.label.push('!');
        assert!(!current_provider("timer", &forged, Some(&enabled), None));
        assert_eq!(multi_launcher::sound::SOUND_NAMES[default_sound()], "None");
        let alarm = TimerPlugin.search("ALARM").pop().unwrap();
        assert!(current_provider("ALARM", &alarm, Some(&enabled), None));
        assert!(!current_provider(
            "alarm 1d 12:00",
            &alarm,
            Some(&enabled),
            None
        ));
        assert_eq!(
            latest_completion(vec!["first".into(), "latest".into()]).as_deref(),
            Some("latest")
        );
        assert_eq!(latest_completion(Vec::new()), None);
    }
}
