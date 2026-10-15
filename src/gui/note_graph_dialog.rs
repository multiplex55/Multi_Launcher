use crate::dashboard::DashboardDataSnapshot;
use crate::graph::note_graph::{
    DrawNode, LayoutConfig, NoteGraphEngine, NoteGraphFilter, RenderSurface, build_draw_primitives,
    project_world_to_screen,
};
use crate::gui::LauncherApp;
use crate::gui::theme::widgets;
use crate::plugins::note::Note;
use crate::settings::{NoteGraphSettings, Settings};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::Arc;

const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
const MIN_CANVAS_WIDTH: f32 = 300.0;
const DETAILS_WIDTH_RATIO: f32 = 0.28;
const DETAILS_MIN: f32 = 180.0;
const DETAILS_MAX: f32 = 280.0;
const MIN_CANVAS_HEIGHT: f32 = 300.0;

#[derive(Default, Deserialize)]
struct NoteGraphDialogArgs {
    #[serde(default)]
    include_tags: Vec<String>,
    #[serde(default)]
    exclude_tags: Vec<String>,
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    local_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum GraphMode {
    #[default]
    Global,
    Local,
}

#[derive(Clone, Copy, Debug)]
struct CameraTransform {
    pan: Vec2,
    zoom: f32,
}

impl Default for CameraTransform {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl CameraTransform {
    fn world_to_screen(&self, world: [f32; 2], rect: Rect) -> Pos2 {
        let point = project_world_to_screen(
            world,
            RenderSurface {
                center: [rect.center().x, rect.center().y],
                pan: [self.pan.x, self.pan.y],
                zoom: self.zoom,
            },
        );
        Pos2::new(point[0], point[1])
    }

    fn screen_to_world(&self, screen: Pos2, rect: Rect) -> [f32; 2] {
        [
            (screen.x - rect.center().x - self.pan.x) / self.zoom,
            (screen.y - rect.center().y - self.pan.y) / self.zoom,
        ]
    }

    fn zoom_about(&mut self, pointer: Pos2, zoom_delta: f32, rect: Rect) {
        let before = self.screen_to_world(pointer, rect);
        self.zoom = (self.zoom * zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.world_to_screen(before, rect);
        self.pan += pointer - after;
    }
}

#[derive(Default)]
struct FilterState {
    include_tags: BTreeSet<String>,
    exclude_tags: BTreeSet<String>,
    include_all: bool,
    orphan_only: bool,
    only_tagged: bool,
    include_input: String,
    exclude_input: String,
}

#[derive(Default)]
struct SearchState {
    query: String,
    results: Vec<SearchResult>,
    selected_idx: usize,
}

#[derive(Clone)]
struct SearchResult {
    slug: String,
    title: String,
    alias: Option<String>,
    score: i64,
}

struct NoteGraphDetailsLayout {
    content: egui::containers::scroll_area::ScrollAreaOutput<()>,
    center_button: egui::Response,
    open_button: egui::Response,
}

#[derive(Default)]
pub struct NoteGraphDialog {
    pub open: bool,
    selected_node_id: Option<String>,
    camera: CameraTransform,
    engine: NoteGraphEngine,
    filter: FilterState,
    search: SearchState,
    graph_mode: GraphMode,
    local_depth: usize,
    simulation_paused: bool,
    max_nodes: usize,
    show_labels: bool,
    label_zoom_threshold: f32,
    layout_iterations_per_frame: usize,
    repulsion_strength: f32,
    link_distance: f32,
    dragged_node: Option<String>,
    center_request: Option<String>,
    show_details_panel: bool,
    hydrated_settings: bool,
    pending_args: Option<NoteGraphDialogArgs>,
    last_saved_settings: Option<NoteGraphSettings>,
    was_open_last_frame: bool,
}

impl NoteGraphDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.hydrated_settings = false;
    }

