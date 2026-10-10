//! Opt-in Windows smoke fixture for the production coordinate-tool runtimes.
//! Run interactively: `cargo run --bin coordinate_tool_smoke`.

#[cfg(windows)]
fn main() {
    if let Err(error) = smoke::run() {
        eprintln!("coordinate-tool-smoke: FAIL fixture: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    println!("coordinate-tool-smoke: UNVERIFIED platform: native smoke requires Windows");
}

#[cfg(windows)]
mod smoke {
    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use multi_launcher::coordinate_tool::{
        CaptureOutcome, CapturePhase, CaptureSessionId, CaptureStatus, CoordinateCaptureController,
        CoordinateEffectsStatus, CoordinateRenderFrame, CoordinateRuntimeFactory, CoordinateSample,
        CoordinateSampler, CoordinateSpace, CoordinateSurfaceBackend, CoordinateToolController,
        ForegroundClientGeometry, MonitorGeometry, MonitorId, NativeCoordinateCaptureRuntime,
        NativeCoordinateRuntimeFactory, PhysicalPoint, PhysicalRect, format_coordinate,
    };
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW,
        MonitorFromPoint, UpdateWindow,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
        MDT_EFFECTIVE_DPI, SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput, VIRTUAL_KEY, VK_A,
        VK_CONTROL, VK_ESCAPE, VK_F24, VK_LBUTTON, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClassNameW,
        GetClientRect, GetForegroundWindow, GetMessageW, GetSystemMetrics,
        GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, RegisterClassW,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW,
        SetCursorPos, SetForegroundWindow, ShowWindow, TranslateMessage, WM_APP, WM_CHAR, WM_CLOSE,
        WM_DESTROY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_SYSKEYDOWN,
        WM_SYSKEYUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::w;

    const TIMEOUT: Duration = Duration::from_secs(5);
    const POLL: Duration = Duration::from_millis(8);
    const SURFACE_CLASS: &str = "MultiLauncherCoordinatePassiveSurface";
    const WM_FENCE: u32 = WM_APP + 0x61;

    #[derive(Clone, Copy)]
    struct Target {
        hwnd: HWND,
        pid: u32,
    }

    #[derive(Default)]
    struct Counts {
        sampler_creations: AtomicUsize,
        backend_creations: AtomicUsize,
        samples: AtomicUsize,
        stationary_refreshes: AtomicUsize,
        renders: AtomicUsize,
        shutdowns: AtomicUsize,
    }

    struct CountingFactory(Arc<Counts>);
    struct CountingSampler(Box<dyn CoordinateSampler>, Arc<Counts>);
    struct CountingBackend(Box<dyn CoordinateSurfaceBackend>, Arc<Counts>);

    impl CoordinateRuntimeFactory for CountingFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            self.0.sampler_creations.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(CountingSampler(
                NativeCoordinateRuntimeFactory.create_sampler()?,
                Arc::clone(&self.0),
            )))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            self.0.backend_creations.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(CountingBackend(
                NativeCoordinateRuntimeFactory.create_backend()?,
                Arc::clone(&self.0),
            )))
        }
    }

    impl CoordinateSampler for CountingSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            self.1.samples.fetch_add(1, Ordering::AcqRel);
            self.0.sample()
        }
    }

    impl CoordinateSurfaceBackend for CountingBackend {
        fn poll_events(&mut self) -> Result<bool, String> {
            self.0.poll_events()
        }
        fn refresh_stationary_sources(
            &mut self,
            frame: &CoordinateRenderFrame,
        ) -> Result<(), String> {
            self.1.stationary_refreshes.fetch_add(1, Ordering::AcqRel);
            self.0.refresh_stationary_sources(frame)
        }
        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            self.1.renders.fetch_add(1, Ordering::AcqRel);
            self.0.render(frame)
        }
        fn effects_status(&self) -> CoordinateEffectsStatus {
            self.0.effects_status()
        }
        fn shutdown(&mut self) -> Result<(), String> {
            self.1.shutdowns.fetch_add(1, Ordering::AcqRel);
            self.0.shutdown()
        }
    }

    struct DpiScope(DPI_AWARENESS_CONTEXT);
    impl DpiScope {
        fn enter() -> Result<Self, String> {
            let old =
                unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if old.0.is_null() {
                Err("could not establish physical per-monitor-aware coordinates".into())
            } else {
                Ok(Self(old))
            }
        }
    }
    impl Drop for DpiScope {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }

    struct Receiver {
        child: Child,
        reader: Option<thread::JoinHandle<()>>,
        output: mpsc::Receiver<String>,
        pending: VecDeque<String>,
        target: Target,
        fence_id: u64,
    }

    impl Receiver {
        fn start() -> Result<Self, String> {
            let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                .arg("--receiver")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|e| format!("start receiver child: {e}"))?;
            let stdout = match child.stdout.take() {
                Some(stdout) => stdout,
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("receiver stdout was not piped".into());
                }
            };
            let (tx, output) = mpsc::channel();
            let reader = thread::Builder::new()
                .name("coordinate-smoke-output".into())
                .spawn(move || {
                    for line in BufReader::new(stdout).lines().flatten() {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                });
            let reader = match reader {
                Ok(reader) => reader,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("start receiver output reader: {error}"));
                }
            };
            let mut receiver = Self {
                child,
                reader: Some(reader),
                output,
                pending: VecDeque::new(),
                target: Target {
                    hwnd: HWND::default(),
                    pid: 0,
                },
                fence_id: 0,
            };
            let ready = receiver.wait_line(|line| line.starts_with("READY "))?;
            let hwnd = field(&ready, "hwnd")
                .ok_or("receiver READY omitted HWND")?
                .parse::<isize>()
                .map_err(|e| format!("parse receiver HWND: {e}"))?;
            let pid = field(&ready, "pid")
                .ok_or("receiver READY omitted PID")?
                .parse::<u32>()
                .map_err(|e| format!("parse receiver PID: {e}"))?;
            receiver.target = Target {
                hwnd: HWND(hwnd as *mut _),
                pid,
            };
            if receiver.target.hwnd.0.is_null()
                || !unsafe { IsWindow(receiver.target.hwnd) }.as_bool()
            {
                return Err("receiver reported an invalid HWND".into());
            }
            let mut actual_pid = 0;
            unsafe { GetWindowThreadProcessId(receiver.target.hwnd, Some(&mut actual_pid)) };
            if actual_pid != pid || pid == std::process::id() {
                return Err(format!("receiver HWND/PID mismatch: {actual_pid}/{pid}"));
            }
            let deadline = Instant::now() + TIMEOUT;
            while !foreground_is(receiver.target) {
                if Instant::now() >= deadline {
                    return Err("could not establish receiver foreground HWND/PID".into());
                }
                thread::sleep(POLL);
            }
            Ok(receiver)
        }

        fn wait_line(&mut self, matches: impl Fn(&str) -> bool) -> Result<String, String> {
            if let Some(index) = self.pending.iter().position(|line| matches(line)) {
                return self
                    .pending
                    .remove(index)
                    .ok_or_else(|| "receiver output queue changed".to_owned());
            }
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err("timed out waiting for receiver acknowledgment".into());
                }
                match self
                    .output
                    .recv_timeout(left.min(Duration::from_millis(100)))
                {
                    Ok(line) if matches(&line) => return Ok(line),
                    Ok(line) if line.starts_with("ERROR ") => return Err(line),
                    Ok(line) => self.pending.push_back(line),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                            return Err(format!("receiver exited early: {status}"));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("receiver output closed before acknowledgment".into());
                    }
                }
            }
        }

        fn event(&mut self, name: &str) -> Result<(), String> {
            let expected = format!("EVENT {name}");
            self.wait_line(|line| line == expected).map(|_| ())
        }

        fn fence(&mut self) -> Result<(usize, usize, usize, usize, usize), String> {
            self.fence_id += 1;
            let id = self.fence_id;
            if unsafe { PostMessageW(self.target.hwnd, WM_FENCE, WPARAM(id as usize), LPARAM(0)) }
                .is_err()
            {
                return Err("could not post receiver message fence".into());
            }
            let prefix = format!("FENCE id={id} ");
            let line = self.wait_line(|line| line.starts_with(&prefix))?;
            Ok((
                number(&line, "left_down")?,
                number(&line, "left_up")?,
                number(&line, "key_down")?,
                number(&line, "key_up")?,
                number(&line, "text")?,
            ))
        }

        fn close(&mut self) -> Result<(), String> {
            if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                self.join_reader();
                return Ok(());
            }
            if self.target.hwnd.0.is_null() {
                self.child.kill().map_err(|e| e.to_string())?;
                let _ = self.child.wait();
                self.join_reader();
                return Ok(());
            }
            let _ = unsafe { PostMessageW(self.target.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    self.join_reader();
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    self.child.kill().map_err(|e| e.to_string())?;
                    let _ = self.child.wait();
                    self.join_reader();
                    return Err(
                        "receiver did not close before its deadline; child terminated".into(),
                    );
                }
                thread::sleep(POLL);
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
    impl Drop for Receiver {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    #[derive(Clone, Copy)]
    struct PriorDesktop {
        hwnd: HWND,
        pid: u32,
        cursor: PhysicalPoint,
    }

    struct DesktopRestore {
        target: Target,
        prior: PriorDesktop,
        last_cursor: Option<PhysicalPoint>,
    }
    impl DesktopRestore {
        fn save_prior() -> Result<PriorDesktop, String> {
            let mut cursor = POINT::default();
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor) }
                .map_err(|e| format!("save cursor: {e}"))?;
            let hwnd = unsafe { GetForegroundWindow() };
            let mut pid = 0;
            if !hwnd.0.is_null() {
                unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            }
            Ok(PriorDesktop {
                hwnd,
                pid,
                cursor: PhysicalPoint::new(cursor.x, cursor.y),
            })
        }
        fn new(target: Target, prior: PriorDesktop) -> Self {
            Self {
                target,
                prior,
                last_cursor: None,
            }
        }
        fn move_to(&mut self, point: PhysicalPoint) -> Result<(), String> {
            require_foreground(self.target)?;
            unsafe { SetCursorPos(point.x, point.y) }.map_err(|e| format!("SetCursorPos: {e}"))?;
            self.last_cursor = Some(point);
            Ok(())
        }
    }
    impl Drop for DesktopRestore {
        fn drop(&mut self) {
            if !foreground_is(self.target) {
                return;
            }
            if let Some(last) = self.last_cursor {
                let mut cursor = POINT::default();
                if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor) }
                    .is_ok()
                    && cursor.x == last.x
                    && cursor.y == last.y
                {
                    let _ = unsafe { SetCursorPos(self.prior.cursor.x, self.prior.cursor.y) };
                }
            }
            if !self.prior.hwnd.0.is_null() && unsafe { IsWindow(self.prior.hwnd) }.as_bool() {
                let mut pid = 0;
                unsafe { GetWindowThreadProcessId(self.prior.hwnd, Some(&mut pid)) };
                if pid == self.prior.pid {
                    let _ = unsafe { SetForegroundWindow(self.prior.hwnd) };
                }
            }
        }
    }

    pub(super) fn run() -> Result<(), String> {
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|arg| arg == "--cursor-effects-auto") {
            return cursor_effects::run(true);
        }
        if args.iter().any(|arg| arg == "--cursor-effects") {
            return cursor_effects::run(false);
        }
        if args.iter().any(|arg| arg == "--coordinate-profile") {
            return coordinate_profile::run();
        }
        let _dpi = DpiScope::enter()?;
        if args.iter().any(|arg| arg == "--receiver") {
            return run_receiver();
        }
        run_smoke()
    }

    fn run_smoke() -> Result<(), String> {
        let prior_desktop = DesktopRestore::save_prior()?;
        let mut receiver = Receiver::start()?;
        let mut restore = DesktopRestore::new(receiver.target, prior_desktop);
        println!(
            "coordinate-tool-smoke: PASS receiver: separate child HWND={:#x} PID={}",
            receiver.target.hwnd.0 as usize, receiver.target.pid
        );

        let counts = Arc::new(Counts::default());
        let mut passive =
            CoordinateToolController::new(Arc::new(CountingFactory(Arc::clone(&counts))));
        let mut capture =
            CoordinateCaptureController::new(Arc::new(NativeCoordinateCaptureRuntime));
        let result = exercise(
            &mut receiver,
            &mut restore,
            &mut passive,
            &mut capture,
            &counts,
        );

        let cleanup_capture = drain_capture(&mut capture);
        let cleanup_passive = passive.shutdown();
        let (total, visible) = passive_windows(std::process::id());
        let samples = counts.samples.load(Ordering::Acquire);
        if cleanup_capture.is_ok() && cleanup_passive.is_ok() && !passive.is_running() && total == 0
        {
            println!(
                "coordinate-tool-smoke: PASS cleanup: capture joined, passive HWNDs={total}/{visible}, passive samples={samples}"
            );
        } else {
            println!(
                "coordinate-tool-smoke: FAIL cleanup: capture={cleanup_capture:?}, passive={cleanup_passive:?}, running={}, HWNDs={total}/{visible}",
                passive.is_running()
            );
        }
        println!(
            "coordinate-tool-smoke: UNVERIFIED native-clipboard-copy: preserving arbitrary Windows clipboard formats was not attempted; clipboard left untouched"
        );
        println!(
            "coordinate-tool-smoke: UNVERIFIED escape-clipboard-sentinel: clipboard left untouched by this fixture; app sentinel behavior remains in focused tests"
        );
        println!(
            "coordinate-tool-smoke: UNVERIFIED gui-clipboard-adapter: standalone fixture uses public controllers; crate-private GUI adapter requires app-level/manual smoke"
        );
        println!(
            "coordinate-tool-smoke: UNVERIFIED mixed-dpi: actual mixed-DPI hardware was not observed by this one-monitor geometry sample"
        );

        drop(restore);
        let close = receiver.close();
        if let Err(ref error) = close {
            println!("coordinate-tool-smoke: FAIL receiver cleanup: {error}");
        }
        result?;
        cleanup_capture?;
        cleanup_passive?;
        close?;
        Ok(())
    }

    fn exercise(
        receiver: &mut Receiver,
        restore: &mut DesktopRestore,
        passive: &mut CoordinateToolController,
        capture: &mut CoordinateCaptureController,
        counts: &Arc<Counts>,
    ) -> Result<(), String> {
        let point = client_center(receiver.target.hwnd)?;
        restore.move_to(point)?;
        if counts.samples.load(Ordering::Acquire) != 0 || passive_windows(std::process::id()).0 != 0
        {
            return Err("passive runtime worked while both modes began disabled".into());
        }
        normal_input(receiver)?;
        println!(
            "coordinate-tool-smoke: PASS baseline-input: guarded click and A/text reached receiver"
        );

        for (label, hud, crosshair, visible) in [
            ("hud-only", true, false, 1),
            ("crosshair-only", false, true, 1),
            ("hud-and-crosshair", true, true, 2),
        ] {
            passive.set_hud_enabled(hud)?;
            passive.set_crosshair_enabled(crosshair)?;
            let sample = wait_sample(passive, point)?;
            wait_surface_count(visible)?;
            if counts.sampler_creations.load(Ordering::Acquire) == 0
                || counts.backend_creations.load(Ordering::Acquire) == 0
                || counts.samples.load(Ordering::Acquire) == 0
                || counts.renders.load(Ordering::Acquire) == 0
            {
                return Err(format!(
                    "{label}: public runtime counters did not observe active sampling/rendering"
                ));
            }
            if label == "hud-only" {
                let geometry = compare_geometry(&sample, receiver.target, point)?;
                println!(
                    "coordinate-tool-smoke: PASS signed-native-geometry: desktop=({},{}), monitor-origin=({},{}), client-origin=({},{}), direct Win32 bounds agree",
                    sample.desktop_point.x,
                    sample.desktop_point.y,
                    geometry.monitor.bounds.left(),
                    geometry.monitor.bounds.top(),
                    geometry.client.origin.x,
                    geometry.client.origin.y
                );
                if geometry.monitor.bounds.left() < 0 || geometry.monitor.bounds.top() < 0 {
                    println!(
                        "coordinate-tool-smoke: PASS negative-monitor-origin: observed monitor origin ({},{})",
                        geometry.monitor.bounds.left(),
                        geometry.monitor.bounds.top()
                    );
                } else {
                    println!(
                        "coordinate-tool-smoke: UNVERIFIED negative-monitor-origin: this desktop exposed no negative target-monitor origin"
                    );
                }
            }
            let before = (
                counts.sampler_creations.load(Ordering::Acquire),
                counts.backend_creations.load(Ordering::Acquire),
                passive_windows(std::process::id()),
            );
            passive.set_hud_enabled(hud)?;
            passive.set_crosshair_enabled(crosshair)?;
            let after = (
                counts.sampler_creations.load(Ordering::Acquire),
                counts.backend_creations.load(Ordering::Acquire),
                passive_windows(std::process::id()),
            );
            if !passive.is_running()
                || before.0 != after.0
                || before.1 != after.1
                || before.2 != after.2
                || after.2.0 != 4
                || after.2.1 != visible
            {
                return Err(format!(
                    "{label}: repeated enable changed runtime or expected visible surface count: {before:?} -> {after:?}"
                ));
            }
            require_foreground(receiver.target)?;
            normal_input(receiver)?;
            println!(
                "coordinate-tool-smoke: PASS {label}: repeated enable kept {} visible surface(s); click/typing did not change receiver foreground",
                visible
            );
        }

        let before_pick = receiver.fence()?;
        let id = capture.begin()?;
        wait_phase(capture, id, CapturePhase::WaitingForFreshClick)?;
        {
            let mut owned_left = HeldLeft::new(receiver.target);
            owned_left.press()?;
            wait_phase(capture, id, CapturePhase::Capturing)?;
            if capture.status().is_none_or(|status| {
                status.session_id != id
                    || status.phase != CapturePhase::Capturing
                    || status.outcome.is_some()
            }) {
                return Err("capture completed while the owned left button was still down".into());
            }
            owned_left.release()?;
        }
        let status = wait_result(capture, id)?;
        let sample = match status.outcome {
            Some(CaptureOutcome::Captured(sample)) => sample,
            other => {
                return Err(format!(
                    "left click did not complete with a sample: {other:?}"
                ));
            }
        };
        if sample.desktop_point != point {
            return Err(format!(
                "captured click sample {:?} differs from click point {point:?}",
                sample.desktop_point
            ));
        }
        compare_geometry(&sample, receiver.target, point)?;
        send_key(receiver.target, VK_F24)?;
        receiver.event("key_down")?;
        receiver.event("key_up")?;
        let after_pick = receiver.fence()?;
        if after_pick.0 != before_pick.0 || after_pick.1 != before_pick.1 {
            return Err(format!(
                "captured click reached receiver before key fence: {before_pick:?} -> {after_pick:?}"
            ));
        }
        let state_during_pick = passive.runtime_state();
        if !state_during_pick.hud_enabled()
            || !state_during_pick.crosshair_enabled()
            || passive_windows(std::process::id()) != (4, 2)
        {
            return Err("HUD/crosshair state or surfaces changed during one-shot capture".into());
        }
        let formatted = format_coordinate(&sample, CoordinateSpace::Desktop)
            .map_err(|e| format!("format click-time coordinate: {e:?}"))?;
        println!(
            "coordinate-tool-smoke: PASS captured-click: separate down/capturing/up transitions were suppressed; click-time sample={} matched native geometry and post-cleanup key fence saw no receiver click",
            formatted.text
        );

        passive.set_hud_enabled(false)?;
        passive.set_crosshair_enabled(false)?;
        if passive.is_running() || passive_windows(std::process::id()) != (0, 0) {
            return Err(
                "passive windows or worker remained after both toggles were disabled".into(),
            );
        }
        if counts.shutdowns.load(Ordering::Acquire)
            != counts.backend_creations.load(Ordering::Acquire)
        {
            return Err("passive backend shutdown count did not match created backends".into());
        }
        let samples_off = counts.samples.load(Ordering::Acquire);
        println!(
            "coordinate-tool-smoke: PASS capture-coexistence: click interception left HUD and crosshair independently enabled and visible"
        );

        require_plain_escape_state()?;
        let before_escape = receiver.fence()?;
        let id = capture.begin()?;
        wait_phase(capture, id, CapturePhase::WaitingForFreshClick)?;
        send_key(receiver.target, VK_ESCAPE)?;
        let status = wait_result(capture, id)?;
        if !matches!(status.outcome, Some(CaptureOutcome::Cancelled)) {
            return Err(format!(
                "Escape did not cancel capture: {:?}",
                status.outcome
            ));
        }
        send_key(receiver.target, VK_F24)?;
        receiver.event("key_down")?;
        receiver.event("key_up")?;
        let after_escape = receiver.fence()?;
        if after_escape.2 != before_escape.2 + 1 || after_escape.3 != before_escape.3 + 1 {
            return Err(format!(
                "Escape reached the receiver before its key fence: {before_escape:?} -> {after_escape:?}"
            ));
        }
        println!(
            "coordinate-tool-smoke: PASS escape-cancel: Escape pair suppressed and session cleanup completed; clipboard untouched"
        );

        let before = receiver.fence()?;
        normal_input(receiver)?;
        let after = receiver.fence()?;
        if after.0 != before.0 + 1 || after.1 != before.1 + 1 || after.4 != before.4 + 1 {
            return Err(format!(
                "normal input did not return after capture: {before:?} -> {after:?}"
            ));
        }
        println!(
            "coordinate-tool-smoke: PASS post-capture-input: normal click and typed text reached receiver"
        );
        if counts.samples.load(Ordering::Acquire) != samples_off {
            return Err("passive sampling continued while all passive modes were off".into());
        }
        println!(
            "coordinate-tool-smoke: PASS passive-idle: controller joined; no passive HWNDs or sample calls while off"
        );
        Ok(())
    }

    struct Geometry {
        monitor: MonitorGeometry,
        client: ForegroundClientGeometry,
    }

    fn compare_geometry(
        sample: &CoordinateSample,
        target: Target,
        point: PhysicalPoint,
    ) -> Result<Geometry, String> {
        if cursor()? != point {
            return Err("cursor moved while comparing native geometry".into());
        }
        let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        let virtual_bounds = PhysicalRect::new(left, top, left + width, top + height)
            .ok_or("invalid virtual desktop bounds")?;
        let monitor_handle = unsafe {
            MonitorFromPoint(
                POINT {
                    x: point.x,
                    y: point.y,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        };
        if monitor_handle.0.is_null() {
            return Err("MonitorFromPoint returned null".into());
        }
        let mut info = MONITORINFOEXW {
            monitorInfo: windows::Win32::Graphics::Gdi::MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor_handle, &mut info.monitorInfo) }.as_bool() {
            return Err("GetMonitorInfoW failed for target monitor".into());
        }
        let bounds = rect(info.monitorInfo.rcMonitor)?;
        let work_area = rect(info.monitorInfo.rcWork)?;
        let name_end = info
            .szDevice
            .iter()
            .position(|u| *u == 0)
            .unwrap_or(info.szDevice.len());
        let mut name = String::from_utf16_lossy(&info.szDevice[..name_end]);
        if name.is_empty() {
            name = format!("monitor:{:x}", monitor_handle.0 as usize);
        }
        let mut dpi_x = 0;
        let mut dpi_y = 0;
        let dpi =
            unsafe { GetDpiForMonitor(monitor_handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
                .ok()
                .filter(|_| dpi_x > 0 && dpi_y > 0)
                .map(|_| (dpi_x, dpi_y));
        let monitor = MonitorGeometry {
            id: MonitorId::new(name),
            bounds,
            work_area,
            effective_dpi: dpi,
        };

        let mut client_rect = RECT::default();
        unsafe { GetClientRect(target.hwnd, &mut client_rect) }.map_err(|e| e.to_string())?;
        let mut origin = POINT {
            x: client_rect.left,
            y: client_rect.top,
        };
        let mut bottom_right = POINT {
            x: client_rect.right,
            y: client_rect.bottom,
        };
        if !unsafe { ClientToScreen(target.hwnd, &mut origin) }.as_bool()
            || !unsafe { ClientToScreen(target.hwnd, &mut bottom_right) }.as_bool()
        {
            return Err("ClientToScreen failed for receiver client bounds".into());
        }
        let client = ForegroundClientGeometry::new(
            PhysicalPoint::new(origin.x, origin.y),
            PhysicalRect::new(origin.x, origin.y, bottom_right.x, bottom_right.y),
        );
        if sample.desktop_point != point
            || sample.virtual_desktop_bounds != Some(virtual_bounds)
            || sample.monitor.as_ref().is_none_or(|s| {
                s.id != monitor.id
                    || s.bounds != monitor.bounds
                    || s.work_area != monitor.work_area
                    || s.effective_dpi != monitor.effective_dpi
            })
            || sample.foreground_client != Some(client)
        {
            return Err(format!(
                "sample/direct geometry mismatch: sample={sample:?}, direct monitor={monitor:?}, client={client:?}"
            ));
        }
        Ok(Geometry { monitor, client })
    }

    fn rect(r: RECT) -> Result<PhysicalRect, String> {
        PhysicalRect::new(r.left, r.top, r.right, r.bottom)
            .ok_or_else(|| "invalid native rectangle".into())
    }

    fn wait_sample(
        controller: &CoordinateToolController,
        point: PhysicalPoint,
    ) -> Result<CoordinateSample, String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Ok(sample) = controller.sample_for_copy() {
                if sample.desktop_point == point {
                    return Ok(sample);
                }
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for passive sample".into());
            }
            thread::sleep(POLL);
        }
    }

    fn wait_surface_count(visible: usize) -> Result<(), String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if passive_windows(std::process::id()) == (4, visible) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("passive HWND count did not reach (4,{visible})"));
            }
            thread::sleep(POLL);
        }
    }

    fn wait_phase(
        controller: &mut CoordinateCaptureController,
        id: CaptureSessionId,
        phase: CapturePhase,
    ) -> Result<(), String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            controller.poll();
            if let Some(status) = controller.status() {
                if status.session_id == id && status.phase == phase {
                    return Ok(());
                }
                if status.session_id == id && status.phase == CapturePhase::Completed {
                    return Err(format!(
                        "capture completed before phase {phase:?}: {status:?}"
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(format!("timed out waiting for capture phase {phase:?}"));
            }
            thread::sleep(POLL);
        }
    }

    fn wait_result(
        controller: &mut CoordinateCaptureController,
        id: CaptureSessionId,
    ) -> Result<CaptureStatus, String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            controller.poll();
            while let Some(status) = controller.take_completed() {
                if status.session_id == id {
                    return Ok(status);
                }
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for capture cleanup/result".into());
            }
            thread::sleep(POLL);
        }
    }

    fn drain_capture(controller: &mut CoordinateCaptureController) -> Result<(), String> {
        if !controller.is_active() {
            while controller.take_completed().is_some() {}
            return Ok(());
        }
        controller.request_shutdown();
        let deadline = Instant::now() + TIMEOUT;
        while controller.is_active() {
            controller.poll();
            if Instant::now() >= deadline {
                return Err("capture shutdown did not drain before deadline".into());
            }
            thread::sleep(POLL);
        }
        while controller.take_completed().is_some() {}
        Ok(())
    }

    fn normal_input(receiver: &mut Receiver) -> Result<(), String> {
        send_click(receiver.target)?;
        receiver.event("left_down")?;
        receiver.event("left_up")?;
        send_key(receiver.target, VK_A)?;
        receiver.event("key_down")?;
        receiver.event("key_up")?;
        receiver.event("text")?;
        require_foreground(receiver.target)
    }

    fn send_click(target: Target) -> Result<(), String> {
        let mut held = HeldLeft::new(target);
        held.press()?;
        held.release()
    }

    fn send_key(target: Target, key: VIRTUAL_KEY) -> Result<(), String> {
        require_foreground(target)?;
        if unsafe { GetAsyncKeyState(key.0 as i32) } < 0 {
            return Err(format!(
                "refusing key injection while virtual key {} is physically held",
                key.0
            ));
        }
        let events = [key_input(key, false), key_input(key, true)];
        let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) } as usize;
        if sent == 1 {
            require_foreground(target).map_err(|_| {
                "key down was inserted but receiver focus changed; refusing key-up injection"
                    .to_owned()
            })?;
            let release = [key_input(key, true)];
            let released = unsafe { SendInput(&release, std::mem::size_of::<INPUT>() as i32) };
            if released != 1 {
                return Err("key down was inserted but matching key-up could not be sent".into());
            }
            return Err(
                "SendInput accepted only key-down; matching key-up cleanup was sent".into(),
            );
        }
        if sent != events.len() {
            return Err(format!("SendInput inserted {sent}/2 events for key pair"));
        }
        require_foreground(target)
    }

    struct HeldLeft {
        target: Target,
        held: bool,
    }

    impl HeldLeft {
        fn new(target: Target) -> Self {
            Self {
                target,
                held: false,
            }
        }

        fn press(&mut self) -> Result<(), String> {
            require_foreground(self.target)?;
            if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0 {
                return Err(
                    "refusing fixture left-down while the physical left button is already held"
                        .into(),
                );
            }
            let down = [mouse_input(true)];
            let sent = unsafe { SendInput(&down, std::mem::size_of::<INPUT>() as i32) };
            if sent != 1 {
                return Err(format!("SendInput inserted {sent}/1 left-down event"));
            }
            self.held = true;
            require_foreground(self.target).map_err(|_| {
                "receiver focus changed after left-down; cleanup releases only if exact receiver foreground returns".into()
            })
        }

        fn release(&mut self) -> Result<(), String> {
            if !self.held {
                return Ok(());
            }
            require_foreground(self.target)?;
            let up = [mouse_input(false)];
            let sent = unsafe { SendInput(&up, std::mem::size_of::<INPUT>() as i32) };
            if sent != 1 {
                return Err(format!(
                    "SendInput inserted {sent}/1 matching left-up event"
                ));
            }
            self.held = false;
            require_foreground(self.target)
        }
    }

    impl Drop for HeldLeft {
        fn drop(&mut self) {
            if self.held && foreground_is(self.target) {
                let up = [mouse_input(false)];
                if unsafe { SendInput(&up, std::mem::size_of::<INPUT>() as i32) } == 1 {
                    self.held = false;
                }
            }
        }
    }

    fn require_foreground(target: Target) -> Result<(), String> {
        if foreground_is(target) {
            Ok(())
        } else {
            Err(format!(
                "refusing injection; receiver HWND={:#x} PID={} is not exact foreground",
                target.hwnd.0 as usize, target.pid
            ))
        }
    }

    fn require_plain_escape_state() -> Result<(), String> {
        if [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0)
        {
            return Err("refusing Escape smoke input while a physical modifier is held".into());
        }
        if unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0 {
            return Err("refusing Escape smoke input while Escape is already held".into());
        }
        Ok(())
    }

    fn foreground_is(target: Target) -> bool {
        let hwnd = unsafe { GetForegroundWindow() };
        let mut pid = 0;
        if !hwnd.0.is_null() {
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        }
        hwnd == target.hwnd && pid == target.pid
    }

    fn key_input(key: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn mouse_input(down: bool) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: if down {
                        MOUSEEVENTF_LEFTDOWN
                    } else {
                        MOUSEEVENTF_LEFTUP
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn client_center(hwnd: HWND) -> Result<PhysicalPoint, String> {
        let mut r = RECT::default();
        unsafe { GetClientRect(hwnd, &mut r) }.map_err(|e| e.to_string())?;
        let mut p = POINT {
            x: (r.left + r.right) / 2,
            y: (r.top + r.bottom) / 2,
        };
        if !unsafe { ClientToScreen(hwnd, &mut p) }.as_bool() {
            return Err("ClientToScreen failed".into());
        }
        Ok(PhysicalPoint::new(p.x, p.y))
    }

    fn cursor() -> Result<PhysicalPoint, String> {
        let mut p = POINT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) }
            .map_err(|e| e.to_string())?;
        Ok(PhysicalPoint::new(p.x, p.y))
    }

    fn passive_windows(pid: u32) -> (usize, usize) {
        struct Scan {
            pid: u32,
            total: usize,
            visible: usize,
        }
        unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
            let scan = unsafe { &mut *(data.0 as *mut Scan) };
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == scan.pid {
                let mut name = [0_u16; 128];
                let n = unsafe { GetClassNameW(hwnd, &mut name) };
                if n > 0 && String::from_utf16_lossy(&name[..n as usize]) == SURFACE_CLASS {
                    scan.total += 1;
                    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
                        scan.visible += 1;
                    }
                }
            }
            BOOL(1)
        }
        let mut scan = Scan {
            pid,
            total: 0,
            visible: 0,
        };
        let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut scan as *mut _ as isize)) };
        (scan.total, scan.visible)
    }

    fn run_receiver() -> Result<(), String> {
        let instance = windows::Win32::Foundation::HINSTANCE(
            unsafe { GetModuleHandleW(None) }
                .map_err(|e| e.to_string())?
                .0,
        );
        let class = WNDCLASSW {
            lpfnWndProc: Some(receiver_proc),
            hInstance: instance,
            lpszClassName: w!("MultiLauncherCoordinateSmokeReceiver"),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err("RegisterClassW failed".into());
        }
        let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width < 480 || height < 360 {
            return Err(format!("virtual desktop too small: {width}x{height}"));
        }
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("MultiLauncherCoordinateSmokeReceiver"),
                w!("Coordinate tool smoke receiver"),
                WS_OVERLAPPEDWINDOW,
                left + 16,
                top + 16,
                560.min(width - 32),
                400.min(height - 32),
                None,
                None,
                instance,
                None,
            )
        }
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
            let _ = SetForegroundWindow(hwnd);
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        println!("READY hwnd={} pid={pid}", hwnd.0 as isize);
        let _ = std::io::stdout().flush();
        loop {
            let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            let result = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
            if result <= 0 || msg.message == WM_QUIT {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    #[derive(Default)]
    struct InputCounts {
        left_down: usize,
        left_up: usize,
        key_down: usize,
        key_up: usize,
        text: usize,
    }
    thread_local! { static INPUT_COUNTS: std::cell::RefCell<InputCounts> = std::cell::RefCell::new(InputCounts::default()); }

    unsafe extern "system" fn receiver_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if message == WM_FENCE {
            INPUT_COUNTS.with(|counts| {
                let c = counts.borrow();
                println!(
                    "FENCE id={} left_down={} left_up={} key_down={} key_up={} text={}",
                    wparam.0, c.left_down, c.left_up, c.key_down, c.key_up, c.text
                );
                let _ = std::io::stdout().flush();
            });
            return windows::Win32::Foundation::LRESULT(0);
        }
        let event = match message {
            WM_LBUTTONDOWN => Some("left_down"),
            WM_LBUTTONUP => Some("left_up"),
            WM_KEYDOWN | WM_SYSKEYDOWN => Some("key_down"),
            WM_KEYUP | WM_SYSKEYUP => Some("key_up"),
            WM_CHAR => Some("text"),
            WM_DESTROY => {
                unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
                None
            }
            _ => None,
        };
        if let Some(event) = event {
            INPUT_COUNTS.with(|counts| {
                let mut c = counts.borrow_mut();
                match event {
                    "left_down" => c.left_down += 1,
                    "left_up" => c.left_up += 1,
                    "key_down" => c.key_down += 1,
                    "key_up" => c.key_up += 1,
                    "text" => c.text += 1,
                    _ => {}
                }
                println!("EVENT {event}");
                let _ = std::io::stdout().flush();
            });
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        line.split_ascii_whitespace()
            .find_map(|part| part.strip_prefix(key)?.strip_prefix('='))
    }
    fn number(line: &str, key: &str) -> Result<usize, String> {
        field(line, key)
            .ok_or_else(|| format!("fence omitted {key}: {line}"))?
            .parse()
            .map_err(|e| format!("parse {key}: {e}"))
    }

    #[path = "../smoke/cursor_effects.rs"]
    mod cursor_effects;

    #[path = "../smoke/coordinate_profile.rs"]
    mod coordinate_profile;
}
