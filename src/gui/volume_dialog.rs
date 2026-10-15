use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use crate::gui::volume_data::{ProcessVolume, get_process_volumes, get_system_volume};
use crate::launcher::launch_action;
use eframe::egui;

pub struct VolumeDialog {
    pub open: bool,
    value: u8,
    processes: Vec<ProcessVolume>,
    #[cfg(test)]
    last_layout: Option<VolumeDialogLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct VolumeWidgetLayout {
    id: egui::Id,
    rect: egui::Rect,
    clicked: bool,
}

#[cfg(test)]
impl VolumeWidgetLayout {
    fn from_response(response: &egui::Response) -> Self {
        Self {
            id: response.id,
            rect: response.rect,
            clicked: response.clicked(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct VolumeScrollLayout {
    id: egui::Id,
    pre_autoshrink_inner_rect: egui::Rect,
    effective_clip_rect: egui::Rect,
    parent_clip_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
}

#[cfg(test)]
#[derive(Clone, Default)]
struct VolumeDialogLayout {
    window_rect: Option<egui::Rect>,
    visible: bool,
    sizing_pass: bool,
    system_slider: Option<VolumeWidgetLayout>,
    system_set: Option<VolumeWidgetLayout>,
    cancel: Option<VolumeWidgetLayout>,
    process_scroll: Option<VolumeScrollLayout>,
    process_controls: Vec<[Option<VolumeWidgetLayout>; 3]>,
    process_identity_rects: Vec<egui::Rect>,
}

impl Default for VolumeDialog {
    fn default() -> Self {
        Self {
            open: false,
            value: 50,
            processes: Vec::new(),
            #[cfg(test)]
            last_layout: None,
        }
    }
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open_from(|| (get_system_volume(), get_process_volumes()));
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(|| (get_system_volume(), get_process_volumes()));
    }

    fn open_from(&mut self, load_snapshot: impl FnOnce() -> (Option<u8>, Vec<ProcessVolume>)) {
        let (value, processes) = load_snapshot();
        self.value = value.unwrap_or(50);
        self.processes = processes;
        self.open = true;
    }

    fn ensure_open_from(
        &mut self,
        load_snapshot: impl FnOnce() -> (Option<u8>, Vec<ProcessVolume>),
    ) {
        if !self.open {
            self.open_from(load_snapshot);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&crate::gui::Panel::VolumeDialog);
        let mut window_open = self.open;
        let mut close = false;
        #[cfg(test)]
        let mut test_layout = VolumeDialogLayout::default();
        let _window_output = egui::Window::new("Volume")
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .open(&mut window_open)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_layout = VolumeDialogLayout {
                        visible: ui.is_visible(),
                        sizing_pass: ui.is_sizing_pass(),
                        ..VolumeDialogLayout::default()
                    };
                }
                compact_section_heading(ui, "System output", None);
                ui.horizontal_wrapped(|ui| {
                    let _system_slider =
                        ui.add(egui::Slider::new(&mut self.value, 0..=100).text("Level"));
                    #[cfg(test)]
                    {
                        test_layout.system_slider =
                            Some(VolumeWidgetLayout::from_response(&_system_slider));
                    }
                    let set_button = ui.button("Set");
                    #[cfg(test)]
                    {
                        test_layout.system_set =
                            Some(VolumeWidgetLayout::from_response(&set_button));
                    }
                    let set_button = with_button_emphasis(ui, set_button, ButtonEmphasis::Primary);
                    if set_button.clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Volume".into(),
                            action: format!("volume:set:{}", self.value),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    let cancel_button = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        test_layout.cancel =
                            Some(VolumeWidgetLayout::from_response(&cancel_button));
                    }
                    if cancel_button.clicked() {
                        close = true;
                    }
                });

                ui.separator();
                compact_section_heading(ui, "Applications", None);
                let process_list_height = ui.available_height().max(0.0);
                #[cfg(test)]
                let process_parent_clip_rect = ui.clip_rect();
                #[cfg(test)]
                let mut effective_process_clip_rect = egui::Rect::NOTHING;
                let _process_scroll = egui::ScrollArea::vertical()
                    .id_salt("volume_process_list")
                    .auto_shrink([true, true])
                    .min_scrolled_height(0.0)
                    .max_height(process_list_height)
                    .show(ui, |ui| {
                        #[cfg(test)]
                        {
                            effective_process_clip_rect = ui.clip_rect();
                        }
                        for (row_index, process) in self.processes.iter_mut().enumerate() {
                            ui.push_id(("volume_process", row_index, process.pid), |ui| {
                                ui.vertical(|ui| {
                                    let identity =
                                        format!("{} (PID {})", process.name, process.pid);
                                    let _identity_response =
                                        ui.add(egui::Label::new(identity).wrap());
                                    #[cfg(test)]
                                    test_layout
                                        .process_identity_rects
                                        .push(_identity_response.rect);
                                    ui.horizontal_wrapped(|ui| {
                                        let slider_response = ui.add(
                                            egui::Slider::new(&mut process.value, 0..=100)
                                                .text("Level"),
                                        );
                                        #[cfg(test)]
                                        let mut process_controls = [None; 3];
                                        #[cfg(test)]
                                        {
                                            process_controls[0] = Some(
                                                VolumeWidgetLayout::from_response(&slider_response),
                                            );
                                        }
                                        if slider_response.changed()
                                            && let Some(action) = process.slider_changed()
                                        {
                                            let _ = launch_action(&Action {
                                                label: String::new(),
                                                desc: "Volume".into(),
                                                action,
                                                args: None,
                                            });
                                        }
                                        let set_button = ui.button("Set");
                                        #[cfg(test)]
                                        {
                                            process_controls[1] = Some(
                                                VolumeWidgetLayout::from_response(&set_button),
                                            );
                                        }
                                        if set_button.clicked() {
                                            let _ = launch_action(&Action {
                                                label: String::new(),
                                                desc: "Volume".into(),
                                                action: format!(
                                                    "volume:pid:{}:{}",
                                                    process.pid, process.value
                                                ),
                                                args: None,
                                            });
                                        }
                                        let mute_button = ui.button("Mute");
                                        #[cfg(test)]
                                        {
                                            process_controls[2] = Some(
                                                VolumeWidgetLayout::from_response(&mute_button),
                                            );
                                        }
                                        if mute_button.clicked() {
                                            let _ = launch_action(&Action {
                                                label: String::new(),
                                                desc: "Volume".into(),
                                                action: format!(
                                                    "volume:pid_toggle_mute:{}",
                                                    process.pid
                                                ),
                                                args: None,
                                            });
                                            process.muted = !process.muted;
                                        }
                                        if process.muted {
                                            semantic_status(ui, StatusTone::Info, "Muted", None);
                                        }
                                        #[cfg(test)]
                                        test_layout.process_controls.push(process_controls);
                                    });
                                });
                            });
                            ui.add_space(ui.spacing().item_spacing.y);
                        }
                    });
                #[cfg(test)]
                {
                    test_layout.process_scroll = Some(VolumeScrollLayout {
                        id: _process_scroll.id,
                        pre_autoshrink_inner_rect: _process_scroll.inner_rect,
                        effective_clip_rect: effective_process_clip_rect,
                        parent_clip_rect: process_parent_clip_rect,
                        content_size: _process_scroll.content_size,
                        offset: _process_scroll.state.offset,
                    });
                }
            });
        #[cfg(test)]
        {
            if let Some(window_output) = _window_output.as_ref() {
                test_layout.window_rect = Some(window_output.response.rect);
            }
            self.last_layout = Some(test_layout);
        }
        if close {
            window_open = false;
        }
        if !pinned {
            self.open = window_open;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{VolumeDialog, VolumeDialogLayout, VolumeScrollLayout, VolumeWidgetLayout};
    use crate::gui::volume_data::ProcessVolume;
    use crate::gui::{LauncherApp, Panel};
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use eframe::egui;
    use std::collections::HashSet;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
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

    fn test_process(pid: u32, name: String, value: u8, muted: bool) -> ProcessVolume {
        ProcessVolume {
            pid,
            name,
            value,
            muted,
        }
    }

    fn snapshot(dialog: &VolumeDialog) -> (u8, Vec<(u32, String, u8, bool)>) {
        (
            dialog.value,
            dialog
                .processes
                .iter()
                .map(|process| {
                    (
                        process.pid,
                        process.name.clone(),
                        process.value,
                        process.muted,
                    )
                })
                .collect(),
        )
    }

    fn frame(
        ctx: &egui::Context,
        dialog: &mut VolumeDialog,
        app: &mut LauncherApp,
        screen_rect: egui::Rect,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        )
    }

