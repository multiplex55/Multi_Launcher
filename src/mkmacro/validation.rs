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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticScope {
    Macro,
    Document,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkDiagnostic {
    pub severity: DiagnosticSeverity,
    pub scope: DiagnosticScope,
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub code: &'static str,
    pub message: String,
    pub target_macro_id: Option<u64>,
    pub cycle_path: Vec<u64>,
}
impl MkDiagnostic {
    pub(crate) fn fatal(
        macro_id: u64,
        step_id: Option<u64>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Fatal,
            scope: DiagnosticScope::Macro,
            macro_id,
            step_id,
            code,
            message: message.into(),
            target_macro_id: None,
            cycle_path: Vec::new(),
        }
    }
    pub(crate) fn warning(
        macro_id: u64,
        step_id: Option<u64>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            ..Self::fatal(macro_id, step_id, code, message)
        }
    }
}
pub(crate) fn push(
    out: &mut Vec<MkDiagnostic>,
    m: u64,
    s: Option<u64>,
    code: &'static str,
    msg: impl Into<String>,
) {
    out.push(MkDiagnostic::fatal(m, s, code, msg))
}
fn delay(p: &MkDelayPayload, m: u64, s: Option<u64>, o: &mut Vec<MkDiagnostic>) {
    match p.mode {
        MkDelayMode::Fixed => {
            if p.fixed_ms > MAX_DELAY_MS {
                push(
                    o,
                    m,
                    s,
                    "invalid_delay_fixed",
                    format!(
                        "Fixed delay {} ms exceeds maximum {} ms",
                        p.fixed_ms, MAX_DELAY_MS
                    ),
                );
            }
        }
        MkDelayMode::RandomRange => {
            if p.minimum_ms > p.maximum_ms {
                push(
                    o,
                    m,
                    s,
                    "invalid_delay_range",
                    format!(
                        "Random delay minimum_ms ({}) must not exceed maximum_ms ({})",
                        p.minimum_ms, p.maximum_ms
                    ),
                );
            }
            for (field, value) in [("minimum_ms", p.minimum_ms), ("maximum_ms", p.maximum_ms)] {
                if value > MAX_DELAY_MS {
                    push(
                        o,
                        m,
                        s,
                        "invalid_delay_range_endpoint",
                        format!(
                            "Random delay {field} value {value} ms exceeds maximum {} ms",
                            MAX_DELAY_MS
                        ),
                    );
                }
            }
        }
    }
}
pub(crate) fn interpolation_syntax(template: &str) -> Result<(), &'static str> {
    super::interpolation::scan_template(template).try_for_each(|part| part.map(|_| ()))
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
    analyze_document_with_context(doc, context).diagnostics
}

/// Semantic analysis shared by authoring and compiled-program admission.
pub struct DocumentAnalysis {
    pub graph: super::call_graph::CallGraph,
    pub diagnostics: Vec<MkDiagnostic>,
}

pub fn analyze_document(doc: &MkMacroDocument) -> DocumentAnalysis {
    analyze_document_with_context(
        doc,
        ValidationContext {
            asset_root: None,
            monitors: MonitorValidation::NotRequested,
        },
    )
}

fn analyze_document_with_context(
    doc: &MkMacroDocument,
    context: ValidationContext<'_>,
) -> DocumentAnalysis {
    let asset_root = context.asset_root;
    let graph = super::call_graph::CallGraph::build(doc);
    let mut out = graph.identity_diagnostics().to_vec();
    out.extend(graph.cycle_diagnostics(super::call_graph::DependencyPolicy::EnabledCalls));
    let mut invalid_signatures = HashSet::new();
    for m in &doc.macros {
        let start = out.len();
        super::reusable_validation::signature(m.id, &m.signature, &mut out);
        if out.len() != start {
            invalid_signatures.insert(m.id);
        }
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
                MkAction::CallMacro(_) | MkAction::Return(_) => {}
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
                MkAction::Delay(payload) => delay(payload, m.id, sid, &mut out),
                MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop }) => {
                    if *desktop == 0 {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_virtual_desktop_number",
                            "Virtual desktop number must be at least 1",
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
                MkAction::UiReadValue { variable, .. } => {
                    if let Err(reason) = validate_variable_name(variable) {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_variable",
                            format!("Variable name is invalid: {reason}"),
                        );
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
                    asset(&p.image, m.id, sid, asset_root, &mut out);
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
                    target(
                        &p.target,
                        "Mouse Move",
                        false,
                        m.id,
                        sid,
                        asset_root,
                        &mut out,
                    );
                    validate_pixel_reference(&p.target, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::MouseClick(p) => {
                    target(
                        &p.target,
                        "Mouse Click",
                        true,
                        m.id,
                        sid,
                        asset_root,
                        &mut out,
                    );
                    validate_pixel_reference(&p.target, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::ClickWithinRegion(p) => {
                    if p.rect.width == 0 {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_click_region_width",
                            "Click region width must be positive",
                        );
                    }
                    if p.rect.height == 0 {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_click_region_height",
                            "Click region height must be positive",
                        );
                    }
                    if p.rect.width > 0
                        && p.rect.height > 0
                        && let Err(error) = p.rect.validate_capture()
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_click_region",
                            format!("Click region rectangle is invalid: {error}"),
                        );
                    }
                    if p.clicks == 0 {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_click_count",
                            "Click region click count must be at least 1",
                        );
                    }
                    if p.rect.validate_capture().is_ok()
                        && super::executor::usable_region(p.rect, p.edge_padding_px).is_err()
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_click_region_padding",
                            format!(
                                "Edge padding {} leaves no clickable area inside a {}×{} rectangle.",
                                p.edge_padding_px, p.rect.width, p.rect.height
                            ),
                        );
                    }
                }
                MkAction::MouseDrag(p) => {
                    target(
                        &p.from,
                        "Mouse Drag",
                        false,
                        m.id,
                        sid,
                        asset_root,
                        &mut out,
                    );
                    target(&p.to, "Mouse Drag", false, m.id, sid, asset_root, &mut out);
                    validate_pixel_reference(&p.from, &pixel_search_ids, m.id, sid, &mut out);
                    validate_pixel_reference(&p.to, &pixel_search_ids, m.id, sid, &mut out);
                }
                MkAction::PixelCheck {
                    target: t, color, ..
                } => {
                    target(t, "Pixel Check", false, m.id, sid, asset_root, &mut out);
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
                // These actions have no additional payload constraints. Keep
                // this exhaustive so new actions require a validation decision.
                MkAction::KeyDown(_)
                | MkAction::KeyUp(_)
                | MkAction::KeyPress(_)
                | MkAction::Hotkey(_)
                | MkAction::Text(_)
                | MkAction::MouseDown(_)
                | MkAction::MouseUp(_)
                | MkAction::MouseScroll { .. }
                | MkAction::Process(_)
                | MkAction::VirtualDesktop(
                    MkVirtualDesktopAction::Create
                    | MkVirtualDesktopAction::SwitchLeft
                    | MkVirtualDesktopAction::SwitchRight
                    | MkVirtualDesktopAction::CloseCurrent,
                )
                | MkAction::UiInvoke(_)
                | MkAction::UiSetValue { .. }
                | MkAction::UiToggle(_)
                | MkAction::UiSelect(_)
                | MkAction::UiFocus(_)
                | MkAction::UiWait(_) => {}
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
    for owner in &doc.macros {
        super::authoring_analysis::analyze_macro(doc, &graph, owner, &invalid_signatures, &mut out);
    }
    DocumentAnalysis {
        graph,
        diagnostics: out,
    }
}

