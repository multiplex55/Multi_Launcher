use super::SearchRegion;
use super::model::*;
use super::variables::*;
use super::{CaptureGeometryError, MonitorDescriptor};
use regex::Regex;
use std::{collections::HashSet, path::Path};

#[derive(Debug, Clone, Copy)]
pub enum MonitorValidation<'a> {
    NotRequested,
    Available(&'a [MonitorDescriptor]),
    EnumerationFailed,
}
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    pub asset_root: Option<&'a Path>,
    pub monitors: MonitorValidation<'a>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Fatal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkDiagnostic {
    pub severity: DiagnosticSeverity,
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub code: &'static str,
    pub message: String,
}
fn push(
    out: &mut Vec<MkDiagnostic>,
    m: u64,
    s: Option<u64>,
    code: &'static str,
    msg: impl Into<String>,
) {
    out.push(MkDiagnostic {
        severity: DiagnosticSeverity::Fatal,
        macro_id: m,
        step_id: s,
        code,
        message: msg.into(),
    })
}
fn interpolation_syntax(template: &str) -> Result<(), &'static str> {
    let mut cursor = 0;
    while cursor < template.len() {
        let rest = &template[cursor..];
        let prefix = if rest.starts_with("$${") {
            Some((3, "escaped "))
        } else if rest.starts_with("${") {
            Some((2, ""))
        } else {
            None
        };
        if let Some((offset, escaped)) = prefix {
            let start = cursor + offset;
            let Some(end) = template[start..].find('}').map(|end| start + end) else {
                return Err(if escaped.is_empty() {
                    "unclosed interpolation placeholder"
                } else {
                    "unclosed escaped interpolation placeholder"
                });
            };
            if end == start {
                return Err(if escaped.is_empty() {
                    "empty interpolation placeholder"
                } else {
                    "empty escaped interpolation placeholder"
                });
            }
            cursor = end + 1;
        } else {
            cursor += rest.chars().next().unwrap().len_utf8();
        }
    }
    Ok(())
}
fn image_outputs(p: &MkImagePayload, m: u64, s: Option<u64>, out: &mut Vec<MkDiagnostic>) {
    let slots = [
        ("found", "invalid_image_output_found", &p.outputs.found),
        ("point", "invalid_image_output_point", &p.outputs.point),
        ("x", "invalid_image_output_x", &p.outputs.x),
        ("y", "invalid_image_output_y", &p.outputs.y),
    ];
    let mut names = HashSet::new();
    for (slot, code, name) in slots {
        let Some(name) = name else { continue };
        if let Err(reason) = validate_variable_name(name) {
            push(
                out,
                m,
                s,
                code,
                format!("Image output {slot} name '{name}' is invalid: {reason}"),
            );
        } else if !names.insert(name.as_str()) {
            push(
                out,
                m,
                s,
                code,
                format!("Image output {slot} duplicates configured name '{name}'"),
            );
        }
    }
}
pub fn can_run(ds: &[MkDiagnostic]) -> bool {
    !ds.iter().any(|d| d.severity == DiagnosticSeverity::Fatal)
}
pub fn validate_document(doc: &MkMacroDocument, asset_root: Option<&Path>) -> Vec<MkDiagnostic> {
    validate_document_with_context(
        doc,
        ValidationContext {
            asset_root,
            monitors: MonitorValidation::NotRequested,
        },
    )
}
pub fn validate_document_with_context(
    doc: &MkMacroDocument,
    context: ValidationContext<'_>,
) -> Vec<MkDiagnostic> {
    let asset_root = context.asset_root;
    let mut out = vec![];
    let mut mids = HashSet::new();
    for m in &doc.macros {
        if m.id == 0 || !mids.insert(m.id) {
            push(
                &mut out,
                m.id,
                None,
                "invalid_macro_id",
                "Macro IDs must be non-zero and unique",
            )
        };
        let mut ids = HashSet::new();
        let pixel_search_ids: HashSet<u64> = m
            .steps
            .iter()
            .filter_map(|s| match &s.action {
                MkAction::FindPixel(p) if p.search_id != 0 => Some(p.search_id),
                _ => None,
            })
            .collect();
        let mut seen_pixel_ids = HashSet::new();
        let mut stack: Vec<(&str, bool)> = vec![];
        if m.playback.speed_percent == 0 {
            push(
                &mut out,
                m.id,
                None,
                "invalid_speed",
                "Playback speed must be positive",
            )
        }
        for s in &m.steps {
            let sid = Some(s.id);
            if s.id == 0 || !ids.insert(s.id) {
                push(
                    &mut out,
                    m.id,
                    sid,
                    "invalid_step_id",
                    "Step IDs must be non-zero and unique",
                )
            };
            if s.repeat == 0 {
                push(
                    &mut out,
                    m.id,
                    sid,
                    "invalid_repeat",
                    "Step repeat must be positive",
                )
            };
            if let MkErrorPolicy::Retry(r) = &s.on_error
                && r.attempts == 0
            {
                push(
                    &mut out,
                    m.id,
                    sid,
                    "invalid_retry",
                    "Retry attempts must be positive",
                )
            }
            match &s.action {
                MkAction::LauncherCommand(payload) => {
                    if let Some(action) = &payload.legacy_resolved_action {
                        if action.action.trim().is_empty() {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "invalid_legacy_launcher_action",
                                "Preserved Launcher action requires a canonical action",
                            );
                        }
                    } else if payload.query.trim().is_empty() {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "empty_launcher_command",
                            "Launcher Command requires a command/query",
                        );
                    }
                }
                MkAction::Notify(payload) => {
                    // Windows notifications require a title; diagnose this before delivery.
                    if payload.title.trim().is_empty() {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "empty_notify_title",
                            "Notification title cannot be empty",
                        );
                    }
                    for (field, value, code) in [
                        (
                            "notify.title",
                            &payload.title,
                            "invalid_notify_title_interpolation",
                        ),
                        (
                            "notify.description",
                            &payload.description,
                            "invalid_notify_description_interpolation",
                        ),
                    ] {
                        if let Err(reason) = interpolation_syntax(value) {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                code,
                                format!("Malformed interpolation in {field}: {reason}"),
                            );
                        }
                    }
                }
                MkAction::PlaySound(payload) => {
                    if payload.sound == "None"
                        || !crate::sound::SOUND_NAMES.contains(&payload.sound.as_str())
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_play_sound",
                            format!("Unknown macro sound name '{}'", payload.sound),
                        );
                    }
                }
                MkAction::If(c) => {
                    condition(c, m.id, sid, asset_root, &mut out);
                    stack.push(("if", false))
                }
                MkAction::Else => match stack.last_mut() {
                    Some(("if", seen)) if !*seen => *seen = true,
                    Some(("if", _)) => push(
                        &mut out,
                        m.id,
                        sid,
                        "multiple_else",
                        "An If block may have only one Else",
                    ),
                    _ => push(
                        &mut out,
                        m.id,
                        sid,
                        "invalid_else",
                        "Else is not inside an If",
                    ),
                },
                MkAction::EndIf => {
                    if !matches!(stack.pop(), Some(("if", _))) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_endif",
                            "EndIf does not close an If",
                        )
                    }
                }
                MkAction::RepeatStart { count: _ } => stack.push(("repeat", false)),
                MkAction::RepeatEnd => {
                    if !matches!(stack.pop(), Some(("repeat", _))) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_repeat_end",
                            "RepeatEnd does not close RepeatStart",
                        )
                    }
                }
                MkAction::WhileStart { condition: c } => {
                    condition(c, m.id, sid, asset_root, &mut out);
                    stack.push(("while", false))
                }
                MkAction::WhileEnd => {
                    if !matches!(stack.pop(), Some(("while", _))) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_while_end",
                            "WhileEnd does not close WhileStart",
                        )
                    }
                }
                MkAction::Break | MkAction::Continue => {
                    if !stack.iter().any(|x| x.0 == "repeat" || x.0 == "while") {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "loop_control_outside_loop",
                            "Break/Continue requires an enclosing loop",
                        )
                    }
                }
                MkAction::SetVariable { name, .. } | MkAction::UnsetVariable { name } => {
                    if let Err(e) = validate_variable_name(name) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_variable",
                            format!("Variable name is invalid: {e}"),
                        )
                    }
                }
                MkAction::PromptInput(payload) => {
                    if let Err(e) = validate_variable_name(&payload.variable) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_variable",
                            format!("Variable name is invalid: {e}"),
                        )
                    }
                }
                MkAction::WaitUntil {
                    condition: c,
                    wait: w,
                } => {
                    condition(c, m.id, sid, asset_root, &mut out);
                    wait(w, m.id, sid, &mut out)
                }
                MkAction::WindowActivate(p) | MkAction::WindowWait(p) => {
                    matcher(&p.matcher, m.id, sid, &mut out);
                    if let Some(w) = &p.wait {
                        wait(w, m.id, sid, &mut out)
                    }
                }
                MkAction::WindowClose(x) => matcher(x, m.id, sid, &mut out),
                MkAction::WindowState { matcher: x, .. } => matcher(x, m.id, sid, &mut out),
                MkAction::WindowMoveResize(p) => {
                    matcher(&p.matcher, m.id, sid, &mut out);
                    match (p.x, p.y) {
                        (Some(_), Some(_)) | (None, None) => {}
                        _ => push(
                            &mut out,
                            m.id,
                            sid,
                            "incomplete_window_move",
                            "Move requires both X and Y",
                        ),
                    }
                    match (p.width, p.height) {
                        (Some(w), Some(h)) => {
                            if w == 0 || h == 0 {
                                push(
                                    &mut out,
                                    m.id,
                                    sid,
                                    "invalid_window_resize",
                                    "Resize Width and Height must be at least 1",
                                );
                            }
                        }
                        (None, None) => {}
                        _ => push(
                            &mut out,
                            m.id,
                            sid,
                            "incomplete_window_resize",
                            "Resize requires both Width and Height",
                        ),
                    }
                    if p.x.is_none() && p.y.is_none() && p.width.is_none() && p.height.is_none() {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "empty_window_move_resize",
                            "Enable Move or Resize",
                        );
                    }
                }
                MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
                    wait(&p.wait, m.id, sid, &mut out);
                    asset(p.asset_id, m.id, sid, asset_root, &mut out);
                    image_outputs(p, m.id, sid, &mut out);
                    match &p.region {
                        SearchRegion::Rectangle { rect } => {
                            if let Err(error) = rect.validate_capture() {
                                let message = match error {
                                    CaptureGeometryError::ZeroWidth => {
                                        "Image search rectangle width must be positive"
                                    }
                                    CaptureGeometryError::ZeroHeight => {
                                        "Image search rectangle height must be positive"
                                    }
                                    CaptureGeometryError::RightOverflow => {
                                        "Image search rectangle right endpoint is out of range"
                                    }
                                    CaptureGeometryError::BottomOverflow => {
                                        "Image search rectangle bottom endpoint is out of range"
                                    }
                                    CaptureGeometryError::AllocationOverflow => {
                                        "Image search rectangle is too large to capture"
                                    }
                                };
                                push(&mut out, m.id, sid, "invalid_image_region", message)
                            }
                        }
                        SearchRegion::Monitor { index } => match context.monitors {
                            MonitorValidation::Available(monitors)
                                if !monitors.iter().any(|d| d.index == *index) =>
                            {
                                push(
                                    &mut out,
                                    m.id,
                                    sid,
                                    "unavailable_monitor",
                                    format!("Selected monitor {index} is no longer available"),
                                )
                            }
                            MonitorValidation::EnumerationFailed => push(
                                &mut out,
                                m.id,
                                sid,
                                "monitor_enumeration_failed",
                                "Monitor enumeration is unavailable",
                            ),
                            _ => {}
                        },
                        SearchRegion::Window { matcher: window }
                        | SearchRegion::ClientArea { matcher: window } => {
                            matcher(window, m.id, sid, &mut out)
                        }
                        _ => {}
                    }
                }
                MkAction::FindPixel(p) => {
                    if p.search_id == 0 || !seen_pixel_ids.insert(p.search_id) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_pixel_search_id",
                            "Pixel search IDs must be non-zero and unique",
                        );
                    }
                    wait(&p.wait, m.id, sid, &mut out);
                    if super::screen::parse_rgb(&p.color).is_err() {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_pixel_color",
                            "Enter a color as #RRGGBB",
                        );
                    }
                    for name in [
                        &p.outputs.found,
                        &p.outputs.point,
                        &p.outputs.x,
                        &p.outputs.y,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if validate_variable_name(name).is_err() {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "invalid_pixel_output",
                                format!("Invalid pixel output variable '{name}'"),
                            );
                        }
                    }
                    match &p.region {
                        SearchRegion::Rectangle { rect } if rect.validate_capture().is_err() => {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "invalid_pixel_region",
                                "Pixel search rectangle is invalid",
                            )
                        }
                        SearchRegion::Window { matcher: x }
                        | SearchRegion::ClientArea { matcher: x } => {
                            matcher(x, m.id, sid, &mut out)
                        }
                        _ => {}
                    }
                }
                MkAction::CaptureScreenshot(p) => {
                    if p.destination.produces_file() {
                        if p.path.as_ref().is_none_or(|path| path.trim().is_empty()) {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "empty_screenshot_path",
                                "File screenshot destination requires a non-empty path",
                            );
                        }
                    } else {
                        if p.path.is_some() {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "incompatible_screenshot_path",
                                "Clipboard-only screenshots cannot specify a file path",
                            );
                        }
                        if p.path_output.is_some() {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "incompatible_screenshot_output",
                                "Clipboard-only screenshots cannot set a path output variable",
                            );
                        }
                    }
                    if let Some(name) = &p.path_output
                        && validate_variable_name(name).is_err()
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_screenshot_output",
                            format!("Invalid screenshot path output variable '{name}'"),
                        );
                    }
                    match &p.region {
                        SearchRegion::Rectangle { rect } if rect.validate_capture().is_err() => {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "invalid_screenshot_region",
                                "Screenshot rectangle is invalid",
                            )
                        }
                        SearchRegion::Window { matcher: x }
                        | SearchRegion::ClientArea { matcher: x } => {
                            matcher(x, m.id, sid, &mut out)
                        }
                        SearchRegion::Monitor { index } if matches!(context.monitors, MonitorValidation::Available(monitors) if !monitors.iter().any(|d| d.index == *index)) => {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "unavailable_screenshot_monitor",
                                format!("Selected monitor {index} is no longer available"),
                            )
                        }
                        _ => {}
                    }
                }
                MkAction::WaitForVisualChange(p) => {
                    if !p.change_threshold_percent.is_finite()
                        || !(0.0..=100.0).contains(&p.change_threshold_percent)
                        || p.change_threshold_percent == 0.0
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_visual_change_threshold",
                            "Visual change threshold must be greater than 0 and at most 100 percent",
                        );
                    }
                    if p.poll_interval_ms == 0
                        || (p.timeout_ms > 0 && p.poll_interval_ms > p.timeout_ms)
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_visual_change_poll",
                            "Visual change poll interval must be greater than zero and no longer than a finite timeout",
                        );
                    }
                    if p.consecutive_changed_frames.unwrap_or(1) == 0
                        || (p.timeout_ms > 0
                            && u64::from(p.consecutive_changed_frames.unwrap_or(1))
                                > p.timeout_ms / p.poll_interval_ms.max(1) + 1)
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "impossible_visual_change_settling",
                            "Visual change settling frames cannot be observed within the timeout",
                        );
                    }
                    match &p.region {
                        SearchRegion::Rectangle { rect } if rect.validate_capture().is_err() => {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "invalid_visual_change_region",
                                "Visual change rectangle is invalid",
                            )
                        }
                        SearchRegion::Window { matcher: x }
                        | SearchRegion::ClientArea { matcher: x } => {
                            matcher(x, m.id, sid, &mut out)
                        }
                        SearchRegion::Monitor { index } if matches!(context.monitors, MonitorValidation::Available(monitors) if !monitors.iter().any(|d| d.index == *index)) => {
                            push(
                                &mut out,
                                m.id,
                                sid,
                                "unavailable_visual_change_monitor",
                                format!("Selected monitor {index} is no longer available"),
                            )
                        }
                        _ => {}
                    }
                }
                MkAction::MouseMove(p) => {
                    target(&p.target, m.id, sid, asset_root, &mut out);
                    validate_pixel_reference(&p.target, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::MouseClick(p) => {
                    target(&p.target, m.id, sid, asset_root, &mut out);
                    validate_pixel_reference(&p.target, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::MouseDrag(p) => {
                    target(&p.from, m.id, sid, asset_root, &mut out);
                    target(&p.to, m.id, sid, asset_root, &mut out);
                    validate_pixel_reference(&p.from, &pixel_search_ids, m.id, sid, &mut out);
                    validate_pixel_reference(&p.to, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::PixelCheck {
                    target: t, color, ..
                } => {
                    target(t, m.id, sid, asset_root, &mut out);
                    if super::screen::parse_rgb(color).is_err() {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_pixel_color",
                            "Enter a color as #RRGGBB",
                        );
                    }
                }
                _ => {}
            }
        }
        for (kind, _) in stack {
            push(
                &mut out,
                m.id,
                None,
                "unclosed_block",
                format!("Unclosed {kind} block"),
            )
        }
    }
    out
}
fn wait(w: &MkWaitOptions, m: u64, s: Option<u64>, o: &mut Vec<MkDiagnostic>) {
    if w.poll_interval_ms == 0 || (w.timeout_ms > 0 && w.poll_interval_ms > w.timeout_ms) {
        push(
            o,
            m,
            s,
            "invalid_wait",
            "Polling interval must be positive and cannot exceed a finite timeout",
        )
    }
}

#[cfg(test)]
mod optional_wait_validation_tests {
    use super::*;

    fn wait_codes(timeout_ms: u64, poll_interval_ms: u64) -> Vec<&'static str> {
        let mut diagnostics = Vec::new();
        wait(
            &MkWaitOptions {
                timeout_ms,
                poll_interval_ms,
            },
            1,
            Some(1),
            &mut diagnostics,
        );
        diagnostics.into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn optional_timeout_wait_rules() {
        assert!(wait_codes(0, 1).is_empty());
        assert!(wait_codes(0, u64::MAX).is_empty());
        assert_eq!(wait_codes(0, 0), ["invalid_wait"]);
        assert_eq!(wait_codes(10, 0), ["invalid_wait"]);
        assert_eq!(wait_codes(10, 11), ["invalid_wait"]);
        assert!(wait_codes(10, 10).is_empty());
    }

    fn visual_codes(payload: WaitForVisualChange) -> Vec<&'static str> {
        let document = MkMacroDocument {
            macros: vec![MkMacro {
                id: 1,
                name: "visual wait".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: MkPlayback::default(),
                steps: vec![MkStep {
                    id: 1,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: MkErrorPolicy::default(),
                    action: MkAction::WaitForVisualChange(payload),
                }],
                image_assets: vec![],
            }],
            ..MkMacroDocument::default()
        };
        validate_document(&document, None)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn visual_settling_is_only_bounded_by_finite_timeouts() {
        let indefinite = WaitForVisualChange {
            timeout_ms: 0,
            poll_interval_ms: u64::MAX,
            consecutive_changed_frames: Some(u32::MAX),
            ..WaitForVisualChange::default()
        };
        assert!(visual_codes(indefinite).is_empty());

        let finite = WaitForVisualChange {
            timeout_ms: 100,
            poll_interval_ms: 50,
            consecutive_changed_frames: Some(4),
            ..WaitForVisualChange::default()
        };
        assert!(visual_codes(finite).contains(&"impossible_visual_change_settling"));

        for payload in [
            WaitForVisualChange {
                timeout_ms: 0,
                poll_interval_ms: 0,
                ..WaitForVisualChange::default()
            },
            WaitForVisualChange {
                timeout_ms: 0,
                consecutive_changed_frames: Some(0),
                ..WaitForVisualChange::default()
            },
            WaitForVisualChange {
                timeout_ms: 0,
                change_threshold_percent: f64::NAN,
                ..WaitForVisualChange::default()
            },
        ] {
            assert!(!visual_codes(payload).is_empty());
        }
    }
}
fn matcher(x: &MkWindowMatcher, m: u64, s: Option<u64>, o: &mut Vec<MkDiagnostic>) {
    let usable = |value: &Option<String>| value.as_ref().is_some_and(|v| !v.trim().is_empty());
    if !usable(&x.title) && !usable(&x.title_regex) && !usable(&x.process) && !usable(&x.class) {
        push(
            o,
            m,
            s,
            "empty_window_matcher",
            "Enter at least one window matcher",
        )
    };
    if let Some(r) = &x.title_regex
        && Regex::new(r).is_err()
    {
        push(o, m, s, "invalid_regex", "Window title regex is invalid")
    }
}
fn asset(id: u64, m: u64, s: Option<u64>, root: Option<&Path>, o: &mut Vec<MkDiagnostic>) {
    if id == 0 {
        push(
            o,
            m,
            s,
            "reference_image_missing",
            "Reference image is missing",
        )
    } else if let Some(root) = root {
        let Ok(path) = super::store::managed_asset_path(root, m, id) else {
            push(
                o,
                m,
                s,
                "reference_image_missing",
                "Reference image is missing",
            );
            return;
        };
        if !path.exists() {
            push(
                o,
                m,
                s,
                "reference_image_missing",
                "Reference image is missing",
            );
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(v) => v,
            Err(_) => {
                push(
                    o,
                    m,
                    s,
                    "reference_image_unreadable",
                    "Reference image could not be read",
                );
                return;
            }
        };
        let decoded = match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
            Ok(v) => v,
            Err(_) => {
                push(
                    o,
                    m,
                    s,
                    "reference_image_undecodable",
                    "Reference image could not be decoded",
                );
                return;
            }
        };
        if !usable_image_dimensions(decoded.width(), decoded.height()) {
            push(
                o,
                m,
                s,
                "reference_image_invalid_dimensions",
                "Reference image has invalid dimensions",
            );
        }
    }
}
fn usable_image_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

