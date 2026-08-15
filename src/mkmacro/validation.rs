use super::SearchRegion;
use super::model::*;
use super::variables::*;
use regex::Regex;
use std::{collections::HashSet, path::Path};
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
pub fn can_run(ds: &[MkDiagnostic]) -> bool {
    !ds.iter().any(|d| d.severity == DiagnosticSeverity::Fatal)
}
pub fn validate_document(doc: &MkMacroDocument, asset_root: Option<&Path>) -> Vec<MkDiagnostic> {
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
                        push(&mut out, m.id, sid, "invalid_variable", e)
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
                MkAction::ImageFind(p) | MkAction::ImageClick(p) => {
                    wait(&p.wait, m.id, sid, &mut out);
                    asset(p.asset_id, m.id, sid, asset_root, &mut out);
                    if let SearchRegion::Rectangle { rect } = &p.region
                        && (rect.width == 0 || rect.height == 0)
                    {
                        push(
                            &mut out,
                            m.id,
                            sid,
                            "invalid_image_region",
                            "Image search rectangle must have positive width and height",
                        )
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
    if w.timeout_ms == 0 || w.poll_interval_ms == 0 || w.poll_interval_ms > w.timeout_ms {
        push(
            o,
            m,
            s,
            "invalid_wait",
            "Timeout and polling interval must be positive, and polling cannot exceed timeout",
        )
    }
}
fn matcher(x: &MkWindowMatcher, m: u64, s: Option<u64>, o: &mut Vec<MkDiagnostic>) {
    if x.title.is_none() && x.title_regex.is_none() && x.process.is_none() && x.class.is_none() {
        push(
            o,
            m,
            s,
            "empty_window_matcher",
            "Window matcher needs at least one criterion",
        )
    };
    if let Some(r) = &x.title_regex
        && Regex::new(r).is_err()
    {
        push(o, m, s, "invalid_regex", "Window title regex is invalid")
    }
}
fn asset(id: u64, m: u64, s: Option<u64>, root: Option<&Path>, o: &mut Vec<MkDiagnostic>) {
    if id == 0 || root.is_some_and(|r| !r.join(m.to_string()).join(format!("{id}.png")).is_file()) {
        push(
            o,
            m,
            s,
            "missing_asset",
            format!("Image asset {id} is missing"),
        )
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
        MkCondition::ImageResult { asset_id, .. } => asset(*asset_id, m, s, root, o),
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            for x in conditions {
                condition(x, m, s, root, o)
            }
        }
        MkCondition::Not { condition: x } => condition(x, m, s, root, o),
        _ => {}
    }
}