#[cfg(test)]
mod reusable_tests {
    use super::*;
    use crate::mkmacro::{
        call_graph::{CallGraph, DependencyPolicy},
        compiler::compile_program,
    };

    fn owner(id: u64, actions: Vec<MkAction>) -> MkMacro {
        MkMacro {
            id,
            name: format!("Macro {id}"),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: actions
                .into_iter()
                .enumerate()
                .map(|(index, action)| MkStep {
                    id: index as u64 + 1,
                    enabled: true,
                    breakpoint: false,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    metadata: Default::default(),
                    action,
                })
                .collect(),
        }
    }
    fn call(id: u64) -> MkAction {
        MkAction::CallMacro(MkCallMacroPayload {
            macro_id: id,
            ..Default::default()
        })
    }
    fn document(macros: Vec<MkMacro>) -> MkMacroDocument {
        MkMacroDocument {
            macros,
            ..Default::default()
        }
    }
    fn diagnostics(doc: &MkMacroDocument) -> Vec<MkDiagnostic> {
        analyze_document(doc).diagnostics
    }
    fn parameter(
        id: u64,
        name: &str,
        kind: MkValueType,
        default_value: Option<MkValue>,
    ) -> MkMacroParameter {
        MkMacroParameter {
            id: MkSignatureId(id),
            name: name.into(),
            value_type: kind,
            description: String::new(),
            default_value,
        }
    }
    fn output(id: u64, name: &str, kind: MkValueType) -> MkMacroOutput {
        MkMacroOutput {
            id: MkSignatureId(id),
            name: name.into(),
            value_type: kind,
            description: String::new(),
        }
    }
    fn has(
        ds: &[MkDiagnostic],
        code: &str,
        owner: u64,
        step: Option<u64>,
        severity: DiagnosticSeverity,
    ) -> bool {
        ds.iter().any(|d| {
            d.code == code
                && d.macro_id == owner
                && d.step_id == step
                && d.severity == severity
                && d.scope == DiagnosticScope::Macro
        })
    }

    #[test]
    fn signature_names_ids_defaults_and_binding_identity_are_central() {
        let mut callee = owner(2, vec![]);
        callee.signature.parameters = vec![
            parameter(11, "first", MkValueType::Number, Some(MkValue::Number(1.0))),
            parameter(
                12,
                "second",
                MkValueType::Boolean,
                Some(MkValue::Boolean(true)),
            ),
        ];
        let mut doc = document(vec![owner(1, vec![call(2)]), callee]);
        assert!(can_run(&diagnostics(&doc)));
        if let MkAction::CallMacro(call) = &mut doc.macros[0].steps[0].action {
            call.arguments.push(MkCallArgumentBinding {
                parameter_id: MkSignatureId(11),
                source: MkValueSource::Variable {
                    name: "mouse.x".into(),
                },
            });
        }
        doc.macros[1].signature.parameters.reverse();
        doc.macros[1].signature.parameters[1].name = "renamed".into();
        assert!(can_run(&diagnostics(&doc)));
        assert_eq!(
            compile_program(&doc, 1)
                .unwrap()
                .signature(2)
                .unwrap()
                .parameter(MkSignatureId(11))
                .unwrap()
                .name,
            "renamed"
        );
        doc.macros[1]
            .signature
            .parameters
            .retain(|p| p.id != MkSignatureId(11));
        assert!(has(
            &diagnostics(&doc),
            "dangling_call_parameter",
            1,
            Some(1),
            DiagnosticSeverity::Fatal
        ));
        assert!(
            matches!(&doc.macros[0].steps[0].action, MkAction::CallMacro(call) if call.arguments[0].parameter_id == MkSignatureId(11))
        );
        doc.macros[1].signature.parameters.push(parameter(
            12,
            "second",
            MkValueType::Number,
            Some(MkValue::Null),
        ));
        doc.macros[1]
            .signature
            .outputs
            .push(output(0, "macro.id", MkValueType::String));
        let ds = diagnostics(&doc);
        for code in [
            "invalid_signature_id",
            "duplicate_signature_name",
            "invalid_signature_name",
            "invalid_parameter_default",
        ] {
            assert!(
                has(&ds, code, 2, None, DiagnosticSeverity::Fatal),
                "{code}: {ds:?}"
            );
        }
        assert!(has(
            &ds,
            "invalid_call_signature",
            1,
            Some(1),
            DiagnosticSeverity::Fatal
        ));
        assert!(!ds.iter().any(|d| d.code == "binding_type_mismatch"));
    }

