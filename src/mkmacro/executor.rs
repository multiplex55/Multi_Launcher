//! Platform-neutral plan executor and injectable effect boundaries.
use super::{
    CapturedRegion, Jump, MkAction, MkCompareOp, MkCondition, MkCoordinateTarget, MkDelayMode,
    MkExecutionPlan, MkFileCollisionPolicy, MkImageNotFoundPolicy, MkImageOutputs, MkImagePayload,
    MkKey, MkMacroStore, MkMouseButton, MkMouseScrollAxis, MkNotificationDuration,
    MkNotificationKind, MkNotifyPayload, MkPlaySoundPayload, MkPlayback, MkPoint, MkProcessPayload,
    MkPromptInputPayload, MkScreenshotFormat, MkTextPayload, MkUiPayload, MkValue, MkWaitOptions,
    MkWindowMatcher, MkWindowMoveResizePayload, MkWindowPayload, MkWindowState, PromptBackend,
    PromptRequest, PromptResponse, RuntimeVariables, ScreenCaptureBackend, ScreenRect,
    SearchRegion, interpolate,
};
use rand::RngExt;
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    TargetNotFound,
    AmbiguousTarget,
    Timeout,
    UnsupportedOperation,
    InputRejected,
    InvalidTarget,
    InvalidPlan,
    InvalidSelection,
    Backend,
    Cancelled,
    Panic,
    RuntimeUnavailable,
    TypeMismatch,
    InvalidRegex,
    IterationLimit,
    UnsupportedPattern,
    StaleElement,
    ComFailure,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDiagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub context: BTreeMap<String, String>,
}
impl ExecutionDiagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }
    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}
impl fmt::Display for ExecutionDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ExecutionDiagnostic {}
pub type ExecResult<T = ()> = Result<T, ExecutionDiagnostic>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Normal,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionOptions {
    pub mode: ExecutionMode,
}

/// In [`ExecutionMode::Debug`], an enabled breakpoint is checked once when its
/// instruction is entered. The instruction's `repeat` body runs as one entry,
/// so repetitions do not retrigger that breakpoint. Loop opcodes can naturally
/// retrigger when the program counter revisits their instruction.
impl ExecutionOptions {
    pub const fn normal() -> Self {
        Self {
            mode: ExecutionMode::Normal,
        }
    }

    pub const fn debug() -> Self {
        Self {
            mode: ExecutionMode::Debug,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugSnapshotReason {
    RunStarted,
    Breakpoint,
    StepBoundary,
    RunFinished,
    RunFailed,
    RunCancelled,
}

/// A notification after all runtime-variable interpolation has completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNotification {
    pub title: String,
    pub description: String,
    pub kind: MkNotificationKind,
    pub duration: MkNotificationDuration,
    pub show_symbol: bool,
}

pub trait NotificationBackend: Send + Sync {
    fn notify(&self, notification: &ResolvedNotification) -> ExecResult;
}
pub trait SoundBackend: Send + Sync {
    fn play(&self, sound: &str) -> ExecResult;
}

pub trait InputBackend: Send + Sync {
    /// Reports the physical Escape key state so playback can be cancelled.
    fn escape_pressed(&self) -> bool {
        false
    }
    fn key_down(&self, key: &MkKey) -> ExecResult;
    fn key_up(&self, key: &MkKey) -> ExecResult;
    fn button_down(&self, button: MkMouseButton) -> ExecResult;
    fn button_up(&self, button: MkMouseButton) -> ExecResult;
    fn move_mouse(&self, point: MkPoint) -> ExecResult;
    /// Moves over a duration. Backends may override this boundary to provide a
    /// deterministic clock (notably the executor fake) without sleeping.
    fn move_mouse_smooth(
        &self,
        control: &RunControl,
        from: MkPoint,
        to: MkPoint,
        duration: Duration,
    ) -> ExecResult {
        super::input::smooth_move(self, control, from, to, duration)
    }
    fn cursor_position(&self) -> ExecResult<MkPoint>;
    fn scroll(&self, axis: MkMouseScrollAxis, delta: i32) -> ExecResult;
    fn text(&self, payload: &MkTextPayload) -> ExecResult;
}
pub trait WindowBackend: Send + Sync {
    fn exists(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn is_active(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn activate(&self, p: &MkWindowPayload) -> ExecResult;
    fn close(&self, m: &MkWindowMatcher) -> ExecResult;
    fn move_resize(&self, p: &MkWindowMoveResizePayload) -> ExecResult;
    fn set_state(&self, m: &MkWindowMatcher, state: MkWindowState) -> ExecResult;
}
pub trait ScreenBackend: Send + Sync {
    fn resolve(
        &self,
        target: &MkCoordinateTarget,
        variables: &RuntimeVariables,
    ) -> ExecResult<MkPoint>;
    /// Applies platform desktop bounds to a fully resolved and randomized point.
    fn finalize_point(&self, point: MkPoint) -> ExecResult<MkPoint> {
        Ok(point)
    }
    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>>;
    fn find_pixel(&self, _: &super::MkPixelSearchPayload) -> ExecResult<Option<MkPoint>> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "pixel search is unavailable",
        ))
    }
    fn pixel_matches(
        &self,
        target: &MkCoordinateTarget,
        color: &str,
        tolerance: u8,
        variables: &RuntimeVariables,
    ) -> ExecResult<bool>;
}
pub trait UiAutomationBackend: Send + Sync {
    fn exists(&self, p: &MkUiPayload) -> ExecResult<bool>;
    fn invoke(&self, p: &MkUiPayload) -> ExecResult;
    fn set_value(&self, p: &MkUiPayload, value: &str) -> ExecResult;
    fn read_value(&self, _: &MkUiPayload) -> ExecResult<String> {
        unsupported()
    }
    fn toggle(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn select(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn focus(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
}
pub trait UiAutomationInspector: Send + Sync {
    fn inspect_at(&self, point: MkPoint) -> ExecResult<super::UiElementInfo>;
}
pub trait LauncherBackend: Send + Sync {
    fn launch_process(&self, p: &MkProcessPayload) -> ExecResult;
    /// Submits raw text to the Launcher query/search pipeline.
    fn command(&self, query: &str, control: &RunControl) -> ExecResult;
    /// Submits a migration-only resolved action for GUI-thread activation.
    fn resolved_legacy(&self, action: &crate::actions::Action, control: &RunControl) -> ExecResult;
}
pub trait ClipboardBackend: Send + Sync {
    /// Captures the text clipboard. Backends which cannot preserve non-text
    /// formats must reject those contents rather than destroying them.
    fn snapshot_text(&self) -> ExecResult<String>;
    fn set_text(&self, text: &str) -> ExecResult;
    fn set_image(&self, _: &CapturedRegion) -> ExecResult {
        unsupported_context("clipboard", "publish image")
    }
}
pub trait ScreenshotEncoder: Send + Sync {
    fn encode(&self, image: &CapturedRegion, format: MkScreenshotFormat) -> ExecResult<Vec<u8>>;
}
pub trait ScreenshotFileSystem: Send + Sync {
    /// Atomically publishes bytes and returns the final path (which may differ
    /// under `Unique`). No partial destination may be visible on failure.
    fn write_transactional(
        &self,
        path: &Path,
        bytes: &[u8],
        collision: MkFileCollisionPolicy,
    ) -> ExecResult<PathBuf>;
}
#[derive(Clone)]
pub struct Backends {
    pub notification: Arc<dyn NotificationBackend>,
    pub sound: Arc<dyn SoundBackend>,
    pub input: Arc<dyn InputBackend>,
    pub window: Arc<dyn WindowBackend>,
    pub screen: Arc<dyn ScreenBackend>,
    pub uia: Arc<dyn UiAutomationBackend>,
    pub launcher: Arc<dyn LauncherBackend>,
    pub prompt: Arc<dyn PromptBackend>,
    pub clipboard: Arc<dyn ClipboardBackend>,
    pub screenshot_capture: Arc<dyn ScreenCaptureBackend>,
    pub screenshot_encoder: Arc<dyn ScreenshotEncoder>,
    pub screenshot_files: Arc<dyn ScreenshotFileSystem>,
    pub virtual_desktop: Arc<dyn super::virtual_desktops::VirtualDesktopBackend>,
}
impl Backends {
    pub fn unsupported() -> Self {
        let input = Arc::new(Unsupported { backend: "input" });
        let window = Arc::new(Unsupported { backend: "window" });
        let screen = Arc::new(Unsupported { backend: "screen" });
        let uia = Arc::new(Unsupported {
            backend: "UI Automation",
        });
        let launcher = Arc::new(Unsupported {
            backend: "launcher",
        });
        let prompt = Arc::new(Unsupported { backend: "prompt" });
        let clipboard = Arc::new(Unsupported {
            backend: "clipboard",
        });
        Self {
            notification: Arc::new(Unsupported {
                backend: "notification",
            }),
            sound: Arc::new(Unsupported { backend: "sound" }),
            input,
            window,
            screen,
            uia,
            launcher,
            prompt,
            clipboard,
            screenshot_capture: Arc::new(Unsupported {
                backend: "screen capture",
            }),
            screenshot_encoder: Arc::new(ImageScreenshotEncoder),
            screenshot_files: Arc::new(HostScreenshotFileSystem),
            virtual_desktop: Arc::new(super::virtual_desktops::UnsupportedVirtualDesktopBackend),
        }
    }

    /// Backends used by the installed application. Live input remains an explicit
    /// production-only capability; tests should use [`MacroRuntime::new`] instead.
    pub fn production() -> Self {
        production_backends()
    }
}
struct Unsupported {
    backend: &'static str,
}
fn unsupported_context<T>(backend: &'static str, action: &'static str) -> ExecResult<T> {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "This action is not available yet",
    )
    .context("backend", backend)
    .context("action", action))
}
fn unsupported<T>() -> ExecResult<T> {
    unsupported_context("automation", "unknown")
}
impl NotificationBackend for Unsupported {
    fn notify(&self, _: &ResolvedNotification) -> ExecResult {
        unsupported_context(self.backend, "show toast")
    }
}
impl SoundBackend for Unsupported {
    fn play(&self, _: &str) -> ExecResult {
        unsupported_context(self.backend, "play sound")
    }
}

/// Production adapter for the application's asynchronous embedded sounds.
pub struct ProductionSoundBackend;
impl SoundBackend for ProductionSoundBackend {
    fn play(&self, sound: &str) -> ExecResult {
        if !crate::sound::SOUND_NAMES.contains(&sound) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                format!("unknown sound: {sound}"),
            )
            .context("backend", "sound")
            .context("sound", sound));
        }
        crate::sound::play_sound(sound);
        Ok(())
    }
}

#[cfg(test)]
mod sound_backend_tests {
    use super::*;

    #[test]
    fn invalid_sound_is_rejected_before_dispatch() {
        let error = ProductionSoundBackend
            .play("not-a-real-sound.wav")
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            error.context.get("sound").map(String::as_str),
            Some("not-a-real-sound.wav")
        );
    }

    #[test]
    fn production_dispatch_does_not_join_playback_thread() {
        let started = Instant::now();
        ProductionSoundBackend.play("Alarm.wav").unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
impl InputBackend for Unsupported {
    fn key_down(&self, _: &MkKey) -> ExecResult {
        unsupported()
    }
    fn key_up(&self, _: &MkKey) -> ExecResult {
        unsupported()
    }
    fn button_down(&self, _: MkMouseButton) -> ExecResult {
        unsupported()
    }
    fn button_up(&self, _: MkMouseButton) -> ExecResult {
        unsupported()
    }
    fn move_mouse(&self, _: MkPoint) -> ExecResult {
        unsupported()
    }
    fn cursor_position(&self) -> ExecResult<MkPoint> {
        unsupported()
    }
    fn scroll(&self, _: MkMouseScrollAxis, _: i32) -> ExecResult {
        unsupported()
    }
    fn text(&self, _: &MkTextPayload) -> ExecResult {
        unsupported()
    }
}
impl WindowBackend for Unsupported {
    fn exists(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
        unsupported()
    }
    fn is_active(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
        unsupported()
    }
    fn activate(&self, _: &MkWindowPayload) -> ExecResult {
        unsupported()
    }
    fn close(&self, _: &MkWindowMatcher) -> ExecResult {
        unsupported()
    }
    fn move_resize(&self, _: &MkWindowMoveResizePayload) -> ExecResult {
        unsupported()
    }
    fn set_state(&self, _: &MkWindowMatcher, _: MkWindowState) -> ExecResult {
        unsupported()
    }
}
impl ScreenBackend for Unsupported {
    fn resolve(&self, _: &MkCoordinateTarget, _: &RuntimeVariables) -> ExecResult<MkPoint> {
        unsupported_context(self.backend, "resolve coordinates")
    }
    fn find_image(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        unsupported_context(self.backend, "find image")
    }
    fn pixel_matches(
        &self,
        _: &MkCoordinateTarget,
        _: &str,
        _: u8,
        _: &RuntimeVariables,
    ) -> ExecResult<bool> {
        unsupported_context(self.backend, "match pixel")
    }
}
impl UiAutomationBackend for Unsupported {
    fn exists(&self, _: &MkUiPayload) -> ExecResult<bool> {
        uia_unavailable("exists")
    }
    fn invoke(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("invoke")
    }
    fn set_value(&self, _: &MkUiPayload, _: &str) -> ExecResult {
        uia_unavailable("set value")
    }
    fn read_value(&self, _: &MkUiPayload) -> ExecResult<String> {
        uia_unavailable("read value")
    }
    fn toggle(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("toggle")
    }
    fn select(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("select")
    }
    fn focus(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("focus")
    }
}
fn uia_unavailable<T>(action: &'static str) -> ExecResult<T> {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "UI Automation backend is not available yet",
    )
    .context("backend", "UI Automation")
    .context("action", action))
}

#[cfg(any(windows, test))]
struct ProductionLauncher {
    command_broker: Arc<super::LauncherCommandBroker>,
}
#[cfg(any(windows, test))]
impl LauncherBackend for ProductionLauncher {
    fn launch_process(&self, p: &MkProcessPayload) -> ExecResult {
        let mut command = std::process::Command::new(&p.program);
        command.args(&p.arguments);
        if let Some(directory) = p
            .working_directory
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            command.current_dir(directory);
        }
        let result = if p.wait {
            command.status().map(|_| ())
        } else {
            command.spawn().map(|_| ())
        };
        result.map_err(|e| {
            ExecutionDiagnostic::new(DiagnosticKind::Backend, e.to_string())
                .context("backend", "launcher")
                .context("action", p.program.clone())
        })
    }
    fn command(&self, query: &str, control: &RunControl) -> ExecResult {
        let response = self
            .command_broker
            .submit_query(query, control)
            .map_err(|error| error.context("backend", "launcher").context("query", query))?;
        match response {
            super::LauncherCommandResponse::Activated
            | super::LauncherCommandResponse::PresentedForSelection { .. } => Ok(()),
            super::LauncherCommandResponse::NoResults => Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                "Launcher command produced no results",
            )
            .context("backend", "launcher")
            .context("query", query)),
            super::LauncherCommandResponse::Failed(message) => {
                Err(ExecutionDiagnostic::new(DiagnosticKind::Backend, message)
                    .context("backend", "launcher")
                    .context("query", query))
            }
        }
    }

    fn resolved_legacy(&self, action: &crate::actions::Action, control: &RunControl) -> ExecResult {
        let response = self
            .command_broker
            .submit_resolved_legacy(action.clone(), control)
            .map_err(|error| {
                error
                    .context("backend", "launcher")
                    .context("action", action.action.clone())
            })?;
        match response {
            super::LauncherCommandResponse::Activated => Ok(()),
            super::LauncherCommandResponse::Failed(message) => {
                Err(ExecutionDiagnostic::new(DiagnosticKind::Backend, message)
                    .context("backend", "launcher")
                    .context("action", action.action.clone()))
            }
            other => Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!("unexpected response to resolved legacy action: {other:?}"),
            )
            .context("backend", "launcher")
            .context("action", action.action.clone())),
        }
    }
}

/// Constructs the application's effect boundaries without exposing an accidental
/// live-input default.
pub fn production_backends() -> Backends {
    let unsupported = Backends::unsupported();
    #[cfg(windows)]
    {
        let input: Arc<dyn InputBackend> = Arc::new(super::input::Win32InputBackend::system(
            super::input::LiveInputOptIn::production(),
        ));
        Backends {
            notification: Arc::new(super::notifications::WindowsNotificationBackend::new()),
            sound: Arc::new(ProductionSoundBackend),
            virtual_desktop: Arc::new(super::virtual_desktops::WindowsVirtualDesktopBackend(
                input.clone(),
            )),
            input,
            window: Arc::new(super::windows::Win32WindowBackend),
            screen: Arc::new(super::screen::WindowsScreenBackend::system()),
            uia: unsupported.uia,
            launcher: Arc::new(ProductionLauncher {
                command_broker: super::production_launcher_command_broker(),
            }),
            prompt: super::prompt::production_prompt_broker(),
            clipboard: Arc::new(ProductionClipboard),
            screenshot_capture: Arc::new(super::screen::WindowsScreenCaptureBackend::system()),
            screenshot_encoder: Arc::new(ImageScreenshotEncoder),
            screenshot_files: Arc::new(HostScreenshotFileSystem),
        }
    }
    #[cfg(not(windows))]
    {
        Backends {
            sound: Arc::new(ProductionSoundBackend),
            prompt: super::prompt::production_prompt_broker(),
            clipboard: Arc::new(ProductionClipboard),
            ..unsupported
        }
    }
}

/// Store-aware production wiring used by the macro runtime. Keeping this
/// separate preserves callers that only need non-visual production boundaries.
pub fn production_backends_with_store(store: Arc<MkMacroStore>) -> Backends {
    let mut backends = production_backends();
    #[cfg(windows)]
    {
        backends.screen = Arc::new(super::screen::WindowsScreenBackend::production(store));
    }
    #[cfg(not(windows))]
    let _ = store;
    backends
}
impl LauncherBackend for Unsupported {
    fn launch_process(&self, _: &MkProcessPayload) -> ExecResult {
        unsupported()
    }
    fn command(&self, _: &str, control: &RunControl) -> ExecResult {
        control.checkpoint()?;
        unsupported()
    }
    fn resolved_legacy(&self, _: &crate::actions::Action, control: &RunControl) -> ExecResult {
        control.checkpoint()?;
        unsupported()
    }
}
impl PromptBackend for Unsupported {
    fn prompt(&self, _: PromptRequest, _: &RunControl) -> ExecResult<PromptResponse> {
        unsupported_context(self.backend, "prompt")
    }
}
impl ClipboardBackend for Unsupported {
    fn snapshot_text(&self) -> ExecResult<String> {
        unsupported_context(self.backend, "snapshot text")
    }
    fn set_text(&self, _: &str) -> ExecResult {
        unsupported_context(self.backend, "set text")
    }
}
impl ScreenCaptureBackend for Unsupported {
    fn virtual_desktop(&self) -> ExecResult<super::ScreenRect> {
        unsupported_context(self.backend, "resolve virtual desktop")
    }
    fn region_bounds(&self, _: &SearchRegion) -> ExecResult<super::ScreenRect> {
        unsupported_context(self.backend, "resolve capture region")
    }
    fn capture_rect(
        &self,
        _: super::ScreenRect,
        _: &dyn Fn() -> bool,
    ) -> ExecResult<image::RgbaImage> {
        unsupported_context(self.backend, "capture screen")
    }
}
impl ScreenshotEncoder for Unsupported {
    fn encode(&self, _: &CapturedRegion, _: MkScreenshotFormat) -> ExecResult<Vec<u8>> {
        unsupported_context(self.backend, "encode screenshot")
    }
}
impl ScreenshotFileSystem for Unsupported {
    fn write_transactional(
        &self,
        _: &Path,
        _: &[u8],
        _: MkFileCollisionPolicy,
    ) -> ExecResult<PathBuf> {
        unsupported_context(self.backend, "write screenshot")
    }
}

struct ImageScreenshotEncoder;
impl ScreenshotEncoder for ImageScreenshotEncoder {
    fn encode(&self, frame: &CapturedRegion, format: MkScreenshotFormat) -> ExecResult<Vec<u8>> {
        let mut bytes = Cursor::new(Vec::new());
        let format = match format {
            MkScreenshotFormat::Png => image::ImageOutputFormat::Png,
            MkScreenshotFormat::Jpeg => image::ImageOutputFormat::Jpeg(90),
            MkScreenshotFormat::Bmp => image::ImageOutputFormat::Bmp,
        };
        image::DynamicImage::ImageRgba8(frame.image.clone())
            .write_to(&mut bytes, format)
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("failed to encode screenshot: {e}"),
                )
                .context("operation", "encode screenshot")
            })?;
        Ok(bytes.into_inner())
    }
}
struct HostScreenshotFileSystem;
impl ScreenshotFileSystem for HostScreenshotFileSystem {
    fn write_transactional(
        &self,
        requested: &Path,
        bytes: &[u8],
        collision: MkFileCollisionPolicy,
    ) -> ExecResult<PathBuf> {
        let mut path = requested.to_path_buf();
        if path.as_os_str().is_empty() {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "screenshot path is empty",
            ));
        }
        if matches!(collision, MkFileCollisionPolicy::Error) && path.exists() {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InputRejected,
                format!("screenshot file already exists: {}", path.display()),
            )
            .context("path", path.display().to_string()));
        }
        if matches!(collision, MkFileCollisionPolicy::Unique) {
            let mut n = 1;
            while path.exists() {
                let stem = requested
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("screenshot");
                let ext = requested.extension().and_then(|s| s.to_str());
                path.set_file_name(match ext {
                    Some(e) => format!("{stem}_{n}.{e}"),
                    None => format!("{stem}_{n}"),
                });
                n += 1;
            }
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!("failed to create screenshot directory: {e}"),
            )
        })?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!("failed to create temporary screenshot file: {e}"),
            )
        })?;
        std::io::Write::write_all(&mut temp, bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("failed to write screenshot: {e}"),
                )
            })?;
        let publish = if matches!(collision, MkFileCollisionPolicy::Overwrite) {
            temp.persist(&path).map(|_| ()).map_err(|e| e.error)
        } else {
            temp.persist_noclobber(&path)
                .map(|_| ())
                .map_err(|e| e.error)
        };
        publish.map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!(
                    "failed to atomically publish screenshot {}: {e}",
                    path.display()
                ),
            )
        })?;
        Ok(path)
    }
}
struct ProductionClipboard;
impl ClipboardBackend for ProductionClipboard {
    fn snapshot_text(&self) -> ExecResult<String> {
        arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .map_err(|e| clipboard_error("snapshot", e))
    }
    fn set_text(&self, text: &str) -> ExecResult {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_owned()))
            .map_err(|e| clipboard_error("set_text", e))
    }
    fn set_image(&self, frame: &CapturedRegion) -> ExecResult {
        let data = arboard::ImageData {
            width: frame.image.width() as usize,
            height: frame.image.height() as usize,
            bytes: std::borrow::Cow::Owned(frame.image.as_raw().clone()),
        };
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_image(data))
            .map_err(|e| clipboard_error("set_image", e))
    }
}
fn clipboard_error(operation: &'static str, error: impl fmt::Display) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(
        DiagnosticKind::Backend,
        format!("clipboard {operation} failed: {error}"),
    )
    .context("backend", "clipboard")
    .context("operation", operation)
}

/// A text-only clipboard transaction. Drop is a last-resort restoration path;
/// `finish` is used normally so restoration errors can be reported.
struct ClipboardTransaction {
    backend: Arc<dyn ClipboardBackend>,
    snapshot: Option<String>,
}
impl ClipboardTransaction {
    fn install(backend: Arc<dyn ClipboardBackend>, text: &str) -> ExecResult<Self> {
        let snapshot = backend
            .snapshot_text()
            .map_err(|e| e.context("operation", "snapshot"))?;
        backend
            .set_text(text)
            .map_err(|e| e.context("operation", "temporary_write"))?;
        Ok(Self {
            backend,
            snapshot: Some(snapshot),
        })
    }
    fn finish(mut self, primary: ExecResult) -> ExecResult {
        let restore = self
            .backend
            .set_text(self.snapshot.as_deref().unwrap())
            .map_err(|e| e.context("operation", "restore"));
        self.snapshot = None;
        match (primary, restore) {
            (Err(e), Err(r)) => Err(e.context("clipboard_restoration_failure", r.to_string())),
            (Err(e), _) => Err(e),
            (Ok(()), Err(e)) => Err(e),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}
impl Drop for ClipboardTransaction {
    fn drop(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            let _ = self.backend.set_text(&snapshot);
        }
    }
}

/// Intentionally unscaled safety delay: this allows the target application to
/// consume clipboard data before the user's clipboard text is restored.
const PASTE_SETTLE_INTERVAL: Duration = Duration::from_millis(100);
#[derive(Default)]
struct ControlState {
    paused: bool,
    stopped: bool,
    active: bool,
}
#[derive(Default)]
pub struct RunControl {
    state: Mutex<ControlState>,
    wake: Condvar,
}
impl RunControl {
    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        *s = ControlState {
            active: true,
            ..Default::default()
        };
        self.wake.notify_all()
    }
    pub fn pause(&self) {
        self.state.lock().unwrap().paused = true;
        self.wake.notify_all()
    }
    pub fn resume(&self) {
        self.state.lock().unwrap().paused = false;
        self.wake.notify_all()
    }
    pub fn stop(&self) {
        self.state.lock().unwrap().stopped = true;
        self.wake.notify_all()
    }
    pub fn is_active(&self) -> bool {
        self.state.lock().unwrap().active
    }
    pub fn is_stopped(&self) -> bool {
        self.state.lock().unwrap().stopped
    }
    pub fn checkpoint(&self) -> ExecResult {
        let mut s = self.state.lock().unwrap();
        while s.paused && !s.stopped {
            s = self.wake.wait(s).unwrap()
        }
        if s.stopped {
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::Cancelled,
                "automation stopped",
            ))
        } else {
            Ok(())
        }
    }
    pub fn wait(&self, duration: Duration) -> ExecResult {
        let mut remaining = duration;
        loop {
            self.checkpoint()?;
            if remaining.is_zero() {
                return Ok(());
            }
            let start = Instant::now();
            let s = self.state.lock().unwrap();
            let (s, timeout) = self.wake.wait_timeout(s, remaining).unwrap();
            let elapsed = start.elapsed();
            if !s.paused {
                remaining = remaining.saturating_sub(elapsed)
            }
            if s.stopped {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Cancelled,
                    "automation stopped",
                ));
            }
            if timeout.timed_out() && !s.paused {
                return Ok(());
            }
        }
    }
}

/// Clears runtime activity on every executor exit path, including cancellation
/// and backend failures. Without this guard, queued Pause/Resume commands can
/// mistake a finished run for an active one and overwrite its terminal state.
struct RunActivityGuard<'a>(&'a RunControl);
impl Drop for RunActivityGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.active = false;
        self.0.wake.notify_all();
    }
}
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    /// A debugger breakpoint has paused execution before the step starts.
    BreakpointHit {
        step_id: u64,
        variables: RuntimeVariables,
    },
    /// An owned snapshot of the worker-local runtime variables at a debugger
    /// boundary. The map is cloned before it crosses the executor boundary.
    /// For terminal snapshots, `step_id` identifies the last safe instruction
    /// boundary, i.e. the last completed step rather than a step that was only
    /// entered and may still have partially executed.
    DebugVariables {
        step_id: Option<u64>,
        variables: RuntimeVariables,
        reason: DebugSnapshotReason,
    },
    StepStarted(u64),
    /// Kept as a single-field event for observers that only track lifecycle.
    StepFinished(u64),
    /// Optional structured metadata emitted immediately before `StepFinished`.
    StepOutcome(u64, StepOutcome),
    StepSkipped(u64),
    StepFailed(u64, ExecutionDiagnostic),
    Paused,
    Resumed,
}