fn target(
    target: &MkCoordinateTarget,
    m: u64,
    s: Option<u64>,
    root: Option<&Path>,
    out: &mut Vec<MkDiagnostic>,
) {
    match target {
        MkCoordinateTarget::WindowClient { matcher: value, .. } => matcher(value, m, s, out),
        MkCoordinateTarget::Variable { name } if name.trim().is_empty() => push(
            out,
            m,
            s,
            "empty_point_variable",
            "Enter a point variable name",
        ),
        MkCoordinateTarget::Variable { name } if validate_variable_name(name).is_err() => push(
            out,
            m,
            s,
            "invalid_point_variable",
            "Point variable name is invalid",
        ),
        MkCoordinateTarget::Image { asset_id, .. } => asset(*asset_id, m, s, root, out),
        MkCoordinateTarget::Pixel { search_id, .. } if *search_id == 0 => push(
            out,
            m,
            s,
            "missing_pixel_search",
            "Select a Find Pixel Color result",
        ),
        MkCoordinateTarget::Pixel { .. } => {}
        _ => {}
    }
}
fn validate_pixel_reference(
    target: &MkCoordinateTarget,
    ids: &HashSet<u64>,
    m: u64,
    s: Option<u64>,
    out: &mut Vec<MkDiagnostic>,
) {
    if let MkCoordinateTarget::Pixel { search_id, .. } = target
        && *search_id != 0
        && !ids.contains(search_id)
    {
        push(
            out,
            m,
            s,
            "unknown_pixel_search",
            format!("Pixel result references unknown search {search_id}"),
        );
    }
}
fn condition(
    c: &MkCondition,
    m: u64,
    s: Option<u64>,
    root: Option<&Path>,
    o: &mut Vec<MkDiagnostic>,
) {
    match c {
        MkCondition::Variable { name, op, value } => {
            if !is_builtin(name) && validate_variable_name(name).is_err() {
                push(
                    o,
                    m,
                    s,
                    "invalid_variable",
                    "Condition has an invalid variable name",
                )
            };
            let ok = match op {
                MkCompareOp::Eq | MkCompareOp::NotEq => true,
                MkCompareOp::Less
                | MkCompareOp::LessOrEq
                | MkCompareOp::Greater
                | MkCompareOp::GreaterOrEq => {
                    matches!(value, MkValue::Number(_) | MkValue::String(_))
                }
                MkCompareOp::Contains | MkCompareOp::StartsWith | MkCompareOp::EndsWith => {
                    matches!(value, MkValue::String(_))
                }
                MkCompareOp::Regex => matches!(value,MkValue::String(v) if Regex::new(v).is_ok()),
            };
            if !ok {
                push(
                    o,
                    m,
                    s,
                    "invalid_comparison",
                    "Operator and comparison value are incompatible",
                )
            }
        }
        MkCondition::WindowExists { matcher: x } | MkCondition::WindowActive { matcher: x } => {
            matcher(x, m, s, o)
        }
        MkCondition::ImageSearch { search, .. } => {
            asset(search.asset_id, m, s, root, o);
            match &search.region {
                SearchRegion::Rectangle { rect } if rect.validate_capture().is_err() => push(
                    o,
                    m,
                    s,
                    "invalid_image_region",
                    "Image search rectangle is invalid",
                ),
                SearchRegion::Window { matcher: x } | SearchRegion::ClientArea { matcher: x } => {
                    matcher(x, m, s, o)
                }
                _ => {}
            }
        }
        MkCondition::PreviousImageResult {
            asset_id: Some(asset_id),
            ..
        } => asset(*asset_id, m, s, root, o),
        MkCondition::PreviousImageResult { asset_id: None, .. } => {}
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            for x in conditions {
                condition(x, m, s, root, o)
            }
        }
        MkCondition::Not { condition: x } => condition(x, m, s, root, o),
        _ => {}
    }
}

