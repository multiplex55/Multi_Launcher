//! Non-persisted confirmation state for folder mutations.
use crate::diff::file_ops::{CopyPlan, DeletePlan, EntryType};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, semantic_status, with_button_emphasis,
};
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PreviewPlan {
    Copy(CopyPlan),
    Delete(DeletePlan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OperationPreview {
    pub view_id: u64,
    pub plan: PreviewPlan,
    pub planning_error: Option<String>,
}

impl OperationPreview {
    pub fn show(&mut self, ctx: &egui::Context, operation_active: bool) -> PreviewResponse {
        let mut response = PreviewResponse::Keep;
        #[cfg(test)]
        let mut status_response = None;
        #[cfg(test)]
        let mut body_observation = None;
        #[cfg(test)]
        let mut footer_observation = None;
        let window_output = egui::Window::new("Confirm folder operation")
            .id(egui::Id::new(("folder-operation-preview", self.view_id)))
            .collapsible(false)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .show(ctx, |ui| {
                let footer_height = super::rules_dialog::action_footer_reserved_height(
                    ui,
                    &["Execute", "Cancel"],
                );
                let body_height = (ui.available_height() - footer_height).max(0.0);
                let destructive = matches!(&self.plan, PreviewPlan::Delete(_))
                    || matches!(&self.plan, PreviewPlan::Copy(plan) if plan.totals.overwrites > 0);
                let executable = egui::ScrollArea::vertical()
                    .id_salt("folder_operation_preview_body")
                    .auto_shrink([false, true])
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        if let Some(error) = &self.planning_error {
                            let status = semantic_status(
                                ui,
                                StatusTone::Error,
                                "Blocked",
                                Some(error),
                            );
                            #[cfg(test)]
                            {
                                status_response = Some(status);
                            }
                            #[cfg(not(test))]
                            let _ = status;
                        }
                        match &self.plan {
                            PreviewPlan::Copy(plan) => {
                                ui.heading(match plan.direction {
                                    crate::diff::file_ops::CopyDirection::LeftToRight => "Copy →",
                                    crate::diff::file_ops::CopyDirection::RightToLeft => "← Copy",
                                });
                                ui.label(format!(
                                    "Files: {}  Directories: {}  Overwrites: {}  Skipped: {}  Conflicts: {}  Validation errors: {}",
                                    plan.totals.files_copied, plan.totals.directories_created,
                                    plan.totals.overwrites, plan.totals.skips, plan.totals.conflicts,
                                    plan.totals.errors
                                ));
                                details(ui, "Overwrites", plan.copies.iter().filter(|x| x.overwrite)
                                    .map(|x| (x.relative.display().to_string(), "existing destination will be replaced".to_owned())));
                                details(ui, "Conflicts", plan.conflicts.iter()
                                    .map(|x| (x.relative.display().to_string(), x.message.clone())));
                                details(ui, "Skipped", plan.skipped.iter()
                                    .map(|x| (x.relative.display().to_string(), x.reason.clone())));
                                details(ui, "Validation errors", plan.errors.iter().map(|x| (
                                    x.relative.as_ref().map_or_else(|| "root".into(), |p| p.display().to_string()),
                                    x.message.clone(),
                                )));
                                !plan.has_fatal_errors()
                            }
                            PreviewPlan::Delete(plan) => {
                                ui.heading(format!("Delete {} (Recycle Bin)", plan.side));
                                let files = plan.items.iter().filter(|x| x.expected.kind == EntryType::File).count();
                                let directories = plan.items.len() - files;
                                ui.label(format!("Files: {files}  Directories: {directories}"));
                                details(ui, "Affected paths", plan.items.iter().map(|x| (
                                    x.relative.display().to_string(), "move to Recycle Bin".into(),
                                )));
                                details(ui, "Validation errors", plan.errors.iter().map(|x| (
                                    x.relative.as_ref().map_or_else(|| "root".into(), |p| p.display().to_string()),
                                    x.message.clone(),
                                )));
                                plan.errors.iter().all(|x| !x.fatal)
                            }
                        }
                    });
                #[cfg(test)]
                {
                    body_observation = Some((
                        executable.id,
                        executable.inner_rect,
                        executable.content_size,
                        executable.state.offset,
                    ));
                }
                let executable = executable.inner && self.planning_error.is_none();
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let execute = ui.add_enabled(
                        executable && !operation_active,
                        egui::Button::new("Execute"),
                    );
                    let emphasis = if destructive {
                        ButtonEmphasis::Destructive
                    } else {
                        ButtonEmphasis::Primary
                    };
                    let execute = with_button_emphasis(ui, execute, emphasis);
                    if execute.clicked() {
                        response = PreviewResponse::Execute;
                    }
                    let cancel = ui.button("Cancel");
                    if cancel.clicked() {
                        response = PreviewResponse::Cancel;
                    }
                    #[cfg(test)]
                    {
                        footer_observation = Some((
                            execute.id,
                            execute.rect,
                            execute.enabled(),
                            cancel.id,
                            cancel.rect,
                            cancel.enabled(),
                            footer_height,
                        ));
                    }
                });
            });
        #[cfg(test)]
        if let (Some(window), Some(footer), Some(status)) =
            (window_output, footer_observation, status_response.as_ref())
            && let Some(body) = body_observation
        {
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new(("folder-operation-preview-test", self.view_id)),
                    PreviewChromeObservation {
                        window_rect: window.response.rect,
                        body_id: body.0,
                        body_rect: body.1,
                        content_size: body.2,
                        offset: body.3,
                        status_id: status.id,
                        status_rect: status.rect,
                        execute_id: footer.0,
                        execute_rect: footer.1,
                        execute_enabled: footer.2,
                        cancel_id: footer.3,
                        cancel_rect: footer.4,
                        cancel_enabled: footer.5,
                        footer_height: footer.6,
                    },
                );
            });
        }
        #[cfg(not(test))]
        let _ = window_output;
        response
    }
}