/// Structured, per-step result data for successful actions. This is deliberately
/// separate from `RuntimeVariables`, whose lifetime is only one playback run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub last_image_found: Option<bool>,
}

impl StepOutcome {
    fn for_action(action: &MkAction, variables: &RuntimeVariables) -> Self {
        let key = match action {
            MkAction::ImageFind(_) | MkAction::ImageClick(_) => Some("last_image_found"),
            MkAction::FindPixel(_) => Some("last_pixel_found"),
            _ => None,
        };
        Self {
            last_image_found: key.and_then(|key| match variables.get(key) {
                Some(MkValue::Boolean(found)) => Some(*found),
                _ => None,
            }),
        }
    }

    pub fn detail(&self) -> Option<&'static str> {
        match self.last_image_found {
            Some(true) => Some("Success — image found."),
            Some(false) => Some("Success — image not found; continued."),
            None => None,
        }
    }
}

pub struct InputCleanupGuard {
    backend: Arc<dyn InputBackend>,
    keys: Vec<MkKey>,
    buttons: Vec<MkMouseButton>,
}
impl InputCleanupGuard {
    pub fn new(backend: Arc<dyn InputBackend>) -> Self {
        Self {
            backend,
            keys: vec![],
            buttons: vec![],
        }
    }
    fn down_key(&mut self, k: &MkKey) -> ExecResult {
        self.backend.key_down(k)?;
        self.keys.push(k.clone());
        Ok(())
    }
    fn up_key(&mut self, k: &MkKey) -> ExecResult {
        self.backend.key_up(k)?;
        if let Some(i) = self.keys.iter().rposition(|x| x == k) {
            self.keys.remove(i);
        }
        Ok(())
    }
    fn hotkey(&mut self, keys: &[MkKey]) -> ExecResult {
        let mut pressed = 0;
        let mut result = Ok(());
        for key in keys {
            match self.down_key(key) {
                Ok(()) => pressed += 1,
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        for key in keys[..pressed].iter().rev() {
            if let Err(error) = self.up_key(key)
                && result.is_ok()
            {
                result = Err(error);
            }
        }
        result
    }
    fn down_button(&mut self, b: MkMouseButton) -> ExecResult {
        self.backend.button_down(b.clone())?;
        self.buttons.push(b);
        Ok(())
    }
    fn up_button(&mut self, b: MkMouseButton) -> ExecResult {
        self.backend.button_up(b.clone())?;
        if let Some(i) = self.buttons.iter().rposition(|x| x == &b) {
            self.buttons.remove(i);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{fake::FakeBackend, *};
    use crate::mkmacro::{
        AlphaPolicy, LauncherCommandBroker, LauncherCommandKind, LauncherCommandResponse,
        MkDelayPayload, MkErrorPolicy, MkMacro, MkPlayback, MkStep, MkTextMode, ReturnPoint,
        SearchRegion, compile,
    };

    fn run_production_launcher_command(response: LauncherCommandResponse) -> (ExecResult, String) {
        let broker = Arc::new(LauncherCommandBroker::default());
        let launcher = Arc::new(ProductionLauncher {
            command_broker: Arc::clone(&broker),
        });
        let worker = std::thread::spawn(move || {
            launcher.command("raw query --exact", &RunControl::default())
        });
        let pending = loop {
            if let Some(pending) = broker.take_pending() {
                break pending;
            }
            std::thread::yield_now();
        };
        let query = match &pending.request.kind {
            LauncherCommandKind::Query(query) => query.clone(),
            _ => panic!("expected query"),
        };
        assert!(pending.respond(response));
        (worker.join().unwrap(), query)
    }

    #[test]
    fn production_launcher_accepts_activated_and_selection_responses() {
        for response in [
            LauncherCommandResponse::Activated,
            LauncherCommandResponse::PresentedForSelection { result_count: 2 },
        ] {
            assert_eq!(run_production_launcher_command(response).0, Ok(()));
        }
    }

    #[test]
    fn production_launcher_no_results_diagnostic_contains_raw_query() {
        let (result, submitted_query) =
            run_production_launcher_command(LauncherCommandResponse::NoResults);
        let error = result.unwrap_err();
        assert_eq!(submitted_query, "raw query --exact");
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(error.message, "Launcher command produced no results");
        assert_eq!(
            error.context.get("backend").map(String::as_str),
            Some("launcher")
        );
        assert_eq!(
            error.context.get("query").map(String::as_str),
            Some("raw query --exact")
        );
    }

    #[test]
    fn production_launcher_preserves_gui_failure_message() {
        let error = run_production_launcher_command(LauncherCommandResponse::Failed(
            "GUI could not activate that result".into(),
        ))
        .0
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(error.message, "GUI could not activate that result");
        assert_eq!(
            error.context.get("query").map(String::as_str),
            Some("raw query --exact")
        );
    }

    #[test]
    fn production_launcher_responses_do_not_execute_resolved_actions() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let hook_called = Arc::clone(&called);
        crate::gui::set_execute_action_hook(Some(Box::new(move |_| {
            hook_called.store(true, Ordering::SeqCst);
            Ok(())
        })));
        for response in [
            LauncherCommandResponse::Activated,
            LauncherCommandResponse::PresentedForSelection { result_count: 1 },
            LauncherCommandResponse::NoResults,
            LauncherCommandResponse::Failed("failure".into()),
        ] {
            let _ = run_production_launcher_command(response);
        }
        crate::gui::set_execute_action_hook(None);
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn production_launcher_process_launch_is_independent_of_command_broker() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let launcher = ProductionLauncher {
            command_broker: Arc::clone(&broker),
        };
        let control = Arc::new(RunControl::default());
        let submitting_broker = Arc::clone(&broker);
        let submitting_control = Arc::clone(&control);
        let submission = std::thread::spawn(move || {
            submitting_broker.submit_query("occupied", &submitting_control)
        });
        let _pending = loop {
            if let Some(pending) = broker.take_pending() {
                break pending;
            }
            std::thread::yield_now();
        };

        #[cfg(windows)]
        let process = MkProcessPayload {
            program: "cmd".into(),
            arguments: vec!["/C".into(), "exit 0".into()],
            working_directory: None,
            wait: true,
        };
        #[cfg(not(windows))]
        let process = MkProcessPayload {
            program: "sh".into(),
            arguments: vec!["-c".into(), "exit 0".into()],
            working_directory: None,
            wait: true,
        };
        assert_eq!(launcher.launch_process(&process), Ok(()));
        control.stop();
        assert_eq!(
            submission.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
    }

    #[derive(Default)]
    struct ShotCapture {
        regions: Mutex<Vec<SearchRegion>>,
        fail: bool,
    }
    impl ScreenCaptureBackend for ShotCapture {
        fn virtual_desktop(&self) -> ExecResult<super::super::ScreenRect> {
            Ok(super::super::ScreenRect::new(0, 0, 2000, 1200))
        }
        fn region_bounds(&self, region: &SearchRegion) -> ExecResult<super::super::ScreenRect> {
            self.regions.lock().unwrap().push(region.clone());
            if self.fail {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "capture target missing",
                ));
            }
            Ok(super::super::ScreenRect::new(1, 2, 3, 4))
        }
        fn capture_rect(
            &self,
            rect: super::super::ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            Ok(image::RgbaImage::new(rect.width, rect.height))
        }
    }
    struct ShotEncoder {
        calls: Mutex<usize>,
        fail: bool,
    }
    impl ScreenshotEncoder for ShotEncoder {
        fn encode(&self, _: &CapturedRegion, _: MkScreenshotFormat) -> ExecResult<Vec<u8>> {
            *self.calls.lock().unwrap() += 1;
            if self.fail {
                Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "encoder failed",
                ))
            } else {
                Ok(vec![1, 2, 3])
            }
        }
    }
    struct ShotFiles {
        paths: Mutex<Vec<PathBuf>>,
        fail: bool,
    }
    impl ScreenshotFileSystem for ShotFiles {
        fn write_transactional(
            &self,
            path: &Path,
            _: &[u8],
            _: MkFileCollisionPolicy,
        ) -> ExecResult<PathBuf> {
            self.paths.lock().unwrap().push(path.to_owned());
            if self.fail {
                Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InputRejected,
                    "file conflict",
                ))
            } else {
                Ok(PathBuf::from("actual.png"))
            }
        }
    }
    struct ShotClipboard {
        calls: Mutex<usize>,
        fail: bool,
    }
    impl ClipboardBackend for ShotClipboard {
        fn snapshot_text(&self) -> ExecResult<String> {
            Ok(String::new())
        }
        fn set_text(&self, _: &str) -> ExecResult {
            Ok(())
        }
        fn set_image(&self, _: &CapturedRegion) -> ExecResult {
            *self.calls.lock().unwrap() += 1;
            if self.fail {
                Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "clipboard failed",
                ))
            } else {
                Ok(())
            }
        }
    }
    fn screenshot_action(destination: super::super::MkScreenshotDestination) -> MkAction {
        MkAction::CaptureScreenshot(super::super::MkScreenshotPayload {
            region: SearchRegion::Desktop,
            destination,
            path: Some("shots/${name}.png".into()),
            format: MkScreenshotFormat::Png,
            collision: MkFileCollisionPolicy::Error,
            path_output: Some("written".into()),
        })
    }
    fn run_screenshot(
        backends: Backends,
        action: &MkAction,
        vars: &mut RuntimeVariables,
    ) -> ExecResult {
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(backends.clone(), control);
        let mut guard = InputCleanupGuard::new(backends.input.clone());
        executor.action(1, action, &MkPlayback::default(), vars, &mut guard)
    }

    #[test]
    fn screenshot_every_region_uses_shared_capture_boundary() {
        let capture = Arc::new(ShotCapture::default());
        let fake = Arc::new(FakeBackend::default());
        let mut backends = fake.backends();
        backends.screenshot_capture = capture.clone();
        backends.clipboard = Arc::new(ShotClipboard {
            calls: Mutex::new(0),
            fail: false,
        });
        let matcher = super::super::MkWindowMatcher {
            title: Some("App".into()),
            ..Default::default()
        };
        let regions = vec![
            SearchRegion::Desktop,
            SearchRegion::Monitor { index: 2 },
            SearchRegion::Rectangle {
                rect: super::super::ScreenRect::new(1, 2, 3, 4),
            },
            SearchRegion::Window {
                matcher: matcher.clone(),
            },
            SearchRegion::ClientArea { matcher },
        ];
        for region in &regions {
            let mut action = screenshot_action(super::super::MkScreenshotDestination::Clipboard);
            if let MkAction::CaptureScreenshot(p) = &mut action {
                p.region = region.clone();
                p.path = None;
                p.path_output = None;
            }
            run_screenshot(backends.clone(), &action, &mut RuntimeVariables::new()).unwrap();
        }
        assert_eq!(*capture.regions.lock().unwrap(), regions);
    }

    struct VisualChangeCapture {
        regions: Mutex<Vec<SearchRegion>>,
        frames: Mutex<std::collections::VecDeque<image::RgbaImage>>,
    }
    impl ScreenCaptureBackend for VisualChangeCapture {
        fn virtual_desktop(&self) -> ExecResult<super::super::ScreenRect> {
            Ok(super::super::ScreenRect::new(0, 0, 100, 100))
        }
        fn region_bounds(&self, region: &SearchRegion) -> ExecResult<super::super::ScreenRect> {
            self.regions.lock().unwrap().push(region.clone());
            Ok(super::super::ScreenRect::new(10, 20, 2, 2))
        }
        fn capture_rect(
            &self,
            _: super::super::ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            self.frames.lock().unwrap().pop_front().ok_or_else(|| {
                ExecutionDiagnostic::new(DiagnosticKind::Backend, "unexpected extra capture")
            })
        }
    }

    #[test]
    fn wait_visual_change_captures_one_baseline_and_polls_the_configured_region() {
        let region = SearchRegion::Rectangle {
            rect: super::super::ScreenRect::new(10, 20, 2, 2),
        };
        let baseline = image::RgbaImage::new(2, 2);
        let mut below_threshold = baseline.clone();
        below_threshold.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut reaches_threshold = below_threshold.clone();
        reaches_threshold.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
        let capture = Arc::new(VisualChangeCapture {
            regions: Mutex::new(vec![]),
            frames: Mutex::new(
                [
                    baseline,
                    below_threshold.clone(),
                    below_threshold.clone(),
                    below_threshold.clone(),
                    below_threshold,
                    reaches_threshold,
                ]
                .into(),
            ),
        });
        let fake = Arc::new(FakeBackend::default());
        let mut backends = fake.backends();
        backends.screenshot_capture = capture.clone();
        let action = MkAction::WaitForVisualChange(super::super::WaitForVisualChange {
            region: region.clone(),
            timeout_ms: 0,
            poll_interval_ms: 37,
            change_threshold_percent: 50.0,
            per_pixel_tolerance: Some(0),
            consecutive_changed_frames: Some(1),
        });
        let waiter = Arc::new(RecordingWaiter::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::with_waiter(backends.clone(), control, waiter.clone());
        let mut guard = InputCleanupGuard::new(backends.input.clone());
        executor
            .action(
                1,
                &action,
                &MkPlayback::default(),
                &mut RuntimeVariables::new(),
                &mut guard,
            )
            .unwrap();
        assert_eq!(*capture.regions.lock().unwrap(), vec![region; 6]);
        assert_eq!(waiter.sleeps(), vec![Duration::from_millis(37); 4]);
        assert!(capture.frames.lock().unwrap().is_empty());
    }

    #[test]
    fn screenshot_both_captures_once_interpolates_and_sets_output_after_write() {
        let capture = Arc::new(ShotCapture::default());
        let encoder = Arc::new(ShotEncoder {
            calls: Mutex::new(0),
            fail: false,
        });
        let files = Arc::new(ShotFiles {
            paths: Mutex::new(vec![]),
            fail: false,
        });
        let clipboard = Arc::new(ShotClipboard {
            calls: Mutex::new(0),
            fail: false,
        });
        let fake = Arc::new(FakeBackend::default());
        let mut backends = fake.backends();
        backends.screenshot_capture = capture.clone();
        backends.screenshot_encoder = encoder.clone();
        backends.screenshot_files = files.clone();
        backends.clipboard = clipboard.clone();
        let mut vars = RuntimeVariables::new();
        vars.insert("name".into(), MkValue::String("state".into()));
        run_screenshot(
            backends,
            &screenshot_action(super::super::MkScreenshotDestination::Both),
            &mut vars,
        )
        .unwrap();
        assert_eq!(capture.regions.lock().unwrap().len(), 1);
        assert_eq!(*encoder.calls.lock().unwrap(), 1);
        assert_eq!(*clipboard.calls.lock().unwrap(), 1);
        assert_eq!(
            files.paths.lock().unwrap()[0],
            PathBuf::from("shots/state.png")
        );
        assert_eq!(
            vars.get("written"),
            Some(&MkValue::String("actual.png".into()))
        );
    }

    #[test]
    fn screenshot_boundaries_report_independent_failures_and_output_timing() {
        for stage in ["capture", "encode", "file", "clipboard"] {
            let fake = Arc::new(FakeBackend::default());
            let mut backends = fake.backends();
            backends.screenshot_capture = Arc::new(ShotCapture {
                regions: Mutex::new(vec![]),
                fail: stage == "capture",
            });
            backends.screenshot_encoder = Arc::new(ShotEncoder {
                calls: Mutex::new(0),
                fail: stage == "encode",
            });
            backends.screenshot_files = Arc::new(ShotFiles {
                paths: Mutex::new(vec![]),
                fail: stage == "file",
            });
            backends.clipboard = Arc::new(ShotClipboard {
                calls: Mutex::new(0),
                fail: stage == "clipboard",
            });
            let mut vars = RuntimeVariables::new();
            vars.insert("name".into(), MkValue::String("state".into()));
            let error = run_screenshot(
                backends,
                &screenshot_action(super::super::MkScreenshotDestination::Both),
                &mut vars,
            )
            .unwrap_err();
            assert!(
                error.message.contains(stage) || error.context.values().any(|v| v.contains(stage)),
                "{stage}: {error:?}"
            );
            assert_eq!(
                vars.contains_key("written"),
                stage == "clipboard",
                "path output is only visible after the file transaction succeeds"
            );
        }
    }

    #[test]
    fn screenshot_file_collision_policies_are_explicit_and_transactional() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("state.png");
        std::fs::write(&requested, b"old").unwrap();
        let files = HostScreenshotFileSystem;

        let conflict = files
            .write_transactional(&requested, b"new", MkFileCollisionPolicy::Error)
            .unwrap_err();
        assert_eq!(conflict.kind, DiagnosticKind::InputRejected);
        assert_eq!(std::fs::read(&requested).unwrap(), b"old");

        let overwritten = files
            .write_transactional(&requested, b"new", MkFileCollisionPolicy::Overwrite)
            .unwrap();
        assert_eq!(overwritten, requested);
        assert_eq!(std::fs::read(&requested).unwrap(), b"new");

        let unique = files
            .write_transactional(&requested, b"unique", MkFileCollisionPolicy::Unique)
            .unwrap();
        assert_eq!(unique.file_name().unwrap(), "state_1.png");
        assert_eq!(std::fs::read(&unique).unwrap(), b"unique");
        assert_eq!(std::fs::read(&requested).unwrap(), b"new");
        assert_eq!(
            std::fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
                .count(),
            0,
            "successful publishes must not leave temporary files behind"
        );
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }
    fn plan(steps: Vec<MkStep>) -> MkExecutionPlan {
        compile(&MkMacro {
            id: 7,
            name: "test".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps,
            image_assets: vec![],
        })
        .unwrap()
    }

    fn text_action(text: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: text.into(),
            mode: MkTextMode::Type,
        })
    }

    fn execute_mode(
        plan: &MkExecutionPlan,
        fake: Arc<FakeBackend>,
        mode: ExecutionMode,
    ) -> (ExecResult, Vec<ExecutionEvent>) {
        let control = Arc::new(RunControl::default());
        control.reset();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let observer_control = control.clone();
        let result = Executor::new(fake.backends(), control).execute(
            plan,
            ExecutionOptions { mode },
            &|event| {
                if matches!(&event, ExecutionEvent::BreakpointHit { .. }) {
                    observer_control.resume();
                }
                captured.lock().unwrap().push(event);
            },
        );
        (result, events.lock().unwrap().clone())
    }

    #[test]
    fn debug_breakpoint_pauses_after_prior_outputs_and_resumes_current_step_once() {
        let prior_point = MkPoint { x: 37, y: 91 };
        let producer = step(
            1,
            MkAction::SetVariable {
                name: "point".into(),
                value: MkValue::Point(prior_point),
            },
        );
        let mut breakpoint_step = step(
            2,
            MkAction::MouseMove(super::super::MkMouseMovePayload {
                target: MkCoordinateTarget::Variable {
                    name: "point".into(),
                },
                duration_ms: 0,
            }),
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![producer, breakpoint_step]);

        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let lifecycle = Arc::new(Mutex::new(Vec::new()));
        let worker_lifecycle = lifecycle.clone();
        let worker_control = control.clone();
        let worker_fake = fake.clone();
        let (paused_tx, paused_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            Executor::new(worker_fake.backends(), worker_control).execute(
                &plan,
                ExecutionOptions::debug(),
                &|event| match event {
                    ExecutionEvent::StepStarted(1) => {
                        worker_lifecycle.lock().unwrap().push("started:1")
                    }
                    ExecutionEvent::StepFinished(1) => {
                        worker_lifecycle.lock().unwrap().push("finished:1")
                    }
                    ExecutionEvent::BreakpointHit {
                        step_id: 2,
                        variables,
                    } => {
                        worker_lifecycle.lock().unwrap().push("breakpoint:2");
                        paused_tx.send(variables).unwrap();
                    }
                    ExecutionEvent::StepStarted(2) => {
                        worker_lifecycle.lock().unwrap().push("started:2")
                    }
                    ExecutionEvent::StepFinished(2) => {
                        worker_lifecycle.lock().unwrap().push("finished:2")
                    }
                    _ => {}
                },
            )
        });

        let breakpoint_variables = paused_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            *lifecycle.lock().unwrap(),
            ["started:1", "finished:1", "breakpoint:2"]
        );
        assert_eq!(
            breakpoint_variables.get("macro.id"),
            Some(&MkValue::Number(7.0))
        );
        assert_eq!(
            breakpoint_variables.get("last_action_success"),
            Some(&MkValue::Boolean(true))
        );
        assert_eq!(
            breakpoint_variables.get("point"),
            Some(&MkValue::Point(prior_point))
        );
        assert_eq!(
            breakpoint_variables.get("step.id"),
            Some(&MkValue::Number(2.0))
        );
        assert!(
            fake.events().is_empty(),
            "breakpoint action ran before Resume"
        );
        assert!(fake.resolved_variables.lock().unwrap().is_empty());

        control.resume();
        worker.join().unwrap().unwrap();

        assert_eq!(
            *lifecycle.lock().unwrap(),
            [
                "started:1",
                "finished:1",
                "breakpoint:2",
                "started:2",
                "finished:2"
            ]
        );
        assert_eq!(fake.events(), ["move:37,91"]);
        let resolved = fake.resolved_variables.lock().unwrap();
        assert_eq!(
            resolved.len(),
            1,
            "Resume must execute the current step once"
        );
        assert_eq!(resolved[0].get("point"), Some(&MkValue::Point(prior_point)));
        assert_eq!(resolved[0].get("step.id"), Some(&MkValue::Number(2.0)));
    }

    #[test]
    fn breakpoint_event_contains_prior_outputs_but_not_pending_action_outputs() {
        let producer = step(
            1,
            MkAction::SetVariable {
                name: "prior_output".into(),
                value: MkValue::String("ready".into()),
            },
        );
        let mut breakpoint_step = step(
            2,
            MkAction::SetVariable {
                name: "pending_output".into(),
                value: MkValue::String("not yet".into()),
            },
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![producer, breakpoint_step]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let events = Mutex::new(Vec::new());

        Executor::new(fake.backends(), control.clone())
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id, variables } => {
                    assert_eq!(step_id, 2);
                    assert_eq!(variables.get("macro.id"), Some(&MkValue::Number(7.0)));
                    assert_eq!(
                        variables.get("last_action_success"),
                        Some(&MkValue::Boolean(true))
                    );
                    assert_eq!(
                        variables.get("prior_output"),
                        Some(&MkValue::String("ready".into()))
                    );
                    assert_eq!(variables.get("step.id"), Some(&MkValue::Number(2.0)));
                    assert!(!variables.contains_key("pending_output"));
                    events.lock().unwrap().push("breakpoint");
                    control.resume();
                }
                ExecutionEvent::StepStarted(2) => events.lock().unwrap().push("started"),
                ExecutionEvent::StepOutcome(2, _) => events.lock().unwrap().push("outcome"),
                ExecutionEvent::StepFinished(2) => events.lock().unwrap().push("finished"),
                _ => {}
            })
            .unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            ["breakpoint", "started", "finished"]
        );
    }

    #[test]
    fn breakpoint_event_precedes_step_outcome_and_finish_after_resume() {
        let mut breakpoint_step = step(
            1,
            MkAction::ImageFind(MkImagePayload {
                asset_id: 10,
                wait: MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 1,
                },
                region: Default::default(),
                tolerance: 0,
                alpha: Default::default(),
                return_point: Default::default(),
                not_found_policy: MkImageNotFoundPolicy::Continue,
                outputs: MkImageOutputs::default(),
            }),
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![breakpoint_step]);
        let fake = Arc::new(FakeBackend::default());
        fake.script_image(10, Ok(Some(MkPoint { x: 4, y: 8 })));
        let control = Arc::new(RunControl::default());
        control.reset();
        let events = Mutex::new(Vec::new());

        Executor::new(fake.backends(), control.clone())
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id: 1, .. } => {
                    events.lock().unwrap().push("breakpoint");
                    control.resume();
                }
                ExecutionEvent::StepStarted(1) => events.lock().unwrap().push("started"),
                ExecutionEvent::StepOutcome(1, outcome) => {
                    assert_eq!(outcome.last_image_found, Some(true));
                    events.lock().unwrap().push("outcome");
                }
                ExecutionEvent::StepFinished(1) => events.lock().unwrap().push("finished"),
                _ => {}
            })
            .unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            ["breakpoint", "started", "outcome", "finished"]
        );
    }

    #[test]
    fn stop_at_debug_breakpoint_cancels_before_start_or_condition_evaluation() {
        let matcher = MkWindowMatcher {
            title: Some("never queried".into()),
            ..Default::default()
        };
        let mut breakpoint_step = step(1, MkAction::If(MkCondition::WindowExists { matcher }));
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![breakpoint_step, step(2, MkAction::EndIf)]);

        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let lifecycle = Arc::new(Mutex::new(Vec::new()));
        let worker_lifecycle = lifecycle.clone();
        let worker_control = control.clone();
        let worker_fake = fake.clone();
        let (paused_tx, paused_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            Executor::new(worker_fake.backends(), worker_control).execute(
                &plan,
                ExecutionOptions::debug(),
                &|event| match event {
                    ExecutionEvent::BreakpointHit { step_id: 1, .. } => {
                        worker_lifecycle.lock().unwrap().push("breakpoint");
                        paused_tx.send(()).unwrap();
                    }
                    ExecutionEvent::StepStarted(1) => {
                        worker_lifecycle.lock().unwrap().push("started")
                    }
                    _ => {}
                },
            )
        });

        paused_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(*lifecycle.lock().unwrap(), ["breakpoint"]);
        assert!(fake.window_calls.lock().unwrap().is_empty());
        control.stop();

        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(*lifecycle.lock().unwrap(), ["breakpoint"]);
        assert!(fake.window_calls.lock().unwrap().is_empty());
        assert!(fake.events().is_empty());
    }

    #[test]
    fn resume_emitted_from_breakpoint_observer_is_not_lost_before_checkpoint() {
        let mut breakpoint_step = step(
            1,
            MkAction::Text(MkTextPayload {
                text: "resumed".into(),
                mode: crate::mkmacro::MkTextMode::Type,
            }),
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![breakpoint_step]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let events = Mutex::new(Vec::new());

        Executor::new(fake.clone().backends(), control.clone())
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id: 1, .. } => {
                    events.lock().unwrap().push("breakpoint");
                    // The observer runs before Executor calls checkpoint. This
                    // deterministically exercises Resume-before-wait.
                    control.resume();
                }
                ExecutionEvent::StepStarted(1) => events.lock().unwrap().push("started"),
                ExecutionEvent::StepFinished(1) => events.lock().unwrap().push("finished"),
                _ => {}
            })
            .unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            ["breakpoint", "started", "finished"]
        );
        assert_eq!(fake.events(), ["text:resumed"]);
        assert!(
            !control.is_active(),
            "executor worker activity was not cleared"
        );
    }

    #[test]
    fn stop_emitted_from_breakpoint_observer_is_seen_before_action_dispatch() {
        let held_button = step(1, MkAction::MouseDown(MkMouseButton::Left));
        let mut breakpoint_step = step(
            2,
            MkAction::MouseClick(super::super::MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 10, y: 20 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![held_button, breakpoint_step]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let events = Mutex::new(Vec::new());

        let error = Executor::new(fake.clone().backends(), control.clone())
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id: 2, .. } => {
                    events.lock().unwrap().push("breakpoint");
                    // As above, Stop is recorded before checkpoint starts.
                    control.stop();
                }
                ExecutionEvent::StepStarted(2) => events.lock().unwrap().push("started"),
                _ => {}
            })
            .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(*events.lock().unwrap(), ["breakpoint"]);
        assert_eq!(
            fake.events(),
            ["button_down:Left", "button_up:Left"],
            "the cleanup guard must release input owned before the breakpoint"
        );
        assert!(!fake.events().iter().any(|event| event.starts_with("move:")));
        assert!(
            !control.is_active(),
            "executor worker activity was not cleared"
        );
    }

    #[test]
    fn disabled_breakpoint_is_skipped_without_pausing_or_dispatching() {
        let mut disabled = step(
            1,
            MkAction::Text(MkTextPayload {
                text: "disabled".into(),
                mode: crate::mkmacro::MkTextMode::Type,
            }),
        );
        disabled.enabled = false;
        disabled.breakpoint = true;
        let plan = plan(vec![disabled]);

        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let lifecycle = Mutex::new(Vec::new());
        Executor::new(fake.clone().backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::StepSkipped(1) => lifecycle.lock().unwrap().push("skipped"),
                ExecutionEvent::StepStarted(1) => lifecycle.lock().unwrap().push("started"),
                ExecutionEvent::Paused => lifecycle.lock().unwrap().push("paused"),
                ExecutionEvent::BreakpointHit { .. } => {
                    lifecycle.lock().unwrap().push("breakpoint")
                }
                _ => {}
            })
            .unwrap();

        assert_eq!(*lifecycle.lock().unwrap(), ["skipped"]);
        assert!(fake.events().is_empty());
    }

    #[test]
    fn normal_mode_ignores_enabled_breakpoints() {
        let mut breakpoint_step = step(
            1,
            MkAction::Text(MkTextPayload {
                text: "breakpoint action".into(),
                mode: crate::mkmacro::MkTextMode::Type,
            }),
        );
        breakpoint_step.breakpoint = true;
        let plan = plan(vec![breakpoint_step]);

        let normal_fake = Arc::new(FakeBackend::default());
        let normal_control = Arc::new(RunControl::default());
        normal_control.reset();
        let normal_lifecycle = Mutex::new(Vec::new());
        Executor::new(normal_fake.clone().backends(), normal_control)
            .execute(&plan, ExecutionOptions::normal(), &|event| match event {
                ExecutionEvent::StepStarted(1) => normal_lifecycle.lock().unwrap().push("started"),
                ExecutionEvent::StepFinished(1) => {
                    normal_lifecycle.lock().unwrap().push("finished")
                }
                ExecutionEvent::Paused => normal_lifecycle.lock().unwrap().push("paused"),
                ExecutionEvent::BreakpointHit { .. } => {
                    normal_lifecycle.lock().unwrap().push("breakpoint")
                }
                _ => {}
            })
            .unwrap();
        assert_eq!(*normal_lifecycle.lock().unwrap(), ["started", "finished"]);
        assert_eq!(normal_fake.events(), ["text:breakpoint action"]);
    }

    #[test]
    fn breakpoint_fires_once_before_all_step_repetitions() {
        let mut repeated = step(
            1,
            MkAction::MouseClick(super::super::MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 10, y: 20 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
        );
        repeated.repeat = 5;
        repeated.breakpoint = true;
        let plan = plan(vec![repeated]);

        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let lifecycle = Arc::new(Mutex::new(Vec::new()));
        let captured_hits = hits.clone();
        let captured_lifecycle = lifecycle.clone();
        let observer_control = control.clone();

        Executor::new(fake.clone().backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id, .. } => {
                    captured_hits.lock().unwrap().push(step_id);
                    observer_control.resume();
                }
                ExecutionEvent::StepStarted(step_id) => captured_lifecycle
                    .lock()
                    .unwrap()
                    .push(("started", step_id)),
                ExecutionEvent::StepFinished(step_id) => captured_lifecycle
                    .lock()
                    .unwrap()
                    .push(("finished", step_id)),
                _ => {}
            })
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), [1]);
        assert_eq!(
            *lifecycle.lock().unwrap(),
            [("started", 1), ("finished", 1)]
        );
        assert_eq!(
            fake.events(),
            [
                "move:10,20",
                "button_down:Left",
                "button_up:Left",
                "move:10,20",
                "button_down:Left",
                "button_up:Left",
                "move:10,20",
                "button_down:Left",
                "button_up:Left",
                "move:10,20",
                "button_down:Left",
                "button_up:Left",
                "move:10,20",
                "button_down:Left",
                "button_up:Left",
            ]
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "button_down:Left")
                .count(),
            5
        );
    }

    #[test]
    fn debug_breakpoint_on_while_opener_observes_prior_safe_iteration_state() {
        let mut opener = step(
            1,
            MkAction::WhileStart {
                condition: MkCondition::WindowExists {
                    matcher: MkWindowMatcher {
                        title: Some("eventual".into()),
                        ..Default::default()
                    },
                },
            },
        );
        opener.breakpoint = true;
        let plan = plan(vec![
            opener,
            step(2, text_action("while-body")),
            step(
                3,
                MkAction::SetVariable {
                    name: "body_completed".into(),
                    value: MkValue::Boolean(true),
                },
            ),
            step(4, MkAction::WhileEnd),
        ]);

        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [true, true, false]);
        let control = Arc::new(RunControl::default());
        control.reset();
        let hit_variables = Arc::new(Mutex::new(Vec::new()));
        let captured_variables = hit_variables.clone();
        let observer_control = control.clone();
        let observer_fake = fake.clone();
        let hit_number = std::sync::atomic::AtomicUsize::new(0);

        Executor::new(fake.clone().backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::BreakpointHit { step_id, variables } => {
                    assert_eq!(step_id, 1);
                    let number = hit_number.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    assert_eq!(
                        observer_fake.window_calls.lock().unwrap().len(),
                        number,
                        "While's condition must be checked after, not before, each breakpoint"
                    );
                    if number == 0 {
                        assert!(!variables.contains_key("body_completed"));
                        assert!(!variables.contains_key("last_window_result"));
                    } else {
                        assert_eq!(
                            variables.get("body_completed"),
                            Some(&MkValue::Boolean(true))
                        );
                        assert_eq!(
                            variables.get("last_window_result"),
                            Some(&MkValue::Boolean(true))
                        );
                    }
                    captured_variables.lock().unwrap().push(variables);
                    if number == 1 {
                        assert_eq!(observer_fake.events(), ["text:while-body"]);
                    }
                    observer_control.resume();
                }
                _ => {}
            })
            .unwrap();

        assert_eq!(hit_number.load(std::sync::atomic::Ordering::Relaxed), 3);
        assert_eq!(hit_variables.lock().unwrap().len(), 3);
        assert_eq!(fake.events(), ["text:while-body", "text:while-body"]);
        assert_eq!(fake.window_calls.lock().unwrap().len(), 3);
    }

    #[test]
    fn debug_breakpoint_on_if_opener_precedes_condition_and_preserves_branching() {
        for (condition_result, expected_events) in [
            (
                true,
                vec![
                    "move:10,20".to_owned(),
                    "button_down:Left".to_owned(),
                    "button_up:Left".to_owned(),
                ],
            ),
            (false, vec![]),
        ] {
            let mut if_step = step(
                1,
                MkAction::If(MkCondition::WindowExists {
                    matcher: MkWindowMatcher {
                        title: Some("condition".into()),
                        ..Default::default()
                    },
                }),
            );
            if_step.breakpoint = true;
            let plan = plan(vec![
                if_step,
                step(
                    2,
                    MkAction::MouseClick(super::super::MkMousePayload {
                        target: MkCoordinateTarget::Screen {
                            point: MkPoint { x: 10, y: 20 },
                        },
                        button: MkMouseButton::Left,
                        clicks: 1,
                    }),
                ),
                step(3, MkAction::EndIf),
            ]);

            let fake = Arc::new(FakeBackend::default());
            fake.script_condition("window_exists", [condition_result]);
            let control = Arc::new(RunControl::default());
            control.reset();
            let observer_control = control.clone();
            let breakpoint_seen = Arc::new(Mutex::new(false));
            let captured_breakpoint = breakpoint_seen.clone();

            Executor::new(fake.clone().backends(), control)
                .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                    ExecutionEvent::BreakpointHit { step_id: 1, .. } => {
                        assert!(
                            fake.window_calls.lock().unwrap().is_empty(),
                            "the If condition must not be queried before its breakpoint"
                        );
                        *captured_breakpoint.lock().unwrap() = true;
                        observer_control.resume();
                    }
                    _ => {}
                })
                .unwrap();

            assert!(*breakpoint_seen.lock().unwrap());
            assert_eq!(fake.window_calls.lock().unwrap().len(), 1);
            assert_eq!(fake.events(), expected_events);
        }
    }

    #[test]
    fn compiled_structural_markers_can_break_and_resume_through_their_jumps() {
        let mut else_step = step(3, MkAction::Else);
        else_step.breakpoint = true;
        let mut end_if_step = step(8, MkAction::EndIf);
        end_if_step.breakpoint = true;
        let mut repeat_start = step(9, MkAction::RepeatStart { count: 2 });
        repeat_start.breakpoint = true;
        let mut repeat_end = step(11, MkAction::RepeatEnd);
        repeat_end.breakpoint = true;
        let mut while_end = step(14, MkAction::WhileEnd);
        while_end.breakpoint = true;
        let plan = plan(vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(2, text_action("if-true")),
            else_step,
            step(4, text_action("if-false")),
            step(5, MkAction::EndIf),
            step(6, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(7, text_action("if-without-else")),
            end_if_step,
            repeat_start,
            step(10, text_action("repeat")),
            repeat_end,
            step(
                12,
                MkAction::WhileStart {
                    condition: MkCondition::WindowExists {
                        matcher: MkWindowMatcher {
                            title: Some("loop".into()),
                            ..Default::default()
                        },
                    },
                },
            ),
            step(13, text_action("while")),
            while_end,
        ]);

        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [true, true, false]);
        let control = Arc::new(RunControl::default());
        control.reset();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let captured_hits = hits.clone();
        let observer_control = control.clone();

        Executor::new(fake.clone().backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| {
                if let ExecutionEvent::BreakpointHit { step_id, .. } = event {
                    captured_hits.lock().unwrap().push(step_id);
                    observer_control.resume();
                }
            })
            .unwrap();

        assert_eq!(*hits.lock().unwrap(), [3, 8, 9, 11, 11, 14, 14]);
        assert_eq!(
            fake.events(),
            [
                "text:if-true",
                "text:if-without-else",
                "text:repeat",
                "text:repeat",
                "text:while",
                "text:while"
            ]
        );
        assert_eq!(fake.window_calls.lock().unwrap().len(), 3);
    }

    #[test]
    fn disabled_compiled_structural_openers_skip_without_breaking_or_evaluating() {
        let mut disabled_if = step(
            1,
            MkAction::If(MkCondition::WindowExists {
                matcher: MkWindowMatcher {
                    title: Some("disabled if".into()),
                    ..Default::default()
                },
            }),
        );
        disabled_if.enabled = false;
        disabled_if.breakpoint = true;
        let mut disabled_repeat = step(4, MkAction::RepeatStart { count: 3 });
        disabled_repeat.enabled = false;
        disabled_repeat.breakpoint = true;
        let mut disabled_while = step(
            7,
            MkAction::WhileStart {
                condition: MkCondition::WindowExists {
                    matcher: MkWindowMatcher {
                        title: Some("disabled while".into()),
                        ..Default::default()
                    },
                },
            },
        );
        disabled_while.enabled = false;
        disabled_while.breakpoint = true;
        assert!(disabled_if.action.can_be_disabled());
        assert!(disabled_repeat.action.can_be_disabled());
        assert!(disabled_while.action.can_be_disabled());

        let plan = plan(vec![
            disabled_if,
            step(2, text_action("if-body")),
            step(3, MkAction::EndIf),
            disabled_repeat,
            step(5, text_action("repeat-body")),
            step(6, MkAction::RepeatEnd),
            disabled_while,
            step(8, text_action("while-body")),
            step(9, MkAction::Break),
            step(10, MkAction::WhileEnd),
        ]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let skipped = Arc::new(Mutex::new(Vec::new()));
        let breakpoints = Arc::new(Mutex::new(Vec::new()));
        let captured_skipped = skipped.clone();
        let captured_breakpoints = breakpoints.clone();

        Executor::new(fake.clone().backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| match event {
                ExecutionEvent::StepSkipped(step_id) => {
                    captured_skipped.lock().unwrap().push(step_id)
                }
                ExecutionEvent::BreakpointHit { step_id, .. } => {
                    captured_breakpoints.lock().unwrap().push(step_id)
                }
                _ => {}
            })
            .unwrap();

        assert_eq!(*skipped.lock().unwrap(), [1, 4, 7]);
        assert!(breakpoints.lock().unwrap().is_empty());
        assert_eq!(
            fake.events(),
            ["text:if-body", "text:repeat-body", "text:while-body"]
        );
        assert!(fake.window_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn normal_mode_ignores_breakpoints_without_changing_compiled_control_flow() {
        let mut if_step = step(1, MkAction::If(MkCondition::All { conditions: vec![] }));
        if_step.breakpoint = true;
        let mut else_step = step(3, MkAction::Else);
        else_step.breakpoint = true;
        let mut repeat_start = step(6, MkAction::RepeatStart { count: 2 });
        repeat_start.breakpoint = true;
        let mut repeat_end = step(8, MkAction::RepeatEnd);
        repeat_end.breakpoint = true;
        let mut while_end = step(11, MkAction::WhileEnd);
        while_end.breakpoint = true;
        let steps = vec![
            if_step,
            step(2, text_action("if-true")),
            else_step,
            step(4, text_action("if-false")),
            step(5, MkAction::EndIf),
            repeat_start,
            step(7, text_action("repeat")),
            repeat_end,
            step(
                9,
                MkAction::WhileStart {
                    condition: MkCondition::WindowExists {
                        matcher: MkWindowMatcher {
                            title: Some("normal mode while".into()),
                            ..Default::default()
                        },
                    },
                },
            ),
            step(10, text_action("while")),
            while_end,
        ];
        let plan = plan(steps);

        let debug_fake = Arc::new(FakeBackend::default());
        debug_fake.script_condition("window_exists", [true, true, false]);
        let normal_fake = Arc::new(FakeBackend::default());
        normal_fake.script_condition("window_exists", [true, true, false]);
        let (debug_result, debug_events) =
            execute_mode(&plan, debug_fake.clone(), ExecutionMode::Debug);
        let (normal_result, normal_events) =
            execute_mode(&plan, normal_fake.clone(), ExecutionMode::Normal);
        debug_result.unwrap();
        normal_result.unwrap();

        let started = |events: &[ExecutionEvent]| {
            events
                .iter()
                .filter_map(|event| match event {
                    ExecutionEvent::StepStarted(step_id) => Some(*step_id),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(started(&debug_events), started(&normal_events));
        assert_eq!(debug_fake.events(), normal_fake.events());
        assert_eq!(
            *debug_fake.window_calls.lock().unwrap(),
            *normal_fake.window_calls.lock().unwrap()
        );
        assert!(
            normal_events
                .iter()
                .all(|event| !matches!(event, ExecutionEvent::BreakpointHit { .. }))
        );
        assert!(
            debug_events
                .iter()
                .filter(|event| matches!(event, ExecutionEvent::BreakpointHit { .. }))
                .count()
                > 1
        );
    }

    #[test]
    fn repeated_breakpoint_resume_cycles_still_hit_the_control_transition_limit() {
        let mut opener = step(
            1,
            MkAction::WhileStart {
                condition: MkCondition::All { conditions: vec![] },
            },
        );
        opener.breakpoint = true;
        let plan = plan(vec![
            opener,
            step(
                2,
                MkAction::SetVariable {
                    name: "safe".into(),
                    value: MkValue::Boolean(true),
                },
            ),
            step(3, MkAction::WhileEnd),
        ]);
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let captured_hits = hits.clone();
        let observer_control = control.clone();

        let error = Executor::new(fake.backends(), control)
            .execute(&plan, ExecutionOptions::debug(), &|event| {
                if let ExecutionEvent::BreakpointHit { .. } = event {
                    let hit = captured_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if hit < 40_000 {
                        observer_control.resume();
                    } else {
                        // Bound a faulty implementation which resets the
                        // transition counter on every breakpoint resume.
                        observer_control.stop();
                    }
                }
            })
            .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::IterationLimit);
        let hit_count = hits.load(std::sync::atomic::Ordering::Relaxed);
        assert!(hit_count > 30_000, "only {hit_count} breakpoint cycles ran");
        assert!(hit_count < 40_001, "transition counter was bypassed");
    }

    #[test]
    fn debug_variable_snapshots_are_debug_only_boundary_events_and_are_owned() {
        let first = step(
            1,
            MkAction::SetVariable {
                name: "first_output".into(),
                value: MkValue::String("first".into()),
            },
        );
        let second = step(
            2,
            MkAction::SetVariable {
                name: "second_output".into(),
                value: MkValue::String("second".into()),
            },
        );
        let plan = plan(vec![first, second]);

        let normal_fake = Arc::new(FakeBackend::default());
        let normal_events = Mutex::new(Vec::new());
        Executor::new(normal_fake.backends(), Arc::new(RunControl::default()))
            .execute(&plan, ExecutionOptions::normal(), &|event| {
                if matches!(
                    event,
                    ExecutionEvent::DebugVariables { .. } | ExecutionEvent::BreakpointHit { .. }
                ) {
                    normal_events.lock().unwrap().push(event);
                }
            })
            .unwrap();
        assert_eq!(normal_events.lock().unwrap().len(), 0);

        let debug_fake = Arc::new(FakeBackend::default());
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let captured = snapshots.clone();
        Executor::new(debug_fake.backends(), Arc::new(RunControl::default()))
            .execute(&plan, ExecutionOptions::debug(), &|event| {
                if let ExecutionEvent::DebugVariables {
                    step_id,
                    variables,
                    reason,
                } = event
                {
                    captured.lock().unwrap().push((step_id, reason, variables));
                }
            })
            .unwrap();

        let snapshots = snapshots.lock().unwrap();
        assert_eq!(
            snapshots
                .iter()
                .map(|(step_id, reason, _)| (*step_id, *reason))
                .collect::<Vec<_>>(),
            vec![
                (None, DebugSnapshotReason::RunStarted),
                (Some(1), DebugSnapshotReason::StepBoundary),
                (Some(2), DebugSnapshotReason::StepBoundary),
                (Some(2), DebugSnapshotReason::RunFinished),
            ]
        );
        assert_eq!(snapshots[0].2.get("macro.id"), Some(&MkValue::Number(7.0)));
        assert_eq!(
            snapshots[0].2.get("last_action_success"),
            Some(&MkValue::Boolean(true))
        );
        assert!(!snapshots[0].2.contains_key("first_output"));
        assert_eq!(
            snapshots[1].2.get("first_output"),
            Some(&MkValue::String("first".into()))
        );
        assert_eq!(
            snapshots[3].2.get("second_output"),
            Some(&MkValue::String("second".into()))
        );
    }

    fn debug_snapshots(
        plan: &MkExecutionPlan,
        fake: Arc<FakeBackend>,
        control: Arc<RunControl>,
    ) -> (
        ExecResult,
        Vec<(Option<u64>, DebugSnapshotReason, RuntimeVariables)>,
    ) {
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let captured = snapshots.clone();
        let result = Executor::new(fake.backends(), control).execute(
            plan,
            ExecutionOptions::debug(),
            &|event| {
                if let ExecutionEvent::DebugVariables {
                    step_id,
                    variables,
                    reason,
                } = event
                {
                    captured.lock().unwrap().push((step_id, reason, variables));
                }
            },
        );
        (result, snapshots.lock().unwrap().clone())
    }

    fn image_search_condition(found: bool) -> MkCondition {
        MkCondition::ImageSearch {
            search: crate::mkmacro::MkImageSearchCondition {
                asset_id: 42,
                region: SearchRegion::Desktop,
                tolerance: 3,
                alpha: AlphaPolicy::Ignore,
                return_point: ReturnPoint::Center,
            },
            found,
        }
    }

    #[test]
    fn image_condition_results_are_snapshotted_after_if_and_while_evaluation() {
        let if_fake = Arc::new(FakeBackend::default());
        if_fake.script_image(42, Ok(Some(MkPoint { x: 4, y: 8 })));
        let if_plan = plan(vec![
            step(1, MkAction::If(image_search_condition(true))),
            step(
                2,
                MkAction::SetVariable {
                    name: "if_body".into(),
                    value: MkValue::Boolean(true),
                },
            ),
            step(3, MkAction::EndIf),
        ]);
        let (result, if_snapshots) =
            debug_snapshots(&if_plan, if_fake, Arc::new(RunControl::default()));
        result.unwrap();
        assert!(!if_snapshots[0].2.contains_key("last_image_result"));
        let if_boundary = if_snapshots
            .iter()
            .find(|(step_id, reason, _)| {
                *step_id == Some(1) && *reason == DebugSnapshotReason::StepBoundary
            })
            .unwrap();
        assert_eq!(
            if_boundary.2.get("last_image_result"),
            Some(&MkValue::Boolean(true))
        );
        assert_eq!(
            if_boundary.2.get("last_image_found"),
            Some(&MkValue::Boolean(true))
        );

        let while_fake = Arc::new(FakeBackend::default());
        while_fake.script_image(42, Ok(Some(MkPoint { x: 9, y: 10 })));
        while_fake.script_image(42, Ok(None));
        let while_plan = plan(vec![
            step(
                1,
                MkAction::WhileStart {
                    condition: image_search_condition(true),
                },
            ),
            step(
                2,
                MkAction::SetVariable {
                    name: "while_body".into(),
                    value: MkValue::Boolean(true),
                },
            ),
            step(3, MkAction::WhileEnd),
        ]);
        let (result, while_snapshots) =
            debug_snapshots(&while_plan, while_fake, Arc::new(RunControl::default()));
        result.unwrap();
        let while_boundaries = while_snapshots
            .iter()
            .filter(|(step_id, reason, _)| {
                *step_id == Some(1) && *reason == DebugSnapshotReason::StepBoundary
            })
            .map(|(_, _, variables)| variables.get("last_image_result").cloned())
            .collect::<Vec<_>>();
        assert_eq!(
            while_boundaries,
            vec![Some(MkValue::Boolean(true)), Some(MkValue::Boolean(false))]
        );
    }

    #[test]
    fn continue_error_publishes_the_failed_step_final_state() {
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "text:failed",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected failure"),
        );
        let mut failed = step(
            1,
            MkAction::Text(MkTextPayload {
                text: "failed".into(),
                mode: MkTextMode::Type,
            }),
        );
        failed.on_error = MkErrorPolicy::Continue;
        let plan = plan(vec![
            failed,
            step(
                2,
                MkAction::SetVariable {
                    name: "after_failure".into(),
                    value: MkValue::Boolean(true),
                },
            ),
        ]);
        let (result, snapshots) = debug_snapshots(&plan, fake, Arc::new(RunControl::default()));
        result.unwrap();
        let failed_boundary = snapshots
            .iter()
            .find(|(step_id, reason, _)| {
                *step_id == Some(1) && *reason == DebugSnapshotReason::StepBoundary
            })
            .unwrap();
        assert_eq!(
            failed_boundary.2.get("last_action_success"),
            Some(&MkValue::Boolean(false))
        );
        assert_eq!(
            snapshots.last().unwrap().2.get("after_failure"),
            Some(&MkValue::Boolean(true))
        );
        assert_eq!(
            snapshots.last().unwrap().2.get("last_action_success"),
            Some(&MkValue::Boolean(true))
        );
    }

    #[test]
    fn failed_run_publishes_only_the_last_safe_snapshot() {
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "text:failed",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected failure"),
        );
        let plan = plan(vec![
            step(
                1,
                MkAction::SetVariable {
                    name: "safe".into(),
                    value: MkValue::String("complete".into()),
                },
            ),
            step(
                2,
                MkAction::Text(MkTextPayload {
                    text: "failed".into(),
                    mode: MkTextMode::Type,
                }),
            ),
        ]);
        let (result, snapshots) = debug_snapshots(&plan, fake, Arc::new(RunControl::default()));
        let error = result.unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(
            snapshots
                .iter()
                .map(|(step_id, reason, _)| (*step_id, *reason))
                .collect::<Vec<_>>(),
            vec![
                (None, DebugSnapshotReason::RunStarted),
                (Some(1), DebugSnapshotReason::StepBoundary),
                (Some(1), DebugSnapshotReason::RunFailed),
            ]
        );
        assert_eq!(snapshots[2].2, snapshots[1].2);
        assert_eq!(
            snapshots[2].2.get("safe"),
            Some(&MkValue::String("complete".into()))
        );
        assert_eq!(snapshots[2].2.get("step.id"), Some(&MkValue::Number(1.0)));
    }

    #[test]
    fn cancelled_run_retains_the_last_safe_snapshot() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        let plan = plan(vec![
            step(
                1,
                MkAction::SetVariable {
                    name: "safe".into(),
                    value: MkValue::String("complete".into()),
                },
            ),
            step(2, MkAction::Delay(MkDelayPayload::default())),
        ]);
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let captured = snapshots.clone();
        let worker_control = control.clone();
        let result = Executor::new(fake.backends(), control).execute(
            &plan,
            ExecutionOptions::debug(),
            &|event| {
                if matches!(event, ExecutionEvent::StepStarted(2)) {
                    worker_control.stop();
                }
                if let ExecutionEvent::DebugVariables {
                    step_id,
                    variables,
                    reason,
                } = event
                {
                    captured.lock().unwrap().push((step_id, reason, variables));
                }
            },
        );
        assert_eq!(result.unwrap_err().kind, DiagnosticKind::Cancelled);
        let snapshots = snapshots.lock().unwrap();
        assert_eq!(
            snapshots
                .last()
                .map(|(step_id, reason, _)| (*step_id, *reason)),
            Some((Some(1), DebugSnapshotReason::RunCancelled))
        );
        assert_eq!(snapshots.last().unwrap().2, snapshots[1].2);
        assert_eq!(
            snapshots.last().unwrap().2.get("safe"),
            Some(&MkValue::String("complete".into()))
        );
    }

    #[test]
    fn playback_duration_scaling_is_ceiling_and_saturating() {
        assert_eq!(scale_playback_duration(0, 50), 0);
        assert_eq!(scale_playback_duration(5, 50), 10);
        assert_eq!(scale_playback_duration(5, 100), 5);
        assert_eq!(scale_playback_duration(5, 200), 3);
        assert_eq!(scale_playback_duration(1, 1000), 1);
        assert_eq!(scale_playback_duration(u64::MAX, 50), u64::MAX);
    }

    #[test]
    fn sampled_delay_addition_is_deterministic_and_saturating() {
        assert_eq!(add_sampled_random_delay(10, 0), 10);
        assert_eq!(add_sampled_random_delay(10, 7), 17);
        assert_eq!(add_sampled_random_delay(u64::MAX, 1), u64::MAX);
    }
    struct FixedRng(u64);

    impl rand::TryRng for FixedRng {
        type Error = std::convert::Infallible;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            Ok(self.0 as u32)
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            Ok(self.0)
        }

        fn try_fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error> {
            let word = self.0.to_ne_bytes();
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = word[index % word.len()];
            }
            Ok(())
        }
    }

    fn random_delay(minimum_ms: u64, maximum_ms: u64) -> MkAction {
        MkAction::Delay(super::super::MkDelayPayload {
            mode: MkDelayMode::RandomRange,
            minimum_ms,
            maximum_ms,
            ..Default::default()
        })
    }

    fn recorded_delay_executor(waiter: Arc<RecordingWaiter>) -> Executor {
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::with_waiter(Arc::new(FakeBackend::default()).backends(), control, waiter)
    }

    fn execute_delay(executor: &Executor, action: MkAction, speed_percent: u32) -> ExecResult {
        let mut plan = plan(vec![step(1, action)]);
        plan.playback.speed_percent = speed_percent;
        executor.execute(&plan, ExecutionOptions::normal(), &|_| {})
    }

    #[test]
    fn random_delay_schedules_known_sample_at_100_and_200_percent() {
        for (speed, expected_ms) in [(100, 900), (200, 450)] {
            let waiter = Arc::new(RecordingWaiter::default());
            let mut executor = recorded_delay_executor(waiter.clone());
            executor.delay_sampler = Arc::new(|min, max| {
                assert_eq!((min, max), (500, 1500));
                Ok(900)
            });
            execute_delay(&executor, random_delay(500, 1500), speed).unwrap();
            assert_eq!(waiter.sleeps(), [Duration::from_millis(expected_ms)]);
        }
    }

    #[test]
    fn fixed_delay_schedules_1000_ms_at_100_and_200_percent_without_sampling() {
        for (speed, expected_ms) in [(100, 1000), (200, 500)] {
            let waiter = Arc::new(RecordingWaiter::default());
            let mut executor = recorded_delay_executor(waiter.clone());
            executor.delay_sampler = Arc::new(|_, _| panic!("fixed delays must not sample"));
            execute_delay(
                &executor,
                MkAction::Delay(super::super::MkDelayPayload {
                    fixed_ms: 1000,
                    ..Default::default()
                }),
                speed,
            )
            .unwrap();
            assert_eq!(waiter.sleeps(), [Duration::from_millis(expected_ms)]);
        }
    }

    #[test]
    fn random_delay_equal_endpoints_schedule_that_value() {
        for value in [1, 900, super::super::MAX_DELAY_MS] {
            let waiter = Arc::new(RecordingWaiter::default());
            let executor = recorded_delay_executor(waiter.clone());
            execute_delay(&executor, random_delay(value, value), 100).unwrap();
            assert_eq!(waiter.sleeps(), [Duration::from_millis(value)]);
        }
    }

    #[test]
    fn random_delay_sampler_reaches_both_inclusive_endpoints() {
        assert_eq!(
            sample_delay_range(500, 1500, &mut FixedRng(0)).unwrap(),
            500
        );
        assert_eq!(
            sample_delay_range(500, 1500, &mut FixedRng(u64::MAX)).unwrap(),
            1500
        );
    }

    #[test]
    fn random_delay_reversed_range_returns_diagnostic_without_waiting() {
        let waiter = Arc::new(RecordingWaiter::default());
        let executor = recorded_delay_executor(waiter.clone());
        // Bypass compilation deliberately to exercise a malformed execution plan.
        // Validation's random_reversed_endpoints test covers authoring rejection.
        let mut plan = plan(vec![step(1, random_delay(500, 1500))]);
        Arc::make_mut(&mut Arc::make_mut(&mut plan.instructions)[0].step).action =
            random_delay(1500, 500);
        let error = executor
            .execute(&plan, ExecutionOptions::normal(), &|_| {})
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidPlan);
        assert_eq!(error.context["minimum_ms"], "1500");
        assert_eq!(error.context["maximum_ms"], "500");
        assert!(waiter.sleeps().is_empty());
    }

    #[test]
    fn zero_fixed_and_random_delays_complete_without_backend_sleep() {
        for action in [
            MkAction::Delay(super::super::MkDelayPayload {
                fixed_ms: 0,
                ..Default::default()
            }),
            random_delay(0, 0),
        ] {
            for speed in [100, 200] {
                let waiter = Arc::new(RecordingWaiter::default());
                let executor = recorded_delay_executor(waiter.clone());
                execute_delay(&executor, action.clone(), speed).unwrap();
                assert!(waiter.sleeps().is_empty());
            }
        }
    }

    #[test]
    fn random_delay_cancellation_at_scheduled_wait_stops_execution() {
        let waiter = Arc::new(RecordingWaiter::stop_after(1));
        let mut executor = recorded_delay_executor(waiter.clone());
        executor.delay_sampler = Arc::new(|_, _| Ok(900));
        let plan = plan(vec![
            step(1, random_delay(500, 1500)),
            step(2, MkAction::Delay(Default::default())),
        ]);
        let error = executor
            .execute(&plan, ExecutionOptions::normal(), &|_| {})
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(error.context["step_id"], "1");
        assert_eq!(waiter.sleeps(), [Duration::from_millis(900)]);
    }

    fn click_within_action(
        rect: super::super::ScreenRect,
        clicks: u32,
        edge_padding_px: u32,
    ) -> MkAction {
        MkAction::ClickWithinRegion(super::super::MkClickWithinRegionPayload {
            rect,
            button: MkMouseButton::Left,
            clicks,
            edge_padding_px,
        })
    }

    fn runtime_coordinate(vars: &RuntimeVariables, name: &str) -> i32 {
        match vars.get(name) {
            Some(MkValue::Number(value)) => *value as i32,
            other => panic!("expected runtime coordinate {name}, got {other:?}"),
        }
    }

    #[test]
    fn current_position_click_reads_cursor_once_at_action_entry_and_never_moves() {
        let fake = Arc::new(FakeBackend::default());
        let cursor = MkPoint { x: -321, y: 654 };
        *fake.cursor.lock().unwrap() = cursor;
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let action = MkAction::MouseClick(super::super::MkMousePayload {
            target: MkCoordinateTarget::CurrentPosition,
            button: MkMouseButton::Right,
            clicks: 3,
        });
        let playback = MkPlayback {
            random_offset_px: u32::MAX,
            ..MkPlayback::default()
        };
        let mut variables = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());

        executor
            .action(1, &action, &playback, &mut variables, &mut guard)
            .unwrap();

        // Current Position is sampled once when this action starts, not once
        // per click. The same backend point is used for every button pair.
        assert_eq!(
            fake.events(),
            [
                "cursor_position",
                "button_down:Right",
                "button_up:Right",
                "button_down:Right",
                "button_up:Right",
                "button_down:Right",
                "button_up:Right",
            ]
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "cursor_position")
                .count(),
            1
        );
        assert!(!fake.events().iter().any(|event| event.starts_with("move:")));
        assert!(fake.finalized_points.lock().unwrap().is_empty());
        assert!(fake.resolved_variables.lock().unwrap().is_empty());
        for prefix in ["mouse", "last_point"] {
            assert_eq!(
                runtime_coordinate(&variables, &format!("{prefix}.x")),
                cursor.x
            );
            assert_eq!(
                runtime_coordinate(&variables, &format!("{prefix}.y")),
                cursor.y
            );
        }
    }

    #[test]
    fn current_position_cursor_failure_stops_before_buttons_without_fabricating_point() {
        let fake = Arc::new(FakeBackend::default());
        let error = ExecutionDiagnostic::new(DiagnosticKind::Backend, "cursor unavailable");
        fake.fail("cursor_position", error.clone());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let action = MkAction::MouseClick(super::super::MkMousePayload {
            target: MkCoordinateTarget::CurrentPosition,
            button: MkMouseButton::Left,
            clicks: 2,
        });
        let mut variables = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());

        assert_eq!(
            executor
                .action(
                    1,
                    &action,
                    &MkPlayback::default(),
                    &mut variables,
                    &mut guard,
                )
                .unwrap_err(),
            error
        );
        assert_eq!(fake.events(), ["cursor_position"]);
        assert!(variables.is_empty());
        assert!(fake.finalized_points.lock().unwrap().is_empty());
    }

    #[test]
    fn ordinary_click_still_finalizes_and_moves_before_first_button_event() {
        let fake = Arc::new(FakeBackend::default());
        let point = MkPoint { x: 12, y: -8 };
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let action = MkAction::MouseClick(super::super::MkMousePayload {
            target: MkCoordinateTarget::Screen { point },
            button: MkMouseButton::Left,
            clicks: 1,
        });
        let mut variables = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());

        executor
            .action(
                1,
                &action,
                &MkPlayback::default(),
                &mut variables,
                &mut guard,
            )
            .unwrap();

        assert_eq!(*fake.finalized_points.lock().unwrap(), [point]);
        assert_eq!(
            fake.events(),
            ["move:12,-8", "button_down:Left", "button_up:Left"]
        );
        assert_eq!(
            fake.events()
                .iter()
                .position(|event| event.starts_with("button_down:")),
            Some(1)
        );
    }

    #[test]
    fn usable_region_bounds_are_inclusive_and_padding_is_edge_based() {
        assert_eq!(
            usable_region(super::super::ScreenRect::new(100, 200, 400, 300), 0).unwrap(),
            UsableRegion {
                min_x: 100,
                max_x: 499,
                min_y: 200,
                max_y: 499,
            }
        );
        assert_eq!(
            usable_region(super::super::ScreenRect::new(100, 200, 400, 300), 10).unwrap(),
            UsableRegion {
                min_x: 110,
                max_x: 489,
                min_y: 210,
                max_y: 489,
            }
        );
    }

    #[test]
    fn one_by_one_region_samples_its_only_point() {
        let region = usable_region(super::super::ScreenRect::new(-7, 11, 1, 1), 0).unwrap();
        assert_eq!(
            sample_point(region, &mut FixedRng(0)),
            MkPoint { x: -7, y: 11 }
        );
        assert_eq!(
            sample_point(region, &mut FixedRng(u64::MAX)),
            MkPoint { x: -7, y: 11 }
        );
    }

    #[test]
    fn negative_origins_and_invalid_padding_are_handled_without_wrapping() {
        assert_eq!(
            usable_region(super::super::ScreenRect::new(-100, -50, 20, 20), 5).unwrap(),
            UsableRegion {
                min_x: -95,
                max_x: -86,
                min_y: -45,
                max_y: -36,
            }
        );
        let error =
            usable_region(super::super::ScreenRect::new(100, 200, 400, 300), 200).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
    }

    #[test]
    fn sampling_includes_both_endpoints() {
        let region = usable_region(super::super::ScreenRect::new(100, 200, 400, 300), 0).unwrap();
        assert_eq!(
            sample_point(region, &mut FixedRng(0)),
            MkPoint { x: 100, y: 200 }
        );
        assert_eq!(
            sample_point(region, &mut FixedRng(u64::MAX)),
            MkPoint { x: 499, y: 499 }
        );
    }

    fn assert_recorded_moves_within(events: &[String], region: UsableRegion) -> Vec<MkPoint> {
        events
            .iter()
            .filter_map(|event| event.strip_prefix("move:"))
            .map(|coordinates| {
                let (x, y) = coordinates.split_once(',').unwrap();
                let point = MkPoint {
                    x: x.parse().unwrap(),
                    y: y.parse().unwrap(),
                };
                assert!((region.min_x..=region.max_x).contains(&point.x));
                assert!((region.min_y..=region.max_y).contains(&point.y));
                point
            })
            .collect()
    }

    #[test]
    fn click_within_region_repeated_execution_uses_inclusive_samples_and_balanced_left_clicks() {
        // Both halves of the word are at the midpoint for either RNG word size.
        const MIDDLE: u64 = 0x8000_0000_8000_0000;
        for (rect, padding, minimum, maximum, interior) in [
            (
                ScreenRect::new(100, 200, 400, 300),
                10,
                MkPoint { x: 110, y: 210 },
                MkPoint { x: 489, y: 489 },
                MkPoint { x: 300, y: 350 },
            ),
            (
                ScreenRect::new(-100, -50, 20, 20),
                5,
                MkPoint { x: -95, y: -45 },
                MkPoint { x: -86, y: -36 },
                MkPoint { x: -90, y: -40 },
            ),
            (
                ScreenRect::new(-7, 11, 5, 5),
                2,
                MkPoint { x: -5, y: 13 },
                MkPoint { x: -5, y: 13 },
                MkPoint { x: -5, y: 13 },
            ),
            (
                ScreenRect::new(i32::MAX, i32::MIN, 1, 1),
                0,
                MkPoint {
                    x: i32::MAX,
                    y: i32::MIN,
                },
                MkPoint {
                    x: i32::MAX,
                    y: i32::MIN,
                },
                MkPoint {
                    x: i32::MAX,
                    y: i32::MIN,
                },
            ),
        ] {
            let region = usable_region(rect, padding).unwrap();
            let fake = Arc::new(FakeBackend::default());
            let control = Arc::new(RunControl::default());
            control.reset();
            let mut executor = Executor::new(fake.clone().backends(), control);
            // Repeated samples are intentional: randomness is allowed to repeat.
            let samples = Arc::new(Mutex::new(std::collections::VecDeque::from([
                0,
                u64::MAX,
                MIDDLE,
                MIDDLE,
                0,
                u64::MAX,
            ])));
            let pending = samples.clone();
            executor.region_point_sampler = Arc::new(move |actual_region| {
                assert_eq!(actual_region, region);
                let word = pending
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("one sample per execution");
                Ok(sample_point(actual_region, &mut FixedRng(word)))
            });
            let action = click_within_action(rect, 2, padding);
            // Also exercise normal compilation/validation for each legal region.
            plan(vec![step(1, action.clone())]);
            let playback = MkPlayback {
                random_offset_px: u32::MAX,
                ..MkPlayback::default()
            };
            let mut vars = RuntimeVariables::new();
            set_point(&mut vars, "mouse", MkPoint { x: -999, y: -999 });
            set_point(&mut vars, "last_point", MkPoint { x: -999, y: -999 });
            let mut guard = InputCleanupGuard::new(fake.clone());

            for expected in [minimum, maximum, interior, interior, minimum, maximum] {
                let before = fake.events().len();
                executor
                    .action(1, &action, &playback, &mut vars, &mut guard)
                    .unwrap();
                let events = fake.events();
                let execution_events = &events[before..];
                let moves = assert_recorded_moves_within(execution_events, region);
                assert_eq!(
                    moves,
                    vec![expected],
                    "exactly one move to the sampled point"
                );
                assert_eq!(
                    execution_events,
                    [
                        format!("move:{},{}", expected.x, expected.y),
                        "button_down:Left".into(),
                        "button_up:Left".into(),
                        "button_down:Left".into(),
                        "button_up:Left".into(),
                    ],
                );
                for prefix in ["mouse", "last_point"] {
                    assert_eq!(
                        runtime_coordinate(&vars, &format!("{prefix}.x")),
                        expected.x
                    );
                    assert_eq!(
                        runtime_coordinate(&vars, &format!("{prefix}.y")),
                        expected.y
                    );
                }
            }
            assert!(samples.lock().unwrap().is_empty());
            drop(guard);
            assert_eq!(fake.events().len(), 6 * 5, "no buttons left for cleanup");
        }
    }

    #[test]
    fn click_within_region_sampling_or_movement_failure_emits_no_clicks() {
        let rect = ScreenRect::new(100, 200, 400, 300);
        let region = usable_region(rect, 10).unwrap();
        let selected = MkPoint {
            x: region.min_x,
            y: region.min_y,
        };
        for fail_sampling in [true, false] {
            let fake = Arc::new(FakeBackend::default());
            let control = Arc::new(RunControl::default());
            control.reset();
            let mut executor = Executor::new(fake.clone().backends(), control);
            let error = ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected failure");
            let sampling_error = error.clone();
            executor.region_point_sampler = Arc::new(move |actual_region| {
                assert_eq!(actual_region, region);
                if fail_sampling {
                    Err(sampling_error.clone())
                } else {
                    Ok(sample_point(actual_region, &mut FixedRng(0)))
                }
            });
            if !fail_sampling {
                fake.fail(
                    &format!("move:{},{}", selected.x, selected.y),
                    error.clone(),
                );
            }
            let mut vars = RuntimeVariables::new();
            set_point(&mut vars, "mouse", MkPoint { x: -999, y: -999 });
            let mut guard = InputCleanupGuard::new(fake.clone());
            let found = executor
                .action(
                    1,
                    &click_within_action(rect, 2, 10),
                    &MkPlayback::default(),
                    &mut vars,
                    &mut guard,
                )
                .unwrap_err();
            assert_eq!(found, error);
            drop(guard);
            let events = fake.events();
            let moves = assert_recorded_moves_within(&events, region);
            if fail_sampling {
                assert!(events.is_empty(), "sampling failure must not move or click");
            } else {
                assert_eq!(moves, vec![selected]);
                assert_eq!(events, vec![format!("move:{},{}", selected.x, selected.y)]);
            }
            assert_eq!(runtime_coordinate(&vars, "mouse.x"), -999);
            assert_eq!(runtime_coordinate(&vars, "mouse.y"), -999);
        }
    }

    #[test]
    fn click_within_region_consumed_padding_fails_before_sampling_or_input() {
        for (rect, padding) in [
            (ScreenRect::new(100, 200, 20, 20), 10),
            (ScreenRect::new(-100, -50, 21, 21), 11),
            (ScreenRect::new(100, 200, 20, 40), 10),
            (ScreenRect::new(100, 200, 40, 20), 10),
            (ScreenRect::new(100, 200, 20, 20), u32::MAX),
        ] {
            let fake = Arc::new(FakeBackend::default());
            let control = Arc::new(RunControl::default());
            control.reset();
            let mut executor = Executor::new(fake.clone().backends(), control);
            executor.region_point_sampler = Arc::new(|_| panic!("invalid region must not sample"));
            let mut vars = RuntimeVariables::new();
            let mut guard = InputCleanupGuard::new(fake.clone());
            let error = executor
                .action(
                    1,
                    &click_within_action(rect, 1, padding),
                    &MkPlayback::default(),
                    &mut vars,
                    &mut guard,
                )
                .unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
            drop(guard);
            assert!(fake.events().is_empty());
            assert!(vars.is_empty());
        }
    }

    #[test]
    fn click_within_region_does_not_apply_playback_random_offset() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut vars = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());
        let playback = MkPlayback {
            random_offset_px: u32::MAX,
            ..MkPlayback::default()
        };
        executor
            .action(
                1,
                &click_within_action(super::super::ScreenRect::new(100, 200, 1, 1), 1, 0),
                &playback,
                &mut vars,
                &mut guard,
            )
            .unwrap();
        assert_eq!(
            fake.events(),
            vec!["move:100,200", "button_down:Left", "button_up:Left"]
        );
    }

    #[test]
    fn invalid_persisted_click_region_returns_a_diagnostic() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut vars = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());
        let error = executor
            .action(
                1,
                &click_within_action(super::super::ScreenRect::new(100, 200, 0, 1), 1, 0),
                &MkPlayback::default(),
                &mut vars,
                &mut guard,
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert!(fake.events().is_empty());
    }

    #[test]
    fn sequential_order_and_owned_input_cleanup() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let p = plan(vec![
            step(1, MkAction::KeyDown(MkKey::Control)),
            step(
                2,
                MkAction::Text(MkTextPayload {
                    text: "hello".into(),
                    mode: crate::mkmacro::MkTextMode::Type,
                }),
            ),
        ]);
        Executor::new(fake.clone().backends(), control)
            .execute(&p, ExecutionOptions::normal(), &|_| {})
            .unwrap();
        assert_eq!(
            fake.events(),
            vec!["key_down:Control", "text:hello", "key_up:Control"]
        );
    }
    #[test]
    fn never_releases_unowned_input() {
        let fake = Arc::new(FakeBackend::default());
        {
            let _guard = InputCleanupGuard::new(fake.clone());
        }
        assert!(fake.events().is_empty());
    }

    #[test]
    fn hotkey_cleanup_handles_every_down_failure_and_continues_after_up_failure() {
        let keys = [MkKey::Meta, MkKey::Control, MkKey::Left];
        for (position, failed_event) in ["key_down:Meta", "key_down:Control", "key_down:Left"]
            .into_iter()
            .enumerate()
        {
            let fake = Arc::new(FakeBackend::default());
            fake.fail(
                failed_event,
                ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "injected"),
            );
            let mut guard = InputCleanupGuard::new(fake.clone());
            assert!(guard.hotkey(&keys).is_err());
            let expected_releases: Vec<_> = keys[..position]
                .iter()
                .rev()
                .map(|key| format!("key_up:{key:?}"))
                .collect();
            assert!(fake.events().ends_with(&expected_releases));
        }

        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "key_up:Left",
            ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "injected"),
        );
        let mut guard = InputCleanupGuard::new(fake.clone());
        assert!(guard.hotkey(&keys).is_err());
        assert!(fake.events().ends_with(&[
            "key_up:Left".into(),
            "key_up:Control".into(),
            "key_up:Meta".into(),
        ]));
    }

    #[test]
    fn executor_dispatches_every_virtual_desktop_operation_to_injected_backend() {
        let fake = Arc::new(FakeBackend::default());
        let actions = [
            super::super::MkVirtualDesktopAction::Create,
            super::super::MkVirtualDesktopAction::SwitchLeft,
            super::super::MkVirtualDesktopAction::SwitchRight,
            super::super::MkVirtualDesktopAction::CloseCurrent,
            super::super::MkVirtualDesktopAction::GoTo { desktop: 3 },
        ];
        for (index, action) in actions.into_iter().enumerate() {
            let control = Arc::new(RunControl::default());
            control.reset();
            Executor::new(fake.clone().backends(), control)
                .execute(
                    &plan(vec![step(
                        index as u64 + 1,
                        MkAction::VirtualDesktop(action),
                    )]),
                    ExecutionOptions::normal(),
                    &|_| {},
                )
                .unwrap();
        }
        assert_eq!(*fake.virtual_desktop_calls.lock().unwrap(), actions);
        assert_eq!(
            fake.virtual_desktop_calls
                .lock()
                .unwrap()
                .iter()
                .filter(|action| {
                    matches!(
                        action,
                        super::super::MkVirtualDesktopAction::GoTo { desktop: 3 }
                    )
                })
                .count(),
            1
        );
        assert!(
            fake.events()
                .iter()
                .all(|event| event.starts_with("virtual_desktop:"))
        );
        assert!(fake.events().iter().all(|event| !event.starts_with("key_")));

        let diagnostic = ExecutionDiagnostic::new(DiagnosticKind::Backend, "desktop refused")
            .context("operation", "create");
        fake.fail("virtual_desktop:Create", diagnostic.clone());
        let control = Arc::new(RunControl::default());
        control.reset();
        let error = Executor::new(fake.clone().backends(), control)
            .execute(
                &plan(vec![step(
                    9,
                    MkAction::VirtualDesktop(super::super::MkVirtualDesktopAction::Create),
                )]),
                ExecutionOptions::normal(),
                &|_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, diagnostic.kind);
        assert_eq!(error.message, diagnostic.message);
        assert_eq!(error.context.get("operation"), Some(&"create".into()));
        assert_eq!(
            error.context.get("backend_operation"),
            Some(&"virtual desktop".into())
        );
        assert_eq!(error.context.get("attempt"), Some(&"1".into()));
        assert_eq!(
            error.context.get("attempts_exhausted"),
            Some(&"true".into())
        );

        let go_to_diagnostic = ExecutionDiagnostic::new(DiagnosticKind::Backend, "desktop refused")
            .context("operation", "go_to");
        fake.fail(
            "virtual_desktop:GoTo { desktop: 11 }",
            go_to_diagnostic.clone(),
        );
        let control = Arc::new(RunControl::default());
        control.reset();
        let error = Executor::new(fake.clone().backends(), control)
            .execute(
                &plan(vec![step(
                    10,
                    MkAction::VirtualDesktop(super::super::MkVirtualDesktopAction::GoTo {
                        desktop: 11,
                    }),
                )]),
                ExecutionOptions::normal(),
                &|_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, go_to_diagnostic.kind);
        assert_eq!(error.message, go_to_diagnostic.message);
        assert_eq!(error.context.get("operation"), Some(&"go_to".into()));
        assert_eq!(
            error.context.get("action"),
            Some(&"GoTo { desktop: 11 }".into())
        );
        assert_eq!(
            error.context.get("backend_operation"),
            Some(&"virtual desktop".into())
        );
    }

    #[test]
    fn timed_recording_actions_use_smooth_move_and_drag_cleanup() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let target = |x, y| MkCoordinateTarget::Screen {
            point: MkPoint { x, y },
        };
        let p = plan(vec![
            step(
                1,
                MkAction::MouseMove(super::super::MkMouseMovePayload {
                    target: target(5, 6),
                    duration_ms: 1,
                }),
            ),
            step(
                2,
                MkAction::MouseDrag(super::super::MkMouseDragPayload {
                    from: target(5, 6),
                    to: target(9, 10),
                    button: MkMouseButton::Right,
                    duration_ms: 1,
                }),
            ),
        ]);
        Executor::new(fake.clone().backends(), control)
            .execute(&p, ExecutionOptions::normal(), &|_| {})
            .unwrap();
        let events = fake.events();
        assert_eq!(events.iter().filter(|e| *e == "cursor_position").count(), 1);
        let down = events
            .iter()
            .position(|e| e == "button_down:Right")
            .unwrap();
        let up = events.iter().position(|e| e == "button_up:Right").unwrap();
        assert_eq!(
            events.iter().filter(|e| *e == "button_down:Right").count(),
            1
        );
        assert_eq!(events.iter().filter(|e| *e == "button_up:Right").count(), 1);
        assert!(down < up);

        let cancelled = Arc::new(RunControl::default());
        cancelled.reset();
        cancelled.stop();
        let error = super::super::input::drag(
            &*fake,
            &cancelled,
            MkMouseButton::Left,
            MkPoint { x: 0, y: 0 },
            MkPoint { x: 100, y: 100 },
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert!(fake.events().ends_with(&[
            "move:0,0".into(),
            "button_down:Left".into(),
            "button_up:Left".into(),
        ]));
    }
    #[test]
    fn wait_image_records_legacy_and_asset_scoped_results_and_clears_stale_data() {
        let fake = Arc::new(FakeBackend::default());
        fake.conditions.lock().unwrap().insert("image".into(), true);
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let payload = |asset_id| MkImagePayload {
            asset_id,
            wait: MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 1,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs: MkImageOutputs::default(),
        };
        let mut vars = RuntimeVariables::new();
        assert_eq!(
            executor.wait_image(1, &payload(7), &mut vars).unwrap(),
            Some(MkPoint { x: 1, y: 1 })
        );
        assert_eq!(
            executor.wait_image(1, &payload(8), &mut vars).unwrap(),
            Some(MkPoint { x: 1, y: 1 })
        );
        for key in [
            "last_image",
            "last_image_x",
            "last_image_y",
            "last_image_result",
            "last_image_found",
            "__image.7",
            "__image.8",
        ] {
            assert!(
                vars.contains_key(key),
                "missing compatibility/runtime key {key}"
            );
        }
        assert_eq!(
            vars.get("last_image"),
            Some(&MkValue::Point(MkPoint { x: 1, y: 1 }))
        );

        fake.conditions
            .lock()
            .unwrap()
            .insert("image".into(), false);
        assert_eq!(
            executor.wait_image(1, &payload(7), &mut vars).unwrap(),
            None
        );
        assert!(!vars.contains_key("__image.7"));
        assert!(vars.contains_key("__image.8"));
        assert_eq!(vars.get("last_image_found"), Some(&MkValue::Boolean(false)));
        assert!(!vars.contains_key("last_image_x"));
    }
    #[test]
    fn stop_wakes_a_long_wait() {
        let control = Arc::new(RunControl::default());
        control.reset();
        let c = control.clone();
        let worker = std::thread::spawn(move || c.wait(Duration::from_secs(60)));
        std::thread::sleep(Duration::from_millis(10));
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
    }
    #[test]
    fn cancelled_executor_clears_runtime_activity() {
        let control = Arc::new(RunControl::default());
        control.reset();
        let worker_control = control.clone();
        let worker = std::thread::spawn(move || {
            Executor::new(Arc::new(FakeBackend::default()).backends(), worker_control).execute(
                &plan(vec![step(
                    1,
                    MkAction::Delay(crate::mkmacro::MkDelayPayload {
                        fixed_ms: 60_000,
                        ..Default::default()
                    }),
                )]),
                ExecutionOptions::normal(),
                &|_| {},
            )
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !control.is_active() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
        assert!(!control.is_active());
    }

    #[test]
    fn pixel_action_updates_outputs_for_match_mismatch_and_backend_error() {
        let fake = Arc::new(FakeBackend::default());
        let executor = Executor::new(fake.clone().backends(), Arc::new(RunControl::default()));
        let action = MkAction::PixelCheck {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: -1, y: 2 },
            },
            color: "#ABCDEF".into(),
            tolerance: 3,
        };
        let mut vars = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());
        fake.conditions.lock().unwrap().insert("pixel".into(), true);
        executor
            .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
            .unwrap();
        assert_eq!(vars.get("last_pixel_result"), Some(&MkValue::Boolean(true)));
        assert_eq!(vars.get("last_pixel_found"), Some(&MkValue::Boolean(true)));

        fake.conditions
            .lock()
            .unwrap()
            .insert("pixel".into(), false);
        assert_eq!(
            executor
                .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
                .unwrap_err()
                .kind,
            DiagnosticKind::TargetNotFound
        );
        assert_eq!(
            vars.get("last_pixel_result"),
            Some(&MkValue::Boolean(false))
        );
        assert_eq!(vars.get("last_pixel_found"), Some(&MkValue::Boolean(false)));

        fake.conditions
            .lock()
            .unwrap()
            .insert("pixel_error".into(), true);
        assert_eq!(
            executor
                .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
                .unwrap_err()
                .kind,
            DiagnosticKind::Backend
        );
        assert!(!vars.contains_key("last_pixel_result"));
        assert!(!vars.contains_key("last_pixel_found"));
    }
}
impl Drop for InputCleanupGuard {
    fn drop(&mut self) {
        for b in self.buttons.drain(..).rev() {
            if let Err(e) = self.backend.button_up(b) {
                tracing::error!(error=%e,"failed to release owned mouse button")
            }
        }
        for k in self.keys.drain(..).rev() {
            if let Err(e) = self.backend.key_up(&k) {
                tracing::error!(error=%e,"failed to release owned key")
            }
        }
    }
}

