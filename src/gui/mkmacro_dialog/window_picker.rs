//! Shared, transactional window-target picker. Native handles live only in this state.
use crate::mkmacro::{MkWindowMatcher, windows::WindowCandidate};
use eframe::egui;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherDestination {
    Action {
        macro_id: u64,
        draft_generation: u64,
        path: MatcherPath,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherPath {
    Action,
    Condition(Vec<usize>),
    ImageRegion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatcherEditRequest {
    pub destination: MatcherDestination,
    pub original: MkWindowMatcher,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerMode {
    #[default]
    WindowList,
    ScreenPick,
    Preview,
}

#[derive(Clone, Debug)]
pub struct WindowPickerState {
    pub open: bool,
    pub mode: PickerMode,
    pub search: String,
    pub candidates: Vec<WindowCandidate>,
    pub selected: Option<usize>,
    pub executable: String,
    pub title: String,
    pub class_name: String,
    pub process_path: String,
    pub regex: String,
    pub executable_enabled: bool,
    pub title_enabled: bool,
    pub class_enabled: bool,
    pub regex_enabled: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub request: Option<MatcherEditRequest>,
    pub confirm_ready: bool,
    #[cfg(windows)]
    native: NativeScreenPicker,
}

impl Default for WindowPickerState {
    fn default() -> Self {
        Self {
            open: false,
            mode: PickerMode::WindowList,
            search: String::new(),
            candidates: vec![],
            selected: None,
            executable: String::new(),
            title: String::new(),
            class_name: String::new(),
            process_path: String::new(),
            regex: String::new(),
            executable_enabled: true,
            title_enabled: true,
            class_enabled: false,
            regex_enabled: false,
            error: None,
            message: None,
            request: None,
            confirm_ready: false,
            #[cfg(windows)]
            native: Default::default(),
        }
    }
}

pub fn filtered_candidates<'a>(
    items: &'a [WindowCandidate],
    search: &str,
) -> Vec<&'a WindowCandidate> {
    let needle = search.to_lowercase();
    items
        .iter()
        .filter(|c| {
            needle.is_empty()
                || [&c.title, &c.executable, &c.class_name, &c.process_path]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&needle))
        })
        .collect()
}

pub fn matcher_from_preview(s: &WindowPickerState) -> Result<MkWindowMatcher, String> {
    let value = |enabled: bool, text: &str| {
        (enabled && !text.trim().is_empty()).then(|| text.trim().to_owned())
    };
    let title_regex = value(s.regex_enabled, &s.regex);
    if let Some(pattern) = &title_regex {
        regex::Regex::new(pattern).map_err(|e| format!("Invalid title regex: {e}"))?;
    }
    let matcher = MkWindowMatcher {
        process: value(s.executable_enabled, &s.executable),
        title: if title_regex.is_some() {
            None
        } else {
            value(s.title_enabled, &s.title)
        },
        class: value(s.class_enabled, &s.class_name),
        title_regex,
    };
    if matcher.process.is_none()
        && matcher.title.is_none()
        && matcher.class.is_none()
        && matcher.title_regex.is_none()
    {
        Err("Enable and enter at least one matching criterion".into())
    } else {
        Ok(matcher)
    }
}

impl WindowPickerState {
    pub fn open(&mut self, request: MatcherEditRequest) {
        *self = Self::default();
        self.open = true;
        self.request = Some(request);
        self.refresh();
    }
    pub fn cancel(&mut self, message: impl Into<String>) {
        self.open = false;
        self.request = None;
        self.message = Some(message.into());
    }
    pub fn refresh(&mut self) {
        self.error = None;
        match crate::multi_manager::win::enumerate_top_level_windows() {
            Ok(v) => {
                self.candidates = v.into_iter().map(WindowCandidate::from).collect();
                self.selected = None;
            }
            Err(e) => {
                self.candidates.clear();
                self.error = Some(format!("Could not enumerate windows: {e}"));
            }
        }
    }
    fn preview(&mut self, candidate: WindowCandidate) {
        self.executable = candidate
            .executable
            .rsplit(['/', '\\'])
            .next()
            .filter(|v| !v.is_empty())
            .or_else(|| candidate.process_path.rsplit(['/', '\\']).next())
            .unwrap_or_default()
            .to_owned();
        self.title = candidate.title;
        self.class_name = candidate.class_name;
        self.process_path = candidate.process_path;
        self.regex.clear();
        self.executable_enabled = true;
        self.title_enabled = true;
        self.class_enabled = false;
        self.regex_enabled = false;
        self.mode = PickerMode::Preview;
        self.error = None;
    }
    pub fn choose_index(&mut self, index: usize) {
        if let Some(c) = self.candidates.get(index).cloned() {
            self.preview(c);
        }
    }
    pub fn take_confirmation(&mut self) -> Option<(MatcherEditRequest, MkWindowMatcher)> {
        let matcher = matcher_from_preview(self).ok()?;
        let request = self.request.take()?;
        self.open = false;
        Some((request, matcher))
    }
}