    pub fn open_with_args(&mut self, raw_args: Option<&str>) {
        self.pending_args = Some(
            raw_args
                .and_then(|raw| serde_json::from_str::<NoteGraphDialogArgs>(raw).ok())
                .unwrap_or_default(),
        );
        self.open();
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        data_snapshot: Arc<DashboardDataSnapshot>,
        notes_version: u64,
    ) {
        if !self.open {
            self.was_open_last_frame = false;
            return;
        }
        if !self.was_open_last_frame {
            self.hydrated_settings = false;
        }

        self.hydrate_from_settings_if_needed(app);

        let mut notes: Vec<Note> = data_snapshot.notes.iter().cloned().collect();
        notes.retain(|n| self.note_passes_ui_filters(n));
        self.refresh_search(&notes);

        let filter = NoteGraphFilter {
            include_tags: if self.filter.include_all {
                self.filter.include_tags.clone()
            } else {
                BTreeSet::new()
            },
            exclude_tags: self.filter.exclude_tags.clone(),
            orphan_notes_only: self.filter.orphan_only,
            root_slug: if self.graph_mode == GraphMode::Local {
                self.selected_node_id.clone()
            } else {
                None
            },
            depth: (self.graph_mode == GraphMode::Local).then_some(self.local_depth),
            max_nodes: None,
            include_backlinks: true,
        };

        self.engine
            .rebuild_if_needed(&notes, notes_version, &filter);
        if !self.simulation_paused {
            self.engine.layout.step(
                &self.engine.model,
                LayoutConfig {
                    iterations_per_frame: self.layout_iterations_per_frame.max(1),
                    repulsion_strength: self.repulsion_strength.max(100.0),
                    link_distance: self.link_distance.max(10.0),
                    ..LayoutConfig::default()
                },
            );
            ctx.request_repaint();
        }

        let mut persist_requested = false;
        let mut window_open = self.open;
        let screen_size = ctx.content_rect().size();
        let max_window_size = egui::vec2(
            (screen_size.x - 48.0).max(320.0),
            (screen_size.y - 96.0).max(240.0),
        );
        let default_window_size = egui::vec2(
            880.0_f32.min(max_window_size.x),
            620.0_f32.min(max_window_size.y),
        );
        egui::Window::new("Note Graph")
            .open(&mut window_open)
            .resizable(true)
            .default_size(default_window_size)
            .max_size(max_window_size)
            .show(ctx, |ui| {
                persist_requested |= self.top_bar(ui, app);
                ui.separator();
                persist_requested |= self.filters_top_panel(ui, &notes);
                ui.separator();

                let available_rect = ui.available_rect_before_wrap();
                let total_width = available_rect.width();
                let total_height = available_rect.height().max(MIN_CANVAS_HEIGHT);
                let panel_rect = Rect::from_min_size(
                    available_rect.min,
                    egui::vec2(total_width.max(MIN_CANVAS_WIDTH), total_height),
                );
                ui.allocate_rect(panel_rect, Sense::hover());

                let (canvas_width, details_width) =
                    compute_graph_layout(panel_rect.width(), self.show_details_panel);
                let canvas_rect =
                    Rect::from_min_size(panel_rect.min, egui::vec2(canvas_width, total_height));
                let details_rect = details_width.map(|width| {
                    Rect::from_min_size(
                        Pos2::new(canvas_rect.right(), panel_rect.top()),
                        egui::vec2(width, total_height),
                    )
                });

                ui.scope_builder(egui::UiBuilder::new().max_rect(canvas_rect), |ui| {
                    self.main_canvas(ui, ctx, app, canvas_rect.size())
                });

                if let Some((details_rect, details_width)) = details_rect.zip(details_width) {
                    persist_requested |= ui
                        .scope_builder(egui::UiBuilder::new().max_rect(details_rect), |ui| {
                            self.right_panel(ui, app, &notes, details_width)
                        })
                        .inner;
                }
            });
        self.open = window_open;
        self.was_open_last_frame = self.open;

        if persist_requested || self.last_saved_settings.as_ref() != Some(&self.to_settings()) {
            self.persist_settings(app);
        }
    }

    fn hydrate_from_settings_if_needed(&mut self, app: &LauncherApp) {
        if self.hydrated_settings {
            return;
        }

        let settings = Settings::load(&app.settings_path)
            .map(|s| s.note_graph)
            .unwrap_or_default();
        self.apply_settings(settings.clone());
        self.last_saved_settings = Some(settings);

        if let Some(parsed) = self.pending_args.take() {
            self.filter.include_tags = parsed
                .include_tags
                .into_iter()
                .map(|tag| normalize_tag(&tag))
                .filter(|t| !t.is_empty())
                .collect();
            self.filter.exclude_tags = parsed
                .exclude_tags
                .into_iter()
                .map(|tag| normalize_tag(&tag))
                .filter(|t| !t.is_empty())
                .collect();
            self.selected_node_id = parsed.root.filter(|root| !root.trim().is_empty());
            self.graph_mode = if parsed.local_mode || self.selected_node_id.is_some() {
                GraphMode::Local
            } else {
                GraphMode::Global
            };
            self.local_depth = parsed.depth.unwrap_or(1).clamp(1, 3);
        }

        self.hydrated_settings = true;
    }

    fn apply_settings(&mut self, settings: NoteGraphSettings) {
        self.max_nodes = settings.max_nodes.max(20);
        self.show_labels = settings.show_labels;
        self.label_zoom_threshold = settings.label_zoom_threshold.clamp(0.2, MAX_ZOOM);
        self.layout_iterations_per_frame = settings.layout_iterations_per_frame.clamp(1, 12);
        self.repulsion_strength = settings.repulsion_strength.clamp(100.0, 10_000.0);
        self.link_distance = settings.link_distance.clamp(10.0, 300.0);
        self.local_depth = settings.local_graph_depth.clamp(1, 3);
        self.filter.include_tags = settings
            .include_tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        self.filter.exclude_tags = settings
            .exclude_tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
    }