pub struct Executor {
    backends: Backends,
    control: Arc<RunControl>,
    waiter: Arc<dyn ExecutorWaiter>,
    delay_sampler: Arc<dyn Fn(u64, u64) -> ExecResult<u64> + Send + Sync>,
    region_point_sampler: Arc<dyn Fn(UsableRegion) -> ExecResult<MkPoint> + Send + Sync>,
}

/// Clock and interruptible-sleep boundary used by delays and polling actions. Keeping the
/// clock and sleep on the same boundary makes deadline tests deterministic.
trait ExecutorWaiter: Send + Sync {
    fn now(&self) -> Duration;
    fn wait(
        &self,
        duration: Duration,
        control: &RunControl,
        input: &dyn InputBackend,
    ) -> ExecResult;
}

struct SystemExecutorWaiter {
    epoch: Instant,
}
impl Default for SystemExecutorWaiter {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl ExecutorWaiter for SystemExecutorWaiter {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
    fn wait(
        &self,
        duration: Duration,
        control: &RunControl,
        input: &dyn InputBackend,
    ) -> ExecResult {
        let mut remaining = duration;
        while !remaining.is_zero() {
            if input.escape_pressed() {
                control.stop();
            }
            let slice = remaining.min(Duration::from_millis(10));
            control.wait(slice)?;
            remaining = remaining.saturating_sub(slice);
        }
        Ok(())
    }
}
#[cfg(test)]
#[derive(Default)]
struct RecordingWaiter {
    now: Mutex<Duration>,
    sleeps: Mutex<Vec<Duration>>,
    stop_after: Mutex<Option<usize>>,
}
#[cfg(test)]
impl RecordingWaiter {
    fn stop_after(cycles: usize) -> Self {
        Self {
            stop_after: Mutex::new(Some(cycles)),
            ..Self::default()
        }
    }
    fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
}
#[cfg(test)]
impl ExecutorWaiter for RecordingWaiter {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap()
    }
    fn wait(&self, duration: Duration, control: &RunControl, _: &dyn InputBackend) -> ExecResult {
        control.checkpoint()?;
        let count = {
            let mut sleeps = self.sleeps.lock().unwrap();
            sleeps.push(duration);
            sleeps.len()
        };
        *self.now.lock().unwrap() += duration;
        if *self.stop_after.lock().unwrap() == Some(count) {
            control.stop();
        }
        control.checkpoint()
    }
}