#[cfg(test)]
mod coordinate_target_tests {
    use super::*;

    fn diagnostics(matcher: MkWindowMatcher) -> Vec<MkDiagnostic> {
        let mut out = Vec::new();
        target(
            &MkCoordinateTarget::WindowClient {
                matcher,
                point: MkPoint { x: 0, y: 0 },
            },
            1,
            Some(2),
            None,
            &mut out,
        );
        out
    }

    #[test]
    fn matched_coordinate_validates_matcher() {
        assert!(
            diagnostics(MkWindowMatcher::default())
                .iter()
                .any(|d| d.code == "empty_window_matcher")
        );
        assert!(
            diagnostics(MkWindowMatcher {
                title_regex: Some("[".into()),
                ..Default::default()
            })
            .iter()
            .any(|d| d.code == "invalid_regex")
        );
        assert!(
            diagnostics(MkWindowMatcher {
                process: Some("app.exe".into()),
                title: Some("Editor".into()),
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn managed_image_assets_are_validated_as_png_content() {
        use image::{Rgba, RgbaImage};
        let dir = tempfile::tempdir().unwrap();
        let root = &dir.path().join(crate::mkmacro::store::ASSET_DIRECTORY);
        std::fs::create_dir(root).unwrap();
        let mut out = Vec::new();
        asset(0, 7, Some(8), Some(root), &mut out);
        assert!(out.iter().any(|d| d.code == "reference_image_missing"));
        out.clear();
        asset(1, 7, Some(8), Some(root), &mut out);
        assert!(out.iter().any(|d| d.code == "reference_image_missing"));

        let macro_dir = root.join("7");
        std::fs::create_dir_all(&macro_dir).unwrap();
        RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255]))
            .save_with_format(macro_dir.join("1.png"), image::ImageFormat::Png)
            .unwrap();
        out.clear();
        asset(1, 7, Some(8), Some(root), &mut out);
        assert!(out.is_empty());

        for bytes in [
            b"corrupt".as_slice(),
            b"\xff\xd8\xff\xe0renamed jpeg".as_slice(),
        ] {
            std::fs::write(macro_dir.join("1.png"), bytes).unwrap();
            out.clear();
            asset(1, 7, Some(8), Some(root), &mut out);
            assert!(out.iter().any(|d| d.code == "reference_image_undecodable"));
            assert!(!can_run(&out));
        }
    }
}

#[cfg(test)]
mod reference_image_tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;

