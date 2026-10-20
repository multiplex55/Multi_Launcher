//! Non-interactive Jump Mode hint badges and status feedback.
//!
//! The paint model is rebuilt from the current UI pass registry. A frozen
//! hint assignment is drawn only when its exact target ID is present in that
//! pass, so an old generation can never borrow current geometry by index.

use super::ui::{JumpModeUi, RegisteredTarget, TargetRegistry};
use super::{HintSession, MatchOutcome, TargetId};
use eframe::egui;

const BADGE_FONT_SIZE: f32 = 10.0;
const STATUS_FONT_SIZE: f32 = 10.5;
const BADGE_PADDING: egui::Vec2 = egui::vec2(4.0, 2.0);
const BADGE_MARGIN: f32 = 2.0;
const BADGE_LABEL_GAP: f32 = 2.0;
const STATUS_PADDING: egui::Vec2 = egui::vec2(7.0, 4.0);
const STATUS_MARGIN: f32 = 8.0;

#[derive(Clone, Debug, PartialEq)]
struct BadgeModel {
    hint: String,
    accepted_prefix_chars: usize,
    target_id: TargetId,
    target_label: String,
    target_rect: egui::Rect,
    clipped_rect: egui::Rect,
    ui_clip_rect: egui::Rect,
    paint_label_rect: Option<egui::Rect>,
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
    if let Some(registry) = registry {
        for badge in &model.badges {
            paint_badge(ctx, &style, visuals, badge, registry);
        }
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
                target_id: target.id().clone(),
                target_label: target.metadata().label().to_owned(),
                target_rect: target.rect(),
                clipped_rect: target.clipped_rect(),
                ui_clip_rect: target.ui_clip_rect(),
                paint_label_rect: target.paint_label_rect(),
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

fn paint_badge(
    ctx: &egui::Context,
    style: &egui::Style,
    visuals: &egui::Visuals,
    badge: &BadgeModel,
    registry: &TargetRegistry,
) {
    let painter = ctx
        .layer_painter(badge.layer_id)
        .with_clip_rect(badge.ui_clip_rect);
    let font_id = egui::FontId::monospace(BADGE_FONT_SIZE);
    let target_label = painter.layout_no_wrap(
        badge.target_label.clone(),
        egui::TextStyle::Button.resolve(style),
        visuals.widgets.noninteractive.fg_stroke.color,
    );
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
    let target_id = &badge.target_id;
    let layer_id = badge.layer_id;
    let is_occluded = |candidate: egui::Rect| {
        is_occluded_by_registered_target(registry, target_id, layer_id, candidate)
    };
    let Some(rect) = badge_rect(
        badge.target_rect,
        badge.clipped_rect,
        badge.ui_clip_rect,
        galley.size(),
        target_label.size(),
        badge.paint_label_rect,
        is_occluded,
    ) else {
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

fn badge_rect(
    target_rect: egui::Rect,
    target_clipped_rect: egui::Rect,
    ui_clip_rect: egui::Rect,
    text_size: egui::Vec2,
    target_label_size: egui::Vec2,
    paint_label_rect: Option<egui::Rect>,
    is_occluded: impl Fn(egui::Rect) -> bool,
) -> Option<egui::Rect> {
    let size = text_size + BADGE_PADDING * 2.0;
    if ui_clip_rect.width() < size.x + BADGE_MARGIN * 2.0
        || ui_clip_rect.height() < size.y + BADGE_MARGIN * 2.0
    {
        return None;
    }
    let visible_target = target_rect
        .intersect(target_clipped_rect)
        .intersect(ui_clip_rect);
    if visible_target.width() <= 0.0 || visible_target.height() <= 0.0 {
        return None;
    }

    let label_rect = paint_label_rect
        .unwrap_or_else(|| egui::Rect::from_center_size(target_rect.center(), target_label_size));
    let clears_label =
        |candidate: egui::Rect| !candidate.expand(BADGE_LABEL_GAP).intersects(label_rect);
    let inside = egui::Rect::from_min_size(
        egui::pos2(
            visible_target.right() - BADGE_MARGIN - size.x,
            visible_target.top() + BADGE_MARGIN,
        ),
        size,
    );
    let inside_fits = visible_target.contains_rect(inside);
    if inside_fits && clears_label(inside) && !is_occluded(inside) {
        return Some(inside);
    }

    let candidates = [
        egui::Rect::from_min_size(
            egui::pos2(
                visible_target.right() + BADGE_MARGIN,
                visible_target.top() + BADGE_MARGIN,
            ),
            size,
        ),
        egui::Rect::from_min_size(
            egui::pos2(
                visible_target.left() - BADGE_MARGIN - size.x,
                visible_target.top() + BADGE_MARGIN,
            ),
            size,
        ),
        egui::Rect::from_min_size(
            egui::pos2(
                visible_target.right() - size.x,
                visible_target.top() - BADGE_MARGIN - size.y,
            ),
            size,
        ),
        egui::Rect::from_min_size(
            egui::pos2(
                visible_target.right() - size.x,
                visible_target.bottom() + BADGE_MARGIN,
            ),
            size,
        ),
    ];
    let outside = candidates.into_iter().find(|candidate| {
        ui_clip_rect.contains_rect(*candidate)
            && !candidate.intersects(visible_target)
            && (paint_label_rect.is_none() || clears_label(*candidate))
            && !is_occluded(*candidate)
    });
    outside.or_else(|| {
        (inside_fits && (paint_label_rect.is_none() || clears_label(inside))).then_some(inside)
    })
}

fn is_occluded_by_registered_target(
    registry: &TargetRegistry,
    target_id: &TargetId,
    layer_id: egui::LayerId,
    candidate: egui::Rect,
) -> bool {
    registry.targets().iter().any(|other| {
        other.id() != target_id
            && other.layer_id() == layer_id
            && candidate.intersects(other.clipped_rect())
    })
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

    fn adjacent_controls_fixture() -> (JumpModeUi, egui::Context, TargetId, TargetId) {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = jump
            .scope(
                SurfaceId::Dashboard,
                TargetGeneration(9),
                "adjacent-control-test",
                egui::ViewportId::ROOT,
                true,
            )
            .expect("enabled fixture has a dashboard scope");
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(300.0, 100.0),
            )),
            focused: true,
            ..Default::default()
        };
        ctx.run_ui(raw, |root| {
            egui::CentralPanel::default().show(root, |ui| {
                ui.horizontal(|ui| {
                    for (local_id, label, order) in
                        [("refresh", "Refresh", 0), ("clear", "Clear", 1)]
                    {
                        let response = ui.small_button(label);
                        assert!(matches!(
                            scope.register(
                                ui,
                                &response,
                                local_id,
                                label,
                                TargetKind::Button,
                                order,
                            ),
                            RegistrationResult::Registered { .. }
                        ));
                    }
                });
            });
        })
        .drop_without_applying_deltas();
        jump.finish_ui_pass();
        (
            jump,
            ctx,
            scope.target_id("refresh"),
            scope.target_id("clear"),
        )
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
                && registry.targets().iter().any(|target| {
                    badge.target_rect == target.rect()
                        && badge.clipped_rect == target.clipped_rect()
                        && badge.ui_clip_rect == target.ui_clip_rect()
                })
        }));
        assert!(model.indicator.unwrap().label.contains("PREFIX Z"));
    }

    #[test]
    fn narrow_badges_move_outside_labels_and_remain_inside_parent_clip() {
        let target = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(64.0, 24.0));
        let parent_clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(160.0, 80.0));
        let label = egui::Rect::from_center_size(target.center(), egui::vec2(42.0, 16.0));
        let text_size = egui::vec2(10.0, 12.0);

        let badge = badge_rect(
            target,
            target,
            parent_clip,
            text_size,
            label.size(),
            None,
            |_| false,
        )
        .expect("there is room to place the badge next to the button");
        assert!(parent_clip.contains_rect(badge));
        assert!(badge.min.x >= target.right());
        assert!(!badge.intersects(target));
        assert!(!badge.intersects(label));

        let scroll_clip = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(90.0, 80.0));
        let clipped_badge = badge_rect(
            target,
            target,
            scroll_clip,
            text_size,
            label.size(),
            None,
            |_| false,
        )
        .expect("the badge fits below the target within the parent clip");
        assert!(scroll_clip.contains_rect(clipped_badge));
        assert!(clipped_badge.min.y >= target.bottom());
        assert!(!clipped_badge.intersects(target));
        assert!(!clipped_badge.intersects(label));

        let crowded_parent_clip = target;
        let fallback = badge_rect(
            target,
            target,
            crowded_parent_clip,
            text_size,
            label.size(),
            None,
            |_| true,
        )
        .expect("a compact target retains its badge when all outside placements are occupied");
        assert!(crowded_parent_clip.contains_rect(fallback));
        assert!(fallback.intersects(label));
    }

    #[test]
    fn wide_card_badges_stay_in_the_target_and_clear_its_label() {
        let target = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(360.0, 24.0));
        let parent_clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 60.0));
        let label = egui::Rect::from_center_size(target.center(), egui::vec2(120.0, 16.0));
        let badge = badge_rect(
            target,
            target,
            parent_clip,
            egui::vec2(10.0, 12.0),
            label.size(),
            None,
            |_| false,
        )
        .expect("wide controls retain an inside badge");

        assert!(target.contains_rect(badge));
        assert!(!badge.intersects(label));
    }

    #[test]
    fn compact_settings_arrow_badges_clear_the_actual_adjacent_heading() {
        let target = egui::Rect::from_min_size(egui::pos2(25.0, 100.0), egui::vec2(18.0, 16.0));
        let actual_heading = egui::Rect::from_min_size(
            egui::pos2(target.right() + 8.0, target.top() - 1.0),
            egui::vec2(90.0, 18.0),
        );
        let parent_clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 240.0));

        for hint_width in [6.0, 12.0] {
            let badge = badge_rect(
                target,
                target,
                parent_clip,
                egui::vec2(hint_width, 12.0),
                actual_heading.size(),
                Some(actual_heading),
                |_| false,
            )
            .expect("the compact heading has room for an outside badge");

            assert!(parent_clip.contains_rect(badge));
            assert!(!badge.intersects(target));
            assert!(
                !badge.expand(BADGE_LABEL_GAP).intersects(actual_heading),
                "a {hint_width}-point hint must leave the actual heading readable"
            );
            assert!(badge.max.x <= target.left() - BADGE_MARGIN);
        }
    }

    #[test]
    fn narrow_badge_avoids_adjacent_registered_controls() {
        let (jump, ctx, target_id, neighbor_id) = adjacent_controls_fixture();
        let registry = jump.pass_registry().expect("fixture completed a pass");
        let target = registry.get(&target_id).expect("Refresh is registered");
        let neighbor = registry.get(&neighbor_id).expect("Clear is registered");
        assert_eq!(target.layer_id(), neighbor.layer_id());
        let style = ctx.style_of(ctx.theme());
        let painter = ctx
            .layer_painter(target.layer_id())
            .with_clip_rect(target.ui_clip_rect());
        let label = painter.layout_no_wrap(
            target.metadata().label().to_owned(),
            egui::TextStyle::Button.resolve(style.as_ref()),
            egui::Color32::WHITE,
        );
        let hint = painter.layout_no_wrap(
            "A".to_owned(),
            egui::FontId::monospace(BADGE_FONT_SIZE),
            egui::Color32::WHITE,
        );
        let badge = badge_rect(
            target.rect(),
            target.clipped_rect(),
            target.ui_clip_rect(),
            hint.size(),
            label.size(),
            None,
            |candidate| {
                is_occluded_by_registered_target(
                    registry,
                    target.id(),
                    target.layer_id(),
                    candidate,
                )
            },
        )
        .expect("the badge can move below both adjacent controls");

        assert!(target.ui_clip_rect().contains_rect(badge));
        assert!(!badge.intersects(target.clipped_rect()));
        assert!(!badge.intersects(neighbor.clipped_rect()));
        assert!(badge.min.y >= target.rect().bottom());
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