    fn settled_layout(
        ctx: &egui::Context,
        dialog: &mut VolumeDialog,
        app: &mut LauncherApp,
        screen_rect: egui::Rect,
    ) -> VolumeDialogLayout {
        let mut layout = None;
        for _ in 0..3 {
            let output = frame(ctx, dialog, app, screen_rect, Vec::new());
            layout = dialog.last_layout.clone();
            output.drop_without_applying_deltas();
        }
        layout.expect("Volume dialog rendered a settled layout")
    }

    fn click_cancel(
        ctx: &egui::Context,
        dialog: &mut VolumeDialog,
        app: &mut LauncherApp,
        screen_rect: egui::Rect,
        target: VolumeWidgetLayout,
    ) -> VolumeDialogLayout {
        let point = target.rect.center();
        let press = frame(
            ctx,
            dialog,
            app,
            screen_rect,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        press.drop_without_applying_deltas();

        let release = frame(
            ctx,
            dialog,
            app,
            screen_rect,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let layout = dialog
            .last_layout
            .clone()
            .expect("Volume dialog rendered the Cancel response");
        release.drop_without_applying_deltas();

        let cancel = layout.cancel.expect("Cancel response was registered");
        assert_eq!(cancel.id, target.id, "Cancel keeps its native widget ID");
        assert!(
            cancel.clicked,
            "the simulated pointer click registered on Cancel"
        );
        layout
    }

    fn assert_widget_in_window(
        widget: VolumeWidgetLayout,
        window_rect: egui::Rect,
        screen_rect: egui::Rect,
    ) {
        assert!(widget.rect.is_positive());
        assert!(window_rect.contains_rect(widget.rect), "{:?}", widget.rect);
        assert!(screen_rect.contains_rect(widget.rect), "{:?}", widget.rect);
    }

    #[test]
    fn transition_loading_preserves_pinned_cancel_draft_and_reloads_after_unpinned_close() {
        let directory = tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(460.0, 340.0));
        let mut app = test_app(&ctx, directory.path());
        let mut dialog = VolumeDialog::default();
        let mut loads = 0;

        dialog.ensure_open_from(|| {
            loads += 1;
            (
                Some(38),
                vec![test_process(71, "music-player.exe".into(), 42, true)],
            )
        });
        assert!(dialog.open);
        assert_eq!(dialog.value, 38);
        assert_eq!(loads, 1);

        dialog.value = 76;
        dialog.processes[0].value = 63;
        dialog.ensure_open_from(|| {
            loads += 1;
            (
                Some(95),
                vec![test_process(99, "replacement".into(), 4, false)],
            )
        });
        assert_eq!(loads, 1, "a repeated open does not reload the snapshot");
        assert_eq!(dialog.value, 76);
        assert_eq!(dialog.processes[0].value, 63);

        app.pinned_panels.push(Panel::VolumeDialog);
        let pinned_layout = settled_layout(&ctx, &mut dialog, &mut app, screen_rect);
        let window_rect = pinned_layout.window_rect.expect("Volume window response");
        let cancel = pinned_layout.cancel.expect("Cancel response");
        assert!(pinned_layout.visible);
        assert!(!pinned_layout.sizing_pass);
        assert_widget_in_window(cancel, window_rect, screen_rect);
        assert_widget_in_window(
            pinned_layout.system_set.expect("system Set response"),
            window_rect,
            screen_rect,
        );

        let pinned_draft = snapshot(&dialog);
        click_cancel(&ctx, &mut dialog, &mut app, screen_rect, cancel);
        assert!(dialog.open, "pinned Cancel keeps the panel open");
        assert_eq!(snapshot(&dialog), pinned_draft);
        assert_eq!(loads, 1);

        app.pinned_panels.clear();
        let unpinned_layout = settled_layout(&ctx, &mut dialog, &mut app, screen_rect);
        let unpinned_cancel = unpinned_layout.cancel.expect("Cancel response");
        click_cancel(&ctx, &mut dialog, &mut app, screen_rect, unpinned_cancel);
        assert!(!dialog.open, "unpinned Cancel closes the panel");

        dialog.ensure_open_from(|| {
            loads += 1;
            (
                Some(29),
                vec![test_process(82, "new-session.exe".into(), 18, false)],
            )
        });
        assert_eq!(loads, 2, "reopening after close loads one new snapshot");
        assert!(dialog.open);
        assert_eq!(dialog.value, 29);
        assert_eq!(dialog.processes[0].pid, 82);
        assert_eq!(dialog.processes[0].value, 18);
    }

    #[test]
    fn long_process_rows_scroll_inside_the_window_without_mutating_the_snapshot() {
        let directory = tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 270.0));
        let mut app = test_app(&ctx, directory.path());
        let processes = (0..8)
            .map(|index| {
                let pid = if index < 2 { 71 } else { 70 + index };
                let name = format!(
                    "Audio renderer {index} {}",
                    "shared application session ".repeat(5)
                );
                test_process(pid, name, 20 + index as u8, index == 0)
            })
            .collect::<Vec<_>>();
        let mut dialog = VolumeDialog::default();
        dialog.open_from(|| (Some(51), processes));
        let original_snapshot = snapshot(&dialog);

