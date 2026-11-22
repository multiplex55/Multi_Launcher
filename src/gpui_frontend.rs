#[cfg(windows)]
use std::sync::mpsc::{Receiver, channel};
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use gpui_kit::component::{
    button::Button,
    input::{Input, InputState},
    label::Label,
};
use gpui_kit::*;

#[cfg(windows)]
use std::ffi::c_void;

use multi_launcher::hotkey::HotkeyListener;
#[cfg(windows)]
use multi_launcher::hotkey::runtime::HotkeyTrigger;

gpui_kit::actions!([ToggleSecondaryWindow]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Frontend {
    #[default]
    Egui,
    Gpui,
}

pub(crate) fn selection_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Frontend, String> {
    let mut selection = Frontend::Egui;
    let mut has_explicit_selection = false;

    for arg in args {
        match arg.as_os_str() {
            value if value == OsStr::new("--ui=egui") => {
                if has_explicit_selection {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = Frontend::Egui;
                has_explicit_selection = true;
            }
            value if value == OsStr::new("--ui=gpui") => {
                if has_explicit_selection {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = Frontend::Gpui;
                has_explicit_selection = true;
            }
            value if value == OsStr::new("--ui") => {
                return Err("Choose --ui=egui or --ui=gpui.".to_owned());
            }
            value if value.to_string_lossy().starts_with("--ui=") => {
                return Err("Choose --ui=egui or --ui=gpui.".to_owned());
            }
            _ => {}
        }
    }

    Ok(selection)
}

pub(crate) fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows::{
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            },
            core::PCWSTR,
        };

        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Multi Launcher".encode_utf16().chain(Some(0)).collect();

        // SAFETY: both strings remain alive and NUL-terminated throughout the call.
        unsafe {
            MessageBoxW(
                HWND::default(),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    eprintln!("Multi Launcher: {message}");
}

struct RootWindow {
    query: Entity<InputState>,
    native_window: Option<NativeWindow>,
    focus_handle: FocusHandle,
    secondary: SecondaryWindowState,
    secondary_revision: Arc<AtomicU64>,
    secondary_close_subscription: Option<Subscription>,
    secondary_error: Option<String>,
}

enum SecondaryWindowState {
    Closed,
    OpenPending,
    Live {
        window: AnyWindowHandle,
        view: Entity<SecondaryWindow>,
    },
}

fn update_kit_window_content<T: 'static, C: AppContext, R>(
    window: AnyWindowHandle,
    content: &Entity<T>,
    cx: &mut C,
    update: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
) -> anyhow::Result<R> {
    window.update(cx, |_, window, cx| {
        content.update(cx, |content, content_cx| {
            update(content, window, content_cx)
        })
    })
}

impl RootWindow {
    fn open_or_toggle_secondary(&mut self, cx: &mut Context<Self>) {
        let foreground_before_request = current_foreground_identity();
        let (handle, view) = match &self.secondary {
            SecondaryWindowState::Closed => {
                self.open_secondary(cx);
                return;
            }
            SecondaryWindowState::OpenPending => return,
            SecondaryWindowState::Live { window, view } => (*window, view.clone()),
        };

        {
            let state = update_kit_window_content(handle, &view, cx, |secondary, window, cx| {
                let is_visible = secondary
                    .native_window
                    .and_then(|native| native_window_is_visible(native).ok())
                    .unwrap_or(true);
                if !is_visible {
                    secondary.query.focus_handle(cx).focus(window, cx);
                }
                (secondary.native_window, is_visible)
            });

            if let Ok((Some(native_window), was_visible)) = state {
                let show = !was_visible;
                let secondary_revision = Arc::clone(&self.secondary_revision);
                let revision = secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                cx.spawn(async move |_, cx| {
                    if secondary_revision.load(Ordering::Acquire) != revision {
                        return;
                    }
                    if let Err(error) = set_native_window_visible(native_window, show) {
                        tracing::warn!(%error, "could not change the retained GPUI window visibility");
                        return;
                    }
                    if show {
                        let expected_after = current_foreground_identity()
                            .filter(|foreground| Some(*foreground) == foreground_before_request);
                        let secondary_revision = Arc::clone(&secondary_revision);
                        cx.background_executor()
                            .spawn(async move {
                                activate_for_secondary_window(
                                    &secondary_revision,
                                    revision,
                                    native_window,
                                    expected_after,
                                );
                            })
                            .await;
                    }
                })
                .detach();
                return;
            }

            match state {
                Ok(_) => {
                    self.secondary_error = Some(
                        "The retained window has no usable native handle on this platform."
                            .to_owned(),
                    );
                    cx.notify();
                    return;
                }
                Err(_) => {
                    self.secondary = SecondaryWindowState::Closed;
                    self.secondary_revision.fetch_add(1, Ordering::AcqRel);
                    self.secondary_close_subscription = None;
                }
            }
        }

        self.open_secondary(cx);
    }

    fn open_secondary(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.secondary, SecondaryWindowState::Closed) {
            return;
        }
        self.secondary = SecondaryWindowState::OpenPending;
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Retained Window".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let secondary_revision = Arc::clone(&self.secondary_revision);
        cx.spawn(async move |this, cx| {
            let opened = cx.update(|cx| {
                gpui_kit::open_window(options, cx, |window, cx| {
                    let query = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Keep text while this window is open")
                    });
                    query.focus_handle(cx).focus(window, cx);
                    let native_window = match checked_native_window(window) {
                        Ok(native_window) => native_window,
                        Err(error) => {
                            tracing::error!(%error, "could not capture retained GPUI window handle");
                            None
                        }
                    };
                    cx.new(|_| SecondaryWindow {
                        query,
                        native_window,
                        secondary_revision: Arc::clone(&secondary_revision),
                    })
                })
            });
            match opened {
                Ok((window, view)) => {
                    let window_id = window.window_id();
                    let root = this.clone();
                    let close_subscription = cx.update(|cx| {
                        cx.on_window_closed(move |cx, closed| {
                            if closed != window_id {
                                return;
                            }
                            let _ = root.update(cx, |root, cx| {
                                if matches!(
                                    &root.secondary,
                                    SecondaryWindowState::Live { window, .. }
                                        if window.window_id() == window_id
                                ) {
                                    root.secondary = SecondaryWindowState::Closed;
                                    root.secondary_revision.fetch_add(1, Ordering::AcqRel);
                                    root.secondary_close_subscription = None;
                                    cx.notify();
                                }
                            });
                        })
                    });
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Live { window, view };
                            this.secondary_close_subscription = Some(close_subscription);
                            this.secondary_error = None;
                            cx.notify();
                        }
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Closed;
                            this.secondary_error = Some(format!(
                                "Could not open the retained GPUI window: {error}"
                            ));
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
    }
}

impl Render for RootWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .track_focus(&self.focus_handle)
            .key_context("RootWindow")
            .on_action(cx.listener(|this, _: &ToggleSecondaryWindow, _, cx| {
                this.open_or_toggle_secondary(cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w(px(420.))
                    .child(Label::new("Search"))
                    .child(Input::new(&self.query).aria_label("Search"))
                    .child(Label::new(
                        "Ctrl+Enter: open or dismiss the retained window",
                    ))
                    .child(
                        Button::new("retained-window")
                            .label("Open or dismiss retained window")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_or_toggle_secondary(cx);
                            })),
                    )
                    .child(
                        Button::new("quit-gpui")
                            .label("Quit Multi Launcher")
                            .on_click(|_, _, cx| cx.quit()),
                    )
                    .children(
                        self.secondary_error
                            .as_ref()
                            .map(|error| Label::new(error.clone())),
                    ),
            )
    }
}

