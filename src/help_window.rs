use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::launcher::command_help::{command_prefixes, example_line, matches_filter};
use crate::launcher::quick_help::{
    GuideBlock, QUICK_HELP_GUIDE, matches_provider_filter,
};
use eframe::egui;

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

                .resizable(true)
                .max_width(ctx.content_rect().width().max(1.0))
                .max_height(ctx.content_rect().height().max(1.0))
                .default_size((300.0, 200.0))
                .collapsible(false)
                .show_utility(ctx, "quick-help", egui::Id::new("Quick Help"), Some(&mut open), crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::Vec2::from((300.0, 200.0))), egui::Vec2::ZERO, [None, None]).with_max_width(ctx.content_rect().width().max(1.0)).with_max_height(ctx.content_rect().height().max(1.0)), |ui| {
                    let (_filter_label_response, _filter_response) = ui
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
                    #[cfg(test)]
                    let mut content_targets = [None; 3];
                    let _body = egui::ScrollArea::vertical()
                        .id_salt("quick_help_content")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(ui.available_height().max(0.0))
                        .show(ui, |ui| {
                            for block in QUICK_HELP_GUIDE {
                                match *block {
                                    GuideBlock::Heading(text) => {
                                        ui.label(egui::RichText::new(text).strong());
                                    }
                                    GuideBlock::Paragraph(text) => {
                                        let response = ui.label(text);
                                        #[cfg(test)]
                                        if text == "Older query: Ctrl+Up" {
                                            content_targets[0] = Some(response.rect);
                                        }
                                        #[cfg(not(test))]
                                        let _ = response;
                                    }
                                    GuideBlock::WrappedParagraph(text) => {
                                        ui.add(
                                            egui::Label::new(text)
                                                .wrap_mode(egui::TextWrapMode::Wrap),
                                        );
                                    }
                                    GuideBlock::Monospace(text) => {
                                        ui.monospace(text);
                                    }
                                    GuideBlock::Separator => {
                                        ui.separator();
                                    }
                                    GuideBlock::Hotkeys => {
                                        for (label, hotkey) in [
                                            ("Toggle launcher", app.hotkey_str.as_deref()),
                                            ("Quit launcher", app.quit_hotkey_str.as_deref()),
                                            ("Help overlay", app.help_hotkey_str.as_deref()),
                                        ] {
                                            if let Some(hotkey) = hotkey {
                                                ui.label(format!("{label}: {hotkey}"));
                                            }
                                        }
                                    }
                                    GuideBlock::PasteSequenceSummary => {
                                        ui.label(app.paste_sequence_shortcut_summary());
                                    }
                                    GuideBlock::SequenceStarts => {
                                        if let Some(presentation) = &app.sequence_routes {
                                            for (id, chord) in &presentation.starts {
                                                ui.label(format!("Sequence {} Start: {chord}", id.get()));
                                            }
                                        }
                                    }
                                    GuideBlock::Providers => {
                                        for (name, description, _) in &infos {
                                            if !matches_provider_filter(&self.filter, name, description) {
                                                continue;
                                            }
                                            let provider_row = ui.add(
                                                egui::Label::new(format!("{name}: {description}"))
                                                    .wrap_mode(egui::TextWrapMode::Wrap),
                                            );
                                            #[cfg(test)]
                                            if name == "Help fixture plugin 11" {
                                                content_targets[1] = Some(provider_row.rect);
                                            }
                                            #[cfg(not(test))]
                                            let _ = provider_row;
                                        }
                                    }
                                }
                            }
                        });
                    #[cfg(test)]
                    self.test_scrolls.push(HelpScrollObservation {
                        id: _body.id,
                        window_rect: None,
                        inner_rect: _body.inner_rect,
                        content_size: _body.content_size,
                        offset: _body.state.offset,
                        filter_id: _filter_response.id,
                        filter_label_id: _filter_label_response.id,
                        filter_rect: _filter_response.rect,
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

            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .default_size((400.0, 300.0))
            .show_utility(ctx, "command-help", egui::Id::new("Command Help"), Some(&mut open), crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::Vec2::from((400.0, 300.0))), egui::Vec2::ZERO, [None, None]).with_max_width(ctx.content_rect().width().max(1.0)).with_max_height(ctx.content_rect().height().max(1.0)), |ui| {
                ui.vertical_centered(|ui| ui.heading("Available commands"));
                ui.separator();
                let mut show_examples = self.show_examples;
                let examples_response = ui.checkbox(&mut show_examples, "Show examples");
                if examples_response.changed() {
                    let request = crate::settings::SettingsBooleanSaveRequest::new(
                        crate::settings::SettingsBooleanPreference::ShowExamples,
                        self.show_examples,
                        show_examples,
                    );
                    match crate::settings::Settings::save_boolean_preference_checked(
                        &app.settings_path,
                        request,
                    ) {
                        Ok(acknowledgement)
                            if acknowledgement.preference()
                                == crate::settings::SettingsBooleanPreference::ShowExamples
                                && acknowledgement.value() == show_examples =>
                        {
                            self.show_examples = acknowledgement.value();
                        }
                        Ok(_) => app.report_error_message(
                            "help.settings.save",
                            "Failed to save help settings: the acknowledged value did not match."
                                .to_owned(),
                        ),
                        Err(error) => app.report_error_message(
                            "help.settings.save",
                            format!("Failed to save help settings: {error}"),
                        ),
                    }
                }
                let (_filter_label_response, _filter_response) = ui
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
                let _body = egui::ScrollArea::vertical()
                    .id_salt("command_help_content")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height().max(0.0))
                    .show(ui, |ui| {
                        let mut matched = false;
                        for (name, desc, capabilities) in infos {
                            let commands = command_map.get(&name).cloned().unwrap_or_default();
                            if !matches_filter(&filt, &name, &desc, &capabilities, &commands) {
                                continue;
                            }
                            matched = true;
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
                                    if name == "Help fixture plugin 00"
                                        && command.label == "fixture:early-command-marker"
                                    {
                                        content_targets[0] = Some(command_label.rect);
                                    }
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
                        if !matched && !filt.is_empty() {
                            ui.label("No commands match the filter.");
                        }
                    });
                #[cfg(test)]
                self.test_scrolls.push(HelpScrollObservation {
                    id: _body.id,
                    window_rect: None,
                    inner_rect: _body.inner_rect,
                    content_size: _body.content_size,
                    offset: _body.state.offset,
                    filter_id: _filter_response.id,
                    filter_label_id: _filter_label_response.id,
                    filter_rect: _filter_response.rect,
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
            if self.index == 0 {
                vec![Action {
                    label: "fixture:early-command-marker".to_owned(),
                    desc: "Early command for bounded help content".to_owned(),
                    action: "query:fixture-early".to_owned(),
                    args: None,
                }]
            } else if self.index == 11 {
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
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            plugins,
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
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
    fn command_help_filters_real_plugin_commands_without_static_tutorials() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        app.plugins
            .register(Box::new(crate::plugins::folder_size::FolderSizePlugin));
        app.plugins
            .register(Box::new(crate::plugins::search_replace::SearchReplacePlugin));
        let mut help = HelpWindow {
            open: true,
            show_examples: true,
            ..Default::default()
        };
        let size = egui::vec2(700.0, 650.0);
        for (query, expected, absent) in [
            ("OPEN FOLDER SIZE", "folder_size", "search_replace"),
            ("RePlAcE", "search_replace", "folder_size"),
            ("no such command qzx", "", "folder_size"),
            ("", "Help fixture plugin 00", "Query filters"),
        ] {
            help.filter = query.into();
            let mut texts = Vec::new();
            for _ in 0..2 {
                let output = render(&ctx, &mut help, &mut app, size);
                texts.clear();
                for shape in &output.shapes {
                    collect_text_shapes(&shape.shape, shape.clip_rect, &mut texts);
                }
                output.drop_without_applying_deltas();
            }
            if !expected.is_empty() {
                assert!(
                    texts.iter().any(|(text, ..)| text.contains(expected)),
                    "missing {expected} for {query:?}"
                );
            }
            if query == "no such command qzx" {
                assert!(
                    texts
                        .iter()
                        .any(|(text, ..)| text == "No commands match the filter.")
                );
            }
            assert!(
                !texts.iter().any(|(text, ..)| text.contains(absent)),
                "unexpected {absent} for {query:?}"
            );
            for tutorial in [
                "Query filters",
                "Linked note/todo/calendar references",
                "Search and Replace",
                "Launcher actions",
            ] {
                assert!(!texts.iter().any(|(text, ..)| text == tutorial));
            }
        }
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
                "Older query: Ctrl+Up",
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
            assert!(
                layout.inner_rect.contains_rect(target_rect),
                "quick target {target_index}: target={target_rect:?} viewport={:?} offset={:?} content={:?}",
                layout.inner_rect,
                layout.offset,
                layout.content_size
            );
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
            (0, "  fixture:early-command-marker"),
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