pub trait ScreenPickBackend {
    fn poll(&mut self) -> Result<ScreenPickPoll, String>;
    fn identity(&self, root: usize) -> Result<WindowCandidate, String>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPickPoll {
    Idle,
    Hover { child: usize, root: usize },
    Select { child: usize, root: usize },
    Cancel,
}

#[cfg(windows)]
#[derive(Default, Debug, Clone)]
struct NativeScreenPicker {
    left_down: bool,
    escape_down: bool,
}
#[cfg(windows)]
impl ScreenPickBackend for NativeScreenPicker {
    fn poll(&mut self) -> Result<ScreenPickPoll, String> {
        use windows::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
                WindowsAndMessaging::{
                    GA_ROOT, GetAncestor, GetCursorPos, IsWindow, WindowFromPoint,
                },
            },
        };
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }.map_err(|e| e.to_string())?;
        let child = unsafe { WindowFromPoint(p) };
        let root = unsafe { GetAncestor(child, GA_ROOT) };
        if child.0.is_null() || root.0.is_null() || !unsafe { IsWindow(root) }.as_bool() {
            return Err("The pointed window disappeared; move the pointer and retry".into());
        }
        let left = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
        let escape = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
        let event = if escape && !self.escape_down {
            ScreenPickPoll::Cancel
        } else if left && !self.left_down {
            ScreenPickPoll::Select {
                child: child.0 as usize,
                root: root.0 as usize,
            }
        } else {
            ScreenPickPoll::Hover {
                child: child.0 as usize,
                root: root.0 as usize,
            }
        };
        self.left_down = left;
        self.escape_down = escape;
        Ok(event)
    }
    fn identity(&self, root: usize) -> Result<WindowCandidate, String> {
        use crate::multi_manager::win::{
            window_class_name, window_executable, window_process_path, window_title,
        };
        Ok(WindowCandidate {
            handle: root,
            title: window_title(root).ok_or("Could not read the selected window title")?,
            executable: window_executable(root).ok_or("Could not read the selected executable")?,
            class_name: window_class_name(root)
                .ok_or("Could not read the selected window class")?,
            process_path: window_process_path(root)
                .ok_or("Could not read the selected process path")?,
        })
    }
}

fn poll_screen<B: ScreenPickBackend>(state: &mut WindowPickerState, backend: &mut B) {
    match backend.poll() {
        Ok(ScreenPickPoll::Select { root, .. }) => match backend.identity(root) {
            Ok(c) => state.preview(c),
            Err(e) => state.error = Some(e),
        },
        Ok(ScreenPickPoll::Cancel) => state.cancel("Window picking cancelled"),
        Ok(_) => {}
        Err(e) => state.error = Some(e),
    }
}