    #[test]
    fn call_reports_every_invalid_binding_with_caller_and_target_context() {
        let mut callee = owner(2, vec![]);
        callee.signature.parameters = vec![
            parameter(10, "required", MkValueType::Number, None),
            parameter(11, "other", MkValueType::Boolean, None),
        ];
        callee.signature.outputs = vec![output(20, "answer", MkValueType::String)];
        let call = MkCallMacroPayload {
            macro_id: 2,
            arguments: vec![
                MkCallArgumentBinding {
                    parameter_id: MkSignatureId(10),
                    source: MkValueSource::Literal(MkValue::String("wrong".into())),
                },
                MkCallArgumentBinding {
                    parameter_id: MkSignatureId(10),
                    source: MkValueSource::Variable {
                        name: String::new(),
                    },
                },
                MkCallArgumentBinding {
                    parameter_id: MkSignatureId(99),
                    source: MkValueSource::Literal(MkValue::Number(1.0)),
                },
            ],
            outputs: vec![
                MkCallOutputBinding {
                    output_id: MkSignatureId(20),
                    caller_variable: "mouse.x".into(),
                },
                MkCallOutputBinding {
                    output_id: MkSignatureId(20),
                    caller_variable: "same".into(),
                },
                MkCallOutputBinding {
                    output_id: MkSignatureId(99),
                    caller_variable: "same".into(),
                },
            ],
        };
        let doc = document(vec![owner(1, vec![MkAction::CallMacro(call)]), callee]);
        let ds = diagnostics(&doc);
        for code in [
            "duplicate_call_argument",
            "dangling_call_parameter",
            "missing_call_argument",
            "binding_type_mismatch",
            "invalid_binding_reference",
            "duplicate_call_output",
            "dangling_call_output",
            "invalid_call_output_variable",
            "duplicate_call_output_variable",
        ] {
            assert!(
                has(&ds, code, 1, Some(1), DiagnosticSeverity::Fatal),
                "{code}: {ds:?}"
            );
            assert!(
                ds.iter()
                    .filter(|d| d.code == code)
                    .all(|d| d.target_macro_id == Some(2))
            );
        }
    }

    #[test]
    fn graph_is_iterative_deterministic_and_distinguishes_authored_edges() {
        let mut doc = document(vec![
            owner(1, vec![call(2)]),
            owner(2, vec![call(3)]),
            owner(3, vec![call(1)]),
        ]);
        let graph = CallGraph::build(&doc);
        assert_eq!(graph.closure(1, DependencyPolicy::EnabledCalls), [1, 2, 3]);
        let ds = diagnostics(&doc);
        let cycle = ds.iter().find(|d| d.code == "call_cycle").unwrap();
        assert_eq!(cycle.cycle_path, [1, 2, 3, 1]);
        assert_eq!(
            (cycle.macro_id, cycle.step_id, cycle.target_macro_id),
            (3, Some(1), Some(1))
        );
        assert!(
            cycle
                .message
                .contains("Macro 1 (#1) -> Macro 2 (#2) -> Macro 3 (#3) -> Macro 1 (#1)")
        );
        doc.macros[1].steps[0].enabled = false;
        let graph = CallGraph::build(&doc);
        assert_eq!(graph.closure(1, DependencyPolicy::EnabledCalls), [1, 2]);
        assert_eq!(
            graph.closure(1, DependencyPolicy::AllAuthoredCalls),
            [1, 2, 3]
        );
        assert!(
            graph
                .cycle_diagnostics(DependencyPolicy::EnabledCalls)
                .is_empty()
        );
        assert_eq!(
            graph
                .cycle_diagnostics(DependencyPolicy::AllAuthoredCalls)
                .len(),
            1
        );
        let direct = document(vec![owner(1, vec![call(1)])]);
        assert_eq!(
            diagnostics(&direct)
                .iter()
                .find(|d| d.code == "call_cycle")
                .unwrap()
                .cycle_path,
            [1, 1]
        );
        // A deep authored chain exercises bounded Rust stack usage.
        let deep = document(
            (1..=4096)
                .map(|id| {
                    owner(
                        id,
                        if id == 4096 {
                            vec![]
                        } else {
                            vec![call(id + 1)]
                        },
                    )
                })
                .collect(),
        );
        let graph = CallGraph::build(&deep);
        assert_eq!(graph.closure(1, DependencyPolicy::EnabledCalls).len(), 4096);
        assert!(
            graph
                .cycle_diagnostics(DependencyPolicy::EnabledCalls)
                .is_empty()
        );
    }