        let layout = settled_layout(&ctx, &mut dialog, &mut app, screen_rect);
        let window_rect = layout.window_rect.expect("Volume window response");
        assert!(layout.visible);
        assert!(!layout.sizing_pass);
        assert!(screen_rect.contains_rect(window_rect));

        let system_slider = layout.system_slider.expect("system slider response");
        let system_set = layout.system_set.expect("system Set response");
        let cancel = layout.cancel.expect("Cancel response");
        assert_widget_in_window(system_slider, window_rect, screen_rect);
        assert_widget_in_window(system_set, window_rect, screen_rect);
        assert_widget_in_window(cancel, window_rect, screen_rect);
        assert!(system_set.id != cancel.id);

        let scroll = layout.process_scroll.expect("process ScrollArea response");
        let effective_clip = scroll.effective_clip_rect;
        assert!(scroll.pre_autoshrink_inner_rect.is_positive());
        assert!(effective_clip.is_positive());
        assert!(scroll.parent_clip_rect.contains_rect(effective_clip));
        assert!(window_rect.contains_rect(effective_clip));
        assert!(screen_rect.contains_rect(effective_clip));
        assert!(system_set.rect.max.y <= effective_clip.min.y);
        assert!(cancel.rect.max.y <= effective_clip.min.y);
        assert!(scroll.content_size.y > effective_clip.height());
        let max_offset = (scroll.content_size.y - effective_clip.height()).max(0.0);
        assert!(scroll.offset.y >= 0.0 && scroll.offset.y <= max_offset + 1.0);
        assert!(
            layout
                .process_identity_rects
                .iter()
                .any(|rect| rect.height() > 24.0),
            "long process identities wrap across multiple lines"
        );
        assert_eq!(layout.process_controls.len(), dialog.processes.len());