/// Scales playback pacing using ceiling division. Thus a non-zero duration remains
/// non-zero at every valid speed, and widened arithmetic cannot overflow.
pub fn scale_playback_duration(milliseconds: u64, speed_percent: u32) -> u64 {
    if milliseconds == 0 {
        return 0;
    }
    debug_assert!(
        speed_percent > 0,
        "compiler validation rejects zero playback speed"
    );
    if speed_percent == 0 {
        return u64::MAX;
    }
    let numerator = u128::from(milliseconds) * 100;
    let scaled = (numerator + u128::from(speed_percent) - 1) / u128::from(speed_percent);
    scaled.min(u128::from(u64::MAX)) as u64
}

fn offset_point(point: MkPoint, x: i64, y: i64) -> MkPoint {
    let add = |value: i32, delta: i64| {
        (i64::from(value).saturating_add(delta)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            as i32
    };
    MkPoint {
        x: add(point.x, x),
        y: add(point.y, y),
    }
}

/// Inclusive, point-coordinate bounds left inside a click rectangle after
/// applying the same padding to each edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsableRegion {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

/// Computes a rectangle's usable inclusive bounds without allowing malformed
/// persisted geometry to wrap around or create an invalid random range.
pub fn usable_region(rect: ScreenRect, padding: u32) -> ExecResult<UsableRegion> {
    rect.validate_capture().map_err(|error| {
        ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            format!("Click region rectangle is invalid: {error}"),
        )
    })?;

    let padding = i128::from(padding);
    let bounds = |origin: i32, dimension: u32| -> ExecResult<(i32, i32)> {
        let origin = i128::from(origin);
        let dimension = i128::from(dimension);
        let min = origin.checked_add(padding).ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Click region exceeds arithmetic limits",
            )
        })?;
        let max = origin
            .checked_add(dimension)
            .and_then(|value| value.checked_sub(1))
            .and_then(|value| value.checked_sub(padding))
            .ok_or_else(|| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "Click region exceeds arithmetic limits",
                )
            })?;
        if min > max {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Click region is smaller than its edge padding",
            ));
        }
        Ok((
            i32::try_from(min).map_err(|_| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "Click region exceeds coordinate limits",
                )
            })?,
            i32::try_from(max).map_err(|_| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "Click region exceeds coordinate limits",
                )
            })?,
        ))
    };

    let (min_x, max_x) = bounds(rect.x, rect.width)?;
    let (min_y, max_y) = bounds(rect.y, rect.height)?;
    Ok(UsableRegion {
        min_x,
        max_x,
        min_y,
        max_y,
    })
}