struct SecondaryWindow {
    query: Entity<InputState>,
    native_window: Option<NativeWindow>,
    secondary_revision: Arc<AtomicU64>,
}

impl Render for SecondaryWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w(px(420.))
                    .child(Label::new("Retained window input"))
                    .child(Input::new(&self.query).aria_label("Retained window input"))
                    .child(
                        Button::new("dismiss-retained-window")
                            .label("Dismiss window")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(native_window) = this.native_window {
                                    let secondary_revision = Arc::clone(&this.secondary_revision);
                                    let revision =
                                        secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                                    cx.spawn(async move |_, _cx| {
                                        if secondary_revision.load(Ordering::Acquire) != revision {
                                            return;
                                        }
                                        if let Err(error) =
                                            set_native_window_visible(native_window, false)
                                        {
                                            tracing::warn!(%error, "could not dismiss retained GPUI window");
                                        }
                                    })
                                    .detach();
                                }
                            })),
                    ),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForegroundIdentity {
    hwnd: usize,
    process_id: u32,
    thread_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeWindow {
    identity: ForegroundIdentity,
}

#[cfg(windows)]
fn checked_native_window(window: &Window) -> Result<Option<NativeWindow>, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };

    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("GPUI did not expose a native window handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("GPUI returned a non-Windows native window handle".to_owned());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err("GPUI returned an invalid Windows window handle".to_owned());
    }

    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != std::process::id() || thread_id == 0 {
        return Err("GPUI window handle is not owned by this process".to_owned());
    }

    Ok(Some(NativeWindow {
        identity: ForegroundIdentity {
            hwnd: hwnd.0 as usize,
            process_id,
            thread_id,
        },
    }))
}