    #[test]
    fn missing_disabled_and_ambiguous_dependencies_are_never_retargeted() {
        let mut doc = document(vec![owner(1, vec![call(2)])]);
        let ds = diagnostics(&doc);
        assert!(has(
            &ds,
            "missing_call_target",
            1,
            Some(1),
            DiagnosticSeverity::Fatal
        ));
        assert_eq!(
            ds.iter()
                .find(|d| d.code == "missing_call_target")
                .unwrap()
                .target_macro_id,
            Some(2)
        );
        doc.macros.push(owner(2, vec![]));
        doc.macros[1].enabled = false;
        assert!(has(
            &diagnostics(&doc),
            "disabled_call_target",
            1,
            Some(1),
            DiagnosticSeverity::Fatal
        ));
        doc.macros[1].enabled = true;
        doc.macros[1].signature.parameters = vec![
            parameter(1, "one", MkValueType::String, None),
            parameter(1, "two", MkValueType::Number, None),
        ];
        doc.macros[0].steps[0].enabled = false;
        assert!(has(
            &diagnostics(&doc),
            "invalid_call_signature",
            1,
            Some(1),
            DiagnosticSeverity::Fatal
        ));
        assert!(compile_program(&doc, 1).is_err());
    }

    #[test]
    fn return_sets_are_complete_typed_and_fallthrough_is_rejected() {
        let mut root = owner(
            1,
            vec![MkAction::Return(MkReturnPayload {
                outputs: vec![
                    MkReturnValueBinding {
                        output_id: MkSignatureId(1),
                        source: MkValueSource::Literal(MkValue::Boolean(true)),
                    },
                    MkReturnValueBinding {
                        output_id: MkSignatureId(1),
                        source: MkValueSource::Literal(MkValue::Number(2.0)),
                    },
                    MkReturnValueBinding {
                        output_id: MkSignatureId(99),
                        source: MkValueSource::Literal(MkValue::Number(2.0)),
                    },
                ],
            })],
        );
        root.signature.outputs = vec![
            output(1, "value", MkValueType::Number),
            output(2, "other", MkValueType::String),
        ];
        let mut doc = document(vec![root]);
        let ds = diagnostics(&doc);
        for code in [
            "duplicate_return_output",
            "dangling_return_output",
            "missing_return_output",
            "binding_type_mismatch",
        ] {
            assert!(
                has(&ds, code, 1, Some(1), DiagnosticSeverity::Fatal),
                "{code}: {ds:?}"
            );
        }
        doc.macros[0].steps[0].action = MkAction::Return(MkReturnPayload {
            outputs: vec![
                MkReturnValueBinding {
                    output_id: MkSignatureId(1),
                    source: MkValueSource::Literal(MkValue::Number(2.0)),
                },
                MkReturnValueBinding {
                    output_id: MkSignatureId(2),
                    source: MkValueSource::Literal(MkValue::String("ok".into())),
                },
            ],
        });
        assert!(can_run(&diagnostics(&doc)));
        doc.macros[0].steps[0].on_error = MkErrorPolicy::Continue;
        assert!(can_run(&diagnostics(&doc)));
        let fallback = doc.macros[0].steps[0].clone();
        if let MkAction::Return(ret) = &mut doc.macros[0].steps[0].action {
            ret.outputs[0].source = MkValueSource::Variable {
                name: "unresolved".into(),
            };
        }
        assert!(has(
            &diagnostics(&doc),
            "output_return_fallthrough",
            1,
            None,
            DiagnosticSeverity::Fatal
        ));
        doc.macros[0].steps.push(MkStep { id: 2, ..fallback });
        assert!(can_run(&diagnostics(&doc)));
        doc.macros[0].steps.pop();
        doc.macros[0].steps[0].on_error = MkErrorPolicy::Stop;
        doc.macros[0].steps[0].enabled = false;
        assert!(has(
            &diagnostics(&doc),
            "output_return_fallthrough",
            1,
            None,
            DiagnosticSeverity::Fatal
        ));
    }

    #[test]
    fn static_reads_follow_runtime_fields_and_warnings_do_not_block() {
        let mut root = owner(
            1,
            vec![
                MkAction::SetVariable {
                    name: "literal".into(),
                    value: MkValue::String("${not_a_read}".into()),
                },
                MkAction::Text(MkTextPayload {
                    text: "$${escaped} ${missing} ${挨拶}".into(),
                    mode: MkTextMode::Type,
                }),
                MkAction::Return(Default::default()),
                MkAction::Text(MkTextPayload {
                    text: "${unreachable_read}".into(),
                    mode: MkTextMode::Type,
                }),
            ],
        );
        root.steps[0].metadata.label = "${label}".into();
        root.steps[1].metadata.label = "${label}".into();
        root.steps[0].metadata.comment = "${comment}".into();
        let ds = diagnostics(&document(vec![root]));
        assert!(can_run(&ds));
        assert!(has(
            &ds,
            "unused_local",
            1,
            Some(1),
            DiagnosticSeverity::Warning
        ));
        assert!(has(
            &ds,
            "duplicate_label",
            1,
            Some(2),
            DiagnosticSeverity::Warning
        ));
        assert!(has(
            &ds,
            "unreachable_step",
            1,
            Some(4),
            DiagnosticSeverity::Warning
        ));
        let reads: Vec<_> = ds
            .iter()
            .filter(|d| d.code == "read_before_definition")
            .collect();
        assert_eq!(reads.len(), 2);
        assert!(reads.iter().all(|d| d.step_id == Some(2)));
        assert!(reads.iter().any(|d| d.message.contains("挨拶")));
    }