    fn to_settings(&self) -> NoteGraphSettings {
        NoteGraphSettings {
            max_nodes: self.max_nodes.max(20),
            show_labels: self.show_labels,
            label_zoom_threshold: self.label_zoom_threshold,
            layout_iterations_per_frame: self.layout_iterations_per_frame.max(1),
            repulsion_strength: self.repulsion_strength,
            link_distance: self.link_distance,
            local_graph_depth: self.local_depth,
            include_tags: self.filter.include_tags.iter().cloned().collect(),
            exclude_tags: self.filter.exclude_tags.iter().cloned().collect(),
        }
    }

    fn persist_settings(&mut self, app: &mut LauncherApp) {
        let value = self.to_settings();
        if self.last_saved_settings.as_ref() == Some(&value) {
            return;
        }
        match Settings::update(&app.settings_path, |settings| {
            settings.note_graph = value.clone();
            Ok(())
        }) {
            Ok(_) => {
                self.last_saved_settings = Some(value);
            }
            Err(err) => {
                app.report_error_message(
                    "ui operation",
                    format!("Failed to save note graph settings: {err}"),
                );
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            let search_label = ui.label("Search");
            let response = ui
                .add(
                    egui::TextEdit::singleline(&mut self.search.query)
                        .desired_width(240.0)
                        .hint_text("title, alias, slug"),
                )
                .labelled_by(search_label.id);
            if response.changed() {
                self.search.selected_idx = 0;
            }
            if response.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Some(result) = self.search.results.get(self.search.selected_idx)
            {
                self.selected_node_id = Some(result.slug.clone());
                self.center_request = Some(result.slug.clone());
            }

            ui.separator();
            changed |= ui
                .selectable_value(&mut self.graph_mode, GraphMode::Global, "Global")
                .changed();
            changed |= ui
                .selectable_value(&mut self.graph_mode, GraphMode::Local, "Local")
                .changed();
            if self.graph_mode == GraphMode::Local {
                ui.label("Depth");
                changed |= ui
                    .add(egui::DragValue::new(&mut self.local_depth).range(1..=3))
                    .changed();
            }

            ui.separator();
            ui.toggle_value(&mut self.simulation_paused, "Pause sim");
            if ui.button("Reset sim").clicked() {
                self.engine.layout.sync_model(&self.engine.model);
                for node in self.engine.layout.nodes.values_mut() {
                    node.velocity = [0.0, 0.0];
                    node.pinned = false;
                }
            }
            if ui.button("Open selected").clicked()
                && let Some(slug) = self.selected_node_id.clone()
            {
                app.open_note_panel(&slug, None);
            }
            ui.toggle_value(&mut self.show_details_panel, "Details");
        });

        if !self.search.query.trim().is_empty() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_max_height(110.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, result) in self.search.results.iter().take(8).enumerate() {
                        let label = match &result.alias {
                            Some(alias) => {
                                format!("{} ({alias}) [{}]", result.title, result.slug)
                            }
                            None => format!("{} [{}]", result.title, result.slug),
                        };
                        if ui
                            .selectable_label(idx == self.search.selected_idx, label)
                            .clicked()
                        {
                            self.selected_node_id = Some(result.slug.clone());
                            self.center_request = Some(result.slug.clone());
                        }
                    }
                });
            });
        }
        changed
    }

    fn filters_top_panel(&mut self, ui: &mut egui::Ui, notes: &[Note]) -> bool {
        let mut changed = false;
        widgets::compact_section_heading(ui, "Filters", None);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.include_input)
                    .desired_width(140.0)
                    .hint_text("include tag"),
            );
            if ui.button("+ include").clicked() {
                let normalized = normalize_tag(&self.filter.include_input);
                if !normalized.is_empty() {
                    changed |= self.filter.include_tags.insert(normalized);
                }
                self.filter.include_input.clear();
            }

            ui.add(
                egui::TextEdit::singleline(&mut self.filter.exclude_input)
                    .desired_width(140.0)
                    .hint_text("exclude tag"),
            );
            if ui.button("+ exclude").clicked() {
                let normalized = normalize_tag(&self.filter.exclude_input);
                if !normalized.is_empty() {
                    changed |= self.filter.exclude_tags.insert(normalized);
                }
                self.filter.exclude_input.clear();
            }

            changed |= ui
                .radio_value(&mut self.filter.include_all, false, "Any")
                .changed();
            changed |= ui
                .radio_value(&mut self.filter.include_all, true, "All")
                .changed();

            changed |= ui
                .checkbox(&mut self.filter.orphan_only, "Orphans only")
                .changed();
            changed |= ui
                .checkbox(&mut self.filter.only_tagged, "Only tagged notes")
                .changed();

            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.max_nodes)
                        .range(20..=1000)
                        .prefix("Max render "),
                )
                .changed();
            changed |= ui.checkbox(&mut self.show_labels, "Show labels").changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.label_zoom_threshold)
                        .range(0.2..=MAX_ZOOM)
                        .speed(0.01)
                        .prefix("Label zoom >= "),
                )
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.layout_iterations_per_frame)
                        .range(1..=12)
                        .prefix("Iter/frame "),
                )
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.repulsion_strength)
                        .range(100.0..=10_000.0)
                        .prefix("Repel "),
                )
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.link_distance)
                        .range(10.0..=300.0)
                        .prefix("Link dist "),
                )
                .changed();

            ui.label(format!("Notes in scope: {}", notes.len()));
            ui.label(format!("Nodes: {}", self.engine.model.nodes.len()));
        });

        let mut remove_include = None;
        for tag in &self.filter.include_tags {
            if ui.small_button(format!("include:{tag} ✕")).clicked() {
                remove_include = Some(tag.clone());
            }
        }
        if let Some(tag) = remove_include {
            changed = true;
            self.filter.include_tags.remove(&tag);
        }

        let mut remove_exclude = None;
        for tag in &self.filter.exclude_tags {
            if ui.small_button(format!("exclude:{tag} ✕")).clicked() {
                remove_exclude = Some(tag.clone());
            }
        }
        if let Some(tag) = remove_exclude {
            changed = true;
            self.filter.exclude_tags.remove(&tag);
        }
        changed
    }

    fn main_canvas(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        canvas_size: Vec2,
    ) {
        let desired = egui::vec2(
            canvas_size.x.max(MIN_CANVAS_WIDTH),
            canvas_size.y.max(MIN_CANVAS_HEIGHT),
        );
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

        let draw = build_draw_primitives(
            &self.engine.model,
            &self.engine.layout,
            RenderSurface {
                center: [rect.center().x, rect.center().y],
                pan: [self.camera.pan.x, self.camera.pan.y],
                zoom: self.camera.zoom,
            },
            self.max_nodes.max(20),
        );

        let screen_positions: Vec<(String, Pos2)> = draw
            .nodes
            .iter()
            .map(|node| (node.id.clone(), Pos2::new(node.screen[0], node.screen[1])))
            .collect();

        if response.hovered()
            && let Some(zoom_delta) = ctx.input(|i| graph_wheel_zoom_delta(&i.raw.events))
        {
            let pointer = ctx
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(rect.center());
            self.camera.zoom_about(pointer, zoom_delta, rect);
        }

        if let Some(slug) = self.center_request.clone()
            && let Some(node) = self.engine.layout.nodes.get(&slug)
        {
            let at = self.camera.world_to_screen(node.position, rect);
            self.camera.pan += rect.center() - at;
            self.center_request = None;
        }

        if response.dragged() {
            if self.dragged_node.is_none()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.dragged_node = hit_test_node(pointer, &screen_positions, 12.0);
            }
            if let Some(dragged) = self.dragged_node.clone() {
                if let Some(pointer) = response.interact_pointer_pos()
                    && let Some(node) = self.engine.layout.nodes.get_mut(&dragged)
                {
                    node.position = self.camera.screen_to_world(pointer, rect);
                    node.pinned = true;
                    node.velocity = [0.0, 0.0];
                }
            } else {
                self.camera.pan += response.drag_delta();
            }
        }
        if response.drag_stopped() {
            self.dragged_node = None;
        }

        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.selected_node_id = hit_test_node(pointer, &screen_positions, 12.0);
        }

        if (response.double_clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)))
            && let Some(slug) = self.selected_node_id.clone()
        {
            app.open_note_panel(&slug, None);
        }

        for edge in &draw.edges {
            let a = Pos2::new(edge.from[0], edge.from[1]);
            let b = Pos2::new(edge.to[0], edge.to[1]);
            if edge_is_visible(a, b, rect) {
                painter.line_segment([a, b], Stroke::new(1.0_f32, Color32::from_gray(90)));
            }
        }

        for node in &draw.nodes {
            self.paint_node(painter.clone(), ui, node);
        }

        if draw.is_truncated() {
            painter.text(
                rect.left_top() + egui::vec2(8.0, 8.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "Truncated: rendering {}/{}",
                    draw.nodes.len(),
                    draw.total_nodes
                ),
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::YELLOW,
            );
        }
    }

    fn paint_node(&self, painter: egui::Painter, ui: &egui::Ui, node: &DrawNode) {
        let p = Pos2::new(node.screen[0], node.screen[1]);
        let selected = self.selected_node_id.as_deref() == Some(node.id.as_str());
        painter.circle_filled(
            p,
            if selected { 9.0 } else { 7.0 },
            if selected {
                Color32::from_rgb(250, 200, 70)
            } else {
                Color32::from_rgb(110, 185, 130)
            },
        );
        if self.show_labels && self.camera.zoom >= self.label_zoom_threshold {
            painter.text(
                p + egui::vec2(10.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &node.label,
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
        }
    }

    fn right_panel(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        notes: &[Note],
        width: f32,
    ) -> bool {
        ui.set_min_width(width);
        ui.set_max_width(width);
        widgets::compact_section_heading(ui, "Details", None);
        let Some(slug) = self.selected_node_id.as_deref() else {
            widgets::secondary_help(ui, "Select a node");
            return false;
        };
        let Some(note) = notes.iter().find(|n| n.slug == slug) else {
            widgets::secondary_help(ui, "Selected node not in filtered set.");
            return false;
        };

        let layout = render_note_graph_details(
            ui,
            width,
            note,
            notes,
            &mut self.selected_node_id,
            &mut self.center_request,
        );
        if layout.center_button.clicked() {
            self.center_request = Some(note.slug.clone());
        }
        if layout.open_button.clicked() {
            app.open_note_panel(&note.slug, None);
        }
        let _ = layout.content;
        false
    }

    fn note_passes_ui_filters(&self, note: &Note) -> bool {
        if self.filter.only_tagged && note.tags.is_empty() {
            return false;
        }
        let note_tags: BTreeSet<String> = note.tags.iter().map(|t| normalize_tag(t)).collect();

        if !self.filter.include_tags.is_empty() {
            if self.filter.include_all {
                if !self.filter.include_tags.is_subset(&note_tags) {
                    return false;
                }
            } else if self.filter.include_tags.is_disjoint(&note_tags) {
                return false;
            }
        }

        !note_tags
            .iter()
            .any(|tag| self.filter.exclude_tags.contains(tag))
    }

    fn refresh_search(&mut self, notes: &[Note]) {
        self.search.results = rank_search_results(&self.search.query, notes);
        if self.search.selected_idx >= self.search.results.len() {
            self.search.selected_idx = 0;
        }
    }
}

fn render_note_graph_details(
    ui: &mut egui::Ui,
    width: f32,
    note: &Note,
    notes: &[Note],
    selected_node_id: &mut Option<String>,
    center_request: &mut Option<String>,
) -> NoteGraphDetailsLayout {
    let button_heights = note_graph_details_button_heights(ui, width);
    let footer_height = note_graph_details_footer_reserved_height(ui, &button_heights);
    let scroll_height = (ui.available_height() - footer_height).max(0.0);

    let content = egui::ScrollArea::vertical()
        .id_salt("note_graph_details_content")
        .max_height(scroll_height)
        .min_scrolled_height(0.0)
        .show(ui, |ui| {
            widgets::compact_section_heading(ui, &format!("Title: {}", note.title), None);
            widgets::secondary_help(ui, &format!("Slug: {}", note.slug));
            if let Some(alias) = &note.alias {
                widgets::secondary_help(ui, &format!("Alias: {alias}"));
            }

            ui.separator();
            widgets::compact_section_heading(ui, "Tags", None);
            if note.tags.is_empty() {
                widgets::secondary_help(ui, "(none)");
            } else {
                widgets::secondary_help(ui, &note.tags.join(", "));
            }

            ui.separator();
            widgets::compact_section_heading(ui, "Outgoing links", None);
            for link in &note.links {
                if ui.link(link).clicked() {
                    *selected_node_id = Some(link.clone());
                    *center_request = Some(link.clone());
                }
            }

            ui.separator();
            widgets::compact_section_heading(ui, "Backlinks", None);
            for back in notes.iter().filter(|n| n.links.contains(&note.slug)) {
                if ui.link(format!("{} [{}]", back.title, back.slug)).clicked() {
                    *selected_node_id = Some(back.slug.clone());
                    *center_request = Some(back.slug.clone());
                }
            }
        });

    ui.separator();
    let center_button = ui.add_sized(
        egui::vec2(width, button_heights[0]),
        egui::Button::new("Center on node").wrap(),
    );
    let open_button = ui.add_sized(
        egui::vec2(width, button_heights[1]),
        egui::Button::new("Open note").wrap(),
    );

    NoteGraphDetailsLayout {
        content,
        center_button,
        open_button,
    }
}

fn note_graph_details_button_heights(ui: &mut egui::Ui, width: f32) -> [f32; 2] {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let frame_margin = if ui.visuals().button_frame {
        button_frame.total_margin().sum()
    } else {
        Vec2::ZERO
    };
    let text_width = (ui.available_width().min(width) - frame_margin.x).max(1.0);
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);

    ["Center on node", "Open note"].map(|label| {
        let text_height = egui::WidgetText::from(label)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                text_width,
                button_font.clone(),
            )
            .size()
            .y;
        let height = (text_height + frame_margin.y).max(spacing.interact_size.y);
        (height * pixels_per_point).ceil() / pixels_per_point
    })
}