/// Samples each coordinate independently and uniformly from its inclusive
/// usable range. An RNG is injectable so execution can be tested deterministically.
pub fn sample_point<R: rand::Rng + ?Sized>(region: UsableRegion, rng: &mut R) -> MkPoint {
    MkPoint {
        x: rng.random_range(region.min_x..=region.max_x),
        y: rng.random_range(region.min_y..=region.max_y),
    }
}

/// Combines an already-scaled normal delay with a deterministic sampled value.
pub fn add_sampled_random_delay(normal: u64, sampled: u64) -> u64 {
    normal.saturating_add(sampled)
}

fn sample_delay(max: u64) -> u64 {
    if max == 0 {
        0
    } else {
        rand::random_range(0..=max)
    }
}
/// Samples an inclusive delay range; reject malformed plans before calling the RNG.
/// The RNG is injectable so endpoint behavior can be tested without chance.
fn sample_delay_range<R: rand::Rng + ?Sized>(
    min_ms: u64,
    max_ms: u64,
    rng: &mut R,
) -> ExecResult<u64> {
    if min_ms > max_ms {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidPlan,
            "random delay minimum exceeds maximum",
        )
        .context("minimum_ms", min_ms.to_string())
        .context("maximum_ms", max_ms.to_string()));
    }
    Ok(rng.random_range(min_ms..=max_ms))
}

