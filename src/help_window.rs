use crate::actions::Action;
use crate::gui::LauncherApp;
use eframe::egui;
use std::collections::HashSet;

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
    pub show_examples: bool,
    pub overlay_open: bool,
    pub filter: String,
    #[cfg(test)]
    test_scrolls: Vec<HelpScrollObservation>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct HelpScrollObservation {
    id: egui::Id,
    window_rect: Option<egui::Rect>,
    inner_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
    filter_id: egui::Id,
    filter_label_id: egui::Id,
    filter_rect: egui::Rect,
    examples: Option<(egui::Id, egui::Rect)>,
    content_targets: [Option<egui::Rect>; 3],
}

impl HelpWindow {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        #[cfg(test)]
        self.test_scrolls.clear();
        if self.overlay_open {
            let mut open = self.overlay_open;
            let window_output = egui::Window::new("Quick Help")
                .open(&mut open)
                .resizable(true)
                .max_width(ctx.content_rect().width().max(1.0))
                .max_height(ctx.content_rect().height().max(1.0))
                .default_size((300.0, 200.0))
                .collapsible(false)
                .show(ctx, |ui| {
                    let (filter_label_response, filter_response) = ui
                        .horizontal(|ui| {
                            let label = ui.label("Filter:");
                            let filter = ui
                                .text_edit_singleline(&mut self.filter)
                                .labelled_by(label.id);
                            (label, filter)
                        })
                        .inner;
                    let mut infos = app.plugins.plugin_infos();
                    infos.sort_by(|a, b| a.0.cmp(&b.0));
                    let filt = self.filter.to_lowercase();
                    #[cfg(test)]
                    let mut content_targets = [None; 3];
                    let body = egui::ScrollArea::vertical()
                        .id_salt("quick_help_content")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(ui.available_height().max(0.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Hotkeys").strong());
                            if let Some(hk) = &app.hotkey_str {
                                ui.label(format!("Toggle launcher: {hk}"));
                            }
                            if let Some(hk) = &app.quit_hotkey_str {
                                ui.label(format!("Quit launcher: {hk}"));
                            }
                            if let Some(hk) = &app.help_hotkey_str {
                                ui.label(format!("Help overlay: {hk}"));
                            }
                            ui.label("Older query: Ctrl+Up");
                            ui.label("Newer query / restore draft: Ctrl+Down");
                            ui.separator();
                            ui.label(egui::RichText::new("Dashboard").strong());
                            let dashboard_guidance = ui.add(
                                egui::Label::new(
                                    "Open Settings → Dashboard → Customize Dashboard... to edit \
                                     widget layout plus plugin-aware settings such as note/todo queries, \
                                     context links, or weather location.",
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                            #[cfg(test)]
                            {
                                content_targets[0] = Some(dashboard_guidance.rect);
                            }
                            #[cfg(not(test))]
                            let _ = dashboard_guidance;
                            ui.separator();
                            ui.label(egui::RichText::new("Linked context").strong());
                            ui.monospace("todo add Draft release @note:release-plan");
                            ui.monospace("note add Release Notes @todo:todo-123");
                            ui.monospace("cal add tomorrow 09:00 Kickoff @todo:todo-123");
                            ui.label("Use note/todo right-click menus to link existing items.");
                            ui.separator();
                            ui.label(egui::RichText::new("Commands").strong());
                            for (name, desc, _) in &infos {
                                if !filt.is_empty()
                                    && !name.to_lowercase().contains(&filt)
                                    && !desc.to_lowercase().contains(&filt)
                                {
                                    continue;
                                }
                                let description = ui.add(
                                    egui::Label::new(format!("{name}: {desc}"))
                                        .wrap_mode(egui::TextWrapMode::Wrap),
                                );
                                #[cfg(test)]
                                if name == "Help fixture plugin 11" {
                                    content_targets[1] = Some(description.rect);
                                }
                                #[cfg(not(test))]
                                let _ = description;
                            }
                        });
                    #[cfg(test)]
                    self.test_scrolls.push(HelpScrollObservation {
                        id: body.id,
                        window_rect: None,
                        inner_rect: body.inner_rect,
                        content_size: body.content_size,
                        offset: body.state.offset,
                        filter_id: filter_response.id,
                        filter_label_id: filter_label_response.id,
                        filter_rect: filter_response.rect,
                        examples: None,
                        content_targets,
                    });
                });
            #[cfg(test)]
            if let (Some(window), Some(observation)) = (window_output, self.test_scrolls.last_mut())
            {
                observation.window_rect = Some(window.response.rect);
            }
            #[cfg(not(test))]
            let _ = window_output;
            self.overlay_open = open;
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        let window_output = egui::Window::new("Command Help")
            .open(&mut open)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .default_size((400.0, 300.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| ui.heading("Available commands"));
                ui.separator();
                let mut show_examples = self.show_examples;
                let examples_response = ui.checkbox(&mut show_examples, "Show examples");
                if examples_response.changed() {
                    match crate::settings::Settings::update(&app.settings_path, |settings| {
                        settings.show_examples = show_examples;
                        Ok(())
                    }) {
                        Ok(_) => self.show_examples = show_examples,
                        Err(error) => app.report_error_message(
                            "help.settings.save",
                            format!("Failed to save help settings: {error}"),
                        ),
                    }
                }
                let (filter_label_response, filter_response) = ui
                    .horizontal(|ui| {
                        let label = ui.label("Filter:");
                        let filter = ui
                            .text_edit_singleline(&mut self.filter)
                            .labelled_by(label.id);
                        (label, filter)
                    })
                    .inner;
                ui.separator();
                let mut command_map = std::collections::HashMap::new();
                for plugin in app.plugins.iter() {
                    command_map.insert(plugin.name().to_string(), plugin.commands());
                }
                let mut infos = app.plugins.plugin_infos();
                infos.sort_by(|a, b| a.0.cmp(&b.0));
                let filt = self.filter.to_lowercase();
                #[cfg(test)]
                let mut content_targets = [None; 3];
                let body = egui::ScrollArea::vertical()
                    .id_salt("command_help_content")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Query filters").strong());
                        ui.label("Use filters to narrow results across plugins and widgets:");
                        let tag_filters = ui.monospace("  tag:<label>   !tag:<label>");
                        #[cfg(test)]
                        {
                            content_targets[0] = Some(tag_filters.rect);
                        }
                        #[cfg(not(test))]
                        let _ = tag_filters;
                        ui.monospace("  kind:<kind>   id:<action_id>");
                        ui.monospace("  Quotes for spaces: tag:\"high priority\"");
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Linked note/todo/calendar references").strong(),
                        );
                        ui.label("Use lightweight entity refs in command text:");
                        ui.monospace(
                            "  todo add <text> [p=<priority>] [#tag] @note:<id> @event:<id>",
                        );
                        ui.monospace("  note add <title/text> @todo:<id>");
                        ui.monospace(
                            "  cal add <date> <time|all-day> <title> @todo:<id> @note:<id>",
                        );
                        ui.label("UI linking:");
                        ui.monospace("  Notes dialog → right-click note → Link to todo");
                        ui.monospace("  Todos dialog → right-click todo → Link note");
                        ui.separator();
                        ui.label(egui::RichText::new("Launcher actions").strong());
                        ui.label("Use these action IDs in custom actions, macros, or gestures:");
                        ui.monospace("  launcher:toggle  launcher:show  launcher:hide");
                        ui.monospace("  launcher:focus   launcher:restore");
                        ui.separator();
                        for (name, desc, capabilities) in infos {
                            let commands = command_map.get(&name).cloned().unwrap_or_default();
                            if !matches_filter(&filt, &name, &desc, &capabilities, &commands) {
                                continue;
                            }
                            let prefixes = command_prefixes(&commands);
                            ui.label(egui::RichText::new(&name).strong());
                            ui.add(egui::Label::new(desc).wrap_mode(egui::TextWrapMode::Wrap));
                            if capabilities.is_empty() {
                                ui.label("Capabilities: none");
                            } else {
                                ui.label(format!("Capabilities: {}", capabilities.join(", ")));
                            }
                            if prefixes.is_empty() {
                                ui.label("Prefixes: none");
                            } else {
                                ui.label(format!("Prefixes: {}", prefixes.join(", ")));
                            }
                            if commands.is_empty() {
                                ui.label("Usage: none");
                            } else {
                                ui.label("Usage:");
                                for command in &commands {
                                    let command_label = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("  {}", command.label))
                                                .monospace(),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Wrap),
                                    );
                                    #[cfg(test)]
                                    if name == "Help fixture plugin 11"
                                        && command.label == "fixture:long-command-marker"
                                    {
                                        content_targets[1] = Some(command_label.rect);
                                    }
                                    #[cfg(not(test))]
                                    let _ = command_label;
                                }
                            }
                            if self.show_examples && !commands.is_empty() {
                                ui.label("Examples:");
                                for command in &commands {
                                    let example_label = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!(
                                                "  {}",
                                                example_line(command)
                                            ))
                                            .monospace(),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Wrap),
                                    );
                                    #[cfg(test)]
                                    if name == "Help fixture plugin 11"
                                        && command.label == "fixture:long-command-marker"
                                    {
                                        content_targets[2] = Some(example_label.rect);
                                    }
                                    #[cfg(not(test))]
                                    let _ = example_label;
                                }
                            }
                            ui.separator();
                        }
                    });
                #[cfg(test)]
                self.test_scrolls.push(HelpScrollObservation {
                    id: body.id,
                    window_rect: None,
                    inner_rect: body.inner_rect,
                    content_size: body.content_size,
                    offset: body.state.offset,
                    filter_id: filter_response.id,
                    filter_label_id: filter_label_response.id,
                    filter_rect: filter_response.rect,
                    examples: Some((examples_response.id, examples_response.rect)),
                    content_targets,
                });
            });
        #[cfg(test)]
        if let (Some(window), Some(observation)) = (window_output, self.test_scrolls.last_mut()) {
            observation.window_rect = Some(window.response.rect);
        }
        #[cfg(not(test))]
        let _ = window_output;
        self.open = open;
    }
}

