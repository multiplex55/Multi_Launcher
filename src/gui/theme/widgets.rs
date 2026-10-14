//! Small presentation helpers built from native egui widgets.

use super::{TextRole, resolved_for_current_context, text_style_for_role, to_color32};
use eframe::egui::{self, Response, RichText, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusTone {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonEmphasis {
    Ordinary,
    Primary,
    Destructive,
}

/// Draw a compact heading and, when supplied, one line of secondary context.
pub(crate) fn compact_section_heading(
    ui: &mut Ui,
    title: &str,
    description: Option<&str>,
) -> Response {
    let heading = ui.label(
        RichText::new(title)
            .text_style(text_style_for_role(TextRole::SectionHeading))
            .strong(),
    );
    if let Some(description) = description {
        secondary_help(ui, description);
    }
    heading
}

/// Show supporting copy with the theme's secondary text style.
pub(crate) fn secondary_help(ui: &mut Ui, text: &str) -> Response {
    ui.label(
        RichText::new(text)
            .text_style(text_style_for_role(TextRole::Metadata))
            .weak(),
    )
}

/// Keep the caller's label, control, identity, and response in one native row.
/// Wrapped layout lets the control move below a long label in narrow windows.
pub(crate) fn labeled_form_row(
    ui: &mut Ui,
    label: &str,
    control: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        control(ui)
    })
    .inner
}

/// Render a non-interactive semantic status with both a themed marker and text.
/// The text carries the meaning for assistive technology; color is supplemental.
pub(crate) fn semantic_status(
    ui: &mut Ui,
    tone: StatusTone,
    label: &str,
    details: Option<&str>,
) -> Response {
    let color = status_color(ui, tone);
    let text = match details {
        Some(details) => format!("{label}: {details}"),
        None => label.to_owned(),
    };
    ui.horizontal_wrapped(|ui| {
        let marker_size =
            (ui.text_style_height(&text_style_for_role(TextRole::Metadata)) * 0.48).clamp(5.0, 8.0);
        let (marker, _) =
            ui.allocate_exact_size(egui::vec2(marker_size, marker_size), egui::Sense::hover());
        ui.painter()
            .circle_filled(marker.center(), marker_size * 0.5, color);
        // Leave font selection at egui's default so caller style overrides
        // retain the same precedence as a native Label.
        ui.label(RichText::new(text).color(color))
    })
    .inner
}

/// Add the restrained accent marker to an already-created native button.
/// This keeps the button's ID, label, response, and native visual states intact.
pub(crate) fn with_button_emphasis(
    ui: &Ui,
    response: Response,
    emphasis: ButtonEmphasis,
) -> Response {
    if emphasis == ButtonEmphasis::Ordinary || !response.enabled() {
        return response;
    }

    let Some(theme) = resolved_for_current_context(ui.ctx()) else {
        return response;
    };
    let color = match emphasis {
        ButtonEmphasis::Ordinary => return response,
        ButtonEmphasis::Primary => to_color32(theme.palette.accent),
        ButtonEmphasis::Destructive => to_color32(theme.palette.error),
    };
    let marker = response.rect.shrink2(egui::vec2(3.0, 5.0));
    if marker.is_positive() {
        ui.painter().line_segment(
            [marker.left_top(), marker.left_bottom()],
            egui::Stroke::new(2.0, color),
        );
    }
    response
}