fn sample_offset(max: u32) -> i64 {
    if max == 0 {
        0
    } else {
        rand::random_range(-i64::from(max)..=i64::from(max))
    }
}
impl Executor {
    const MAX_CONTROL_TRANSITIONS: u64 = 100_000;
    pub fn new(backends: Backends, control: Arc<RunControl>) -> Self {
        Self {
            backends,
            control,
            waiter: Arc::new(SystemExecutorWaiter::default()),
            delay_sampler: Arc::new(|min, max| sample_delay_range(min, max, &mut rand::rng())),
            region_point_sampler: Arc::new(|region| Ok(sample_point(region, &mut rand::rng()))),
        }
    }
    #[cfg(test)]
    fn with_waiter(
        backends: Backends,
        control: Arc<RunControl>,
        waiter: Arc<dyn ExecutorWaiter>,
    ) -> Self {
        Self {
            waiter,
            ..Self::new(backends, control)
        }
    }
    fn prompt_input(
        &self,
        p: &MkPromptInputPayload,
        variables: &mut RuntimeVariables,
    ) -> ExecResult {
        let expand = |field: &'static str, text: &str| {
            interpolate(text, variables).map_err(|error| error.context("field", field))
        };
        // Expand every prompt field before crossing the prompt effect boundary.
        let title = expand("prompt.title", &p.title)?;
        let prompt = expand("prompt.prompt", &p.prompt)?;
        let default_value = expand("prompt.default_value", &p.default_value)?;
        let response = self.backends.prompt.prompt(
            PromptRequest {
                id: super::prompt::next_request_id(),
                title,
                prompt,
                default_value,
            },
            &self.control,
        )?;
        match response {
            PromptResponse::Cancelled => Err(ExecutionDiagnostic::new(
                DiagnosticKind::Cancelled,
                "input prompt was cancelled",
            )),
            PromptResponse::Submitted(text) => {
                variables.insert(p.variable.clone(), MkValue::String(text.clone()));
                if p.copy_to_clipboard {
                    self.backends.clipboard.set_text(&text)?;
                }
                Ok(())
            }
        }
    }
    fn wait(&self, duration: Duration) -> ExecResult {
        self.control.checkpoint()?;
        self.waiter
            .wait(duration, &self.control, self.backends.input.as_ref())?;
        self.control.checkpoint()
    }

    fn wait_for_visual_change(&self, p: &super::WaitForVisualChange) -> ExecResult {
        let capture = || {
            self.backends
                .screenshot_capture
                .capture(&p.region, &|| self.control.is_stopped())
                .map_err(|e| {
                    e.context("action", "wait for visual change")
                        .context("region", format!("{:?}", p.region))
                })
        };
        // This frame is never replaced: every poll is measured against the
        // visual state at action entry rather than against the previous poll.
        let baseline = capture()?;
        let started = self.waiter.now();
        let timeout = p.timeout_duration();
        let required = p.consecutive_changed_frames.unwrap_or(1).max(1);
        let mut consecutive = 0u32;
        loop {
            self.control.checkpoint()?;
            let fresh = capture()?;
            let changed = match super::visual_frame_difference(
                &baseline,
                &fresh,
                p.per_pixel_tolerance.unwrap_or(0),
            )? {
                super::VisualFrameDifference::RegionSizeChanged => true,
                super::VisualFrameDifference::ChangedPixelPercent(percent) => {
                    percent >= p.change_threshold_percent
                }
            };
            // Drop the fresh frame before sleeping; only the baseline survives.
            drop(fresh);
            consecutive = if changed {
                consecutive.saturating_add(1)
            } else {
                0
            };
            if consecutive >= required {
                return Ok(());
            }
            let elapsed = self.waiter.now().saturating_sub(started);
            let sleep = match timeout {
                Some(timeout) if elapsed >= timeout => {
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Timeout,
                        "timed out waiting for visual change",
                    )
                    .context("timeout_ms", p.timeout_ms.to_string())
                    .context("threshold_percent", p.change_threshold_percent.to_string()));
                }
                Some(timeout) => {
                    Duration::from_millis(p.poll_interval_ms).min(timeout.saturating_sub(elapsed))
                }
                None => Duration::from_millis(p.poll_interval_ms),
            };
            self.wait(sleep)?;
        }
    }
    pub fn execute(
        &self,
        plan: &MkExecutionPlan,
        options: ExecutionOptions,
        observe: &dyn Fn(ExecutionEvent),
    ) -> ExecResult {
        let _activity = RunActivityGuard(&self.control);
        let mut guard = InputCleanupGuard::new(self.backends.input.clone());
        let mut vars = RuntimeVariables::new();
        let mut pc = 0;
        let mut loops: HashMap<usize, u32> = HashMap::new();
        let mut transitions = 0u64;
        let emit_debug_variables = |step_id, reason, variables: &RuntimeVariables| {
            if options.mode == ExecutionMode::Debug {
                observe(ExecutionEvent::DebugVariables {
                    step_id,
                    variables: variables.clone(),
                    reason,
                });
            }
        };
        vars.insert("macro.id".into(), MkValue::Number(plan.macro_id as f64));
        vars.insert("last_action_success".into(), MkValue::Boolean(true));
        emit_debug_variables(None, DebugSnapshotReason::RunStarted, &vars);
        // A snapshot is safe only after the complete instruction, including
        // its error policy and control-flow decision, has settled. This also
        // gives failure/cancellation paths a stable state to publish without
        // exposing variables from a partially executed action.
        let mut last_safe_variables = vars.clone();
        let mut last_safe_step_id = None;
        let result = (|| {
            while pc < plan.instructions.len() {
                if self.backends.input.escape_pressed() {
                    self.control.stop();
                }
                self.control.checkpoint()?;
                transitions += 1;
                if transitions > Self::MAX_CONTROL_TRANSITIONS {
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::IterationLimit,
                        "control-flow safety limit (100000 transitions) exceeded",
                    )
                    .context("limit", Self::MAX_CONTROL_TRANSITIONS.to_string()));
                }
                let ins = &plan.instructions[pc];
                let step = &ins.step;
                if !step.enabled {
                    observe(ExecutionEvent::StepSkipped(step.id));
                    pc += 1;
                    continue;
                }
                vars.insert("step.id".into(), MkValue::Number(step.id as f64));
                if options.mode == ExecutionMode::Debug && step.breakpoint {
                    self.control.pause();
                    observe(ExecutionEvent::BreakpointHit {
                        step_id: step.id,
                        variables: vars.clone(),
                    });
                    self.control.checkpoint()?;
                }
                observe(ExecutionEvent::StepStarted(step.id));
                let mut final_error = None;
                for repetition in 0..step.repeat {
                    vars.insert("iteration".into(), MkValue::Number(repetition as f64));
                    let attempts = match (&step.action.is_structural(), &step.on_error) {
                        (true, _) => 1,
                        (_, super::MkErrorPolicy::Retry(r)) => r.attempts.max(1),
                        _ => 1,
                    };
                    for attempt in 1..=attempts {
                        tracing::debug!(
                            macro_id = plan.macro_id,
                            step_id = step.id,
                            attempt,
                            "executing macro step"
                        );
                        match self.action(
                            plan.macro_id,
                            &step.action,
                            &plan.playback,
                            &mut vars,
                            &mut guard,
                        ) {
                            Ok(()) => {
                                vars.insert("last_action_success".into(), MkValue::Boolean(true));
                                final_error = None;
                                break;
                            }
                            Err(e) => {
                                let e = e
                                    .context("step", step.id.to_string())
                                    .context("step_id", step.id.to_string())
                                    .context("backend_operation", action_name(&step.action))
                                    .context("attempt", attempt.to_string())
                                    .context(
                                        "attempts_exhausted",
                                        (attempt == attempts).to_string(),
                                    );
                                vars.insert("last_action_success".into(), MkValue::Boolean(false));
                                tracing::warn!(macro_id=plan.macro_id,step_id=step.id,attempt,error=%e,"macro step attempt failed");
                                final_error = Some(e);
                                if attempt < attempts
                                    && let super::MkErrorPolicy::Retry(r) = &step.on_error
                                {
                                    self.wait(Duration::from_millis(r.delay_ms))?
                                }
                            }
                        }
                    }
                    if final_error.is_some() {
                        break;
                    }
                    // Retry delay is error-policy backoff, not playback pacing, and is intentionally unscaled.
                    let normal =
                        scale_playback_duration(step.delay_after_ms, plan.playback.speed_percent);
                    let delay = if step.action.is_structural() {
                        normal
                    } else {
                        add_sampled_random_delay(
                            normal,
                            sample_delay(plan.playback.random_delay_ms),
                        )
                    };
                    if delay > 0 {
                        self.wait(Duration::from_millis(delay))?
                    }
                }
                if let Some(e) = final_error {
                    observe(ExecutionEvent::StepFailed(step.id, e.clone()));
                    if e.kind == DiagnosticKind::Cancelled
                        || !matches!(step.on_error, super::MkErrorPolicy::Continue)
                    {
                        return Err(e);
                    }
                } else {
                    let outcome = StepOutcome::for_action(&step.action, &vars);
                    if outcome.last_image_found.is_some() {
                        observe(ExecutionEvent::StepOutcome(step.id, outcome));
                    }
                    observe(ExecutionEvent::StepFinished(step.id))
                }
                let next_pc = match (&step.action, &ins.jump) {
                    (
                        MkAction::If(c) | MkAction::WhileStart { condition: c },
                        Jump::IfFalse(to),
                    ) => {
                        self.control.checkpoint()?;
                        if self.condition(plan.macro_id, c, &mut vars)? {
                            pc + 1
                        } else {
                            *to
                        }
                    }
                    (_, Jump::To(to) | Jump::Break(to) | Jump::Continue(to)) => *to,
                    (MkAction::RepeatStart { count }, Jump::RepeatBegin { exit })
                        if *count == 0 =>
                    {
                        *exit
                    }
                    (MkAction::RepeatStart { count }, _) => {
                        loops.insert(pc, *count);
                        pc + 1
                    }
                    (_, Jump::RepeatEnd { start, exit }) => {
                        let entry = loops.entry(start.saturating_sub(1)).or_default();
                        if *entry > 1 {
                            *entry -= 1;
                            *start
                        } else {
                            loops.remove(&start.saturating_sub(1));
                            *exit
                        }
                    }
                    (_, Jump::WhileEnd { condition }) => *condition,
                    _ => pc + 1,
                };
                last_safe_step_id = Some(step.id);
                last_safe_variables = vars.clone();
                emit_debug_variables(
                    last_safe_step_id,
                    DebugSnapshotReason::StepBoundary,
                    &last_safe_variables,
                );
                pc = next_pc;
            }
            Ok(())
        })();
        let terminal_reason = match &result {
            Ok(()) => DebugSnapshotReason::RunFinished,
            Err(error) if error.kind == DiagnosticKind::Cancelled => {
                DebugSnapshotReason::RunCancelled
            }
            Err(_) => DebugSnapshotReason::RunFailed,
        };
        emit_debug_variables(last_safe_step_id, terminal_reason, &last_safe_variables);
        result
    }
    fn action(
        &self,
        macro_id: u64,
        a: &MkAction,
        playback: &MkPlayback,
        v: &mut RuntimeVariables,
        g: &mut InputCleanupGuard,
    ) -> ExecResult {
        match a {
            MkAction::KeyDown(k) => g.down_key(k),
            MkAction::KeyUp(k) => g.up_key(k),
            MkAction::KeyPress(k) => {
                g.down_key(k)?;
                g.up_key(k)
            }
            MkAction::Hotkey(keys) => g.hotkey(keys),
            MkAction::Text(p) => {
                let mut expanded = p.clone();
                expanded.text =
                    interpolate(&p.text, v).map_err(|error| error.context("field", "text.text"))?;
                match expanded.mode {
                    crate::mkmacro::MkTextMode::Type => self.backends.input.text(&expanded),
                    crate::mkmacro::MkTextMode::Paste => {
                        let transaction = ClipboardTransaction::install(
                            self.backends.clipboard.clone(),
                            &expanded.text,
                        )?;
                        let paste_key = MkKey::Character("V".into());
                        let primary = (|| {
                            g.down_key(&MkKey::Control)?;
                            g.down_key(&paste_key)?;
                            g.up_key(&paste_key)?;
                            g.up_key(&MkKey::Control)?;
                            self.wait(PASTE_SETTLE_INTERVAL)
                        })();
                        transaction.finish(primary)
                    }
                }
            }
            MkAction::MouseMove(p) => self.move_to(p, playback, v),
            MkAction::MouseDrag(p) => {
                let from = self.finalize_target(&p.from, playback, v, "Mouse Drag")?;
                let to = self.finalize_target(&p.to, playback, v, "Mouse Drag")?;
                super::input::drag(
                    &*self.backends.input,
                    &self.control,
                    p.button.clone(),
                    from,
                    to,
                    Duration::from_millis(scale_playback_duration(
                        p.duration_ms,
                        playback.speed_percent,
                    )),
                )?;
                set_point(v, "mouse", to);
                set_point(v, "last_point", to);
                Ok(())
            }
            MkAction::MouseClick(p) => {
                if matches!(&p.target, MkCoordinateTarget::CurrentPosition) {
                    let point = self.backends.input.cursor_position()?;
                    self.click_at_current_position(point, p.button.clone(), p.clicks, v, g)
                } else {
                    let point = self.finalize_target(&p.target, playback, v, "Mouse Click")?;
                    self.click_at(point, p.button.clone(), p.clicks, v, g)
                }
            }
            MkAction::ClickWithinRegion(p) => {
                if p.clicks == 0 {
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::InvalidTarget,
                        "Click region click count must be at least 1",
                    ));
                }
                let region = usable_region(p.rect, p.edge_padding_px)?;
                let point = (self.region_point_sampler)(region)?;
                // A second offset would violate the rectangle boundary guarantee.
                self.click_at(point, p.button.clone(), p.clicks, v, g)
            }
            MkAction::MouseDown(b) => g.down_button(b.clone()),
            MkAction::MouseUp(b) => g.up_button(b.clone()),
            MkAction::MouseScroll { axis, i32_delta } => {
                self.backends.input.scroll(*axis, *i32_delta)
            }
            MkAction::Delay(payload) => {
                let milliseconds = match payload.mode {
                    MkDelayMode::Fixed => payload.fixed_ms,
                    MkDelayMode::RandomRange => {
                        (self.delay_sampler)(payload.minimum_ms, payload.maximum_ms)?
                    }
                };
                let duration = Duration::from_millis(scale_playback_duration(
                    milliseconds,
                    playback.speed_percent,
                ));
                if duration.is_zero() {
                    // No sleep is needed, but cancellation must still be observed.
                    self.control.checkpoint()
                } else {
                    self.wait(duration)
                }
            }
            MkAction::Process(p) => {
                let mut expanded = p.clone();
                expanded.arguments = p
                    .arguments
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| {
                        interpolate(argument, v).map_err(|error| {
                            error.context("field", format!("process.arguments[{index}]"))
                        })
                    })
                    .collect::<ExecResult<_>>()?;
                expanded.working_directory = p
                    .working_directory
                    .as_deref()
                    .filter(|directory| !directory.is_empty())
                    .map(|directory| {
                        interpolate(directory, v)
                            .map_err(|error| error.context("field", "process.working_directory"))
                    })
                    .transpose()?;
                self.backends.launcher.launch_process(&expanded)
            }
            MkAction::LauncherCommand(payload) => {
                if let Some(action) = &payload.legacy_resolved_action {
                    let field = |value: &str, name: &'static str| {
                        interpolate(value, v).map_err(|error| error.context("field", name))
                    };
                    let expanded = crate::actions::Action {
                        label: field(
                            &action.label,
                            "launcher_command.legacy_resolved_action.label",
                        )?,
                        desc: field(&action.desc, "launcher_command.legacy_resolved_action.desc")?,
                        action: field(
                            &action.action,
                            "launcher_command.legacy_resolved_action.action",
                        )?,
                        args: action
                            .args
                            .as_deref()
                            .map(|value| {
                                field(value, "launcher_command.legacy_resolved_action.args")
                            })
                            .transpose()?,
                    };
                    return self
                        .backends
                        .launcher
                        .resolved_legacy(&expanded, Arc::as_ref(&self.control));
                }

                let query = interpolate(&payload.query, v)
                    .map_err(|error| error.context("field", "launcher_command.query"))?;
                // The broker must observe the executor's own pause/stop state;
                // never substitute a fresh RunControl for this blocking call.
                self.backends
                    .launcher
                    .command(&query, Arc::as_ref(&self.control))
                    .map_err(|error| error.context("backend", "launcher").context("query", query))
            }
            MkAction::WindowActivate(p) => self.backends.window.activate(p),
            MkAction::WindowClose(m) => self.backends.window.close(m),
            MkAction::WindowMoveResize(p) => self.backends.window.move_resize(p),
            MkAction::WindowState { matcher, state } => {
                self.backends.window.set_state(matcher, *state)
            }
            MkAction::VirtualDesktop(action) => {
                let result = match action {
                    super::MkVirtualDesktopAction::Create => self.backends.virtual_desktop.create(),
                    super::MkVirtualDesktopAction::SwitchLeft => {
                        self.backends.virtual_desktop.switch_left()
                    }
                    super::MkVirtualDesktopAction::SwitchRight => {
                        self.backends.virtual_desktop.switch_right()
                    }
                    super::MkVirtualDesktopAction::CloseCurrent => {
                        self.backends.virtual_desktop.close_current()
                    }
                    super::MkVirtualDesktopAction::GoTo { desktop } => {
                        self.backends.virtual_desktop.go_to(*desktop)
                    }
                };
                result.map_err(|error| error.context("action", format!("{action:?}")))
            }
            MkAction::WindowWait(p) => self.wait_condition(
                macro_id,
                &MkCondition::WindowExists {
                    matcher: p.matcher.clone(),
                },
                p.wait.as_ref().unwrap_or(&MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                }),
                v,
            ),
            MkAction::WaitUntil { condition, wait } => {
                self.wait_condition(macro_id, condition, wait, v)
            }
            MkAction::SetVariable { name, value } => {
                v.insert(name.clone(), value.clone());
                Ok(())
            }
            MkAction::UnsetVariable { name } => {
                v.remove(name);
                Ok(())
            }
            MkAction::PromptInput(p) => self.prompt_input(p, v),
            MkAction::PlaySound(p) => {
                validate_macro_sound(p)?;
                self.backends.sound.play(&p.sound)
            }
            MkAction::Notify(p) => {
                let resolved = resolve_notification(p, v)?;
                self.backends.notification.notify(&resolved)
            }
            MkAction::ImageFind(p) => self.wait_image(macro_id, p, v).map(|_| ()),
            MkAction::ImageClick(p) => {
                let pt = self.wait_image(macro_id, p, v)?.ok_or_else(|| {
                    ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "Click Image requires a matching point",
                    )
                    .context("macro_id", macro_id.to_string())
                    .context("asset_id", p.asset_id.to_string())
                })?;
                self.backends.input.move_mouse(pt)?;
                set_point(v, "mouse", pt);
                set_point(v, "last_point", pt);
                g.down_button(MkMouseButton::Left)?;
                g.up_button(MkMouseButton::Left)
            }
            MkAction::FindPixel(p) => self.wait_pixel(p, v).map(|_| ()),
            MkAction::CaptureScreenshot(p) => {
                let path = if p.destination.produces_file() {
                    let template = p.path.as_deref().ok_or_else(|| {
                        ExecutionDiagnostic::new(
                            DiagnosticKind::InvalidTarget,
                            "file screenshot destination requires a path",
                        )
                    })?;
                    Some(
                        interpolate(template, v)
                            .map_err(|e| e.context("field", "capture_screenshot.path"))?,
                    )
                } else {
                    None
                };
                // Capture is intentionally performed once and the immutable frame is
                // then supplied to each requested sink.
                let frame = self
                    .backends
                    .screenshot_capture
                    .capture(&p.region, &|| self.control.is_stopped())
                    .map_err(|e| {
                        e.context("action", "capture screenshot")
                            .context("region", format!("{:?}", p.region))
                    })?;
                if let Some(path) = path {
                    let bytes = self
                        .backends
                        .screenshot_encoder
                        .encode(&frame, p.format)
                        .map_err(|e| e.context("format", format!("{:?}", p.format)))?;
                    let actual = self
                        .backends
                        .screenshot_files
                        .write_transactional(Path::new(&path), &bytes, p.collision)
                        .map_err(|e| e.context("path", path))?;
                    if let Some(output) = p.path_output.as_ref() {
                        v.insert(
                            output.clone(),
                            MkValue::String(actual.to_string_lossy().into_owned()),
                        );
                    }
                }
                if p.destination.produces_clipboard() {
                    self.backends
                        .clipboard
                        .set_image(&frame)
                        .map_err(|e| e.context("action", "publish screenshot to clipboard"))?;
                }
                Ok(())
            }
            MkAction::WaitForVisualChange(p) => self.wait_for_visual_change(p),
            MkAction::PixelCheck {
                target,
                color,
                tolerance,
            } => {
                // Never expose a result left behind by an earlier check if
                // coordinate resolution, capture, or color parsing fails.
                v.remove("last_pixel_result");
                v.remove("last_pixel_found");
                let matched = self
                    .backends
                    .screen
                    .pixel_matches(target, color, *tolerance, v)?;
                v.insert("last_pixel_result".into(), MkValue::Boolean(matched));
                v.insert("last_pixel_found".into(), MkValue::Boolean(matched));
                if matched {
                    Ok(())
                } else {
                    Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "pixel did not match",
                    ))
                }
            }
            MkAction::UiInvoke(p) => self.backends.uia.invoke(p),
            MkAction::UiSetValue { target, value } => self.backends.uia.set_value(target, value),
            MkAction::UiReadValue { target, variable } => {
                let value = self.backends.uia.read_value(target)?;
                v.insert(variable.clone(), MkValue::String(value));
                Ok(())
            }
            MkAction::UiToggle(p) => self.backends.uia.toggle(p),
            MkAction::UiSelect(p) => self.backends.uia.select(p),
            MkAction::UiFocus(p) => self.backends.uia.focus(p),
            MkAction::UiWait(p) => self.wait_until(
                p.wait.as_ref().unwrap_or(&MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                }),
                || self.backends.uia.exists(p),
            ),
            // Structural actions are deliberately executed by `run_program`; reaching
            // this payload dispatcher for one is therefore an intentional no-op.
            MkAction::If(_)
            | MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatStart { .. }
            | MkAction::RepeatEnd
            | MkAction::WhileStart { .. }
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue => Ok(()),
        }
    }
    fn finalize_target(
        &self,
        target: &MkCoordinateTarget,
        playback: &MkPlayback,
        v: &RuntimeVariables,
        action: &'static str,
    ) -> ExecResult<MkPoint> {
        let configured = self
            .backends
            .screen
            .resolve(target, v)
            .map_err(|mut error| {
                if error.kind == DiagnosticKind::TypeMismatch {
                    if let (Some(variable), Some(actual)) =
                        (error.context.get("variable"), error.context.get("actual"))
                    {
                        error.message = format!(
                            "Variable '{variable}' contains {actual}; {action} requires Point"
                        );
                    }
                    error.context.insert("action".into(), action.into());
                }
                error
            })?;
        let randomized = offset_point(
            configured,
            sample_offset(playback.random_offset_px),
            sample_offset(playback.random_offset_px),
        );
        self.backends.screen.finalize_point(randomized)
    }
    fn click_at(
        &self,
        point: MkPoint,
        button: MkMouseButton,
        clicks: u32,
        v: &mut RuntimeVariables,
        g: &mut InputCleanupGuard,
    ) -> ExecResult {
        set_point(v, "last_point", point);
        self.backends.input.move_mouse(point)?;
        set_point(v, "mouse", point);
        self.click_buttons(button, clicks, g)
    }
    fn click_at_current_position(
        &self,
        point: MkPoint,
        button: MkMouseButton,
        clicks: u32,
        v: &mut RuntimeVariables,
        g: &mut InputCleanupGuard,
    ) -> ExecResult {
        set_point(v, "last_point", point);
        set_point(v, "mouse", point);
        self.click_buttons(button, clicks, g)
    }
    fn click_buttons(
        &self,
        button: MkMouseButton,
        clicks: u32,
        g: &mut InputCleanupGuard,
    ) -> ExecResult {
        for _ in 0..clicks {
            g.down_button(button.clone())?;
            g.up_button(button.clone())?;
        }
        Ok(())
    }
    fn move_to(
        &self,
        payload: &super::MkMouseMovePayload,
        playback: &MkPlayback,
        v: &mut RuntimeVariables,
    ) -> ExecResult {
        let point = self.finalize_target(&payload.target, playback, v, "Mouse Move")?;
        if payload.duration_ms == 0 {
            self.backends.input.move_mouse(point)?;
        } else {
            let from = self.backends.input.cursor_position()?;
            self.backends.input.move_mouse_smooth(
                &self.control,
                from,
                point,
                Duration::from_millis(scale_playback_duration(
                    payload.duration_ms,
                    playback.speed_percent,
                )),
            )?;
        }
        set_point(v, "mouse", point);
        set_point(v, "last_point", point);
        Ok(())
    }
    fn wait_image(
        &self,
        macro_id: u64,
        p: &MkImagePayload,
        v: &mut RuntimeVariables,
    ) -> ExecResult<Option<MkPoint>> {
        // An attempted search invalidates stale coordinates immediately. This
        // is also important for operational failures: callers must never use a
        // point produced by an earlier attempt after decode/capture fails.
        Self::write_image_result(v, p, None);
        let started = Instant::now();
        let result: ExecResult<Option<MkPoint>> = loop {
            if let Err(error) = self.control.checkpoint() {
                break Err(error);
            }
            match self.backends.screen.find_image(macro_id, p) {
                Err(error) => break Err(error),
                Ok(Some(point)) => break Ok(Some(point)),
                Ok(None) => {}
            }
            let elapsed = started.elapsed();
            if elapsed >= Duration::from_millis(p.wait.timeout_ms) {
                // Give a stop racing with the final unsuccessful poll priority
                // over deadline reporting.
                if let Err(error) = self.control.checkpoint() {
                    break Err(error);
                }
                break match p.not_found_policy {
                    MkImageNotFoundPolicy::Continue => Ok(None),
                    MkImageNotFoundPolicy::Fail => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Timeout,
                        format!("Image was not found within {} ms", p.wait.timeout_ms),
                    )
                    .context("timeout_ms", p.wait.timeout_ms.to_string())
                    .context("elapsed_ms", elapsed.as_millis().to_string())),
                };
            }
            if let Err(error) = self.wait(
                Duration::from_millis(p.wait.poll_interval_ms.max(1))
                    .min(Duration::from_millis(p.wait.timeout_ms).saturating_sub(elapsed)),
            ) {
                break Err(error);
            }
        };
        match result {
            Ok(point) => {
                Self::write_image_result(v, p, point);
                Ok(point)
            }
            Err(error) => Err(error
                .context("macro_id", macro_id.to_string())
                .context("asset_id", p.asset_id.to_string())
                .context("region", format!("{:?}", p.region))
                .context("tolerance", p.tolerance.to_string())
                .context("alpha", format!("{:?}", p.alpha))
                .context("timeout_ms", p.wait.timeout_ms.to_string())
                .context("poll_interval_ms", p.wait.poll_interval_ms.to_string())
                .context("elapsed_ms", started.elapsed().as_millis().to_string())),
        }
    }
    fn wait_pixel(
        &self,
        p: &super::MkPixelSearchPayload,
        v: &mut RuntimeVariables,
    ) -> ExecResult<Option<MkPoint>> {
        Self::write_pixel_result(v, p, None);
        let started = Instant::now();
        loop {
            self.control.checkpoint()?;
            match self.backends.screen.find_pixel(p) {
                Err(e) => return Err(e),
                Ok(Some(point)) => {
                    Self::write_pixel_result(v, p, Some(point));
                    return Ok(Some(point));
                }
                Ok(None) if started.elapsed() >= Duration::from_millis(p.wait.timeout_ms) => {
                    Self::write_pixel_result(v, p, None);
                    return match p.not_found_policy {
                        MkImageNotFoundPolicy::Continue => Ok(None),
                        MkImageNotFoundPolicy::Fail => Err(ExecutionDiagnostic::new(
                            DiagnosticKind::TargetNotFound,
                            "Pixel color was not found",
                        )),
                    };
                }
                Ok(None) => self.wait(Duration::from_millis(p.wait.poll_interval_ms.max(1)))?,
            }
        }
    }
    fn write_pixel_result(
        v: &mut RuntimeVariables,
        p: &super::MkPixelSearchPayload,
        point: Option<MkPoint>,
    ) {
        let found = point.is_some();
        v.insert("last_pixel_result".into(), MkValue::Boolean(found));
        v.insert("last_pixel_found".into(), MkValue::Boolean(found));
        v.insert(
            super::screen::pixel_found_variable(p.search_id),
            MkValue::Boolean(found),
        );
        let key = super::screen::pixel_result_variable(p.search_id);
        if let Some(point) = point {
            v.insert(key, MkValue::Point(point));
            set_point(v, "last_pixel", point);
            v.insert("last_pixel_x".into(), MkValue::Number(point.x.into()));
            v.insert("last_pixel_y".into(), MkValue::Number(point.y.into()));
        } else {
            v.remove(&key);
            for key in [
                "last_pixel",
                "last_pixel.x",
                "last_pixel.y",
                "last_pixel_x",
                "last_pixel_y",
            ] {
                v.remove(key);
            }
        }
        if let Some(name) = &p.outputs.found {
            v.insert(name.clone(), MkValue::Boolean(found));
        }
        for name in [&p.outputs.point, &p.outputs.x, &p.outputs.y]
            .into_iter()
            .flatten()
        {
            v.remove(name);
        }
        if let Some(point) = point {
            if let Some(name) = &p.outputs.point {
                v.insert(name.clone(), MkValue::Point(point));
            }
            if let Some(name) = &p.outputs.x {
                v.insert(name.clone(), MkValue::Number(point.x.into()));
            }
            if let Some(name) = &p.outputs.y {
                v.insert(name.clone(), MkValue::Number(point.y.into()));
            }
        }
    }
    /// Commits one completed search snapshot.
    fn write_image_result(v: &mut RuntimeVariables, p: &MkImagePayload, point: Option<MkPoint>) {
        let found = point.is_some();
        v.insert("last_image_result".into(), MkValue::Boolean(found));
        v.insert("last_image_found".into(), MkValue::Boolean(found));
        let asset = super::screen::image_result_variable(p.asset_id);
        v.insert(
            super::screen::image_found_variable(p.asset_id),
            MkValue::Boolean(found),
        );
        if let Some(point) = point {
            v.insert(asset, MkValue::Point(point));
            v.insert("last_image".into(), MkValue::Point(point));
            set_point(v, "last_image", point);
            v.insert("last_image_x".into(), MkValue::Number(point.x.into()));
            v.insert("last_image_y".into(), MkValue::Number(point.y.into()));
        } else {
            v.remove(&asset);
            for key in [
                "last_image",
                "last_image.x",
                "last_image.y",
                "last_image_x",
                "last_image_y",
            ] {
                v.remove(key);
            }
        }
        for (name, value) in [
            (&p.outputs.found, MkValue::Boolean(found)),
            (
                &p.outputs.point,
                point.map(MkValue::Point).unwrap_or(MkValue::Null),
            ),
            (
                &p.outputs.x,
                point
                    .map(|p| MkValue::Number(p.x.into()))
                    .unwrap_or(MkValue::Null),
            ),
            (
                &p.outputs.y,
                point
                    .map(|p| MkValue::Number(p.y.into()))
                    .unwrap_or(MkValue::Null),
            ),
        ] {
            if let Some(name) = name {
                v.insert(name.clone(), value);
            }
        }
    }
    fn wait_condition(
        &self,
        macro_id: u64,
        condition: &MkCondition,
        o: &MkWaitOptions,
        v: &mut RuntimeVariables,
    ) -> ExecResult {
        let started = self.waiter.now();
        let timeout = o.timeout_duration();
        let mut polls = 0u64;
        loop {
            self.control.checkpoint()?;
            polls += 1;
            if self.condition(macro_id, condition, v)? {
                return Ok(());
            }
            let elapsed = self.waiter.now().saturating_sub(started);
            if timeout.is_some_and(|timeout| elapsed >= timeout) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Timeout,
                    format!("condition timed out after {} ms", o.timeout_ms),
                )
                .context("timeout_ms", o.timeout_ms.to_string())
                .context("poll_interval_ms", o.poll_interval_ms.to_string())
                .context("polls", polls.to_string()));
            }
            let sleep = timeout.map_or(Duration::from_millis(o.poll_interval_ms), |timeout| {
                Duration::from_millis(o.poll_interval_ms).min(timeout.saturating_sub(elapsed))
            });
            self.wait(sleep)?;
        }
    }
    fn wait_until(
        &self,
        o: &MkWaitOptions,
        mut poll: impl FnMut() -> ExecResult<bool>,
    ) -> ExecResult {
        let start = self.waiter.now();
        let timeout = o.timeout_duration();
        loop {
            self.control.checkpoint()?;
            if poll()? {
                return Ok(());
            }
            let elapsed = self.waiter.now().saturating_sub(start);
            if timeout.is_some_and(|timeout| elapsed >= timeout) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Timeout,
                    format!("condition timed out after {} ms", o.timeout_ms),
                ));
            }
            let sleep = timeout.map_or(Duration::from_millis(o.poll_interval_ms), |timeout| {
                Duration::from_millis(o.poll_interval_ms).min(timeout.saturating_sub(elapsed))
            });
            self.wait(sleep)?
        }
    }
    fn condition(
        &self,
        macro_id: u64,
        c: &MkCondition,
        v: &mut RuntimeVariables,
    ) -> ExecResult<bool> {
        match c {
            MkCondition::Variable { name, op, value } => {
                compare(v.get(name).unwrap_or(&MkValue::Null), op, value)
            }
            MkCondition::WindowExists { matcher } => {
                let x = self
                    .backends
                    .window
                    .exists(matcher)
                    .map_err(|e| e.context("matcher", format!("{matcher:?}")))?;
                v.insert("last_window_result".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::WindowActive { matcher } => {
                let x = self
                    .backends
                    .window
                    .is_active(matcher)
                    .map_err(|e| e.context("matcher", format!("{matcher:?}")))?;
                v.insert("last_window_result".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::ImageSearch { search, found } => {
                let payload = search.as_payload();
                // Exactly one immediate backend call per condition evaluation.
                let point = self.backends.screen.find_image(macro_id, &payload)?;
                Self::write_image_result(v, &payload, point);
                Ok(point.is_some() == *found)
            }
            MkCondition::PreviousImageResult { asset_id, found } => {
                let key = asset_id
                    .map(super::screen::image_found_variable)
                    .unwrap_or_else(|| "last_image_found".into());
                // No recorded result is explicitly treated as "not found".
                let recorded = matches!(v.get(&key), Some(MkValue::Boolean(true)));
                Ok(recorded == *found)
            }
            MkCondition::PixelResult {
                target,
                color,
                tolerance,
            } => {
                v.remove("last_pixel_result");
                v.remove("last_pixel_found");
                let x = self
                    .backends
                    .screen
                    .pixel_matches(target, color, *tolerance, v)?;
                v.insert("last_pixel_result".into(), MkValue::Boolean(x));
                v.insert("last_pixel_found".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::All { conditions } => {
                for c in conditions {
                    if !self.condition(macro_id, c, v)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            MkCondition::Any { conditions } => {
                for c in conditions {
                    if self.condition(macro_id, c, v)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            MkCondition::Not { condition } => Ok(!self.condition(macro_id, condition, v)?),
        }
    }
}

fn validate_macro_sound(payload: &MkPlaySoundPayload) -> ExecResult {
    if payload.sound == "None" || !crate::sound::SOUND_NAMES.contains(&payload.sound.as_str()) {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            format!("unknown macro sound name '{}'", payload.sound),
        )
        .context("field", "play_sound.sound")
        .context("sound", &payload.sound));
    }
    Ok(())
}

fn resolve_notification(
    payload: &MkNotifyPayload,
    variables: &RuntimeVariables,
) -> ExecResult<ResolvedNotification> {
    let title = interpolate(&payload.title, variables)
        .map_err(|error| error.context("field", "notify.title"))?;
    let description = interpolate(&payload.description, variables)
        .map_err(|error| error.context("field", "notify.description"))?;
    Ok(ResolvedNotification {
        title,
        description,
        kind: payload.kind,
        duration: payload.duration,
        show_symbol: payload.show_symbol,
    })
}

#[cfg(test)]
mod notification_sound_tests {
    use super::{fake::FakeBackend, *};
    use crate::mkmacro::{
        MkErrorPolicy, MkMacro, MkNotifyPayload, MkPlaySoundPayload, MkRetry, MkStep, MkTextMode,
        compile,
    };
    use std::sync::mpsc;

    struct SchedulingSoundBackend(Mutex<mpsc::Sender<String>>);
    impl SoundBackend for SchedulingSoundBackend {
        fn play(&self, sound: &str) -> ExecResult {
            self.0.lock().unwrap().send(sound.into()).unwrap();
            Ok(())
        }
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }

    fn notification(title: &str, description: &str) -> MkAction {
        MkAction::Notify(MkNotifyPayload {
            title: title.into(),
            description: description.into(),
            ..MkNotifyPayload::default()
        })
    }

    fn execute(steps: Vec<MkStep>, fake: Arc<FakeBackend>) -> ExecResult {
        let plan = compile(&MkMacro {
            id: 42,
            name: "notification and sound".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps,
            image_assets: vec![],
        })
        .unwrap();
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::new(fake.backends(), control).execute(&plan, ExecutionOptions::normal(), &|_| {})
    }

    #[test]
    fn notification_interpolates_each_string_and_preserves_cosmetics() {
        let fake = Arc::new(FakeBackend::default());
        execute(
            vec![
                step(
                    1,
                    MkAction::SetVariable {
                        name: "name".into(),
                        value: MkValue::String("Fred".into()),
                    },
                ),
                step(
                    2,
                    MkAction::SetVariable {
                        name: "path".into(),
                        value: MkValue::String(r"D:\Backup".into()),
                    },
                ),
                step(
                    3,
                    MkAction::SetVariable {
                        name: "job".into(),
                        value: MkValue::String("daily".into()),
                    },
                ),
                step(
                    4,
                    MkAction::Notify(MkNotifyPayload {
                        title: "${job}: ${path}".into(),
                        description: "Hello ${name}; ${job} at ${path}".into(),
                        kind: MkNotificationKind::Warning,
                        duration: MkNotificationDuration::Long,
                        show_symbol: false,
                    }),
                ),
            ],
            fake.clone(),
        )
        .unwrap();
        assert_eq!(
            fake.notifications(),
            vec![ResolvedNotification {
                title: r"daily: D:\Backup".into(),
                description: r"Hello Fred; daily at D:\Backup".into(),
                kind: MkNotificationKind::Warning,
                duration: MkNotificationDuration::Long,
                show_symbol: false,
            }]
        );
    }

    #[test]
    fn notification_interpolation_failure_names_field_without_delivery() {
        for (action, field) in [
            (notification("${missing}", "ok"), "notify.title"),
            (notification("ok", "${missing}"), "notify.description"),
        ] {
            let fake = Arc::new(FakeBackend::default());
            let mut guard = InputCleanupGuard::new(fake.clone());
            let control = Arc::new(RunControl::default());
            control.reset();
            let error = Executor::new(fake.clone().backends(), control)
                .action(
                    42,
                    &action,
                    &MkPlayback::default(),
                    &mut RuntimeVariables::new(),
                    &mut guard,
                )
                .unwrap_err();
            assert_eq!(error.context.get("field").map(String::as_str), Some(field));
            assert!(fake.notifications().is_empty());
        }
    }

    #[test]
    fn notification_failure_obeys_stop_continue_and_retry() {
        for (policy, expected_attempts, should_continue) in [
            (MkErrorPolicy::Stop, 1, false),
            (MkErrorPolicy::Continue, 1, true),
            (
                MkErrorPolicy::Retry(MkRetry {
                    attempts: 3,
                    delay_ms: 0,
                }),
                3,
                false,
            ),
        ] {
            let fake = Arc::new(FakeBackend::default());
            fake.fail(
                "notification",
                ExecutionDiagnostic::new(DiagnosticKind::Backend, "toast failed"),
            );
            let mut first = step(1, notification("title", "description"));
            first.on_error = policy;
            let result = execute(
                vec![
                    first,
                    step(
                        2,
                        MkAction::Text(MkTextPayload {
                            text: "next".into(),
                            mode: MkTextMode::Type,
                        }),
                    ),
                ],
                fake.clone(),
            );
            assert_eq!(fake.notifications().len(), expected_attempts);
            assert_eq!(
                fake.events().iter().any(|event| event == "text:next"),
                should_continue
            );
            assert_eq!(result.is_ok(), should_continue);
        }
    }

    #[test]
    fn sound_is_exactly_once_and_invalid_names_never_reach_backend() {
        let fake = Arc::new(FakeBackend::default());
        execute(
            vec![step(
                1,
                MkAction::PlaySound(MkPlaySoundPayload {
                    sound: "ReminderStart.wav".into(),
                }),
            )],
            fake.clone(),
        )
        .unwrap();
        assert_eq!(fake.sounds(), vec!["ReminderStart.wav"]);

        let invalid = MkAction::PlaySound(MkPlaySoundPayload {
            sound: "reminderstart.wav".into(),
        });
        let mut guard = InputCleanupGuard::new(fake.clone());
        let control = Arc::new(RunControl::default());
        control.reset();
        let error = Executor::new(fake.clone().backends(), control)
            .action(
                42,
                &invalid,
                &MkPlayback::default(),
                &mut RuntimeVariables::new(),
                &mut guard,
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(fake.sounds(), vec!["ReminderStart.wav"]);
    }

    #[test]
    fn successful_sound_with_retry_policy_is_not_duplicated_and_next_step_runs() {
        let fake = Arc::new(FakeBackend::default());
        let mut sound = step(
            1,
            MkAction::PlaySound(MkPlaySoundPayload {
                sound: "ReminderStart.wav".into(),
            }),
        );
        sound.on_error = MkErrorPolicy::Retry(MkRetry {
            attempts: 5,
            delay_ms: 0,
        });
        execute(
            vec![
                sound,
                step(
                    2,
                    MkAction::Text(MkTextPayload {
                        text: "after".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
            ],
            fake.clone(),
        )
        .unwrap();
        assert_eq!(fake.sounds(), vec!["ReminderStart.wav"]);
        assert_eq!(fake.events(), vec!["sound", "text:after"]);
    }

    #[test]
    fn sound_delivery_is_fire_and_forget() {
        let fake = Arc::new(FakeBackend::default());
        let (scheduled_tx, scheduled_rx) = mpsc::channel();
        let mut backends = fake.clone().backends();
        backends.sound = Arc::new(SchedulingSoundBackend(Mutex::new(scheduled_tx)));
        let plan = compile(&MkMacro {
            id: 43,
            name: "async sound".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps: vec![
                step(
                    1,
                    MkAction::PlaySound(MkPlaySoundPayload {
                        sound: "ReminderStart.wav".into(),
                    }),
                ),
                step(
                    2,
                    MkAction::Text(MkTextPayload {
                        text: "following step".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
            ],
            image_assets: vec![],
        })
        .unwrap();
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::new(backends, control)
            .execute(&plan, ExecutionOptions::normal(), &|_| {})
            .unwrap();

        // Receiving the scheduled work is independent from playback completion;
        // execution has already advanced to the following backend call.
        assert_eq!(scheduled_rx.try_recv().unwrap(), "ReminderStart.wav");
        assert_eq!(fake.events(), vec!["text:following step"]);
    }
}
/// Testable declaration that every action has deliberate executor handling.
pub fn has_runtime_support(action: &MkAction) -> bool {
    // This is production capability metadata, not merely a mirror of the
    // executor match. Mouse support includes the wired WindowsScreenBackend
    // coordinate resolver and SendInput paths (including drag).
    match action {
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => false,
        MkAction::Notify(_) => cfg!(windows),
        MkAction::KeyDown(_)
        | MkAction::KeyUp(_)
        | MkAction::KeyPress(_)
        | MkAction::Hotkey(_)
        | MkAction::Text(_)
        | MkAction::MouseMove(_)
        | MkAction::MouseDrag(_)
        | MkAction::MouseClick(_)
        | MkAction::ClickWithinRegion(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. }
        | MkAction::Delay(_)
        | MkAction::Process(_)
        | MkAction::LauncherCommand(_)
        | MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. }
        | MkAction::WaitUntil { .. }
        | MkAction::SetVariable { .. }
        | MkAction::UnsetVariable { .. }
        | MkAction::PromptInput(_)
        | MkAction::PlaySound(_)
        | MkAction::If(_)
        | MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatStart { .. }
        | MkAction::RepeatEnd
        | MkAction::WhileStart { .. }
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue
        | MkAction::PixelCheck { .. }
        | MkAction::FindPixel(_) => true,
        MkAction::CaptureScreenshot(_) | MkAction::WaitForVisualChange(_) => true,
        MkAction::VirtualDesktop(_) => cfg!(windows),
        // Production installs `ProductionVisualSearch`, backed by the same
        // screen capture and matcher used by all visual-search execution.
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => true,
    }
}
fn action_name(a: &MkAction) -> &'static str {
    match a {
        MkAction::KeyDown(_)
        | MkAction::KeyUp(_)
        | MkAction::KeyPress(_)
        | MkAction::Hotkey(_)
        | MkAction::Text(_) => "SendInput",
        MkAction::MouseMove(_)
        | MkAction::MouseDrag(_)
        | MkAction::MouseClick(_)
        | MkAction::ClickWithinRegion(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. } => "SendInput",
        MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. } => "window",
        MkAction::VirtualDesktop(_) => "virtual desktop",
        MkAction::ImageFind(_)
        | MkAction::ImageClick(_)
        | MkAction::FindPixel(_)
        | MkAction::PixelCheck { .. } => "screen",
        MkAction::CaptureScreenshot(_) | MkAction::WaitForVisualChange(_) => "screen capture",
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => "UIAutomation",
        MkAction::Process(_) | MkAction::LauncherCommand(_) => "launcher",
        MkAction::WaitUntil { .. } => "condition_evaluator",
        MkAction::PromptInput(_) => "prompt",
        MkAction::Notify(_) => "notification",
        MkAction::PlaySound(_) => "sound",
        MkAction::Delay(_)
        | MkAction::SetVariable { .. }
        | MkAction::UnsetVariable { .. }
        | MkAction::If(_)
        | MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatStart { .. }
        | MkAction::RepeatEnd
        | MkAction::WhileStart { .. }
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => "runtime",
    }
}
fn set_point(v: &mut RuntimeVariables, prefix: &str, p: MkPoint) {
    v.insert(format!("{prefix}.x"), MkValue::Number(p.x as f64));
    v.insert(format!("{prefix}.y"), MkValue::Number(p.y as f64));
}
pub fn compare(a: &MkValue, op: &MkCompareOp, b: &MkValue) -> ExecResult<bool> {
    match op {
        MkCompareOp::Eq | MkCompareOp::NotEq => {
            if std::mem::discriminant(a) != std::mem::discriminant(b) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "mixed-type comparison is not allowed",
                ));
            }
            Ok(if matches!(op, MkCompareOp::Eq) {
                a == b
            } else {
                a != b
            })
        }
        MkCompareOp::Less
        | MkCompareOp::LessOrEq
        | MkCompareOp::Greater
        | MkCompareOp::GreaterOrEq => {
            let (MkValue::Number(a), MkValue::Number(b)) = (a, b) else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "ordering requires two numbers",
                ));
            };
            Ok(match op {
                MkCompareOp::Less => a < b,
                MkCompareOp::LessOrEq => a <= b,
                MkCompareOp::Greater => a > b,
                _ => a >= b,
            })
        }
        MkCompareOp::Contains
        | MkCompareOp::StartsWith
        | MkCompareOp::EndsWith
        | MkCompareOp::Regex => {
            let (MkValue::String(a), MkValue::String(b)) = (a, b) else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "string operator requires two strings",
                ));
            };
            Ok(match op {
                MkCompareOp::Contains => a.contains(b),
                MkCompareOp::StartsWith => a.starts_with(b),
                MkCompareOp::EndsWith => a.ends_with(b),
                _ => regex::Regex::new(b)
                    .map_err(|e| {
                        ExecutionDiagnostic::new(
                            DiagnosticKind::InvalidRegex,
                            format!("invalid regular expression: {e}"),
                        )
                    })?
                    .is_match(a),
            })
        }
    }
}

/// Configurable synchronized fake implementing every effect boundary.
pub mod fake {
    use super::*;
    use std::collections::VecDeque;
    #[derive(Debug, Clone, PartialEq)]
    pub enum WindowCall {
        Exists(MkWindowMatcher),
        IsActive(MkWindowMatcher),
        Activate(MkWindowPayload),
        Close(MkWindowMatcher),
        MoveResize(MkWindowMoveResizePayload),
        SetState(MkWindowMatcher, MkWindowState),
    }
    pub struct FakeBackend {
        pub notifications: Mutex<Vec<ResolvedNotification>>,
        pub sounds: Mutex<Vec<String>>,
        pub events: Mutex<Vec<String>>,
        pub failures: Mutex<HashMap<String, ExecutionDiagnostic>>,
        pub conditions: Mutex<HashMap<String, bool>>,
        pub condition_results: Mutex<HashMap<String, VecDeque<bool>>>,
        pub cursor: Mutex<MkPoint>,
        pub prompt_responses: Mutex<Vec<PromptResponse>>,
        pub processes: Mutex<Vec<MkProcessPayload>>,
        pub launcher_queries: Mutex<Vec<String>>,
        pub legacy_actions: Mutex<Vec<crate::actions::Action>>,
        pub command_controls: Mutex<Vec<usize>>,
        pub prompts: Mutex<Vec<PromptRequest>>,
        pub window_calls: Mutex<Vec<WindowCall>>,
        pub virtual_desktop_calls: Mutex<Vec<super::super::MkVirtualDesktopAction>>,
        pub image_results: Mutex<HashMap<u64, VecDeque<ExecResult<Option<MkPoint>>>>>,
        pub resolved_variables: Mutex<Vec<RuntimeVariables>>,
        pub finalized_points: Mutex<Vec<MkPoint>>,
    }
    impl Default for FakeBackend {
        fn default() -> Self {
            Self {
                notifications: Mutex::new(Vec::new()),
                sounds: Mutex::new(Vec::new()),
                events: Mutex::new(Vec::new()),
                failures: Mutex::new(HashMap::new()),
                conditions: Mutex::new(HashMap::new()),
                condition_results: Mutex::new(HashMap::new()),
                cursor: Mutex::new(MkPoint { x: 0, y: 0 }),
                prompt_responses: Mutex::new(Vec::new()),
                processes: Mutex::new(Vec::new()),
                launcher_queries: Mutex::new(Vec::new()),
                legacy_actions: Mutex::new(Vec::new()),
                command_controls: Mutex::new(Vec::new()),
                prompts: Mutex::new(Vec::new()),
                window_calls: Mutex::new(Vec::new()),
                virtual_desktop_calls: Mutex::new(Vec::new()),
                image_results: Mutex::new(HashMap::new()),
                resolved_variables: Mutex::new(Vec::new()),
                finalized_points: Mutex::new(Vec::new()),
            }
        }
    }
    impl FakeBackend {
        pub fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
        pub fn notifications(&self) -> Vec<ResolvedNotification> {
            self.notifications.lock().unwrap().clone()
        }
        pub fn sounds(&self) -> Vec<String> {
            self.sounds.lock().unwrap().clone()
        }
        pub fn fail(&self, name: &str, d: ExecutionDiagnostic) {
            self.failures.lock().unwrap().insert(name.into(), d);
        }
        fn event(&self, e: String) -> ExecResult {
            self.events.lock().unwrap().push(e.clone());
            if let Some(d) = self.failures.lock().unwrap().get(&e).cloned() {
                Err(d)
            } else {
                Ok(())
            }
        }
        pub fn backends(self: Arc<Self>) -> Backends {
            Backends {
                notification: self.clone(),
                sound: self.clone(),
                input: self.clone(),
                window: self.clone(),
                screen: self.clone(),
                uia: self.clone(),
                launcher: self.clone(),
                prompt: self.clone(),
                clipboard: self.clone(),
                screenshot_capture: Arc::new(Unsupported {
                    backend: "fake screen capture",
                }),
                screenshot_encoder: Arc::new(ImageScreenshotEncoder),
                screenshot_files: Arc::new(HostScreenshotFileSystem),
                virtual_desktop: self.clone(),
            }
        }
        pub fn script_prompt(&self, response: PromptResponse) {
            self.prompt_responses.lock().unwrap().push(response);
        }
        /// Queues a distinct match, clean miss, decode error, or capture error
        /// for an asset. `Ok(None)` is deliberately not conflated with `Err`.
        pub fn script_image(&self, asset_id: u64, result: ExecResult<Option<MkPoint>>) {
            self.image_results
                .lock()
                .unwrap()
                .entry(asset_id)
                .or_default()
                .push_back(result);
        }
        pub fn script_condition(&self, name: &str, results: impl IntoIterator<Item = bool>) {
            self.condition_results
                .lock()
                .unwrap()
                .insert(name.into(), results.into_iter().collect());
        }
    }
    impl NotificationBackend for FakeBackend {
        fn notify(&self, notification: &ResolvedNotification) -> ExecResult {
            self.notifications
                .lock()
                .unwrap()
                .push(notification.clone());
            self.event("notification".into())
        }
    }
    impl SoundBackend for FakeBackend {
        fn play(&self, sound: &str) -> ExecResult {
            self.sounds.lock().unwrap().push(sound.to_owned());
            self.event("sound".into())
        }
    }
    impl InputBackend for FakeBackend {
        fn key_down(&self, k: &MkKey) -> ExecResult {
            self.event(format!("key_down:{k:?}"))
        }
        fn key_up(&self, k: &MkKey) -> ExecResult {
            self.event(format!("key_up:{k:?}"))
        }
        fn button_down(&self, b: MkMouseButton) -> ExecResult {
            self.event(format!("button_down:{b:?}"))
        }
        fn button_up(&self, b: MkMouseButton) -> ExecResult {
            self.event(format!("button_up:{b:?}"))
        }
        fn move_mouse(&self, p: MkPoint) -> ExecResult {
            self.event(format!("move:{},{}", p.x, p.y))
        }
        fn move_mouse_smooth(
            &self,
            _: &RunControl,
            _: MkPoint,
            to: MkPoint,
            duration: Duration,
        ) -> ExecResult {
            self.event(format!(
                "smooth_move:{},{}:{}",
                to.x,
                to.y,
                duration.as_millis()
            ))
        }
        fn cursor_position(&self) -> ExecResult<MkPoint> {
            self.event("cursor_position".into())?;
            Ok(*self.cursor.lock().unwrap())
        }
        fn scroll(&self, axis: MkMouseScrollAxis, d: i32) -> ExecResult {
            self.event(format!("scroll:{axis:?}:{d}"))
        }
        fn text(&self, p: &MkTextPayload) -> ExecResult {
            self.event(format!("text:{}", p.text))
        }
    }
    impl super::super::virtual_desktops::VirtualDesktopBackend for FakeBackend {
        fn create(&self) -> ExecResult {
            self.virtual_desktop(super::super::MkVirtualDesktopAction::Create)
        }
        fn switch_left(&self) -> ExecResult {
            self.virtual_desktop(super::super::MkVirtualDesktopAction::SwitchLeft)
        }
        fn switch_right(&self) -> ExecResult {
            self.virtual_desktop(super::super::MkVirtualDesktopAction::SwitchRight)
        }
        fn close_current(&self) -> ExecResult {
            self.virtual_desktop(super::super::MkVirtualDesktopAction::CloseCurrent)
        }
        fn go_to(&self, desktop: u32) -> ExecResult {
            self.virtual_desktop(super::super::MkVirtualDesktopAction::GoTo { desktop })
        }
    }
    impl FakeBackend {
        fn virtual_desktop(&self, action: super::super::MkVirtualDesktopAction) -> ExecResult {
            self.virtual_desktop_calls.lock().unwrap().push(action);
            self.event(format!("virtual_desktop:{action:?}"))
        }
    }
    impl WindowBackend for FakeBackend {
        fn exists(&self, matcher: &MkWindowMatcher) -> ExecResult<bool> {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::Exists(matcher.clone()));
            if let Some(result) = self
                .condition_results
                .lock()
                .unwrap()
                .get_mut("window_exists")
                .and_then(VecDeque::pop_front)
            {
                return Ok(result);
            }
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("window_exists")
                .unwrap_or(&false))
        }
        fn is_active(&self, matcher: &MkWindowMatcher) -> ExecResult<bool> {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::IsActive(matcher.clone()));
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("window_active")
                .unwrap_or(&false))
        }
        fn activate(&self, payload: &MkWindowPayload) -> ExecResult {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::Activate(payload.clone()));
            self.event("window_activate".into())
        }
        fn close(&self, matcher: &MkWindowMatcher) -> ExecResult {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::Close(matcher.clone()));
            self.event("window_close".into())
        }
        fn move_resize(&self, payload: &MkWindowMoveResizePayload) -> ExecResult {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::MoveResize(payload.clone()));
            self.event("window_move_resize".into())
        }
        fn set_state(&self, matcher: &MkWindowMatcher, state: MkWindowState) -> ExecResult {
            self.window_calls
                .lock()
                .unwrap()
                .push(WindowCall::SetState(matcher.clone(), state));
            self.event(format!("window_state:{state:?}"))
        }
    }
    impl ScreenBackend for FakeBackend {
        fn resolve(&self, t: &MkCoordinateTarget, v: &RuntimeVariables) -> ExecResult<MkPoint> {
            self.resolved_variables.lock().unwrap().push(v.clone());
            match t {
                MkCoordinateTarget::CurrentPosition => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::UnsupportedOperation,
                    "Current Position must be handled by Mouse Click",
                )),
                MkCoordinateTarget::Screen { point }
                | MkCoordinateTarget::ActiveWindow { point } => Ok(*point),
                MkCoordinateTarget::Variable { name } => match v.get(name) {
                    Some(MkValue::Point(p)) => Ok(*p),
                    Some(value) => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TypeMismatch,
                        format!(
                            "Variable '{name}' contains {}; coordinate target requires Point",
                            match value {
                                MkValue::String(_) => "String",
                                MkValue::Number(_) => "Number",
                                MkValue::Boolean(_) => "Boolean",
                                MkValue::Point(_) => "Point",
                                MkValue::Null => "Null",
                            }
                        ),
                    )
                    .context("variable", name)
                    .context("expected", "Point")),
                    None => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        format!("point variable '{name}' is missing"),
                    )
                    .context("variable", name)
                    .context("expected", "Point")),
                },
                MkCoordinateTarget::Image { asset_id, offset } => {
                    match v.get(&super::super::screen::image_result_variable(*asset_id)) {
                        Some(MkValue::Point(point)) => Ok(offset_point(
                            *point,
                            i64::from(offset.x),
                            i64::from(offset.y),
                        )),
                        _ => Err(ExecutionDiagnostic::new(
                            DiagnosticKind::TargetNotFound,
                            "image target not found",
                        )
                        .context("asset_id", asset_id.to_string())),
                    }
                }
                _ => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "target not found",
                )),
            }
        }
        fn finalize_point(&self, point: MkPoint) -> ExecResult<MkPoint> {
            self.finalized_points.lock().unwrap().push(point);
            Ok(point)
        }
        fn find_image(&self, _: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
            if let Some(result) = self
                .image_results
                .lock()
                .unwrap()
                .get_mut(&payload.asset_id)
                .and_then(VecDeque::pop_front)
            {
                return result;
            }
            Ok(self
                .conditions
                .lock()
                .unwrap()
                .get("image")
                .copied()
                .unwrap_or(false)
                .then_some(MkPoint { x: 1, y: 1 }))
        }
        fn pixel_matches(
            &self,
            _: &MkCoordinateTarget,
            _: &str,
            _: u8,
            _: &RuntimeVariables,
        ) -> ExecResult<bool> {
            if self
                .conditions
                .lock()
                .unwrap()
                .get("pixel_error")
                .copied()
                .unwrap_or(false)
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "injected pixel capture failure",
                ));
            }
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("pixel")
                .unwrap_or(&false))
        }
    }
    impl UiAutomationBackend for FakeBackend {
        fn exists(&self, _: &MkUiPayload) -> ExecResult<bool> {
            Ok(*self.conditions.lock().unwrap().get("uia").unwrap_or(&false))
        }
        fn invoke(&self, _: &MkUiPayload) -> ExecResult {
            self.event("uia_invoke".into())
        }
        fn set_value(&self, _: &MkUiPayload, v: &str) -> ExecResult {
            self.event(format!("uia_value:{v}"))
        }
    }
    impl LauncherBackend for FakeBackend {
        fn launch_process(&self, p: &MkProcessPayload) -> ExecResult {
            self.processes.lock().unwrap().push(p.clone());
            self.event(format!("process:{}", p.program))
        }
        fn command(&self, c: &str, control: &RunControl) -> ExecResult {
            self.launcher_queries.lock().unwrap().push(c.into());
            self.command_controls
                .lock()
                .unwrap()
                .push(control as *const RunControl as usize);
            control.checkpoint()?;
            self.event(format!("command:{c}"))
        }
        fn resolved_legacy(
            &self,
            action: &crate::actions::Action,
            control: &RunControl,
        ) -> ExecResult {
            self.legacy_actions.lock().unwrap().push(action.clone());
            self.command_controls
                .lock()
                .unwrap()
                .push(control as *const RunControl as usize);
            control.checkpoint()?;
            self.event(format!("legacy:{}", action.action))
        }
    }
    impl PromptBackend for FakeBackend {
        fn prompt(&self, request: PromptRequest, _: &RunControl) -> ExecResult<PromptResponse> {
            self.prompts.lock().unwrap().push(request.clone());
            self.event(format!("prompt:{}", request.id))?;
            if self.prompt_responses.lock().unwrap().is_empty() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "no scripted prompt response",
                )
                .context("backend", "prompt"));
            }
            Ok(self.prompt_responses.lock().unwrap().remove(0))
        }
    }
    impl ClipboardBackend for FakeBackend {
        fn snapshot_text(&self) -> ExecResult<String> {
            self.event("clipboard_snapshot".into())?;
            Ok("original".into())
        }
        fn set_text(&self, text: &str) -> ExecResult {
            self.event(format!("clipboard:{text}"))
        }
    }
}