    fn validate(id: u64, root: Option<&Path>) -> Vec<MkDiagnostic> {
        let mut out = Vec::new();
        asset(id, 7, Some(11), root, &mut out);
        out
    }

    #[test]
    fn unset_absent_corrupt_and_valid_assets_have_distinct_results() {
        assert_eq!(validate(0, None)[0].code, "reference_image_missing");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(crate::mkmacro::store::ASSET_DIRECTORY);
        std::fs::create_dir_all(root.join("7")).unwrap();
        assert_eq!(validate(4, Some(&root))[0].code, "reference_image_missing");
        std::fs::write(root.join("7/4.png"), b"not png").unwrap();
        assert_eq!(
            validate(4, Some(&root))[0].code,
            "reference_image_undecodable"
        );
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(1, 1))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        std::fs::write(root.join("7/4.png"), bytes.into_inner()).unwrap();
        assert!(validate(4, Some(&root)).is_empty());
        assert!(!usable_image_dimensions(0, 1));
        assert!(!usable_image_dimensions(1, 0));
    }
}

#[cfg(test)]
mod notification_action_tests {
    use super::*;

    fn diagnostics(action: MkAction) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        id: 1,
                        enabled: true,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action,
                    }],
                    image_assets: vec![],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    #[test]
    fn only_exact_playable_macro_sound_names_validate() {
        for sound in crate::sound::SOUND_NAMES {
            let found = diagnostics(MkAction::PlaySound(MkPlaySoundPayload {
                sound: (*sound).into(),
            }));
            assert_eq!(found.is_empty(), *sound != "None", "{sound}");
        }
        for sound in ["", "Unknown.wav", "sounds/Alarm.wav", "alarm.wav"] {
            let found = diagnostics(MkAction::PlaySound(MkPlaySoundPayload {
                sound: sound.into(),
            }));
            assert_eq!(found.len(), 1, "{sound}");
            assert_eq!(found[0].code, "invalid_play_sound");
        }
    }

    #[test]
    fn notification_interpolation_diagnostics_identify_the_field() {
        let found = diagnostics(MkAction::Notify(MkNotifyPayload {
            title: "${".into(),
            description: "${}".into(),
            ..MkNotifyPayload::default()
        }));
        assert!(
            found
                .iter()
                .any(|d| d.code == "invalid_notify_title_interpolation"
                    && d.message.contains("notify.title"))
        );
        assert!(
            found
                .iter()
                .any(|d| d.code == "invalid_notify_description_interpolation"
                    && d.message.contains("notify.description"))
        );
        assert!(
            diagnostics(MkAction::Notify(MkNotifyPayload {
                title: "Done ${job}".into(),
                description: "Result $${literal}".into(),
                ..MkNotifyPayload::default()
            }))
            .is_empty()
        );
    }
}