fn note_graph_details_footer_reserved_height(ui: &egui::Ui, button_heights: &[f32; 2]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height =
        button_heights.iter().sum::<f32>() + 3.0 * spacing.item_spacing.y + separator_spacing;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

fn compute_graph_layout(total_width: f32, details_visible: bool) -> (f32, Option<f32>) {
    let total_width = total_width.max(0.0);
    if !details_visible {
        return (total_width.max(MIN_CANVAS_WIDTH), None);
    }

    let details_width = (total_width * DETAILS_WIDTH_RATIO)
        .clamp(DETAILS_MIN, DETAILS_MAX)
        .min((total_width - MIN_CANVAS_WIDTH).max(0.0));
    let canvas_width = (total_width - details_width).max(MIN_CANVAS_WIDTH);
    (canvas_width, Some(details_width))
}

fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('#')
        .trim_start_matches('@')
        .to_lowercase()
}

fn hit_test_node(pointer: Pos2, nodes: &[(String, Pos2)], radius: f32) -> Option<String> {
    let mut best: Option<(String, f32)> = None;
    for (id, p) in nodes {
        let dist = pointer.distance(*p);
        if dist <= radius {
            match &best {
                Some((_, best_dist)) if dist >= *best_dist => {}
                _ => best = Some((id.clone(), dist)),
            }
        }
    }
    best.map(|(id, _)| id)
}