        let mut ids = HashSet::new();
        for id in [system_slider.id, system_set.id, cancel.id] {
            assert!(ids.insert(id), "native controls have distinct widget IDs");
        }
        for controls in &layout.process_controls {
            for control in controls.iter().flatten() {
                assert!(
                    ids.insert(control.id),
                    "process controls have distinct widget IDs"
                );
            }
        }
        let first_duplicate_pid_row = layout.process_controls[0];
        let second_duplicate_pid_row = layout.process_controls[1];
        for (first, second) in first_duplicate_pid_row
            .iter()
            .zip(second_duplicate_pid_row.iter())
        {
            assert_ne!(first.unwrap().id, second.unwrap().id);
        }

        let max_offset = (scroll.content_size.y - effective_clip.height()).max(0.0);
        assert!(max_offset > 0.0);
        let mut native_scroll_state = egui::containers::scroll_area::State::load(&ctx, scroll.id)
            .expect("the rendered ScrollArea has registered native state");
        native_scroll_state.offset.y = max_offset;
        native_scroll_state.store(&ctx, scroll.id);

        // Render settled visible frames so the assertion below observes the
        // viewport at its final offset, after egui has applied the new state.
        let scrolled_layout = settled_layout(&ctx, &mut dialog, &mut app, screen_rect);

        let scrolled = scrolled_layout
            .process_scroll
            .expect("process ScrollArea after scrolling");
        assert_eq!(
            scrolled.id, scroll.id,
            "the process list keeps its scroll ID"
        );
        assert!(
            (scrolled.offset.y - max_offset).abs() <= 1.0,
            "the list reaches the bounded native offset"
        );
        let scrolled_max_offset =
            (scrolled.content_size.y - scrolled.effective_clip_rect.height()).max(0.0);
        assert!(scrolled.offset.y <= scrolled_max_offset + 1.0);
        let last_controls = scrolled_layout
            .process_controls
            .last()
            .expect("last process row controls");
        for control in last_controls.iter().flatten() {
            assert!(
                scrolled.effective_clip_rect.contains_rect(control.rect),
                "scrolled process control is reachable within the actual viewport"
            );
        }
        assert_eq!(snapshot(&dialog), original_snapshot);
    }
}
