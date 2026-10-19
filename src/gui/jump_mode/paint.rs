//! Non-interactive Jump Mode hint badges and status feedback.
//!
//! The paint model is rebuilt from the current UI pass registry. A frozen
//! hint assignment is drawn only when its exact target ID is present in that
//! pass, so an old generation can never borrow current geometry by index.

use super::ui::{JumpModeUi, RegisteredTarget, TargetRegistry};
use super::{HintSession, MatchOutcome};
use eframe::egui;

const BADGE_FONT_SIZE: f32 = 10.0;
const STATUS_FONT_SIZE: f32 = 10.5;
const BADGE_PADDING: egui::Vec2 = egui::vec2(4.0, 2.0);
const BADGE_MARGIN: f32 = 2.0;
const STATUS_PADDING: egui::Vec2 = egui::vec2(7.0, 4.0);
const STATUS_MARGIN: f32 = 8.0;

#[derive(Clone, Debug, PartialEq)]
struct BadgeModel {
    hint: String,
    accepted_prefix_chars: usize,
    clipped_rect: egui::Rect,
    layer_id: egui::LayerId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndicatorModel {
    label: String,
    warning: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct FocusOutlineModel {
    target_rect: egui::Rect,
    clipped_rect: egui::Rect,
    layer_id: egui::LayerId,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct OverlayPaintModel {
    badges: Vec<BadgeModel>,
    indicator: Option<IndicatorModel>,
    focus_outline: Option<FocusOutlineModel>,
}

/// Paint current-pass badges and status using egui points and the registered
/// target layers. This function only adds shapes; it does not allocate input
/// handlers or request additional repaints.
pub(crate) fn paint(ctx: &egui::Context, jump_mode: &JumpModeUi) {
    if !jump_mode.is_enabled() {
        return;
    }

    let registry = jump_mode.pass_registry();
    let focused_response_id = ctx.memory(|memory| memory.focused());
    let model = build_overlay_model(
        true,
        registry,
        jump_mode.active_session(),
        jump_mode.last_outcome(),
        focused_response_id,
    );

    let style = ctx.style_of(ctx.theme());
    let visuals = &style.visuals;
    if let Some(outline) = &model.focus_outline {
        paint_focus_outline(ctx, visuals, outline);
    }
    for badge in &model.badges {
        paint_badge(ctx, visuals, badge);
    }
    if let Some(indicator) = &model.indicator {
        paint_indicator(ctx, visuals, registry, indicator);
    }
}

fn build_overlay_model(
    enabled: bool,
    registry: Option<&TargetRegistry>,
    session: Option<&HintSession>,
    last_outcome: Option<&MatchOutcome>,
    focused_response_id: Option<egui::Id>,
) -> OverlayPaintModel {
    if !enabled {
        return OverlayPaintModel::default();
    }

    let mut model = OverlayPaintModel::default();
    if let (Some(registry), Some(session)) = (registry, session) {
        for assignment in session.matching_assignments() {
            let id = assignment.target().id();
            let Some(target) = registry.get(id).filter(|target| is_paintable(target)) else {
                continue;
            };
            model.badges.push(BadgeModel {
                hint: assignment.hint().as_str().to_owned(),
                accepted_prefix_chars: session.prefix().len(),
                clipped_rect: target.clipped_rect(),
                layer_id: target.layer_id(),
            });
        }
    }

    model.indicator = indicator_for(session, last_outcome, model.badges.len());
    if let (Some(registry), Some(MatchOutcome::Completed(target_id)), Some(focused_response_id)) =
        (registry, last_outcome, focused_response_id)
        && let Some(target) = registry
            .get(target_id)
            .filter(|target| is_paintable(target) && target.response_id() == focused_response_id)
    {
        model.focus_outline = Some(FocusOutlineModel {
            target_rect: target.rect(),
            clipped_rect: target.clipped_rect(),
            layer_id: target.layer_id(),
        });
    }

    model
}

fn is_paintable(target: &RegisteredTarget) -> bool {
    let clip = target.clipped_rect();
    target.enabled()
        && target.eligible()
        && clip.width() > 0.0
        && clip.height() > 0.0
        && clip.width().is_finite()
        && clip.height().is_finite()
}

fn indicator_for(
    session: Option<&HintSession>,
    last_outcome: Option<&MatchOutcome>,
    visible_badges: usize,
) -> Option<IndicatorModel> {
    if let Some(session) = session {
        if session.hint_map().is_empty() {
            return Some(IndicatorModel {
                label: "JUMP MODE | NO TARGETS".to_owned(),
                warning: true,
            });
        }
        let prefix = session.prefix();
        let hint_progress = if prefix.is_empty() {
            "TYPE A-Z".to_owned()
        } else {
            format!("PREFIX {prefix}")
        };
        return Some(match last_outcome {
            Some(MatchOutcome::Invalid) => IndicatorModel {
                label: format!("JUMP MODE | INVALID HINT | {hint_progress} | BACKSPACE / ESC"),
                warning: true,
            },
            Some(MatchOutcome::NoTarget) => IndicatorModel {
                label: "JUMP MODE | TARGET UNAVAILABLE".to_owned(),
                warning: true,
            },
            Some(MatchOutcome::NoTargets) => IndicatorModel {
                label: "JUMP MODE | NO TARGETS".to_owned(),
                warning: true,
            },
            _ if visible_badges == 0 => IndicatorModel {
                label: format!("JUMP MODE | NO CURRENT TARGET | {hint_progress} | ESC CANCEL"),
                warning: true,
            },
            _ => IndicatorModel {
                label: format!(
                    "JUMP MODE | {hint_progress} | TYPE HINT | BACKSPACE ERASES | ESC CANCEL"
                ),
                warning: false,
            },
        });
    }

    match last_outcome {
        Some(MatchOutcome::NoTargets) => Some(IndicatorModel {
            label: "JUMP MODE | NO TARGETS".to_owned(),
            warning: true,
        }),
        Some(MatchOutcome::NoTarget) => Some(IndicatorModel {
            label: "JUMP MODE | TARGET UNAVAILABLE".to_owned(),
            warning: true,
        }),
        _ => None,
    }
}

fn paint_badge(ctx: &egui::Context, visuals: &egui::Visuals, badge: &BadgeModel) {
    let painter = ctx
        .layer_painter(badge.layer_id)
        .with_clip_rect(badge.clipped_rect);
    let font_id = egui::FontId::monospace(BADGE_FONT_SIZE);
    let accepted = badge.accepted_prefix_chars.min(badge.hint.len());
    let mut layout = egui::text::LayoutJob::default();
    if accepted > 0 {
        layout.append(
            &badge.hint[..accepted],
            0.0,
            egui::TextFormat::simple(font_id.clone(), visuals.selection.stroke.color),
        );
    }
    if accepted < badge.hint.len() {
        layout.append(
            &badge.hint[accepted..],
            0.0,
            egui::TextFormat::simple(font_id, visuals.widgets.noninteractive.fg_stroke.color),
        );
    }
    let galley = painter.layout_job(layout);
    let Some(rect) = badge_rect(badge.clipped_rect, galley.size()) else {
        return;
    };
    let active_prefix = accepted > 0;
    let fill = if active_prefix {
        visuals.selection.bg_fill
    } else {
        visuals.extreme_bg_color
    };
    let stroke = if active_prefix {
        visuals.selection.stroke
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };
    painter.rect_filled(rect, egui::CornerRadius::same(3), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(3),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.galley(
        rect.min + (rect.size() - galley.size()) * 0.5,
        galley,
        visuals.widgets.noninteractive.fg_stroke.color,
    );
}

fn badge_rect(clip: egui::Rect, text_size: egui::Vec2) -> Option<egui::Rect> {
    let size = text_size + BADGE_PADDING * 2.0;
    if clip.width() < size.x + BADGE_MARGIN * 2.0 || clip.height() < size.y + BADGE_MARGIN * 2.0 {
        return None;
    }
    Some(egui::Rect::from_min_size(
        egui::pos2(
            clip.right() - BADGE_MARGIN - size.x,
            clip.top() + BADGE_MARGIN,
        ),
        size,
    ))
}

fn paint_focus_outline(ctx: &egui::Context, visuals: &egui::Visuals, outline: &FocusOutlineModel) {
    let painter = ctx
        .layer_painter(outline.layer_id)
        .with_clip_rect(outline.clipped_rect);
    let rect = outline.target_rect.shrink(1.0);
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(1.5, visuals.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
}

fn paint_indicator(
    ctx: &egui::Context,
    visuals: &egui::Visuals,
    registry: Option<&TargetRegistry>,
    indicator: &IndicatorModel,
) {
    let content_rect = ctx.content_rect();
    let layer_id = registry
        .and_then(|registry| registry.targets().first())
        .map(RegisteredTarget::layer_id)
        .unwrap_or_else(|| {
            egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("jump-mode-status-indicator"),
            )
        });
    let painter = ctx.layer_painter(layer_id).with_clip_rect(content_rect);
    let text_color = if indicator.warning {
        visuals.warn_fg_color
    } else {
        visuals.widgets.noninteractive.fg_stroke.color
    };
    let font_id = egui::FontId::monospace(STATUS_FONT_SIZE);
    let galley = painter.layout(indicator.label.clone(), font_id, text_color, f32::INFINITY);
    let size = galley.size() + STATUS_PADDING * 2.0;
    if content_rect.width() < size.x + STATUS_MARGIN * 2.0
        || content_rect.height() < size.y + STATUS_MARGIN * 2.0
    {
        return;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            content_rect.right() - STATUS_MARGIN - size.x,
            content_rect.top() + STATUS_MARGIN,
        ),
        size,
    );
    let stroke = if indicator.warning {
        egui::Stroke::new(1.0, visuals.warn_fg_color)
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };
    painter.rect_filled(rect, egui::CornerRadius::same(4), visuals.panel_fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + STATUS_PADDING, galley, text_color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::jump_mode::ui::RegistrationResult;
    use crate::gui::jump_mode::{
        SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
    };

    fn fixture(target_count: usize) -> (JumpModeUi, egui::Context) {
        let ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::LauncherResults,
                TargetGeneration(7),
                "paint-test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled fixture has a render scope");
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 900.0),
            )),
            focused: true,
            ..Default::default()
        };
        ctx.run_ui(raw, |root| {
            egui::CentralPanel::default().show(root, |ui| {
                for index in 0..target_count {
                    let response = ui.add_sized(
                        egui::vec2(240.0, 24.0),
                        egui::Button::new(format!("Target {index}")),
                    );
                    let old_clip = ui.clip_rect();
                    let visible_clip = egui::Rect::from_min_max(
                        response.rect.min + egui::vec2(12.0, 0.0),
                        response.rect.max - egui::vec2(8.0, 0.0),
                    );
                    ui.set_clip_rect(old_clip.intersect(visible_clip));
                    let result = scope.register(
                        ui,
                        &response,
                        format!("target-{index}"),
                        format!("Target {index}"),
                        TargetKind::Button,
                        index as u64,
                    );
                    assert!(matches!(result, RegistrationResult::Registered { .. }));
                    ui.set_clip_rect(old_clip);
                }
            });
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        (jump, ctx)
    }

    #[test]
    fn badge_models_use_current_clipped_geometry_and_style_the_accepted_prefix() {
        let (jump, _ctx) = fixture(27);
        let registry = jump.pass_registry().expect("fixture completed a pass");
        let map = super::super::HintMap::assign(registry.metadata().cloned())
            .expect("fixture target IDs are unique");
        let mut session = HintSession::new(map);
        assert!(matches!(
            session.push_char('Z'),
            MatchOutcome::Partial {
                matching_targets: 2
            }
        ));

        let model = build_overlay_model(
            true,
            Some(registry),
            Some(&session),
            Some(&MatchOutcome::Partial {
                matching_targets: 2,
            }),
            None,
        );

        assert_eq!(model.badges.len(), 2);
        assert!(model.badges.iter().all(|badge| {
            badge.accepted_prefix_chars == 1
                && badge.hint.starts_with('Z')
                && registry
                    .targets()
                    .iter()
                    .any(|target| badge.clipped_rect == target.clipped_rect())
        }));
        assert!(model.indicator.unwrap().label.contains("PREFIX Z"));
    }

    #[test]
    fn disabled_and_stale_assignments_do_not_produce_badges_or_focus_indications() {
        let (jump, _ctx) = fixture(1);
        let registry = jump.pass_registry().expect("fixture completed a pass");
        let current = &registry.targets()[0];
        let stale_metadata = TargetMetadata::new(
            TargetId::new(
                SurfaceId::LauncherResults,
                SemanticTargetId::RootResult(0),
                TargetGeneration(8),
            ),
            "stale target",
            TargetKind::Button,
            0,
        );
        let stale_map = super::super::HintMap::assign([stale_metadata])
            .expect("one stale fixture target is unique");
        let session = HintSession::new(stale_map);
        let completed_stale =
            MatchOutcome::Completed(session.hint_map().assignments()[0].target().id().clone());

        let disabled = build_overlay_model(
            false,
            Some(registry),
            Some(&session),
            Some(&completed_stale),
            Some(current.response_id()),
        );
        assert!(disabled.badges.is_empty());
        assert!(disabled.indicator.is_none());
        assert!(disabled.focus_outline.is_none());

        let stale = build_overlay_model(
            true,
            Some(registry),
            Some(&session),
            Some(&completed_stale),
            Some(current.response_id()),
        );
        assert!(stale.badges.is_empty());
        assert!(stale.focus_outline.is_none());
    }

    #[test]
    fn focus_outline_requires_the_exact_completed_target_and_focused_response() {
        let (jump, _ctx) = fixture(1);
        let registry = jump.pass_registry().expect("fixture completed a pass");
        let target = &registry.targets()[0];
        let completed = MatchOutcome::Completed(target.id().clone());

        let focused = build_overlay_model(
            true,
            Some(registry),
            None,
            Some(&completed),
            Some(target.response_id()),
        );
        assert!(focused.focus_outline.is_some());

        let unfocused = build_overlay_model(true, Some(registry), None, Some(&completed), None);
        assert!(unfocused.focus_outline.is_none());
    }

    #[test]
    fn status_feedback_covers_invalid_and_empty_sessions() {
        let empty_session = HintSession::new(super::super::HintMap::default());
        let no_targets = build_overlay_model(
            true,
            None,
            Some(&empty_session),
            Some(&MatchOutcome::NoTargets),
            None,
        );
        assert_eq!(
            no_targets.indicator.unwrap().label,
            "JUMP MODE | NO TARGETS"
        );

        let (jump, _ctx) = fixture(27);
        let registry = jump.pass_registry().expect("fixture completed a pass");
        let mut session =
            HintSession::new(super::super::HintMap::assign(registry.metadata().cloned()).unwrap());
        let _ = session.push_char('Z');
        let invalid = build_overlay_model(
            true,
            Some(registry),
            Some(&session),
            Some(&MatchOutcome::Invalid),
            None,
        );
        let indicator = invalid.indicator.unwrap();
        assert!(indicator.warning);
        assert!(indicator.label.contains("INVALID HINT"));
    }
}