fn edge_is_visible(a: Pos2, b: Pos2, rect: Rect) -> bool {
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let min_y = a.y.min(b.y);
    let max_y = a.y.max(b.y);
    max_x >= rect.left() && min_x <= rect.right() && max_y >= rect.top() && min_y <= rect.bottom()
}

fn rank_search_results(query: &str, notes: &[Note]) -> Vec<SearchResult> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored = Vec::new();

    for note in notes {
        let mut best = matcher.fuzzy_match(&note.title, q).unwrap_or(i64::MIN / 2);
        best = best.max(matcher.fuzzy_match(&note.slug, q).unwrap_or(i64::MIN / 2));
        if let Some(alias) = &note.alias {
            best = best.max(matcher.fuzzy_match(alias, q).unwrap_or(i64::MIN / 2));
        }
        if best > i64::MIN / 4 {
            scored.push(SearchResult {
                slug: note.slug.clone(),
                title: note.title.clone(),
                alias: note.alias.clone(),
                score: best,
            });
        }
    }

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    scored
}

fn graph_wheel_zoom_delta(events: &[egui::Event]) -> Option<f32> {
    let scroll = events
        .iter()
        .filter_map(|event| match event {
            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
            _ => None,
        })
        .sum::<f32>();

    (scroll.abs() > f32::EPSILON).then_some(if scroll > 0.0 { 1.1 } else { 0.9 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(slug: &str, title: &str, alias: Option<&str>, tags: &[&str]) -> Note {
        Note {
            title: title.to_string(),
            path: PathBuf::from(format!("{slug}.md")),
            content: String::new(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            links: Vec::new(),
            slug: slug.to_string(),
            alias: alias.map(str::to_string),
            aliases: alias.map(|a| vec![a.to_string()]).unwrap_or_default(),
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn transform_roundtrip_and_zoom_about_pointer() {
        let mut t = CameraTransform::default();
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(100.0, 100.0));
        let world = [8.0, -3.0];
        let screen = t.world_to_screen(world, rect);
        assert_eq!(t.screen_to_world(screen, rect), world);

        t.zoom_about(Pos2::new(50.0, 50.0), 1.5, rect);
        let screen2 = t.world_to_screen(world, rect);
        assert!((screen2.x - 62.0).abs() < 0.001);
        assert!((screen2.y - 45.5).abs() < 0.001);
    }

    #[test]
    fn zoom_clamps_to_new_max_bound() {
        let mut t = CameraTransform::default();
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(320.0, 240.0));

        t.zoom_about(rect.center(), 100.0, rect);

        assert_eq!(t.zoom, MAX_ZOOM);
    }

    #[test]
    fn zoom_about_preserves_anchor_near_min_and_max_bounds() {
        let mut t = CameraTransform {
            pan: egui::vec2(37.0, -22.0),
            zoom: 1.0,
        };
        let rect = Rect::from_min_size(Pos2::new(20.0, 30.0), egui::vec2(640.0, 480.0));
        let pointer = Pos2::new(211.0, 189.0);

        let anchor_world = t.screen_to_world(pointer, rect);

        t.zoom_about(pointer, 100.0, rect);
        assert_eq!(t.zoom, MAX_ZOOM);
        let anchored_at_max = t.world_to_screen(anchor_world, rect);
        assert!((anchored_at_max.x - pointer.x).abs() < 1e-3);
        assert!((anchored_at_max.y - pointer.y).abs() < 1e-3);

        t.zoom_about(pointer, 0.0001, rect);
        assert_eq!(t.zoom, MIN_ZOOM);
        let anchored_at_min = t.world_to_screen(anchor_world, rect);
        assert!((anchored_at_min.x - pointer.x).abs() < 1e-3);
        assert!((anchored_at_min.y - pointer.y).abs() < 1e-3);
    }

    #[test]
    fn graph_zoom_uses_only_current_mouse_wheel_events() {
        assert_eq!(
            graph_wheel_zoom_delta(&[egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 40.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            }]),
            Some(1.1)
        );
        assert_eq!(
            graph_wheel_zoom_delta(&[egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            }]),
            Some(0.9)
        );
        assert_eq!(
            graph_wheel_zoom_delta(&[egui::Event::PointerMoved(Pos2::ZERO)]),
            None
        );
    }

    #[test]
    fn label_zoom_threshold_accepts_values_up_to_max_zoom() {
        let mut dialog = NoteGraphDialog::default();
        dialog.apply_settings(NoteGraphSettings {
            label_zoom_threshold: MAX_ZOOM * 2.0,
            ..NoteGraphSettings::default()
        });

        assert_eq!(dialog.label_zoom_threshold, MAX_ZOOM);
    }

    #[test]
    fn hit_test_returns_closest_node_within_radius() {
        let nodes = vec![
            ("a".to_string(), Pos2::new(10.0, 10.0)),
            ("b".to_string(), Pos2::new(14.0, 10.0)),
        ];
        let hit = hit_test_node(Pos2::new(13.5, 10.0), &nodes, 5.0);
        assert_eq!(hit.as_deref(), Some("b"));
        assert!(hit_test_node(Pos2::new(30.0, 30.0), &nodes, 5.0).is_none());
    }

    #[test]
    fn search_ranks_slug_and_alias_matches() {
        let notes = vec![
            note("ml-roadmap", "Roadmap", Some("plan"), &[]),
            note("journal", "Daily Journal", Some("diary"), &[]),
        ];
        let results = rank_search_results("dia", &notes);
        assert_eq!(results.first().map(|r| r.slug.as_str()), Some("journal"));
    }

    #[test]
    fn open_with_args_queues_prefilter_and_root_mode() {
        let mut dlg = NoteGraphDialog::default();
        dlg.open_with_args(Some(
            r##"{"include_tags":["#work"],"exclude_tags":["@old"],"root":"alpha","depth":4,"local_mode":true}"##,
        ));
        assert!(dlg.open);
        assert!(dlg.pending_args.is_some());
    }

    #[test]
    fn compute_graph_layout_table_driven_cases() {
        struct Case {
            total_width: f32,
            details_visible: bool,
            expected_canvas: f32,
            expected_details: Option<f32>,
        }

        let cases = [
            Case {
                total_width: 260.0,
                details_visible: false,
                expected_canvas: MIN_CANVAS_WIDTH,
                expected_details: None,
            },
            Case {
                total_width: 420.0,
                details_visible: true,
                expected_canvas: MIN_CANVAS_WIDTH,
                expected_details: Some(120.0),
            },
            Case {
                total_width: 1000.0,
                details_visible: true,
                expected_canvas: 720.0,
                expected_details: Some(280.0),
            },
            Case {
                total_width: 1000.0,
                details_visible: false,
                expected_canvas: 1000.0,
                expected_details: None,
            },
        ];

        for case in cases {
            let (canvas, details) = compute_graph_layout(case.total_width, case.details_visible);
            assert!((canvas - case.expected_canvas).abs() < f32::EPSILON);
            match (details, case.expected_details) {
                (Some(actual), Some(expected)) => {
                    assert!((actual - expected).abs() < f32::EPSILON);
                }
                (None, None) => {}
                _ => panic!(
                    "details width mismatch for total width {}",
                    case.total_width
                ),
            }
        }
    }

    #[test]
    fn details_visible_canvas_only_shrinks_by_computed_details_width() {
        let total_width = 900.0;
        let (hidden_canvas, hidden_details) = compute_graph_layout(total_width, false);
        let (visible_canvas, visible_details) = compute_graph_layout(total_width, true);

        assert_eq!(hidden_details, None);
        let details_width = visible_details.expect("details panel should have a width");
        assert!((hidden_canvas - total_width).abs() < f32::EPSILON);
        assert!((visible_canvas - (hidden_canvas - details_width)).abs() < f32::EPSILON);
        assert!((visible_canvas + details_width - total_width).abs() < f32::EPSILON);
    }

    #[test]
    fn long_details_scroll_keeps_actions_reachable_and_state_stable() {
        struct Frame {
            panel_clip: Rect,
            scroll_id: egui::Id,
            content_size: Vec2,
            inner_rect: Rect,
            scroll_offset: Vec2,
            center_id: egui::Id,
            center_rect: Rect,
            open_id: egui::Id,
            open_rect: Rect,
        }

        fn render_frame(
            ctx: &egui::Context,
            dialog: &mut NoteGraphDialog,
            notes: &[Note],
            screen_rect: Rect,
            panel_rect: Rect,
        ) -> (egui::FullOutput, Frame) {
            let mut rendered = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen_rect),
                    ..Default::default()
                },
                |ui| {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(panel_rect), |ui| {
                        let panel_rect = ui.max_rect();
                        let panel_clip = ui.clip_rect().intersect(panel_rect);
                        let note = notes
                            .iter()
                            .find(|note| note.slug == "root")
                            .expect("synthetic root note is present");
                        let layout = render_note_graph_details(
                            ui,
                            panel_rect.width(),
                            note,
                            notes,
                            &mut dialog.selected_node_id,
                            &mut dialog.center_request,
                        );
                        rendered = Some(Frame {
                            panel_clip,
                            scroll_id: layout.content.id,
                            content_size: layout.content.content_size,
                            inner_rect: layout.content.inner_rect,
                            scroll_offset: layout.content.state.offset,
                            center_id: layout.center_button.id,
                            center_rect: layout.center_button.rect,
                            open_id: layout.open_button.id,
                            open_rect: layout.open_button.rect,
                        });
                    });
                },
            );
            (
                output,
                rendered.expect("details panel rendered in the constrained UI"),
            )
        }

        let ctx = egui::Context::default();
        let mut style = ctx.style_of(ctx.theme()).as_ref().clone();
        style.override_font_id = Some(egui::FontId::proportional(30.0));
        ctx.set_style_of(ctx.theme(), style);

        let screen_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 320.0));
        let panel_rect = Rect::from_min_size(egui::pos2(12.0, 12.0), egui::vec2(180.0, 280.0));
        let title = "A deliberately long note title ".repeat(12);
        let alias = "a deliberately long alternate title ".repeat(8);
        let mut root = note(
            "root",
            &title,
            Some(&alias),
            &["#work", "#a-tag-that-wraps-inside-the-narrow-panel"],
        );
        root.links = (0..24)
            .map(|index| format!("outgoing-destination-{index:02}-with-a-long-slug"))
            .collect();
        let mut notes = vec![root];
        for index in 0..18 {
            let mut backlink = note(
                &format!("backlink-{index:02}"),
                &format!("Backlink {index}: a long title that wraps in details"),
                None,
                &[],
            );
            backlink.links.push("root".to_owned());
            notes.push(backlink);
        }

        let mut dialog = NoteGraphDialog::default();
        dialog.selected_node_id = Some("root".to_owned());
        dialog.center_request = Some("previous-center-request".to_owned());
        dialog.filter.include_tags = BTreeSet::from(["work".to_owned()]);
        dialog.filter.exclude_tags = BTreeSet::from(["archived".to_owned()]);
        dialog.filter.include_all = true;
        dialog.filter.only_tagged = true;
        let selection_before = dialog.selected_node_id.clone();
        let center_before = dialog.center_request.clone();
        let filter_before = (
            dialog.filter.include_tags.clone(),
            dialog.filter.exclude_tags.clone(),
            dialog.filter.include_all,
            dialog.filter.only_tagged,
        );

        let (first_output, first_frame) =
            render_frame(&ctx, &mut dialog, &notes, screen_rect, panel_rect);
        first_output.drop_without_applying_deltas();
        let (second_output, second_frame) =
            render_frame(&ctx, &mut dialog, &notes, screen_rect, panel_rect);
        second_output.drop_without_applying_deltas();

        assert!(
            second_frame.content_size.y > second_frame.inner_rect.height(),
            "long note metadata, outgoing links, and backlinks retain a scroll range"
        );
        assert!(
            second_frame
                .panel_clip
                .contains_rect(second_frame.inner_rect)
        );
        assert!(
            second_frame
                .panel_clip
                .contains_rect(second_frame.center_rect)
        );
        assert!(
            second_frame
                .panel_clip
                .contains_rect(second_frame.open_rect)
        );
        assert!(second_frame.center_rect.is_positive());
        assert!(second_frame.open_rect.is_positive());
        assert_ne!(second_frame.center_id, second_frame.open_id);
        assert_eq!(first_frame.scroll_id, second_frame.scroll_id);
        assert_eq!(first_frame.center_id, second_frame.center_id);
        assert_eq!(first_frame.open_id, second_frame.open_id);
        assert_eq!(first_frame.center_rect, second_frame.center_rect);
        assert_eq!(first_frame.open_rect, second_frame.open_rect);

        let max_scroll_y =
            (second_frame.content_size.y - second_frame.inner_rect.height()).max(0.0);
        assert!(second_frame.scroll_offset.x.abs() <= f32::EPSILON);
        assert!(second_frame.scroll_offset.y >= -f32::EPSILON);
        assert!(second_frame.scroll_offset.y <= max_scroll_y + f32::EPSILON);
        let stored_state = egui::scroll_area::State::load(&ctx, second_frame.scroll_id)
            .expect("the details scroll area persists its actual state");
        assert_eq!(stored_state.offset, second_frame.scroll_offset);

        assert_eq!(dialog.selected_node_id, selection_before);
        assert_eq!(dialog.center_request, center_before);
        assert_eq!(
            (
                dialog.filter.include_tags.clone(),
                dialog.filter.exclude_tags.clone(),
                dialog.filter.include_all,
                dialog.filter.only_tagged,
            ),
            filter_before
        );
    }
}