#[cfg(not(windows))]
fn checked_native_window(_: &Window) -> Result<Option<NativeWindow>, String> {
    Ok(None)
}

#[cfg(windows)]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    native_identity(hwnd)
}

#[cfg(not(windows))]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

#[cfg(windows)]
fn native_identity(hwnd: windows::Win32::Foundation::HWND) -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return None;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    (process_id != 0 && thread_id != 0).then_some(ForegroundIdentity {
        hwnd: hwnd.0 as usize,
        process_id,
        thread_id,
    })
}

#[cfg(windows)]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    let foreground = current_foreground_identity()?;
    if foreground.process_id == std::process::id() {
        return Some(foreground);
    }

    multi_launcher::active_window::capture_external_foreground_window()?;
    (current_foreground_identity() == Some(foreground)).then_some(foreground)
}

#[cfg(not(windows))]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

#[cfg(windows)]
fn native_window_is_valid(window: NativeWindow) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return false;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id == window.identity.process_id && thread_id == window.identity.thread_id
}

#[cfg(not(windows))]
fn native_window_is_valid(_: NativeWindow) -> bool {
    false
}

#[cfg(windows)]
fn native_window_is_visible(window: NativeWindow) -> Result<bool, String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindowVisible};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    Ok(unsafe { IsWindowVisible(HWND(window.identity.hwnd as *mut c_void)) }.as_bool())
}