pub fn show(ctx: &egui::Context, state: &mut WindowPickerState) {
    if !state.open {
        return;
    }
    if state.mode == PickerMode::ScreenPick {
        ctx.request_repaint();
        #[cfg(windows)]
        {
            let mut native = std::mem::take(&mut state.native);
            poll_screen(state, &mut native);
            state.native = native;
        }
    }
    let mut open = state.open;
    egui::Window::new("Window Picker")
        .collapsible(false)
        .open(&mut open)
        .default_width(720.0)
        .show(ctx, |ui| match state.mode {
            PickerMode::WindowList => {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.text_edit_singleline(&mut state.search);
                    if ui.button("Refresh").clicked() {
                        state.refresh()
                    }
                    if ui.button("Pick From Screen").clicked() {
                        #[cfg(windows)]
                        {
                            state.mode = PickerMode::ScreenPick;
                            state.error = None;
                        }
                        #[cfg(not(windows))]
                        {
                            state.error =
                                Some("Direct screen picking is available only on Windows".into());
                        }
                    }
                });
                ui.columns(3, |c| {
                    c[0].strong("Application");
                    c[1].strong("Title");
                    c[2].strong("Class");
                });
                let visible: Vec<usize> = state
                    .candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        filtered_candidates(std::slice::from_ref(c), &state.search).len() == 1
                    })
                    .map(|(i, _)| i)
                    .collect();
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for i in visible {
                            let c = &state.candidates[i];
                            if ui
                                .selectable_label(
                                    state.selected == Some(i),
                                    format!(
                                        "{}    |    {}    |    {}",
                                        c.executable, c.title, c.class_name
                                    ),
                                )
                                .clicked()
                            {
                                state.selected = Some(i);
                            }
                        }
                    });
                if state.error.is_none() {
                    if state.candidates.is_empty() {
                        ui.label("No windows found.");
                    } else if filtered_candidates(&state.candidates, &state.search).is_empty() {
                        ui.label("No windows match this search.");
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.selected.is_some(),
                            egui::Button::new("Choose Selected Window"),
                        )
                        .clicked()
                    {
                        state.choose_index(state.selected.unwrap())
                    }
                    if ui.button("Cancel").clicked() {
                        state.cancel("Window picking cancelled")
                    }
                });
            }
            PickerMode::ScreenPick => {
                ui.heading("Pick From Screen");
                ui.label("Move the pointer over a window. Click to select it. Esc cancels.");
                if ui.button("Back").clicked() {
                    state.mode = PickerMode::WindowList;
                }
            }
            PickerMode::Preview => {
                ui.heading("Window Target");
                ui.checkbox(&mut state.executable_enabled, "Executable");
                ui.text_edit_singleline(&mut state.executable);
                ui.checkbox(&mut state.title_enabled, "Title contains");
                ui.text_edit_singleline(&mut state.title);
                if ui
                    .checkbox(
                        &mut state.regex_enabled,
                        "Regex (takes precedence over title contains)",
                    )
                    .clicked()
                    && state.regex_enabled
                {
                    state.title_enabled = false;
                }
                ui.text_edit_singleline(&mut state.regex);
                ui.checkbox(&mut state.class_enabled, "Class");
                ui.text_edit_singleline(&mut state.class_name);
                ui.label(format!("Process path: {}", state.process_path));
                let validation = matcher_from_preview(state).err();
                if let Some(e) = &validation {
                    ui.colored_label(egui::Color32::RED, e);
                }
                ui.horizontal(|ui| {
                    if ui.button("Back / Retry").clicked() {
                        state.mode = PickerMode::WindowList
                    }
                    if ui.button("Cancel").clicked() {
                        state.cancel("Window picking cancelled")
                    }
                    if ui
                        .add_enabled(validation.is_none(), egui::Button::new("Use Target"))
                        .clicked()
                    {
                        state.confirm_ready = true;
                    }
                });
            }
        });
    if !open {
        state.cancel("Window picking cancelled")
    }
    if let Some(e) = &state.error {
        egui::Area::new(egui::Id::new("picker_error")).show(ctx, |ui| {
            ui.colored_label(egui::Color32::RED, e);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(h: usize, t: &str, e: &str, k: &str, p: &str) -> WindowCandidate {
        WindowCandidate {
            handle: h,
            title: t.into(),
            executable: e.into(),
            class_name: k.into(),
            process_path: p.into(),
        }
    }
    fn preview() -> WindowPickerState {
        let mut s = WindowPickerState::default();
        s.preview(c(
            99,
            "Editor - file",
            "edit.exe",
            "Main",
            "C:/Apps/edit.exe",
        ));
        s
    }
    #[test]
    fn filtering_all_fields() {
        let v = vec![c(
            1,
            "Hello World",
            "APP.EXE",
            "MainClass",
            "C:/Tools/app.exe",
        )];
        for q in ["hello", "app.exe", "mainclass", "tools", "HELLO"] {
            assert_eq!(filtered_candidates(&v, q).len(), 1)
        }
        assert_eq!(filtered_candidates(&v, "").len(), 1);
        assert!(filtered_candidates(&v, "other").is_empty())
    }
    #[test]
    fn enumerated_conversion_preserves_identity() {
        let w = crate::multi_manager::win::EnumeratedWindow {
            hwnd: 42,
            title: "Title".into(),
            executable: "app.exe".into(),
            class_name: "Class".into(),
            process_path: "C:/app.exe".into(),
            rect: crate::multi_manager::model::MmRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            },
        };
        let c = WindowCandidate::from(w);
        assert_eq!(
            (
                c.handle,
                c.title.as_str(),
                c.executable.as_str(),
                c.class_name.as_str(),
                c.process_path.as_str()
            ),
            (42, "Title", "app.exe", "Class", "C:/app.exe")
        );
    }
    #[test]
    fn defaults_and_options() {
        let mut s = preview();
        let m = matcher_from_preview(&s).unwrap();
        assert_eq!(m.process.as_deref(), Some("edit.exe"));
        assert_eq!(m.title.as_deref(), Some("Editor - file"));
        assert!(m.class.is_none());
        s.class_enabled = true;
        assert_eq!(
            matcher_from_preview(&s).unwrap().class.as_deref(),
            Some("Main")
        );
        s.regex_enabled = true;
        s.regex = "^Editor".into();
        let m = matcher_from_preview(&s).unwrap();
        assert!(m.title.is_none());
        assert_eq!(m.title_regex.as_deref(), Some("^Editor"));
    }
    #[test]
    fn validation_and_handle_never_persist() {
        let mut s = preview();
        s.regex_enabled = true;
        s.regex = "[".into();
        assert!(matcher_from_preview(&s).is_err());
        s.regex_enabled = false;
        s.executable_enabled = false;
        s.title_enabled = false;
        assert!(matcher_from_preview(&s).is_err());
        let matcher = matcher_from_preview(&preview()).unwrap();
        let json = serde_json::to_string(&matcher).unwrap();
        assert!(!json.contains("99"));
    }
    #[test]
    fn cancellation_preserves_original_matcher() {
        let original = MkWindowMatcher {
            title: Some("Original".into()),
            ..Default::default()
        };
        let mut s = WindowPickerState::default();
        s.open(MatcherEditRequest {
            destination: MatcherDestination::Action {
                macro_id: 1,
                draft_generation: 2,
                path: MatcherPath::Action,
            },
            original: original.clone(),
        });
        s.cancel("cancelled");
        assert!(s.request.is_none());
        assert_eq!(original.title.as_deref(), Some("Original"));
    }
    #[test]
    fn generated_matcher_is_runtime_compatible() {
        let s = preview();
        let m = matcher_from_preview(&s).unwrap();
        let source = c(99, "Editor - file", "edit.exe", "Main", "C:/Apps/edit.exe");
        assert!(crate::mkmacro::windows::candidate_matches(&m, &source).unwrap());
        assert!(
            !crate::mkmacro::windows::candidate_matches(
                &m,
                &c(2, "Other", "edit.exe", "Main", "C:/Apps/edit.exe")
            )
            .unwrap()
        );
        assert!(
            !crate::mkmacro::windows::candidate_matches(
                &m,
                &c(3, "Editor - file", "other.exe", "Main", "C:/Apps/other.exe")
            )
            .unwrap()
        );
    }
    struct Fake {
        polls: Vec<Result<ScreenPickPoll, String>>,
        identity: Result<WindowCandidate, String>,
    }
    impl ScreenPickBackend for Fake {
        fn poll(&mut self) -> Result<ScreenPickPoll, String> {
            self.polls.remove(0)
        }
        fn identity(&self, _: usize) -> Result<WindowCandidate, String> {
            self.identity.clone()
        }
    }
    #[test]
    fn screen_poll_transitions_are_transactional() {
        let candidate = c(7, "Picked", "pick.exe", "Root", "C:/pick.exe");
        let mut s = WindowPickerState::default();
        s.open = true;
        s.mode = PickerMode::ScreenPick;
        let mut fake = Fake {
            polls: vec![
                Ok(ScreenPickPoll::Hover { child: 8, root: 7 }),
                Ok(ScreenPickPoll::Select { child: 8, root: 7 }),
            ],
            identity: Ok(candidate),
        };
        poll_screen(&mut s, &mut fake);
        assert_eq!(s.mode, PickerMode::ScreenPick);
        poll_screen(&mut s, &mut fake);
        assert_eq!(s.mode, PickerMode::Preview);
        assert_eq!(s.executable, "pick.exe");
    }
    #[test]
    fn screen_poll_cancel_and_failures_are_non_destructive() {
        let mut s = WindowPickerState::default();
        s.open = true;
        s.mode = PickerMode::ScreenPick;
        let mut fake = Fake {
            polls: vec![Ok(ScreenPickPoll::Select { child: 2, root: 1 })],
            identity: Err("disappeared".into()),
        };
        poll_screen(&mut s, &mut fake);
        assert_eq!(s.error.as_deref(), Some("disappeared"));
        assert_eq!(s.mode, PickerMode::ScreenPick);
        fake.polls = vec![Ok(ScreenPickPoll::Cancel)];
        poll_screen(&mut s, &mut fake);
        assert!(!s.open);
    }
}