    #[test]
    fn branches_loops_and_disabled_openers_keep_conservative_reachability() {
        let condition = MkCondition::All { conditions: vec![] };
        let delay = || MkAction::Delay(Default::default());
        for control in [MkAction::Break, MkAction::Continue] {
            let root = owner(
                1,
                vec![
                    MkAction::RepeatStart { count: 2 },
                    control,
                    delay(),
                    MkAction::RepeatEnd,
                    delay(),
                ],
            );
            let ds = diagnostics(&document(vec![root]));
            assert!(has(
                &ds,
                "unreachable_step",
                1,
                Some(3),
                DiagnosticSeverity::Warning
            ));
            assert!(!has(
                &ds,
                "unreachable_step",
                1,
                Some(5),
                DiagnosticSeverity::Warning
            ));
        }
        let mut root = owner(
            1,
            vec![
                MkAction::If(condition),
                MkAction::Return(Default::default()),
                MkAction::EndIf,
                delay(),
            ],
        );
        assert!(
            !diagnostics(&document(vec![root.clone()]))
                .iter()
                .any(|d| d.code == "unreachable_step")
        );
        root.steps[0].enabled = false;
        assert!(has(
            &diagnostics(&document(vec![root.clone()])),
            "unreachable_step",
            1,
            Some(4),
            DiagnosticSeverity::Warning
        ));
        root.steps[1].enabled = false;
        assert!(
            !diagnostics(&document(vec![root]))
                .iter()
                .any(|d| d.code == "unreachable_step")
        );
    }
}

#[cfg(test)]
mod delay_validation_tests {
    use super::*;