#[cfg(test)]
mod launcher_command_action_tests {
    use super::*;
    use crate::actions::Action;

    fn diagnostics(payload: MkLauncherCommandPayload) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        id: 1,
                        enabled: true,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action: MkAction::LauncherCommand(payload),
                    }],
                    image_assets: vec![],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    #[test]
    fn empty_and_whitespace_only_queries_are_rejected_without_mutation() {
        for query in ["", " ", "\t", " \t\n\r\u{2003} "] {
            let payload = MkLauncherCommandPayload {
                query: query.into(),
                legacy_resolved_action: None,
            };
            let found = diagnostics(payload.clone());
            assert_eq!(found.len(), 1, "{query:?}");
            assert_eq!(
                found[0].message,
                "Launcher Command requires a command/query"
            );
            assert_eq!(payload.query, query);
        }
    }

    #[test]
    fn arbitrary_queries_pass_structural_validation_without_launcher_resolution() {
        for query in [
            "note list",
            "Notepad",
            "note open ${note_name}",
            "unknown-plugin command available only during playback",
        ] {
            assert!(
                diagnostics(MkLauncherCommandPayload {
                    query: query.into(),
                    legacy_resolved_action: None,
                })
                .is_empty(),
                "{query:?}"
            );
        }
    }

    #[test]
    fn usable_legacy_action_allows_an_empty_display_query_but_is_structurally_validated() {
        let legacy = |action: &str| MkLauncherCommandPayload {
            query: String::new(),
            legacy_resolved_action: Some(Action {
                label: String::new(),
                desc: String::new(),
                action: action.into(),
                args: Some("daily".into()),
            }),
        };
        assert!(diagnostics(legacy("note:open")).is_empty());
        let found = diagnostics(legacy(" \t"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "invalid_legacy_launcher_action");
    }
}