#[cfg(test)]
mod phase_d_tests {
    use super::{fake::FakeBackend, *};
    use crate::mkmacro::{
        MkErrorPolicy, MkLauncherCommandPayload, MkMacro, MkPlayback, MkRetry, MkStep, MkTextMode,
        compile,
    };

    fn s(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }

    fn paste(text: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: text.into(),
            mode: MkTextMode::Paste,
        })
    }

    #[test]
    fn paste_interpolates_and_restores_in_guarded_input_order() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut vars = RuntimeVariables::new();
        vars.insert("who".into(), MkValue::String("world".into()));
        let mut guard = InputCleanupGuard::new(fake.clone());
        executor
            .action(
                7,
                &paste("hello ${who}"),
                &MkPlayback::default(),
                &mut vars,
                &mut guard,
            )
            .unwrap();
        assert_eq!(
            fake.events(),
            vec![
                "clipboard_snapshot",
                "clipboard:hello world",
                "key_down:Control",
                "key_down:Character(\"V\")",
                "key_up:Character(\"V\")",
                "key_up:Control",
                "clipboard:original"
            ]
        );
    }

    #[test]
    fn paste_failures_restore_once_and_release_modifiers() {
        for failed in [
            "key_down:Control",
            "key_down:Character(\"V\")",
            "key_up:Character(\"V\")",
        ] {
            let fake = Arc::new(FakeBackend::default());
            fake.fail(
                failed,
                ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "primary"),
            );
            let control = Arc::new(RunControl::default());
            control.reset();
            let executor = Executor::new(fake.clone().backends(), control);
            let mut guard = InputCleanupGuard::new(fake.clone());
            assert!(
                executor
                    .action(
                        7,
                        &paste("temporary"),
                        &MkPlayback::default(),
                        &mut RuntimeVariables::new(),
                        &mut guard
                    )
                    .is_err()
            );
            drop(guard);
            let events = fake.events();
            assert_eq!(
                events.iter().filter(|e| *e == "clipboard:original").count(),
                1
            );
            // A rejected key-down is never added to InputCleanupGuard's owned
            // keys, so it must not synthesize a matching key-up. Once Control
            // was accepted, every later failure must release it during cleanup.
            if failed == "key_down:Control" {
                assert!(!events.iter().any(|e| e == "key_up:Control"));
            } else {
                assert!(events.iter().any(|e| e == "key_up:Control"));
            }
        }
    }

    #[test]
    fn paste_does_not_restore_when_snapshot_or_temporary_write_fails() {
        for failed in ["clipboard_snapshot", "clipboard:temporary"] {
            let fake = Arc::new(FakeBackend::default());
            fake.fail(
                failed,
                ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected"),
            );
            let control = Arc::new(RunControl::default());
            control.reset();
            let executor = Executor::new(fake.clone().backends(), control);
            let mut guard = InputCleanupGuard::new(fake.clone());
            assert!(
                executor
                    .action(
                        7,
                        &paste("temporary"),
                        &MkPlayback::default(),
                        &mut RuntimeVariables::new(),
                        &mut guard
                    )
                    .is_err()
            );
            assert!(!fake.events().iter().any(|e| e == "clipboard:original"));
        }
    }

    #[test]
    fn primary_error_wins_over_restoration_error() {
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "key_down:Character(\"V\")",
            ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "primary"),
        );
        fake.fail(
            "clipboard:original",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "restore"),
        );
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut guard = InputCleanupGuard::new(fake);
        let error = executor
            .action(
                7,
                &paste("temporary"),
                &MkPlayback::default(),
                &mut RuntimeVariables::new(),
                &mut guard,
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InputRejected);
        assert!(error.context.contains_key("clipboard_restoration_failure"));
    }

    #[test]
    fn restoration_error_is_reported_after_successful_paste() {
        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "clipboard:original",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "restore"),
        );
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut guard = InputCleanupGuard::new(fake.clone());
        let error = executor
            .action(
                7,
                &paste("temporary"),
                &MkPlayback::default(),
                &mut RuntimeVariables::new(),
                &mut guard,
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(
            error.context.get("operation").map(String::as_str),
            Some("restore")
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|e| *e == "clipboard:original")
                .count(),
            1
        );
    }

    #[test]
    fn cancellation_during_settle_restores_clipboard_once() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let worker_fake = fake.clone();
        let worker_control = control.clone();
        let worker = std::thread::spawn(move || {
            let executor = Executor::new(worker_fake.clone().backends(), worker_control);
            let mut guard = InputCleanupGuard::new(worker_fake);
            executor.action(
                7,
                &paste("temporary"),
                &MkPlayback::default(),
                &mut RuntimeVariables::new(),
                &mut guard,
            )
        });
        while !fake.events().iter().any(|event| event == "key_up:Control") {
            std::thread::yield_now();
        }
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|e| *e == "clipboard:original")
                .count(),
            1
        );
    }
    fn plan(steps: Vec<MkStep>) -> MkExecutionPlan {
        compile(&MkMacro {
            id: 9,
            name: "flow".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps,
            image_assets: vec![],
        })
        .unwrap()
    }
    fn text(value: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: value.into(),
            mode: MkTextMode::Type,
        })
    }
    fn run(steps: Vec<MkStep>, fake: Arc<FakeBackend>) -> ExecResult {
        let c = Arc::new(RunControl::default());
        c.reset();
        Executor::new(fake.backends(), c).execute(&plan(steps), ExecutionOptions::normal(), &|_| {})
    }

    fn run_recorded(
        action: &MkAction,
        fake: Arc<FakeBackend>,
        waiter: Arc<RecordingWaiter>,
    ) -> ExecResult {
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::with_waiter(fake.clone().backends(), control, waiter);
        let mut guard = InputCleanupGuard::new(fake);
        executor.action(
            9,
            action,
            &MkPlayback::default(),
            &mut RuntimeVariables::new(),
            &mut guard,
        )
    }

    fn window_condition() -> MkCondition {
        MkCondition::WindowExists {
            matcher: MkWindowMatcher {
                title: Some("eventual".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn indefinite_wait_until_records_every_unsuccessful_poll_only() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [false, false, false, true]);
        let waiter = Arc::new(RecordingWaiter::default());
        run_recorded(
            &MkAction::WaitUntil {
                condition: window_condition(),
                wait: MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                },
            },
            fake.clone(),
            waiter.clone(),
        )
        .unwrap();
        assert_eq!(fake.window_calls.lock().unwrap().len(), 4);
        assert_eq!(waiter.sleeps(), vec![Duration::from_millis(10); 3]);
    }

    #[test]
    fn indefinite_wait_is_interruptible_at_the_wait_boundary() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [false, false, false]);
        let waiter = Arc::new(RecordingWaiter::stop_after(2));
        let error = run_recorded(
            &MkAction::WaitUntil {
                condition: window_condition(),
                wait: MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 17,
                },
            },
            fake.clone(),
            waiter.clone(),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(fake.window_calls.lock().unwrap().len(), 2);
        assert_eq!(waiter.sleeps(), vec![Duration::from_millis(17); 2]);
    }

    fn image_condition(found: bool) -> MkCondition {
        MkCondition::ImageSearch {
            search: crate::mkmacro::MkImageSearchCondition {
                asset_id: 42,
                region: SearchRegion::Desktop,
                tolerance: 3,
                alpha: crate::mkmacro::AlphaPolicy::Ignore,
                return_point: crate::mkmacro::ReturnPoint::Center,
            },
            found,
        }
    }

    #[test]
    fn image_appearance_and_disappearance_poll_through_wait_until() {
        for (found, results) in [
            (true, vec![None, None, Some(MkPoint { x: 4, y: 5 })]),
            (
                false,
                vec![
                    Some(MkPoint { x: 1, y: 1 }),
                    Some(MkPoint { x: 2, y: 2 }),
                    None,
                ],
            ),
        ] {
            let fake = Arc::new(FakeBackend::default());
            for result in results {
                fake.script_image(42, Ok(result));
            }
            let waiter = Arc::new(RecordingWaiter::default());
            run_recorded(
                &MkAction::WaitUntil {
                    condition: image_condition(found),
                    wait: MkWaitOptions {
                        timeout_ms: 0,
                        poll_interval_ms: 23,
                    },
                },
                fake,
                waiter.clone(),
            )
            .unwrap();
            assert_eq!(waiter.sleeps(), vec![Duration::from_millis(23); 2]);
        }
    }

    #[test]
    fn finite_wait_caps_final_sleep_and_preserves_timeout_context() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [false, false, false]);
        let waiter = Arc::new(RecordingWaiter::default());
        let error = run_recorded(
            &MkAction::WaitUntil {
                condition: window_condition(),
                wait: MkWaitOptions {
                    timeout_ms: 25,
                    poll_interval_ms: 10,
                },
            },
            fake,
            waiter.clone(),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Timeout);
        assert_eq!(
            error.context.get("timeout_ms").map(String::as_str),
            Some("25")
        );
        assert_eq!(
            error.context.get("poll_interval_ms").map(String::as_str),
            Some("10")
        );
        assert_eq!(
            waiter.sleeps(),
            [
                Duration::from_millis(10),
                Duration::from_millis(10),
                Duration::from_millis(5)
            ]
        );
    }

    #[test]
    fn playback_passes_exact_scroll_axis_and_delta_to_backend() {
        let fake = Arc::new(FakeBackend::default());
        run(
            vec![
                s(
                    1,
                    MkAction::MouseScroll {
                        axis: MkMouseScrollAxis::Vertical,
                        i32_delta: -37,
                    },
                ),
                s(
                    2,
                    MkAction::MouseScroll {
                        axis: MkMouseScrollAxis::Horizontal,
                        i32_delta: 241,
                    },
                ),
            ],
            fake.clone(),
        )
        .unwrap();
        assert_eq!(
            fake.events(),
            ["scroll:Vertical:-37", "scroll:Horizontal:241"]
        );
    }

    #[test]
    fn typed_comparisons_cover_every_operator_and_bad_regex() {
        use MkCompareOp::*;
        assert!(compare(&MkValue::Number(1.0), &Less, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(2.0), &LessOrEq, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(3.0), &Greater, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(3.0), &GreaterOrEq, &MkValue::Number(3.0)).unwrap());
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &Contains,
                &MkValue::String("bc".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &StartsWith,
                &MkValue::String("ab".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &EndsWith,
                &MkValue::String("cd".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &Regex,
                &MkValue::String("^a.*d$".into())
            )
            .unwrap()
        );
        assert!(compare(&MkValue::Boolean(true), &Eq, &MkValue::Boolean(true)).unwrap());
        assert!(compare(&MkValue::Boolean(true), &NotEq, &MkValue::Boolean(false)).unwrap());
        assert_eq!(
            compare(&MkValue::Number(1.0), &Eq, &MkValue::String("1".into()))
                .unwrap_err()
                .kind,
            DiagnosticKind::TypeMismatch
        );
        assert_eq!(
            compare(
                &MkValue::String("x".into()),
                &Regex,
                &MkValue::String("[".into())
            )
            .unwrap_err()
            .kind,
            DiagnosticKind::InvalidRegex
        );
    }

    #[test]
    fn nested_flow_zero_repeat_break_and_continue_have_correct_destinations() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![
                s(1, MkAction::RepeatStart { count: 0 }),
                s(2, text("never")),
                s(3, MkAction::RepeatEnd),
                s(4, MkAction::RepeatStart { count: 3 }),
                s(5, text("once")),
                s(6, MkAction::Break),
                s(7, text("never2")),
                s(8, MkAction::RepeatEnd),
                s(9, MkAction::RepeatStart { count: 2 }),
                s(10, MkAction::Continue),
                s(11, text("never3")),
                s(12, MkAction::RepeatEnd),
                s(
                    16,
                    MkAction::SetVariable {
                        name: "x".into(),
                        value: MkValue::Boolean(false),
                    },
                ),
                s(
                    13,
                    MkAction::If(MkCondition::Not {
                        condition: Box::new(MkCondition::Variable {
                            name: "x".into(),
                            op: MkCompareOp::Eq,
                            value: MkValue::Boolean(true),
                        }),
                    }),
                ),
                s(14, text("nested")),
                s(15, MkAction::EndIf),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(f.events(), vec!["text:once", "text:nested"]);
    }

    #[test]
    fn all_any_short_circuit_and_while_false_exit() {
        let f = Arc::new(FakeBackend::default());
        let false_c = MkCondition::Variable {
            name: "x".into(),
            op: MkCompareOp::Eq,
            value: MkValue::Boolean(true),
        };
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "x".into(),
                        value: MkValue::Boolean(false),
                    },
                ),
                s(
                    2,
                    MkAction::If(MkCondition::All {
                        conditions: vec![
                            false_c.clone(),
                            MkCondition::Variable {
                                name: "x".into(),
                                // Valid at compile time, but would be a runtime type error if
                                // All failed to short-circuit after the false first condition.
                                op: MkCompareOp::Less,
                                value: MkValue::Number(1.0),
                            },
                        ],
                    }),
                ),
                s(3, text("never")),
                s(4, MkAction::EndIf),
                s(
                    5,
                    MkAction::If(MkCondition::Any {
                        conditions: vec![
                            MkCondition::Not {
                                condition: Box::new(false_c.clone()),
                            },
                            MkCondition::Variable {
                                name: "x".into(),
                                // Likewise, Any must not evaluate this after its true branch.
                                op: MkCompareOp::Less,
                                value: MkValue::Number(1.0),
                            },
                        ],
                    }),
                ),
                s(6, text("yes")),
                s(7, MkAction::EndIf),
                s(8, MkAction::WhileStart { condition: false_c }),
                s(9, text("never2")),
                s(10, MkAction::WhileEnd),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(f.events(), vec!["text:yes"]);
    }

    #[test]
    fn wait_until_immediate_timeout_and_retry_context() {
        let f = Arc::new(FakeBackend::default());
        f.conditions
            .lock()
            .unwrap()
            .insert("window_exists".into(), true);
        let w = MkWaitOptions {
            timeout_ms: 5,
            poll_interval_ms: 1,
        };
        run(
            vec![s(
                1,
                MkAction::WaitUntil {
                    condition: MkCondition::WindowExists {
                        matcher: MkWindowMatcher {
                            title: Some("x".into()),
                            title_regex: None,
                            process: None,
                            class: None,
                        },
                    },
                    wait: w.clone(),
                },
            )],
            f.clone(),
        )
        .unwrap();
        f.conditions
            .lock()
            .unwrap()
            .insert("window_exists".into(), false);
        let mut step = s(
            2,
            MkAction::WaitUntil {
                condition: MkCondition::WindowExists {
                    matcher: MkWindowMatcher {
                        title: Some("x".into()),
                        title_regex: None,
                        process: None,
                        class: None,
                    },
                },
                wait: w,
            },
        );
        step.on_error = MkErrorPolicy::Retry(MkRetry {
            attempts: 2,
            delay_ms: 0,
        });
        let e = run(vec![step], f).unwrap_err();
        assert_eq!(e.kind, DiagnosticKind::Timeout);
        assert_eq!(
            e.context.get("attempts_exhausted").map(String::as_str),
            Some("true")
        );
        assert!(e.context.contains_key("poll_interval_ms"));
    }

    #[test]
    fn indefinite_wait_until_polls_until_success() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.backends(), control);
        let mut polls = 0;
        executor
            .wait_until(
                &MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 1,
                },
                || {
                    polls += 1;
                    Ok(polls == 4)
                },
            )
            .unwrap();
        assert_eq!(polls, 4);
    }

    #[test]
    fn window_wait_inherits_indefinite_condition_polling() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_condition("window_exists", [false, false, false, true]);
        run(
            vec![s(
                1,
                MkAction::WindowWait(MkWindowPayload {
                    matcher: MkWindowMatcher {
                        title: Some("eventual".into()),
                        ..MkWindowMatcher::default()
                    },
                    wait: Some(MkWaitOptions {
                        timeout_ms: 0,
                        poll_interval_ms: 1,
                    }),
                }),
            )],
            fake.clone(),
        )
        .unwrap();
        assert_eq!(fake.window_calls.lock().unwrap().len(), 4);
    }

    #[test]
    fn interpolation_uses_current_variables_and_prompt_answers() {
        let f = Arc::new(FakeBackend::default());
        f.script_prompt(PromptResponse::Submitted("my_project".into()));
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "project_name".into(),
                        value: MkValue::String("initial".into()),
                    },
                ),
                s(2, text("Creating project ${project_name}")),
                s(
                    3,
                    MkAction::PromptInput(MkPromptInputPayload {
                        title: "Rename ${project_name}".into(),
                        prompt: "Current: ${project_name}".into(),
                        default_value: "${project_name}".into(),
                        variable: "project_name".into(),
                        copy_to_clipboard: false,
                    }),
                ),
                s(4, text("Creating project ${project_name}")),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(
            f.events()
                .into_iter()
                .filter(|event| event.starts_with("text:"))
                .collect::<Vec<_>>(),
            [
                "text:Creating project initial",
                "text:Creating project my_project"
            ]
        );
        let prompts = f.prompts.lock().unwrap();
        assert_eq!(prompts[0].title, "Rename initial");
        assert_eq!(prompts[0].prompt, "Current: initial");
        assert_eq!(prompts[0].default_value, "initial");
    }

    #[test]
    fn process_fields_expand_without_changing_process_boundaries() {
        let f = Arc::new(FakeBackend::default());
        let original_process = MkProcessPayload {
            program: "tool-${value}".into(),
            arguments: vec!["--name".into(), "${value}".into()],
            working_directory: Some("/work/${folder}".into()),
            wait: true,
        };
        let original_action = MkAction::Process(original_process.clone());
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "value".into(),
                        value: MkValue::String("two words".into()),
                    },
                ),
                s(
                    2,
                    MkAction::SetVariable {
                        name: "folder".into(),
                        value: MkValue::String("project".into()),
                    },
                ),
                s(3, original_action.clone()),
            ],
            f.clone(),
        )
        .unwrap();
        let processes = f.processes.lock().unwrap();
        assert_eq!(processes[0].program, "tool-${value}");
        assert_eq!(processes[0].arguments, ["--name", "two words"]);
        assert_eq!(
            processes[0].working_directory.as_deref(),
            Some("/work/project")
        );
        assert_eq!(original_action, MkAction::Process(original_process));
        assert!(f.launcher_queries.lock().unwrap().is_empty());
    }

    #[test]
    fn newly_authored_launcher_command_interpolates_one_complete_raw_query() {
        let f = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::new(f.clone().backends(), control.clone())
            .execute(
                &plan(vec![
                    s(
                        1,
                        MkAction::SetVariable {
                            name: "value".into(),
                            value: MkValue::String("two words".into()),
                        },
                    ),
                    s(
                        2,
                        MkAction::LauncherCommand(MkLauncherCommandPayload {
                            query: "note open ${value}".into(),
                            legacy_resolved_action: None,
                        }),
                    ),
                ]),
                ExecutionOptions::normal(),
                &|_| {},
            )
            .unwrap();

        assert_eq!(
            f.launcher_queries.lock().unwrap().as_slice(),
            ["note open two words"]
        );
        assert!(
            !f.launcher_queries
                .lock()
                .unwrap()
                .iter()
                .any(|query| query == "note open ${value}")
        );
        assert!(f.legacy_actions.lock().unwrap().is_empty());
        assert_eq!(f.processes.lock().unwrap().len(), 0);
        assert_eq!(
            f.command_controls.lock().unwrap().as_slice(),
            [Arc::as_ptr(&control) as usize]
        );
    }

    #[test]
    fn schema_v7_migrated_launcher_action_uses_compatibility_gui_boundary() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "name".into(),
                        value: MkValue::String("daily".into()),
                    },
                ),
                s(
                    2,
                    MkAction::LauncherCommand(MkLauncherCommandPayload {
                        query: String::new(),
                        legacy_resolved_action: Some(crate::actions::Action {
                            label: "Open ${name}".into(),
                            desc: "Description ${name}".into(),
                            action: "note:open:${name}".into(),
                            args: Some(r#"{"note":"${name}"}"#.into()),
                        }),
                    }),
                ),
            ],
            f.clone(),
        )
        .unwrap();
        assert!(f.launcher_queries.lock().unwrap().is_empty());
        assert!(f.processes.lock().unwrap().is_empty());
        assert_eq!(
            f.legacy_actions.lock().unwrap().as_slice(),
            &[crate::actions::Action {
                label: "Open daily".into(),
                desc: "Description daily".into(),
                action: "note:open:daily".into(),
                args: Some(r#"{"note":"daily"}"#.into()),
            }]
        );
    }

    #[test]
    fn launcher_command_preserves_spaces_in_interpolated_raw_query() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "project_name".into(),
                        value: MkValue::String("Daily Work Notes".into()),
                    },
                ),
                s(
                    2,
                    MkAction::LauncherCommand(MkLauncherCommandPayload {
                        query: "note open ${project_name}".into(),
                        legacy_resolved_action: None,
                    }),
                ),
            ],
            f.clone(),
        )
        .unwrap();

        let queries = f.launcher_queries.lock().unwrap();
        assert_eq!(
            queries.len(),
            1,
            "the query is submitted once, not tokenized"
        );
        assert_eq!(queries[0], "note open Daily Work Notes");
        assert!(queries[0].contains("Daily Work Notes"));
        assert!(!queries[0].contains(['\'', '"', '\\']));
        assert!(f.legacy_actions.lock().unwrap().is_empty());
        assert!(f.processes.lock().unwrap().is_empty());
    }

    #[test]
    fn launcher_command_repeated_executions_use_each_current_value() {
        let f = Arc::new(FakeBackend::default());
        for value in ["alpha", "beta"] {
            run(
                vec![
                    s(
                        1,
                        MkAction::SetVariable {
                            name: "note_name".into(),
                            value: MkValue::String(value.into()),
                        },
                    ),
                    s(
                        2,
                        MkAction::LauncherCommand(MkLauncherCommandPayload {
                            query: "note open ${note_name}".into(),
                            legacy_resolved_action: None,
                        }),
                    ),
                ],
                f.clone(),
            )
            .unwrap();
        }

        assert_eq!(
            f.launcher_queries.lock().unwrap().as_slice(),
            ["note open alpha", "note open beta"]
        );
        assert!(f.legacy_actions.lock().unwrap().is_empty());
    }

    #[test]
    fn launcher_command_interpolation_failure_has_context_and_no_effects() {
        let f = Arc::new(FakeBackend::default());
        let error = run(
            vec![
                s(
                    1,
                    MkAction::LauncherCommand(MkLauncherCommandPayload {
                        query: "note open ${note_name}".into(),
                        legacy_resolved_action: None,
                    }),
                ),
                s(2, text("later observable step")),
            ],
            f.clone(),
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            error.context.get("field").map(String::as_str),
            Some("launcher_command.query")
        );
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("note_name")
        );
        assert!(f.launcher_queries.lock().unwrap().is_empty());
        assert!(f.legacy_actions.lock().unwrap().is_empty());
        assert!(f.events().is_empty());
    }

    #[test]
    fn launcher_command_uses_interpolation_escape_contract() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![s(
                1,
                MkAction::LauncherCommand(MkLauncherCommandPayload {
                    query: "note open $${note_name}".into(),
                    legacy_resolved_action: None,
                }),
            )],
            f.clone(),
        )
        .unwrap();

        assert_eq!(
            f.launcher_queries.lock().unwrap().as_slice(),
            ["note open ${note_name}"]
        );
    }

    #[test]
    fn process_uses_only_process_launching() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![s(
                1,
                MkAction::Process(MkProcessPayload {
                    program: "tool".into(),
                    arguments: vec![],
                    working_directory: None,
                    wait: false,
                }),
            )],
            f.clone(),
        )
        .unwrap();
        assert_eq!(f.processes.lock().unwrap().len(), 1);
        assert!(f.launcher_queries.lock().unwrap().is_empty());
    }

    #[test]
    fn launcher_backend_error_has_backend_query_and_step_context() {
        let f = Arc::new(FakeBackend::default());
        f.fail(
            "command:broken query",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "query failed"),
        );
        let error = run(
            vec![s(
                42,
                MkAction::LauncherCommand(MkLauncherCommandPayload {
                    query: "broken query".into(),
                    legacy_resolved_action: None,
                }),
            )],
            f,
        )
        .unwrap_err();
        assert_eq!(
            error.context.get("backend").map(String::as_str),
            Some("launcher")
        );
        assert_eq!(
            error.context.get("query").map(String::as_str),
            Some("broken query")
        );
        assert_eq!(error.context.get("step").map(String::as_str), Some("42"));
    }

    #[test]
    fn stopped_launcher_run_is_cancelled_without_launching_process() {
        let f = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        control.stop();
        let error = Executor::new(f.clone().backends(), control)
            .execute(
                &plan(vec![s(
                    1,
                    MkAction::LauncherCommand(MkLauncherCommandPayload {
                        query: "anything".into(),
                        legacy_resolved_action: None,
                    }),
                )]),
                ExecutionOptions::normal(),
                &|_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert!(f.processes.lock().unwrap().is_empty());
    }

    #[test]
    fn failed_interpolation_has_no_effect_and_stops_later_steps() {
        let f = Arc::new(FakeBackend::default());
        let error = run(
            vec![s(1, text("secret ${missing}")), s(2, text("later"))],
            f.clone(),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("missing")
        );
        assert!(f.events().is_empty());
    }

    #[test]
    fn repeated_steps_interpolate_each_iteration_at_execution_time() {
        let f = Arc::new(FakeBackend::default());
        let mut repeated = s(1, text("iteration ${iteration}"));
        repeated.repeat = 2;
        run(vec![repeated], f.clone()).unwrap();
        assert_eq!(f.events(), ["text:iteration 0", "text:iteration 1"]);
    }

    fn image_payload(asset_id: u64, policy: MkImageNotFoundPolicy) -> MkImagePayload {
        MkImagePayload {
            asset_id,
            wait: MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 1,
            },
            region: Default::default(),
            tolerance: 0,
            alpha: Default::default(),
            return_point: Default::default(),
            not_found_policy: policy,
            outputs: MkImageOutputs {
                found: Some("found".into()),
                point: Some("point".into()),
                x: Some("x".into()),
                y: Some("y".into()),
            },
        }
    }

    fn image_action(
        executor: &Executor,
        payload: &MkImagePayload,
        vars: &mut RuntimeVariables,
    ) -> ExecResult {
        let fake = Arc::new(FakeBackend::default());
        let mut guard = InputCleanupGuard::new(fake);
        executor.action(
            77,
            &MkAction::ImageFind(payload.clone()),
            &MkPlayback::default(),
            vars,
            &mut guard,
        )
    }

    #[test]
    fn write_image_result_atomically_updates_success_and_miss_snapshots() {
        for named in [false, true] {
            let mut payload = image_payload(10, MkImageNotFoundPolicy::Continue);
            if !named {
                payload.outputs = MkImageOutputs::default();
            }
            let mut vars = RuntimeVariables::new();
            for point in [Some(MkPoint { x: 23, y: -4 }), None] {
                Executor::write_image_result(&mut vars, &payload, point);
                let found = point.is_some();
                assert_eq!(
                    vars.get("last_image_result"),
                    Some(&MkValue::Boolean(found))
                );
                assert_eq!(vars.get("__image_found.10"), Some(&MkValue::Boolean(found)));
                let expected_point = point.map(MkValue::Point);
                assert_eq!(vars.get("__image.10"), expected_point.as_ref());
                if named {
                    assert_eq!(vars.get("found"), Some(&MkValue::Boolean(found)));
                    assert_eq!(
                        vars.get("point"),
                        Some(&point.map(MkValue::Point).unwrap_or(MkValue::Null))
                    );
                    assert_eq!(
                        vars.get("x"),
                        Some(
                            &point
                                .map(|p| MkValue::Number(p.x.into()))
                                .unwrap_or(MkValue::Null)
                        )
                    );
                    assert_eq!(
                        vars.get("y"),
                        Some(
                            &point
                                .map(|p| MkValue::Number(p.y.into()))
                                .unwrap_or(MkValue::Null)
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn image_find_distinguishes_match_miss_and_operational_failure() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let payload = image_payload(10, MkImageNotFoundPolicy::Continue);
        let point = MkPoint { x: 40, y: 50 };
        fake.script_image(10, Ok(Some(point)));
        let mut vars = RuntimeVariables::new();
        image_action(&executor, &payload, &mut vars).unwrap();
        assert_eq!(vars.get("point"), Some(&MkValue::Point(point)));
        assert_eq!(vars.get("x"), Some(&MkValue::Number(40.0)));

        fake.script_image(10, Ok(None));
        image_action(&executor, &payload, &mut vars).unwrap();
        for key in ["__image.10", "last_image"] {
            assert!(!vars.contains_key(key));
        }
        for key in ["point", "x", "y"] {
            assert_eq!(vars.get(key), Some(&MkValue::Null));
        }
        assert_eq!(vars.get("found"), Some(&MkValue::Boolean(false)));

        for (message, path) in [
            ("corrupt image data", "assets/corrupt.png"),
            ("screen capture failed", "capture region"),
        ] {
            Executor::write_image_result(&mut vars, &payload, Some(point));
            fake.script_image(
                10,
                Err(ExecutionDiagnostic::new(DiagnosticKind::Backend, message)
                    .context("source", path)),
            );
            let error = image_action(&executor, &payload, &mut vars).unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::Backend);
            assert_eq!(
                error.context.get("asset_id").map(String::as_str),
                Some("10")
            );
            assert!(error.context.contains_key("region"));
            assert!(!vars.contains_key("__image.10"));
            assert_eq!(vars.get("found"), Some(&MkValue::Boolean(false)));
        }
    }

    #[test]
    fn failing_miss_clears_stale_outputs_before_returning_timeout() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_image(10, Ok(None));
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.backends(), control);
        let payload = image_payload(10, MkImageNotFoundPolicy::Fail);
        let mut vars = RuntimeVariables::new();
        Executor::write_image_result(&mut vars, &payload, Some(MkPoint { x: 1, y: 2 }));
        let error = image_action(&executor, &payload, &mut vars).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Timeout);
        assert!(!vars.contains_key("__image.10"));
        assert_eq!(vars.get("point"), Some(&MkValue::Null));
    }

    #[test]
    fn image_namespaces_resolve_independently_and_mouse_paths_are_injectable() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let mut vars = RuntimeVariables::new();
        for (id, point) in [
            (10, MkPoint { x: 10, y: 20 }),
            (11, MkPoint { x: 90, y: 80 }),
        ] {
            fake.script_image(id, Ok(Some(point)));
            image_action(
                &executor,
                &image_payload(id, MkImageNotFoundPolicy::Continue),
                &mut vars,
            )
            .unwrap();
        }
        assert_eq!(
            vars.get("__image.10"),
            Some(&MkValue::Point(MkPoint { x: 10, y: 20 }))
        );
        let mut guard = InputCleanupGuard::new(fake.clone());
        let target = MkCoordinateTarget::Image {
            asset_id: 10,
            offset: MkPoint { x: 3, y: -2 },
        };
        for duration_ms in [125, 0] {
            executor
                .action(
                    77,
                    &MkAction::MouseMove(super::super::MkMouseMovePayload {
                        target: target.clone(),
                        duration_ms,
                    }),
                    &MkPlayback::default(),
                    &mut vars,
                    &mut guard,
                )
                .unwrap();
        }
        assert!(fake.events().contains(&"smooth_move:13,18:125".into()));
        assert!(fake.events().contains(&"move:13,18".into()));

        fake.script_image(10, Ok(None));
        image_action(
            &executor,
            &image_payload(10, MkImageNotFoundPolicy::Continue),
            &mut vars,
        )
        .unwrap();
        assert_eq!(vars.get("__image_found.10"), Some(&MkValue::Boolean(false)));
        assert_eq!(
            vars.get("__image.11"),
            Some(&MkValue::Point(MkPoint { x: 90, y: 80 }))
        );
        assert_eq!(
            vars.get("last_image_result"),
            Some(&MkValue::Boolean(false))
        );
        let error = executor
            .action(
                77,
                &MkAction::MouseMove(super::super::MkMouseMovePayload {
                    target,
                    duration_ms: 0,
                }),
                &MkPlayback::default(),
                &mut vars,
                &mut guard,
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::TargetNotFound);
    }

    #[test]
    fn pixel_found_outputs_and_miss_clear_coordinate_state() {
        let payload = super::super::MkPixelSearchPayload {
            search_id: 12,
            color: "#32FF70".into(),
            tolerance: 10,
            region: super::super::SearchRegion::Desktop,
            wait: MkWaitOptions::default(),
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs: MkImageOutputs {
                found: Some("hit".into()),
                point: Some("pixel".into()),
                x: Some("px".into()),
                y: Some("py".into()),
            },
        };
        let mut vars = RuntimeVariables::new();
        Executor::write_pixel_result(&mut vars, &payload, Some(MkPoint { x: 8, y: 9 }));
        assert_eq!(vars.get("hit"), Some(&MkValue::Boolean(true)));
        assert_eq!(
            vars.get("pixel"),
            Some(&MkValue::Point(MkPoint { x: 8, y: 9 }))
        );
        Executor::write_pixel_result(&mut vars, &payload, None);
        assert_eq!(vars.get("hit"), Some(&MkValue::Boolean(false)));
        assert!(!vars.contains_key("pixel"));
        assert!(!vars.contains_key("__pixel.12"));
    }

    fn variable_move() -> MkAction {
        MkAction::MouseMove(super::super::MkMouseMovePayload {
            target: MkCoordinateTarget::Variable {
                name: "point".into(),
            },
            duration_ms: 0,
        })
    }

    #[test]
    fn find_image_point_output_mandatory_match_moves_mouse_in_one_compiled_playback() {
        let fake = Arc::new(FakeBackend::default());
        let point = MkPoint { x: 823, y: 441 };
        fake.script_image(10, Ok(Some(point)));
        let mut payload = image_payload(10, MkImageNotFoundPolicy::Fail);
        payload.outputs = MkImageOutputs {
            point: Some("point".into()),
            ..Default::default()
        };

        run(
            vec![s(1, MkAction::ImageFind(payload)), s(2, variable_move())],
            fake.clone(),
        )
        .unwrap();

        let snapshots = fake.resolved_variables.lock().unwrap();
        assert_eq!(snapshots[0].get("point"), Some(&MkValue::Point(point)));
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| event.starts_with("move:"))
                .collect::<Vec<_>>(),
            ["move:823,441"]
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| **event == "move:823,441")
                .count(),
            1
        );
    }

    #[test]
    fn find_image_point_output_continued_miss_writes_null_and_reports_variable_type() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_image(10, Ok(None));
        let mut payload = image_payload(10, MkImageNotFoundPolicy::Continue);
        payload.outputs = MkImageOutputs {
            point: Some("point".into()),
            ..Default::default()
        };
        let error = run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "point".into(),
                        value: MkValue::Point(MkPoint { x: 1, y: 2 }),
                    },
                ),
                s(2, MkAction::ImageFind(payload)),
                s(3, variable_move()),
            ],
            fake.clone(),
        )
        .unwrap_err();

        let snapshots = fake.resolved_variables.lock().unwrap();
        assert_eq!(snapshots[0].get("point"), Some(&MkValue::Null));
        assert_eq!(error.kind, DiagnosticKind::TypeMismatch);
        assert_eq!(
            error.message,
            "Variable 'point' contains Null; coordinate target requires Point"
        );
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("point")
        );
        assert_eq!(
            error.context.get("expected").map(String::as_str),
            Some("Point")
        );
        assert_eq!(
            error.context.get("action").map(String::as_str),
            Some("Mouse Move")
        );
        assert!(!fake.events().iter().any(|event| event.starts_with("move:")));
    }

    #[test]
    fn repeated_find_image_point_output_match_then_miss_overwrites_same_runtime_value() {
        let fake = Arc::new(FakeBackend::default());
        fake.script_image(10, Ok(Some(MkPoint { x: 10, y: 20 })));
        fake.script_image(10, Ok(None));
        let mut payload = image_payload(10, MkImageNotFoundPolicy::Continue);
        payload.outputs = MkImageOutputs {
            point: Some("point".into()),
            ..Default::default()
        };

        let error = run(
            vec![
                s(1, MkAction::ImageFind(payload.clone())),
                s(2, variable_move()),
                s(3, MkAction::ImageFind(payload)),
                s(4, variable_move()),
            ],
            fake.clone(),
        )
        .unwrap_err();

        let snapshots = fake.resolved_variables.lock().unwrap();
        assert_eq!(
            snapshots[0].get("point"),
            Some(&MkValue::Point(MkPoint { x: 10, y: 20 }))
        );
        assert_eq!(snapshots[1].get("point"), Some(&MkValue::Null));
        assert_ne!(
            snapshots[1].get("point"),
            Some(&MkValue::Point(MkPoint { x: 10, y: 20 }))
        );
        assert_eq!(error.kind, DiagnosticKind::TypeMismatch);
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| event.starts_with("move:"))
                .collect::<Vec<_>>(),
            ["move:10,20"]
        );
    }

    #[test]
    fn separate_executor_invocations_do_not_share_runtime_outputs_or_mutate_plan() {
        let fake = Arc::new(FakeBackend::default());
        let mut payload = image_payload(10, MkImageNotFoundPolicy::Continue);
        payload.outputs = MkImageOutputs {
            point: Some("point".into()),
            ..Default::default()
        };
        let configured = vec![
            s(1, MkAction::ImageFind(payload.clone())),
            s(2, variable_move()),
        ];

        fake.script_image(10, Ok(Some(MkPoint { x: 500, y: 300 })));
        run(configured.clone(), fake.clone()).unwrap();
        fake.script_image(10, Ok(Some(MkPoint { x: 500, y: 300 })));
        run(vec![configured[0].clone()], fake.clone()).unwrap();
        let error = run(vec![configured[1].clone()], fake).unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::TargetNotFound);
        assert_eq!(configured[0].action, MkAction::ImageFind(payload));
    }
}