    fn diagnostics(payload: MkDelayPayload) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action: MkAction::Delay(payload),
                    }],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    #[test]
    fn fixed_zero_is_valid() {
        assert!(
            diagnostics(MkDelayPayload {
                fixed_ms: 0,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn fixed_maximum_is_valid() {
        assert!(
            diagnostics(MkDelayPayload {
                fixed_ms: MAX_DELAY_MS,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn fixed_over_maximum_is_rejected() {
        let found = diagnostics(MkDelayPayload {
            fixed_ms: MAX_DELAY_MS + 1,
            ..Default::default()
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "invalid_delay_fixed");
        assert!(found[0].message.contains(&(MAX_DELAY_MS + 1).to_string()));
    }

    #[test]
    fn random_zero_range_is_valid() {
        assert!(
            diagnostics(MkDelayPayload {
                mode: MkDelayMode::RandomRange,
                minimum_ms: 0,
                maximum_ms: 0,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn random_equal_nonzero_endpoints_are_valid() {
        assert!(
            diagnostics(MkDelayPayload {
                mode: MkDelayMode::RandomRange,
                minimum_ms: 25,
                maximum_ms: 25,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn random_ordered_range_is_valid() {
        assert!(
            diagnostics(MkDelayPayload {
                mode: MkDelayMode::RandomRange,
                minimum_ms: 10,
                maximum_ms: 25,
                ..Default::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn random_reversed_endpoints_are_rejected_with_both_values() {
        let minimum_ms = 25;
        let maximum_ms = 10;
        let found = diagnostics(MkDelayPayload {
            mode: MkDelayMode::RandomRange,
            minimum_ms,
            maximum_ms,
            ..Default::default()
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "invalid_delay_range");
        assert!(found[0].message.contains(&minimum_ms.to_string()));
        assert!(found[0].message.contains(&maximum_ms.to_string()));
    }

    #[test]
    fn random_endpoint_over_maximum_is_rejected() {
        let found = diagnostics(MkDelayPayload {
            mode: MkDelayMode::RandomRange,
            minimum_ms: 0,
            maximum_ms: MAX_DELAY_MS + 1,
            ..Default::default()
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "invalid_delay_range_endpoint");
        assert!(found[0].message.contains(&(MAX_DELAY_MS + 1).to_string()));
    }

    #[test]
    fn inactive_fields_are_not_validated() {
        assert!(
            diagnostics(MkDelayPayload {
                mode: MkDelayMode::Fixed,
                fixed_ms: 0,
                minimum_ms: u64::MAX,
                maximum_ms: u64::MAX,
            })
            .is_empty()
        );
        assert!(
            diagnostics(MkDelayPayload {
                mode: MkDelayMode::RandomRange,
                fixed_ms: u64::MAX,
                minimum_ms: 0,
                maximum_ms: 0,
            })
            .is_empty()
        );
    }
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
                signature: Default::default(),
                id: 1,
                name: "visual wait".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: MkPlayback::default(),
                steps: vec![MkStep {
                    metadata: Default::default(),
                    id: 1,
                    enabled: true,
                    breakpoint: false,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: MkErrorPolicy::default(),
                    action: MkAction::WaitForVisualChange(payload),
                }],
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatcherValidationError {
    pub code: &'static str,
    pub message: String,
}

/// Checks matcher configuration without a window snapshot or GUI dependency.
/// Whitespace-only fields count as absent; nonempty criteria are left unchanged
/// so process paths, title substrings, regexes, and class names keep their semantics.
pub(crate) fn validate_window_matcher(x: &MkWindowMatcher) -> Result<(), MatcherValidationError> {
    let usable = |value: &Option<String>| value.as_ref().is_some_and(|v| !v.trim().is_empty());
    if !usable(&x.title) && !usable(&x.title_regex) && !usable(&x.process) && !usable(&x.class) {
        return Err(MatcherValidationError {
            code: "empty_window_matcher",
            message: "Enter at least one non-whitespace window criterion: process, title, title regex, or class.".into(),
        });
    }
    if let Some(r) = x.title_regex.as_ref().filter(|r| !r.trim().is_empty())
        && let Err(reason) = Regex::new(r)
    {
        return Err(MatcherValidationError {
            code: "invalid_regex",
            message: format!("Window title regex is invalid: {reason}"),
        });
    }
    Ok(())
}

fn matcher(x: &MkWindowMatcher, m: u64, s: Option<u64>, o: &mut Vec<MkDiagnostic>) {
    if let Err(error) = validate_window_matcher(x) {
        push(o, m, s, error.code, error.message);
    }
}
fn asset(
    image: &MkImageRef,
    m: u64,
    s: Option<u64>,
    root: Option<&Path>,
    o: &mut Vec<MkDiagnostic>,
) {
    let name = image.filename();
    if image.is_empty() {
        push(
            o,
            m,
            s,
            "reference_image_empty",
            "No reference image is selected",
        )
    } else if !image.is_valid_filename() {
        push(
            o,
            m,
            s,
            "reference_image_invalid_filename",
            format!("Reference image '{name}' has an invalid direct-child PNG filename"),
        )
    } else if let Some(root) = root {
        let path = root.join(name);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                push(
                    o,
                    m,
                    s,
                    "reference_image_missing",
                    format!("Reference image '{name}' is missing"),
                );
                return;
            }
            Err(_) => {
                push(
                    o,
                    m,
                    s,
                    "reference_image_unreadable",
                    format!("Reference image '{name}' could not be read"),
                );
                return;
            }
        };
        let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let safe = !metadata.file_type().is_symlink()
            && path
                .canonicalize()
                .ok()
                .is_some_and(|canonical| canonical.parent() == Some(root_canonical.as_path()));
        if !safe {
            push(
                o,
                m,
                s,
                "reference_image_invalid_filename",
                format!("Reference image '{name}' is not a safe direct-child file"),
            );
            return;
        };
        if !metadata.is_file() {
            push(
                o,
                m,
                s,
                "reference_image_unreadable",
                format!("Reference image '{name}' is not a regular file"),
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
                    format!("Reference image '{name}' could not be read"),
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
                    format!("Reference image '{name}' is not valid PNG data"),
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
                format!("Reference image '{name}' has invalid dimensions"),
            );
        }
    }
}
fn usable_image_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

fn target(
    target: &MkCoordinateTarget,
    consumer: &'static str,
    allow_current_position: bool,
    m: u64,
    s: Option<u64>,
    root: Option<&Path>,
    out: &mut Vec<MkDiagnostic>,
) {
    match target {
        MkCoordinateTarget::CurrentPosition if !allow_current_position => push(
            out,
            m,
            s,
            "unsupported_current_position",
            format!(
                "{consumer} does not support Current Position; Current Position is valid only for Mouse Click."
            ),
        ),
        MkCoordinateTarget::CurrentPosition => {}
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
        MkCoordinateTarget::Image { image, .. } => asset(image, m, s, root, out),
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
            asset(&search.image, m, s, root, o);
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
            image: Some(image), ..
        } => asset(image, m, s, root, o),
        MkCondition::PreviousImageResult { image: None, .. } => {}
        MkCondition::PixelResult {
            target: coordinate_target,
            ..
        } => {
            target(
                coordinate_target,
                "Pixel Result condition",
                false,
                m,
                s,
                root,
                o,
            );
        }
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            for x in conditions {
                condition(x, m, s, root, o)
            }
        }
        MkCondition::Not { condition: x } => condition(x, m, s, root, o),
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
            "Mouse Move",
            false,
            1,
            Some(2),
            None,
            &mut out,
        );
        out
    }

    fn action_diagnostics(action: MkAction) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 2,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action,
                    }],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    #[test]
    fn current_position_is_valid_only_for_mouse_click() {
        assert!(
            action_diagnostics(MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::CurrentPosition,
                button: MkMouseButton::Left,
                clicks: 1,
            }))
            .is_empty()
        );

        for (consumer, action) in [
            (
                "Mouse Move",
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::CurrentPosition,
                    duration_ms: 0,
                }),
            ),
            (
                "Pixel Check",
                MkAction::PixelCheck {
                    target: MkCoordinateTarget::CurrentPosition,
                    color: "#000000".into(),
                    tolerance: 0,
                },
            ),
            (
                "Pixel Result condition",
                MkAction::If(MkCondition::PixelResult {
                    target: MkCoordinateTarget::CurrentPosition,
                    color: "#000000".into(),
                    tolerance: 0,
                }),
            ),
        ] {
            let found = action_diagnostics(action);
            let diagnostic = found
                .iter()
                .find(|diagnostic| diagnostic.code == "unsupported_current_position")
                .unwrap_or_else(|| panic!("missing Current Position diagnostic for {consumer}"));
            assert_eq!(diagnostic.severity, DiagnosticSeverity::Fatal);
            assert!(diagnostic.message.contains(consumer));
            assert!(
                diagnostic
                    .message
                    .contains("Current Position is valid only for Mouse Click")
            );
            assert!(!can_run(&found));
        }
    }

    #[test]
    fn both_mouse_drag_endpoints_reject_current_position() {
        let found = action_diagnostics(MkAction::MouseDrag(MkMouseDragPayload {
            from: MkCoordinateTarget::CurrentPosition,
            to: MkCoordinateTarget::CurrentPosition,
            button: MkMouseButton::Left,
            duration_ms: 0,
        }));
        assert_eq!(
            found
                .iter()
                .filter(|diagnostic| diagnostic.code == "unsupported_current_position")
                .count(),
            2
        );
        assert!(!can_run(&found));
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
    fn matcher_validation_preserves_action_diagnostics_and_ignores_blank_fields() {
        for matcher in [
            MkWindowMatcher::default(),
            MkWindowMatcher {
                title: Some(" \t\n".into()),
                title_regex: Some("\u{2003}".into()),
                process: Some(String::new()),
                class: Some(" ".into()),
            },
            MkWindowMatcher {
                title_regex: Some("[".into()),
                ..Default::default()
            },
        ] {
            let expected = validate_window_matcher(&matcher).unwrap_err();
            let actual = diagnostics(matcher);
            assert_eq!(actual.len(), 1);
            assert_eq!(actual[0].severity, DiagnosticSeverity::Fatal);
            assert_eq!(actual[0].macro_id, 1);
            assert_eq!(actual[0].step_id, Some(2));
            assert_eq!(actual[0].code, expected.code);
            assert_eq!(actual[0].message, expected.message);
        }
        for field in ["process", "title", "title_regex", "class"] {
            let mut matcher = MkWindowMatcher {
                title: Some(" \t".into()),
                title_regex: Some("\n".into()),
                process: Some("\u{2003}".into()),
                class: Some(String::new()),
            };
            let slot = match field {
                "process" => &mut matcher.process,
                "title" => &mut matcher.title,
                "title_regex" => &mut matcher.title_regex,
                "class" => &mut matcher.class,
                _ => unreachable!(),
            };
            *slot = Some("Editor".into());
            let before = matcher.clone();
            assert!(validate_window_matcher(&matcher).is_ok(), "{field}");
            assert_eq!(matcher, before);
            assert!(diagnostics(matcher).is_empty(), "{field}");
        }
    }

    #[test]
    fn managed_image_assets_are_validated_as_png_content() {
        use image::{Rgba, RgbaImage};
        let dir = tempfile::tempdir().unwrap();
        let root = &dir.path().join(crate::mkmacro::store::ASSET_DIRECTORY);
        std::fs::create_dir(root).unwrap();
        let mut out = Vec::new();
        asset(&MkImageRef::default(), 7, Some(8), Some(root), &mut out);
        assert!(out.iter().any(|d| d.code == "reference_image_empty"));
        out.clear();
        asset(
            &MkImageRef::from_filename("login.png"),
            7,
            Some(8),
            Some(root),
            &mut out,
        );
        assert!(out.iter().any(|d| d.code == "reference_image_missing"));

        let image_path = root.join("login.png");
        RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255]))
            .save_with_format(&image_path, image::ImageFormat::Png)
            .unwrap();
        out.clear();
        asset(
            &MkImageRef::from_filename("login.png"),
            7,
            Some(8),
            Some(root),
            &mut out,
        );
        assert!(out.is_empty());

        for bytes in [
            b"corrupt".as_slice(),
            b"\xff\xd8\xff\xe0renamed jpeg".as_slice(),
        ] {
            std::fs::write(&image_path, bytes).unwrap();
            out.clear();
            asset(
                &MkImageRef::from_filename("login.png"),
                7,
                Some(8),
                Some(root),
                &mut out,
            );
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

    fn validate(image: &str, root: Option<&Path>) -> Vec<MkDiagnostic> {
        let mut out = Vec::new();
        let image = if image.is_empty() {
            MkImageRef::default()
        } else {
            MkImageRef::from_filename(image)
        };
        asset(&image, 7, Some(11), root, &mut out);
        out
    }

    #[test]
    fn unset_absent_corrupt_and_valid_assets_have_distinct_results() {
        assert_eq!(validate("", None)[0].code, "reference_image_empty");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(crate::mkmacro::store::ASSET_DIRECTORY);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(
            validate("login.png", Some(&root))[0].code,
            "reference_image_missing"
        );
        std::fs::write(root.join("login.png"), b"not png").unwrap();
        assert_eq!(
            validate("login.png", Some(&root))[0].code,
            "reference_image_undecodable"
        );
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(1, 1))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        std::fs::write(root.join("login.png"), bytes.into_inner()).unwrap();
        assert!(validate("login.png", Some(&root)).is_empty());
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
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action,
                    }],
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
        let valid = diagnostics(MkAction::Notify(MkNotifyPayload {
            title: "Done ${job}".into(),
            description: "Result $${literal}".into(),
            ..MkNotifyPayload::default()
        }));
        assert!(can_run(&valid));
        assert_eq!(
            valid.iter().map(|d| d.code).collect::<Vec<_>>(),
            ["read_before_definition"]
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
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action: MkAction::LauncherCommand(payload),
                    }],
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
            let found = diagnostics(MkLauncherCommandPayload {
                query: query.into(),
                legacy_resolved_action: None,
            });
            assert!(can_run(&found), "{query:?}: {found:?}");
            assert_eq!(
                found
                    .iter()
                    .filter(|d| d.code == "read_before_definition")
                    .count(),
                usize::from(query.contains("${note_name}"))
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

#[cfg(test)]
mod click_within_region_validation_tests {
    use super::*;
    use crate::mkmacro::ScreenRect;

    fn diagnostics(rect: ScreenRect, clicks: u32, edge_padding_px: u32) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action: MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
                            rect,
                            button: MkMouseButton::Left,
                            clicks,
                            edge_padding_px,
                        }),
                    }],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    fn codes(diagnostics: &[MkDiagnostic]) -> Vec<&'static str> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn zero_padding_regions_are_valid() {
        for rect in [
            ScreenRect::new(0, 0, 100, 80),
            ScreenRect::new(10, 20, 1, 1),
        ] {
            assert!(
                diagnostics(rect, 1, 0).is_empty(),
                "unexpected diagnostics for {rect:?}"
            );
        }
    }

    #[test]
    fn padded_region_is_valid_when_both_usable_dimensions_remain() {
        assert!(diagnostics(ScreenRect::new(0, 0, 100, 80), 1, 10).is_empty());
    }

    #[test]
    fn one_by_one_region_is_valid_at_zero_padding() {
        assert!(diagnostics(ScreenRect::new(0, 0, 1, 1), 1, 0).is_empty());
    }

    #[test]
    fn inclusive_padded_boundary_agrees_with_execution_geometry() {
        for (rect, padding, valid) in [
            (ScreenRect::new(-7, 11, 5, 5), 2, true),
            (ScreenRect::new(i32::MAX, i32::MIN, 1, 1), 0, true),
            (ScreenRect::new(100, 200, 20, 20), 10, false),
            (ScreenRect::new(-100, -50, 21, 21), 11, false),
            (ScreenRect::new(100, 200, 20, 40), 10, false),
            (ScreenRect::new(100, 200, 40, 20), 10, false),
            (ScreenRect::new(100, 200, 20, 20), u32::MAX, false),
        ] {
            let found = diagnostics(rect, 1, padding);
            assert_eq!(
                super::super::executor::usable_region(rect, padding).is_ok(),
                valid
            );
            assert_eq!(
                found.is_empty(),
                valid,
                "{rect:?}, padding {padding}: {found:?}"
            );
            if !valid {
                assert_eq!(codes(&found), ["invalid_click_region_padding"]);
            }
        }
    }

    #[test]
    fn zero_width_has_a_field_specific_diagnostic() {
        let found = diagnostics(ScreenRect::new(10, 20, 0, 80), 1, 0);
        assert!(codes(&found).contains(&"invalid_click_region_width"));
        assert!(
            found
                .iter()
                .any(|diagnostic| diagnostic.message == "Click region width must be positive")
        );
    }

    #[test]
    fn zero_height_has_a_field_specific_diagnostic() {
        let found = diagnostics(ScreenRect::new(10, 20, 100, 0), 1, 0);
        assert!(codes(&found).contains(&"invalid_click_region_height"));
        assert!(
            found
                .iter()
                .any(|diagnostic| diagnostic.message == "Click region height must be positive")
        );
    }

    #[test]
    fn zero_clicks_are_rejected() {
        let found = diagnostics(ScreenRect::new(0, 0, 100, 80), 0, 0);
        assert_eq!(codes(&found), ["invalid_click_count"]);
    }

    #[test]
    fn padding_can_be_invalid_on_only_one_axis() {
        let found = diagnostics(ScreenRect::new(0, 0, 100, 80), 1, 50);
        assert_eq!(codes(&found), ["invalid_click_region_padding"]);
        assert_eq!(
            found[0].message,
            "Edge padding 50 leaves no clickable area inside a 100×80 rectangle."
        );
    }

    #[test]
    fn very_large_padding_is_rejected_without_overflow() {
        let found = diagnostics(ScreenRect::new(0, 0, 100, 80), 1, u32::MAX);
        assert_eq!(codes(&found), ["invalid_click_region_padding"]);
        assert_eq!(
            found[0].message,
            format!(
                "Edge padding {} leaves no clickable area inside a 100×80 rectangle.",
                u32::MAX
            )
        );
    }

    #[test]
    fn negative_coordinates_do_not_make_a_valid_region_invalid() {
        assert!(diagnostics(ScreenRect::new(-100, -50, 20, 20), 1, 5).is_empty());
    }
}

#[cfg(test)]
mod virtual_desktop_validation_tests {
    use super::*;

    fn diagnostics(desktop: u32) -> Vec<MkDiagnostic> {
        validate_document(
            &MkMacroDocument {
                macros: vec![MkMacro {
                    signature: Default::default(),
                    id: 1,
                    name: "test".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: MkPlayback::default(),
                    steps: vec![MkStep {
                        metadata: Default::default(),
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: MkErrorPolicy::default(),
                        action: MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop }),
                    }],
                }],
                ..MkMacroDocument::default()
            },
            None,
        )
    }

    #[test]
    fn persisted_zero_virtual_desktop_number_is_rejected() {
        let found = diagnostics(0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "invalid_virtual_desktop_number");
        assert_eq!(
            found[0].message,
            "Virtual desktop number must be at least 1"
        );
    }

    #[test]
    fn one_based_virtual_desktop_number_is_valid() {
        assert!(diagnostics(1).is_empty());
        assert!(diagnostics(3).is_empty());
    }
}