fn status_color(ui: &Ui, tone: StatusTone) -> egui::Color32 {
    let palette = resolved_for_current_context(ui.ctx()).map(|theme| theme.palette);
    match tone {
        StatusTone::Info => ui.visuals().text_color(),
        StatusTone::Success => palette
            .map(|palette| to_color32(palette.success))
            .unwrap_or_else(|| ui.visuals().hyperlink_color),
        StatusTone::Warning => palette
            .map(|palette| to_color32(palette.warning))
            .unwrap_or_else(|| ui.visuals().warn_fg_color),
        StatusTone::Error => palette
            .map(|palette| to_color32(palette.error))
            .unwrap_or_else(|| ui.visuals().error_fg_color),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
        semantic_status, with_button_emphasis,
    };
    use crate::gui::theme::initialize_runtime;
    use crate::settings::{ThemeMode, ThemeSettings};
    use eframe::egui::{self, RawInput};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ShowcaseIds {
        status: egui::Id,
        primary: egui::Id,
        ordinary: egui::Id,
        disabled: egui::Id,
    }

    fn render_showcase(
        ctx: &egui::Context,
        input: RawInput,
        override_font_id: Option<egui::FontId>,
        clicked: &mut bool,
        ids: &mut Option<ShowcaseIds>,
        control_bounds: &mut egui::Rect,
        disabled_enabled: &mut bool,
        expected_status_font: &mut Option<egui::FontId>,
        status_fully_visible: &mut bool,
    ) -> egui::FullOutput {
        let mut hotkey = "Ctrl+Space".to_owned();
        ctx.run_ui(input, |ui| {
            ui.set_max_width(220.0);
            ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
            compact_section_heading(ui, "Keyboard shortcuts", Some("Edit the launch shortcut."));
            let original_font_override = ui.style().override_font_id.clone();
            ui.style_mut().override_font_id = override_font_id.clone();
            *expected_status_font = Some(override_font_id.clone().unwrap_or_else(|| {
                ui.style()
                    .text_styles
                    .get(&egui::TextStyle::Heading)
                    .expect("the heading text style is installed")
                    .clone()
            }));
            let status = semantic_status(ui, StatusTone::Success, "Valid", None);
            *status_fully_visible = ui.clip_rect().contains_rect(status.rect);
            ui.style_mut().override_font_id = original_font_override;
            secondary_help(ui, "Enter a supported shortcut.");
            let control = labeled_form_row(ui, "Launcher hotkey", |ui| {
                ui.add(egui::TextEdit::singleline(&mut hotkey).desired_width(180.0))
            });
            *control_bounds = control.rect;
            let primary_button = ui.button("Save");
            let primary = with_button_emphasis(ui, primary_button, ButtonEmphasis::Primary);
            *clicked = primary.clicked();
            let ordinary_button = ui.button("Cancel");
            let ordinary = with_button_emphasis(ui, ordinary_button, ButtonEmphasis::Ordinary);
            let disabled = ui
                .add_enabled_ui(false, |ui| {
                    let response = ui.button("Unavailable");
                    with_button_emphasis(ui, response, ButtonEmphasis::Primary)
                })
                .inner;
            *disabled_enabled = disabled.enabled();
            *ids = Some(ShowcaseIds {
                status: status.id,
                primary: primary.id,
                ordinary: ordinary.id,
                disabled: disabled.id,
            });
        })
    }

    fn status_font(output: &egui::FullOutput) -> Option<egui::FontId> {
        fn find_status_font(shape: &egui::Shape) -> Option<egui::FontId> {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().find_map(find_status_font),
                egui::Shape::Text(text) if text.galley.job.text == "Valid" => text
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|section| section.format.font_id.clone()),
                _ => None,
            }
        }

        output
            .shapes
            .iter()
            .find_map(|clipped| find_status_font(&clipped.shape))
    }

    fn button_center(output: &egui::FullOutput, id: egui::Id) -> Option<egui::Pos2> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find(|(node_id, node)| {
                *node_id == id.accesskit_id() && node.role() == egui::accesskit::Role::Button
            })
            .and_then(|(_, node)| node.bounds())
            .map(|bounds| {
                egui::pos2(
                    ((bounds.x0 + bounds.x1) * 0.5) as f32,
                    ((bounds.y0 + bounds.y1) * 0.5) as f32,
                )
            })
    }

    #[test]
    fn native_showcase_keeps_accessible_status_ids_and_controls_across_palettes() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut initial = ThemeSettings::default();
        initial.mode = ThemeMode::Dark;
        initialize_runtime(&ctx, &initial);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(260.0, 640.0));
        let input = || RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        let mut clicked = false;
        let mut ids = None;
        let mut control_bounds = egui::Rect::NOTHING;
        let mut disabled_enabled = true;
        let mut expected_status_font = None;
        let mut status_fully_visible = false;
        let output = render_showcase(
            &ctx,
            input(),
            None,
            &mut clicked,
            &mut ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut expected_status_font,
            &mut status_fully_visible,
        );
        let first_ids = ids;
        let first_status_font = status_font(&output);
        let first_status_fully_visible = status_fully_visible;
        let control_is_visible =
            control_bounds.is_positive() && screen_rect.contains_rect(control_bounds);
        let disabled_is_disabled = !disabled_enabled;
        let accesskit_update = output.platform_output.accesskit_update.as_ref();
        let status_is_accessible = accesskit_update.is_some_and(|update| {
            update.nodes.iter().any(|(_, node)| {
                node.role() == egui::accesskit::Role::Label && node.value() == Some("Valid")
            })
        });
        let primary_is_accessible = accesskit_update.is_some_and(|update| {
            first_ids.is_some_and(|ids| {
                update.nodes.iter().any(|(node_id, node)| {
                    *node_id == ids.primary.accesskit_id()
                        && node.role() == egui::accesskit::Role::Button
                })
            })
        });
        let button_count = accesskit_update.map_or(0, |update| {
            update
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
                .count()
        });
        output.drop_without_applying_deltas();

        assert_eq!(
            first_status_font, expected_status_font,
            "a default semantic label honors the caller's Heading override"
        );
        assert!(control_is_visible, "the constrained form remains usable");
        assert!(
            disabled_is_disabled,
            "the native disabled state is preserved"
        );
        assert!(
            first_status_fully_visible,
            "the semantic status response remains inside the clipped viewport"
        );
        assert!(status_is_accessible, "the semantic status exposes its text");
        assert!(
            primary_is_accessible,
            "Save keeps its native accessible identity"
        );
        assert_eq!(
            button_count, 3,
            "the non-interactive status marker adds no focusable control"
        );
        let first_ids = first_ids.expect("the showcase records native widget identities");

        let mut light = ThemeSettings::default();
        light.mode = ThemeMode::Light;
        assert!(crate::gui::theme::publish_committed(&ctx, &light));
        let mut second_ids = None;
        let mut expected_status_font = None;
        let mut status_fully_visible = false;
        let mut output = render_showcase(
            &ctx,
            input(),
            None,
            &mut clicked,
            &mut second_ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut expected_status_font,
            &mut status_fully_visible,
        );
        let second_status_font = status_font(&output);
        let second_status_fully_visible = status_fully_visible;
        let second_disabled_is_disabled = !disabled_enabled;
        let second_ids = second_ids;
        output.drop_without_applying_deltas();
        assert_eq!(
            second_ids,
            Some(first_ids),
            "palette changes do not change widget IDs"
        );
        assert_eq!(
            second_status_font, expected_status_font,
            "palette changes preserve the caller's Heading font override"
        );
        assert!(second_status_fully_visible);
        assert!(second_disabled_is_disabled);

        let explicit_font = egui::FontId::new(31.0, egui::FontFamily::Monospace);
        let mut font_override_ids = None;
        let mut expected_status_font = None;
        let mut status_fully_visible = false;
        let output = render_showcase(
            &ctx,
            input(),
            Some(explicit_font.clone()),
            &mut clicked,
            &mut font_override_ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut expected_status_font,
            &mut status_fully_visible,
        );
        let explicit_status_font = status_font(&output);
        let explicit_expected_status_font = expected_status_font.clone();
        let explicit_status_fully_visible = status_fully_visible;
        let font_override_ids = font_override_ids;
        output.drop_without_applying_deltas();
        assert_eq!(
            explicit_status_font.as_ref(),
            Some(&explicit_font),
            "override_font_id takes precedence over the Heading text-style override"
        );
        assert_eq!(
            explicit_status_font, explicit_expected_status_font,
            "the emitted galley uses the resolved override font"
        );
        assert!(
            explicit_status_fully_visible,
            "the explicit-font status response remains inside the clipped viewport"
        );
        assert_eq!(font_override_ids, Some(first_ids));

        let mut click_setup_ids = None;
        let mut click_setup_expected_font = None;
        let mut click_setup_status_fully_visible = false;
        let output = render_showcase(
            &ctx,
            input(),
            None,
            &mut clicked,
            &mut click_setup_ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut click_setup_expected_font,
            &mut click_setup_status_fully_visible,
        );
        let click_setup_status_font = status_font(&output);
        let save_point = button_center(&output, first_ids.primary);
        let click_setup_ids = click_setup_ids;
        output.drop_without_applying_deltas();
        assert_eq!(click_setup_ids, Some(first_ids));
        assert_eq!(click_setup_status_font, click_setup_expected_font);
        assert!(click_setup_status_fully_visible);
        let save_point = save_point.expect("Save retains its native button identity");
        assert!(screen_rect.contains(save_point));

        let pointer_input = |time, pressed| RawInput {
            screen_rect: Some(screen_rect),
            time: Some(time),
            events: vec![
                egui::Event::PointerMoved(save_point),
                egui::Event::PointerButton {
                    pos: save_point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut ids = None;
        let mut output = render_showcase(
            &ctx,
            pointer_input(1.0, true),
            None,
            &mut clicked,
            &mut ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut expected_status_font,
            &mut status_fully_visible,
        );
        output.drop_without_applying_deltas();
        let mut output = render_showcase(
            &ctx,
            pointer_input(1.02, false),
            None,
            &mut clicked,
            &mut ids,
            &mut control_bounds,
            &mut disabled_enabled,
            &mut expected_status_font,
            &mut status_fully_visible,
        );
        let primary_was_clicked = clicked;
        let release_disabled_is_disabled = !disabled_enabled;
        let release_ids = ids;
        output.drop_without_applying_deltas();
        assert_eq!(release_ids, Some(first_ids));
        assert!(
            primary_was_clicked,
            "the decorated primary control remains clickable"
        );
        assert!(release_disabled_is_disabled);
    }
}