#[cfg(not(windows))]
fn native_window_is_visible(_: NativeWindow) -> Result<bool, String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn set_native_window_visible(window: NativeWindow, visible: bool) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            IsWindowVisible, SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
        },
    };
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    let command: SHOW_WINDOW_CMD = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    // SAFETY: the HWND was captured from GPUI and revalidated against this process and thread.
    unsafe {
        let _ = ShowWindow(hwnd, command);
    }
    if !native_window_is_valid(window) || unsafe { IsWindowVisible(hwnd) }.as_bool() != visible {
        return Err("Windows did not apply the requested GPUI visibility".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_native_window_visible(_: NativeWindow, _: bool) -> Result<(), String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn activate_native_window(window: NativeWindow) -> Result<(), String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    // One guarded USER32 attempt avoids an asynchronous retry that could steal
    // focus after the foreground has changed.
    if unsafe { SetForegroundWindow(HWND(window.identity.hwnd as *mut c_void)) }.as_bool() {
        Ok(())
    } else {
        Err("Windows refused to activate the GPUI window".to_owned())
    }
}

#[cfg(not(windows))]
fn activate_native_window(_: NativeWindow) -> Result<(), String> {
    Err("native GPUI window activation is only available on Windows".to_owned())
}

#[derive(Clone)]
struct RootVisibility {
    visible: Arc<AtomicBool>,
    request_revision: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
}

impl RootVisibility {
    fn new() -> Self {
        Self {
            visible: Arc::new(AtomicBool::new(true)),
            request_revision: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    fn next_toggle(&self) -> (u64, bool) {
        let visible = !self.visible.load(Ordering::Acquire);
        let revision = self
            .request_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        self.visible.store(visible, Ordering::Release);
        (revision, visible)
    }

    fn is_current(&self, revision: u64, visible: bool) -> bool {
        self.active.load(Ordering::Acquire)
            && self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
    }

    fn revert_if_current(&self, revision: u64, visible: bool) {
        if self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
        {
            self.visible.store(!visible, Ordering::Release);
        }
    }
}

#[cfg(windows)]
fn activate_for_root_request(
    visibility: &RootVisibility,
    revision: u64,
    desired_visible: bool,
    target: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) {
    if !visibility.is_current(revision, desired_visible) || !native_window_is_valid(target) {
        return;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(target.identity) {
        return;
    }
    let Some(expected_foreground) = expected_foreground else {
        return;
    };
    if foreground != Some(expected_foreground) {
        return;
    }
    if !visibility.is_current(revision, desired_visible) {
        return;
    }
    if let Err(error) = activate_native_window(target) {
        tracing::debug!(%error, "GPUI foreground activation was refused");
    }
}

#[cfg(windows)]
fn activate_for_secondary_window(
    secondary_revision: &AtomicU64,
    revision: u64,
    window: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) {
    if secondary_revision.load(Ordering::Acquire) != revision || !native_window_is_valid(window) {
        return;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(window.identity) {
        return;
    }
    let Some(expected_foreground) = expected_foreground else {
        return;
    };
    if foreground != Some(expected_foreground)
        || secondary_revision.load(Ordering::Acquire) != revision
    {
        return;
    }
    if let Err(error) = activate_native_window(window) {
        tracing::debug!(%error, "retained GPUI window activation was refused");
    }
}

#[cfg(not(windows))]
fn activate_for_secondary_window(
    _: &AtomicU64,
    _: u64,
    _: NativeWindow,
    _: Option<ForegroundIdentity>,
) {
}

pub(crate) fn run_foundation_window(hotkey: multi_launcher::hotkey::Hotkey) -> Result<(), String> {
    let startup_error = Rc::new(RefCell::new(None));
    let startup_error_sink = Rc::clone(&startup_error);
    let subscriptions = Rc::new(RefCell::new(Vec::new()));
    let subscription_sink = Rc::clone(&subscriptions);
    let listener_slot = Arc::new(Mutex::new(None::<HotkeyListener>));
    let listener_slot_for_run = Arc::clone(&listener_slot);
    let listener_slot_for_quit = Arc::clone(&listener_slot);
    let visibility = RootVisibility::new();
    let visibility_for_quit = visibility.clone();
    let initial_return_target = capture_foreground_identity();

    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        cx.bind_keys([KeyBinding::new(
            "ctrl-enter",
            ToggleSecondaryWindow,
            Some("RootWindow"),
        )]);
        let quit_subscription = cx.on_app_quit(move |_| {
            visibility_for_quit.active.store(false, Ordering::Release);
            if let Ok(mut listener) = listener_slot_for_quit.lock()
                && let Some(mut listener) = listener.take()
            {
                listener.stop();
            }
            async {}
        });
        subscription_sink.borrow_mut().push(quit_subscription);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Foundation".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (root_window, root_view) = match open_window(options, cx, |window, cx| {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Type a query"));
            query.focus_handle(cx).focus(window, cx);
            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);
            cx.new(|_| RootWindow {
                query,
                native_window: None,
                focus_handle,
                secondary: SecondaryWindowState::Closed,
                secondary_revision: Arc::new(AtomicU64::new(0)),
                secondary_close_subscription: None,
                secondary_error: None,
            })
        }) {
            Ok((window, view)) => (window, view),
            Err(error) => {
                *startup_error_sink.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
        };

        let native_window =
            match update_kit_window_content(root_window, &root_view, cx, |root, window, cx| {
                let native_window = checked_native_window(window)?;
                root.native_window = native_window;
                root.query.focus_handle(cx).focus(window, cx);
                Ok::<_, String>(native_window)
            }) {
                Ok(Ok(native_window)) if !cfg!(windows) || native_window.is_some() => native_window,
                Ok(Ok(_)) => {
                    *startup_error_sink.borrow_mut() = Some(
                        "GPUI did not expose a checked Windows window handle for the launcher."
                            .to_owned(),
                    );
                    cx.quit();
                    return;
                }
                Ok(Err(error)) => {
                    *startup_error_sink.borrow_mut() = Some(error);
                    cx.quit();
                    return;
                }
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "GPUI could not initialize its root window: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };

        let root_id = root_window.window_id();
        subscription_sink
            .borrow_mut()
            .push(cx.on_window_closed(move |cx, closed| {
                if closed == root_id {
                    cx.quit();
                }
            }));

        #[cfg(windows)]
        {
            let Some(native_window) = native_window else {
                *startup_error_sink.borrow_mut() = Some(
                    "GPUI did not provide a Windows window handle for the launcher.".to_owned(),
                );
                cx.quit();
                return;
            };
            let (event_tx, event_rx) = channel();
            let trigger = Arc::new(HotkeyTrigger::new(hotkey));
            let listener = match HotkeyTrigger::prepare_listener(
                vec![Arc::clone(&trigger)],
                "gpui-frontend",
                event_tx,
            ) {
                Ok(listener) => listener,
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "Could not start the configured launcher hotkey: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };
            if let Ok(mut slot) = listener_slot_for_run.lock() {
                *slot = Some(listener);
            } else {
                *startup_error_sink.borrow_mut() =
                    Some("Could not initialize the GPUI hotkey owner.".to_owned());
                cx.quit();
                return;
            }
            if let Ok(slot) = listener_slot_for_run.lock()
                && let Some(listener) = slot.as_ref()
            {
                listener.activate();
            }
            visibility.active.store(true, Ordering::Release);

            let root_window = root_window;
            let root_view = root_view;
            let visibility = visibility.clone();
            cx.spawn(async move |cx| {
                let mut event_rx: Receiver<()> = event_rx;
                let mut return_target = initial_return_target;
                loop {
                    let (receiver, event) = cx
                        .background_executor()
                        .spawn(async move {
                            let event = event_rx.recv();
                            (event_rx, event)
                        })
                        .await;
                    event_rx = receiver;
                    if event.is_err() || !visibility.active.load(Ordering::Acquire) {
                        break;
                    }
                    if !trigger.take() {
                        continue;
                    }

                    let (revision, desired_visible) = visibility.next_toggle();
                    let foreground = if desired_visible {
                        cx.background_executor()
                            .spawn(async { capture_foreground_identity() })
                            .await
                    } else {
                        None
                    };
                    if !visibility.is_current(revision, desired_visible) {
                        continue;
                    }
                    let query_window = update_kit_window_content(
                        root_window,
                        &root_view,
                        &mut *cx,
                        |root, window, cx| {
                            if desired_visible {
                                root.query.focus_handle(cx).focus(window, cx);
                            }
                            root.native_window
                        },
                    );
                    if !matches!(query_window, Ok(Some(current)) if current == native_window) {
                        tracing::warn!("GPUI root window closed before hotkey presentation");
                        cx.update(|cx| cx.quit());
                        break;
                    }

                    // Sample the active window on GPUI's owner thread. A stale
                    // worker snapshot must never authorize restoring focus.
                    let foreground_before_transition = current_foreground_identity();
                    let should_restore = !desired_visible
                        && foreground_before_transition == Some(native_window.identity);
                    let show_request_is_current = desired_visible
                        && foreground
                            .is_some_and(|captured| Some(captured) == foreground_before_transition);
                    if desired_visible {
                        return_target = foreground
                            .filter(|captured| Some(*captured) == foreground_before_transition)
                            .filter(|captured| captured.hwnd != native_window.identity.hwnd);
                    } else if !should_restore {
                        return_target = None;
                    }

                    if let Err(error) = set_native_window_visible(native_window, desired_visible) {
                        visibility.revert_if_current(revision, desired_visible);
                        tracing::error!(%error, "could not change GPUI root window visibility");
                        continue;
                    }
                    // Query the actual post-ShowWindow foreground on the GPUI owner thread.
                    // The activation worker will only proceed while this identity is still current.
                    let foreground_after_transition = current_foreground_identity();
                    let show_request_is_current = show_request_is_current
                        && foreground_after_transition == foreground_before_transition;

                    let target = if desired_visible {
                        show_request_is_current.then_some(native_window)
                    } else if should_restore
                        && foreground_after_transition == Some(native_window.identity)
                    {
                        return_target.map(|identity| NativeWindow { identity })
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        let expected_foreground = foreground_after_transition;
                        let visibility = visibility.clone();
                        cx.background_executor()
                            .spawn(async move {
                                activate_for_root_request(
                                    &visibility,
                                    revision,
                                    desired_visible,
                                    target,
                                    expected_foreground,
                                );
                            })
                            .await;
                    }
                }
            })
            .detach();
        }

        #[cfg(not(windows))]
        {
            let _ = (native_window, hotkey, initial_return_target);
        }
    });

    if let Ok(mut listener) = listener_slot.lock()
        && let Some(mut listener) = listener.take()
    {
        listener.stop();
    }

    match startup_error.borrow_mut().take() {
        Some(error) => Err(format!(
            "GPUI could not open its window: {error}. Restart with --ui=egui to use the fallback frontend."
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Frontend, selection_from_args};
    use std::ffi::OsString;

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|arg| OsString::from(*arg)).collect()
    }

    #[test]
    fn defaults_to_egui_and_preserves_other_arguments() {
        assert_eq!(
            selection_from_args(args(&["--open", "settings"])),
            Ok(Frontend::Egui)
        );
    }

    #[test]
    fn selects_each_explicit_frontend() {
        assert_eq!(
            selection_from_args(args(&["--ui=egui"])),
            Ok(Frontend::Egui)
        );
        assert_eq!(
            selection_from_args(args(&["--ui=gpui"])),
            Ok(Frontend::Gpui)
        );
    }

    #[test]
    fn rejects_invalid_and_repeated_frontend_selectors() {
        assert!(selection_from_args(args(&["--ui=unknown"])).is_err());
        assert!(selection_from_args(args(&["--ui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=gpui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=egui"])).is_err());
    }
}