fn matches_filter(
    filter: &str,
    name: &str,
    desc: &str,
    capabilities: &[String],
    commands: &[Action],
) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.to_lowercase();
    if name.to_lowercase().contains(&filter) || desc.to_lowercase().contains(&filter) {
        return true;
    }
    if capabilities
        .iter()
        .any(|cap| cap.to_lowercase().contains(&filter))
    {
        return true;
    }
    commands.iter().any(|command| {
        command.label.to_lowercase().contains(&filter)
            || command.desc.to_lowercase().contains(&filter)
            || command.action.to_lowercase().contains(&filter)
    })
}

fn command_prefixes(commands: &[Action]) -> Vec<String> {
    let mut prefixes = HashSet::new();
    for command in commands {
        if let Some(prefix) = command.label.split_whitespace().next() {
            prefixes.insert(prefix.to_string());
        }
    }
    let mut out: Vec<String> = prefixes.into_iter().collect();
    out.sort();
    out
}

fn example_line(command: &Action) -> String {
    if let Some(query) = command.action.strip_prefix("query:") {
        query.trim().to_string()
    } else {
        command.label.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{HelpScrollObservation, HelpWindow};
    use crate::actions::Action;
    use crate::plugin::{Plugin, PluginManager};
    use crate::settings::Settings;
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};

    struct HelpFixturePlugin {
        index: usize,
        name: String,
    }

    impl Plugin for HelpFixturePlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            Vec::new()
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            // The repeated marker also makes the body taller than the native window.
            "Help fixture description marker. This plugin content remains available in the full scroll body."
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn commands(&self) -> Vec<Action> {
            if self.index == 11 {
                vec![Action {
                    label: "fixture:long-command-marker".to_owned(),
                    desc: "Long command for bounded help content".to_owned(),
                    action: "query:fixture-example-marker".to_owned(),
                    args: None,
                }]
            } else {
                Vec::new()
            }
        }
    }

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        let mut plugins = PluginManager::new_inert_for_test();
        for index in 0..12 {
            plugins.register(Box::new(HelpFixturePlugin {
                index,
                name: format!("Help fixture plugin {index:02}"),
            }));
        }
        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            plugins,
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn input(size: egui::Vec2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            ..Default::default()
        }
    }

    fn collect_text_shapes(
        shape: &egui::epaint::Shape,
        clip_rect: egui::Rect,
        output: &mut Vec<(String, egui::Rect, egui::Rect, bool)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => output.push((
                text.galley.job.text.clone(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                clip_rect,
                text.galley.elided,
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_shapes(shape, clip_rect, output);
                }
            }
            _ => {}
        }
    }

    fn visible_text_shapes(output: &egui::FullOutput, bounds: egui::Rect) -> Vec<String> {
        let mut texts = Vec::new();
        for clipped_shape in &output.shapes {
            collect_text_shapes(&clipped_shape.shape, clipped_shape.clip_rect, &mut texts);
        }
        texts
            .into_iter()
            .filter_map(|(text, galley_rect, clip_rect, elided)| {
                (!elided
                    && bounds.contains_rect(galley_rect)
                    && clip_rect.contains_rect(galley_rect))
                .then_some(text)
            })
            .collect()
    }

    fn access_node(
        output: &egui::FullOutput,
        response_id: egui::Id,
    ) -> Option<(egui::accesskit::Role, Option<String>)> {
        let node_id = egui::accesskit::NodeId(response_id.value());
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find(|(id, _)| *id == node_id)
            .map(|(_, node)| (node.role(), node.label().map(str::to_owned)))
    }

    fn access_filter_relationship(
        output: &egui::FullOutput,
        filter_id: egui::Id,
        label_id: egui::Id,
    ) -> Option<(
        egui::accesskit::Role,
        bool,
        egui::accesskit::Role,
        Option<String>,
    )> {
        let nodes = &output.platform_output.accesskit_update.as_ref()?.nodes;
        let filter_node_id = egui::accesskit::NodeId(filter_id.value());
        let label_node_id = egui::accesskit::NodeId(label_id.value());
        let filter = nodes
            .iter()
            .find_map(|(id, node)| (*id == filter_node_id).then_some(node))?;
        let label = nodes
            .iter()
            .find_map(|(id, node)| (*id == label_node_id).then_some(node))?;
        Some((
            filter.role(),
            filter.labelled_by().contains(&label_node_id),
            label.role(),
            label.value().map(str::to_owned),
        ))
    }

    fn render(
        ctx: &egui::Context,
        help: &mut HelpWindow,
        app: &mut crate::gui::LauncherApp,
        size: egui::Vec2,
    ) -> egui::FullOutput {
        ctx.run_ui(input(size), |ctx| help.ui(ctx, app))
    }

    fn settle_target(
        ctx: &egui::Context,
        help: &mut HelpWindow,
        app: &mut crate::gui::LauncherApp,
        size: egui::Vec2,
        observation: HelpScrollObservation,
        target_index: usize,
    ) -> Option<(HelpScrollObservation, Vec<String>)> {
        let target = observation
            .content_targets
            .get(target_index)
            .copied()
            .flatten()?;
        let mut state = egui::scroll_area::State::load(ctx, observation.id)?;
        let max_offset = (observation.content_size.y - observation.inner_rect.height()).max(0.0);
        let target_content_y = target.top() + observation.offset.y;
        state.offset = egui::vec2(
            0.0,
            (target_content_y - observation.inner_rect.top()).clamp(0.0, max_offset),
        );
        state.store(ctx, observation.id);

        let mut settled = None;
        let mut visible_text = Vec::new();
        for _ in 0..2 {
            let output = render(ctx, help, app, size);
            settled = help.test_scrolls.last().copied();
            visible_text = settled
                .map(|layout| visible_text_shapes(&output, layout.inner_rect))
                .unwrap_or_default();
            output.drop_without_applying_deltas();
        }
        settled.map(|layout| (layout, visible_text))
    }

    #[test]
    fn bounded_help_scroll_keeps_controls_and_full_content_reachable() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = test_app(&ctx, directory.path());
        let mut help = HelpWindow {
            overlay_open: true,
            show_examples: true,
            ..Default::default()
        };

        let quick_size = egui::vec2(300.0, 200.0);
        let mut quick_layout = None;
        let mut quick_filter_relationship = None;
        for _ in 0..2 {
            let output = render(&ctx, &mut help, &mut app, quick_size);
            quick_layout = help.test_scrolls.last().copied();
            quick_filter_relationship = quick_layout.and_then(|layout| {
                access_filter_relationship(&output, layout.filter_id, layout.filter_label_id)
            });
            output.drop_without_applying_deltas();
        }

        let quick_layout = quick_layout.expect("Quick Help reports its native scroll area");
        let quick_window = quick_layout
            .window_rect
            .expect("Quick Help window is rendered");
        assert!(quick_layout.content_size.y > quick_layout.inner_rect.height());
        assert!(quick_window.contains_rect(quick_layout.filter_rect));
        assert!(quick_layout.filter_rect.bottom() <= quick_layout.inner_rect.top());
        assert_eq!(
            quick_filter_relationship,
            Some((
                egui::accesskit::Role::TextInput,
                true,
                egui::accesskit::Role::Label,
                Some("Filter:".to_owned()),
            ))
        );
        assert!(quick_layout.content_targets[0].is_some());
        assert!(quick_layout.content_targets[1].is_some());
        assert!(!help.open && help.overlay_open);

        for (target_index, expected) in [
            (
                0,
                "Open Settings → Dashboard → Customize Dashboard... to edit widget layout plus plugin-aware settings such as note/todo queries, context links, or weather location.",
            ),
            (
                1,
                "Help fixture plugin 11: Help fixture description marker. This plugin content remains available in the full scroll body.",
            ),
        ] {
            let (layout, visible_text) = settle_target(
                &ctx,
                &mut help,
                &mut app,
                quick_size,
                quick_layout,
                target_index,
            )
            .expect("the native help scroll state and target response remain available");
            let target_rect = layout.content_targets[target_index]
                .expect("the target label reports its actual widget bounds");
            let max_offset = (layout.content_size.y - layout.inner_rect.height()).max(0.0);
            assert!(layout.inner_rect.contains_rect(target_rect));
            assert!(layout.offset.y >= -f32::EPSILON);
            assert!(layout.offset.y <= max_offset + f32::EPSILON);
            assert!(
                visible_text.iter().any(|text| text == expected),
                "the complete {expected:?} label is painted inside the settled native viewport"
            );
        }
        assert!(help.overlay_open);

        help.overlay_open = false;
        help.open = true;
        let command_size = egui::vec2(400.0, 220.0);
        let mut command_layout = None;
        let mut filter_relationship = None;
        let mut examples_node = None;
        for _ in 0..2 {
            let output = render(&ctx, &mut help, &mut app, command_size);
            command_layout = help.test_scrolls.last().copied();
            filter_relationship = command_layout.and_then(|layout| {
                access_filter_relationship(&output, layout.filter_id, layout.filter_label_id)
            });
            examples_node = command_layout
                .and_then(|layout| layout.examples.and_then(|(id, _)| access_node(&output, id)));
            output.drop_without_applying_deltas();
        }

        let command_layout = command_layout.expect("Command Help reports its native scroll area");
        let command_window = command_layout
            .window_rect
            .expect("Command Help window is rendered");
        let (examples_id, examples_rect) = command_layout
            .examples
            .expect("Show examples remains outside the scrolling body");
        assert!(command_layout.content_size.y > command_layout.inner_rect.height());
        assert!(command_window.contains_rect(command_layout.filter_rect));
        assert!(command_window.contains_rect(examples_rect));
        assert!(command_layout.filter_rect.bottom() <= command_layout.inner_rect.top());
        assert!(examples_rect.bottom() <= command_layout.inner_rect.top());
        assert_ne!(command_layout.filter_id, examples_id);
        assert_eq!(
            filter_relationship,
            Some((
                egui::accesskit::Role::TextInput,
                true,
                egui::accesskit::Role::Label,
                Some("Filter:".to_owned()),
            ))
        );
        assert_eq!(
            examples_node,
            Some((
                egui::accesskit::Role::CheckBox,
                Some("Show examples".to_owned())
            ))
        );
        assert!(command_layout.content_targets.iter().all(Option::is_some));
        for (target_index, expected) in [
            (0, "  tag:<label>   !tag:<label>"),
            (1, "  fixture:long-command-marker"),
            (2, "  fixture-example-marker"),
        ] {
            let (layout, visible_text) = settle_target(
                &ctx,
                &mut help,
                &mut app,
                command_size,
                command_layout,
                target_index,
            )
            .expect("the native help scroll state and target response remain available");
            let target_rect = layout.content_targets[target_index]
                .expect("the target label reports its actual widget bounds");
            let max_offset = (layout.content_size.y - layout.inner_rect.height()).max(0.0);
            assert!(layout.inner_rect.contains_rect(target_rect));
            assert!(layout.offset.y >= -f32::EPSILON);
            assert!(layout.offset.y <= max_offset + f32::EPSILON);
            assert!(
                visible_text.iter().any(|text| text == expected),
                "the complete {expected:?} label is painted inside the settled native viewport"
            );
        }
        assert!(help.open && !help.overlay_open);
    }
}