fn details(ui: &mut egui::Ui, title: &str, items: impl Iterator<Item = (String, String)>) {
    let items: Vec<_> = items.collect();
    if !items.is_empty() {
        ui.collapsing(format!("{title} ({})", items.len()), |ui| {
            for (path, explanation) in items {
                ui.add(
                    egui::Label::new(format!("{} — {}", path, explanation))
                        .wrap_mode(egui::TextWrapMode::Wrap),
                );
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewResponse {
    Keep,
    Execute,
    Cancel,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct PreviewChromeObservation {
    window_rect: egui::Rect,
    body_id: egui::Id,
    body_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
    status_id: egui::Id,
    status_rect: egui::Rect,
    execute_id: egui::Id,
    execute_rect: egui::Rect,
    execute_enabled: bool,
    cancel_id: egui::Id,
    cancel_rect: egui::Rect,
    cancel_enabled: bool,
    footer_height: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::file_ops::{CopyDirection, plan_copy};
    use std::path::PathBuf;

    fn input(size: egui::Vec2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            ..Default::default()
        }
    }

    fn full_status_is_visible(
        output: &egui::FullOutput,
        expected: &str,
        viewport: egui::Rect,
    ) -> bool {
        fn collect(
            shape: &egui::epaint::Shape,
            clip_rect: egui::Rect,
            expected: &str,
            viewport: egui::Rect,
        ) -> bool {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    let galley_rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    text.galley.job.text == expected
                        && !text.galley.elided
                        && viewport.intersects(galley_rect)
                        && clip_rect.intersects(galley_rect)
                        && galley_rect.left() >= viewport.left() - 1.0
                        && galley_rect.right() <= viewport.right() + 1.0
                        && galley_rect.left() >= clip_rect.left() - 1.0
                        && galley_rect.right() <= clip_rect.right() + 1.0
                }
                egui::epaint::Shape::Vec(shapes) => shapes
                    .iter()
                    .any(|shape| collect(shape, clip_rect, expected, viewport)),
                _ => false,
            }
        }

        output
            .shapes
            .iter()
            .any(|clipped| collect(&clipped.shape, clipped.clip_rect, expected, viewport))
    }

    fn has_accessible_button(output: &egui::FullOutput, id: egui::Id, label: &str) -> bool {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|update| {
                update.nodes.iter().any(|(node_id, node)| {
                    *node_id == id.accesskit_id()
                        && node.role() == egui::accesskit::Role::Button
                        && node.label() == Some(label)
                })
            })
    }

    fn has_accessible_status(output: &egui::FullOutput, id: egui::Id, label: &str) -> bool {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .is_some_and(|update| {
                update.nodes.iter().any(|(node_id, node)| {
                    *node_id == id.accesskit_id()
                        && node.role() == egui::accesskit::Role::Label
                        && node.label().or(node.value()) == Some(label)
                })
            })
    }

    #[test]
    fn diff_forms_chrome_blocked_long_preview_keeps_actions_reachable() {
        let roots = tempfile::tempdir().unwrap();
        let source = roots.path().join("source");
        let destination = roots.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        let long_relative = (0..14)
            .map(|index| format!("long-path-segment-{index:02}-{}", "x".repeat(28)))
            .collect::<Vec<_>>()
            .join("/");
        let plan = plan_copy(
            &source,
            &destination,
            CopyDirection::LeftToRight,
            [PathBuf::from(format!("../{long_relative}"))],
            17,
        )
        .unwrap();
        let error = format!(
            "Unsaved Diff changes block this operation. Source: {}. Destination: {}. {}",
            source.display(),
            destination.display(),
            (0..24)
                .map(|index| format!("Conflict {index}: {long_relative}."))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let expected_status = format!("Blocked: {error}");
        let mut preview = OperationPreview {
            view_id: 41,
            plan: PreviewPlan::Copy(plan),
            planning_error: Some(error.clone()),
        };
        let original_plan = preview.plan.clone();
        let original_error = preview.planning_error.clone();
        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| style.animation_time = 0.0);
        let size = egui::vec2(640.0, 360.0);
        let observation_id = egui::Id::new(("folder-operation-preview-test", preview.view_id));
        let mut first_layout = None;
        let mut initial_status_visible = false;
        let mut buttons_accessible = false;
        let mut status_accessible = false;
        let mut result = PreviewResponse::Keep;

        for _ in 0..3 {
            let output = context.run_ui(input(size), |ctx| {
                result = preview.show(ctx, false);
            });
            let layout =
                context.data(|data| data.get_temp::<PreviewChromeObservation>(observation_id));
            if let Some(layout) = layout {
                initial_status_visible |=
                    full_status_is_visible(&output, &expected_status, layout.body_rect);
                buttons_accessible |= has_accessible_button(&output, layout.execute_id, "Execute")
                    && has_accessible_button(&output, layout.cancel_id, "Cancel");
                status_accessible |=
                    has_accessible_status(&output, layout.status_id, &expected_status);
                first_layout = Some(layout);
            }
            output.drop_without_applying_deltas();
        }

        let mut scrolled_layout = None;
        let mut scrolled_status_visible = false;
        let mut reached_legal_end = false;
        if let Some(layout) = first_layout
            && let Some(mut state) = egui::scroll_area::State::load(&context, layout.body_id)
        {
            let max_offset = (layout.content_size.y - layout.body_rect.height()).max(0.0);
            state.offset = egui::vec2(0.0, max_offset);
            state.store(&context, layout.body_id);
            for _ in 0..3 {
                let output = context.run_ui(input(size), |ctx| {
                    result = preview.show(ctx, false);
                });
                scrolled_layout =
                    context.data(|data| data.get_temp::<PreviewChromeObservation>(observation_id));
                if let Some(scrolled) = scrolled_layout {
                    let max_offset =
                        (scrolled.content_size.y - scrolled.body_rect.height()).max(0.0);
                    reached_legal_end = scrolled.offset.y >= max_offset - 1.0
                        && scrolled.offset.y <= max_offset + 1.0;
                    scrolled_status_visible |=
                        full_status_is_visible(&output, &expected_status, scrolled.body_rect);
                }
                output.drop_without_applying_deltas();
            }
        }

        let initial_geometry = first_layout.is_some_and(|layout| {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let max_offset = (layout.content_size.y - layout.body_rect.height()).max(0.0);
            screen.contains_rect(layout.window_rect)
                && layout.window_rect.contains_rect(layout.body_rect)
                && layout.window_rect.contains_rect(layout.execute_rect)
                && layout.window_rect.contains_rect(layout.cancel_rect)
                && layout.body_rect.bottom() <= layout.execute_rect.top()
                && layout.execute_rect.bottom() <= layout.cancel_rect.bottom()
                && layout.body_rect.intersects(layout.status_rect)
                && layout.status_rect.left() >= layout.body_rect.left() - 1.0
                && layout.status_rect.right() <= layout.body_rect.right() + 1.0
                && layout.content_size.y > layout.body_rect.height()
                && layout.offset.y >= 0.0
                && layout.offset.y <= max_offset + 1.0
                && layout.footer_height > 0.0
                && !layout.execute_enabled
                && layout.cancel_enabled
        });
        let scrolled_geometry = scrolled_layout.is_some_and(|layout| {
            layout.body_rect.left() <= layout.execute_rect.left()
                && layout.body_rect.right() >= layout.execute_rect.right()
                && layout.body_rect.left() <= layout.cancel_rect.left()
                && layout.body_rect.right() >= layout.cancel_rect.right()
                && first_layout.is_some_and(|first| {
                    layout.status_id == first.status_id
                        && layout.execute_id == first.execute_id
                        && layout.cancel_id == first.cancel_id
                })
        });

        assert_eq!(result, PreviewResponse::Keep);
        assert!(initial_geometry);
        assert!(scrolled_geometry);
        assert!(reached_legal_end);
        assert!(initial_status_visible);
        assert!(scrolled_status_visible);
        assert!(buttons_accessible);
        assert!(status_accessible);
        assert_eq!(preview.plan, original_plan);
        assert_eq!(preview.planning_error, original_error);
        assert!(matches!(
            &preview.plan,
            PreviewPlan::Copy(plan) if plan.has_fatal_errors()
        ));
    }
}
