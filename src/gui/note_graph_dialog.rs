use crate::dashboard::DashboardDataCache;
use crate::graph::note_graph::{
    build_draw_primitives, project_world_to_screen, DrawNode, LayoutConfig, NoteGraphEngine,
    NoteGraphFilter, RenderSurface,
};
use crate::gui::LauncherApp;
use crate::plugins::note::Note;
use crate::settings::{NoteGraphSettings, Settings};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;
use std::collections::BTreeSet;

const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 2.5;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphMode {
    Global,
    Local,
}

impl Default for GraphMode {
    fn default() -> Self {
        Self::Global
    }
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
        data_cache: &DashboardDataCache,
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

        let mut notes: Vec<Note> = data_cache.snapshot().notes.iter().cloned().collect();
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
        egui::Window::new("Note Graph")
            .open(&mut window_open)
            .resizable(true)
            .default_size((1100.0, 720.0))
            .show(ctx, |ui| {
                persist_requested |= self.top_bar(ui, app);
                ui.separator();

                ui.horizontal(|ui| {
                    ui.set_min_height(ui.available_height());
                    persist_requested |= ui.vertical(|ui| self.left_panel(ui, &notes)).inner;
                    ui.separator();
                    ui.vertical(|ui| self.main_canvas(ui, ctx, app));
                    ui.separator();
                    persist_requested |= ui.vertical(|ui| self.right_panel(ui, app, &notes)).inner;
                });
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
        self.label_zoom_threshold = settings.label_zoom_threshold.clamp(0.2, 1.5);
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
        match Settings::load(&app.settings_path) {
            Ok(mut settings) => {
                settings.note_graph = value.clone();
                if let Err(err) = settings.save(&app.settings_path) {
                    app.set_error(format!("Failed to save note graph settings: {err}"));
                    return;
                }
                self.last_saved_settings = Some(value);
            }
            Err(err) => {
                app.set_error(format!("Failed to load settings for note graph: {err}"));
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Search");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search.query)
                    .desired_width(240.0)
                    .hint_text("title, alias, slug"),
            );
            if response.changed() {
                self.search.selected_idx = 0;
            }
            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(result) = self.search.results.get(self.search.selected_idx) {
                    self.selected_node_id = Some(result.slug.clone());
                    self.center_request = Some(result.slug.clone());
                }
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
                    .add(egui::DragValue::new(&mut self.local_depth).clamp_range(1..=3))
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
            if ui.button("Open selected").clicked() {
                if let Some(slug) = self.selected_node_id.clone() {
                    app.open_note_panel(&slug, None);
                }
            }
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

    fn left_panel(&mut self, ui: &mut egui::Ui, notes: &[Note]) -> bool {
        let mut changed = false;
        ui.set_min_width(220.0);
        ui.label("Filters");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.filter.include_input);
            if ui.button("+ include").clicked() {
                let normalized = normalize_tag(&self.filter.include_input);
                if !normalized.is_empty() {
                    changed |= self.filter.include_tags.insert(normalized);
                }
                self.filter.include_input.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.filter.exclude_input);
            if ui.button("+ exclude").clicked() {
                let normalized = normalize_tag(&self.filter.exclude_input);
                if !normalized.is_empty() {
                    changed |= self.filter.exclude_tags.insert(normalized);
                }
                self.filter.exclude_input.clear();
            }
        });
        ui.horizontal(|ui| {
            changed |= ui
                .radio_value(&mut self.filter.include_all, false, "Any")
                .changed();
            changed |= ui
                .radio_value(&mut self.filter.include_all, true, "All")
                .changed();
        });
        changed |= ui
            .checkbox(&mut self.filter.orphan_only, "Orphans only")
            .changed();
        changed |= ui
            .checkbox(&mut self.filter.only_tagged, "Only tagged notes")
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut self.max_nodes)
                    .clamp_range(20..=1000)
                    .prefix("Max render "),
            )
            .changed();
        changed |= ui.checkbox(&mut self.show_labels, "Show labels").changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut self.label_zoom_threshold)
                    .clamp_range(0.2..=1.5)
                    .speed(0.01)
                    .prefix("Label zoom >= "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut self.layout_iterations_per_frame)
                    .clamp_range(1..=12)
                    .prefix("Iter/frame "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut self.repulsion_strength)
                    .clamp_range(100.0..=10_000.0)
                    .prefix("Repel "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut self.link_distance)
                    .clamp_range(10.0..=300.0)
                    .prefix("Link dist "),
            )
            .changed();

        ui.separator();
        ui.label(format!("Notes in scope: {}", notes.len()));
        ui.label(format!("Nodes: {}", self.engine.model.nodes.len()));

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

    fn main_canvas(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, app: &mut LauncherApp) {
        let desired = egui::vec2(
            ui.available_width().max(300.0),
            ui.available_height().max(300.0),
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

        if response.hovered() {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                let zoom_delta = if scroll > 0.0 { 1.1 } else { 0.9 };
                let pointer = ctx
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(rect.center());
                self.camera.zoom_about(pointer, zoom_delta, rect);
            }
        }

        if let Some(slug) = self.center_request.clone() {
            if let Some(node) = self.engine.layout.nodes.get(&slug) {
                let at = self.camera.world_to_screen(node.position, rect);
                self.camera.pan += rect.center() - at;
                self.center_request = None;
            }
        }

        if response.dragged() {
            if self.dragged_node.is_none() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    self.dragged_node = hit_test_node(pointer, &screen_positions, 12.0);
                }
            }
            if let Some(dragged) = self.dragged_node.clone() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    if let Some(node) = self.engine.layout.nodes.get_mut(&dragged) {
                        node.position = self.camera.screen_to_world(pointer, rect);
                        node.pinned = true;
                        node.velocity = [0.0, 0.0];
                    }
                }
            } else {
                self.camera.pan += response.drag_delta();
            }
        }
        if response.drag_stopped() {
            self.dragged_node = None;
        }

        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                self.selected_node_id = hit_test_node(pointer, &screen_positions, 12.0);
            }
        }

        if response.double_clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(slug) = self.selected_node_id.clone() {
                app.open_note_panel(&slug, None);
            }
        }

        for edge in &draw.edges {
            let a = Pos2::new(edge.from[0], edge.from[1]);
            let b = Pos2::new(edge.to[0], edge.to[1]);
            if edge_is_visible(a, b, rect) {
                painter.line_segment([a, b], Stroke::new(1.0, Color32::from_gray(90)));
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

    fn right_panel(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, notes: &[Note]) -> bool {
        ui.set_min_width(260.0);
        ui.label("Details");
        let Some(slug) = self.selected_node_id.as_deref() else {
            ui.label("Select a node");
            return false;
        };
        let Some(note) = notes.iter().find(|n| n.slug == slug) else {
            ui.label("Selected node not in filtered set.");
            return false;
        };

        ui.label(format!("Title: {}", note.title));
        ui.label(format!("Slug: {}", note.slug));
        if let Some(alias) = &note.alias {
            ui.label(format!("Alias: {alias}"));
        }
        ui.separator();
        ui.label("Tags");
        if note.tags.is_empty() {
            ui.label("(none)");
        } else {
            ui.label(note.tags.join(", "));
        }

        ui.separator();
        ui.label("Outgoing links");
        for link in &note.links {
            if ui.link(link).clicked() {
                self.selected_node_id = Some(link.clone());
                self.center_request = Some(link.clone());
            }
        }

        ui.separator();
        ui.label("Backlinks");
        for back in notes.iter().filter(|n| n.links.contains(&note.slug)) {
            if ui.link(format!("{} [{}]", back.title, back.slug)).clicked() {
                self.selected_node_id = Some(back.slug.clone());
                self.center_request = Some(back.slug.clone());
            }
        }

        ui.separator();
        if ui.button("Center on node").clicked() {
            self.center_request = Some(note.slug.clone());
        }
        if ui.button("Open note").clicked() {
            app.open_note_panel(&note.slug, None);
        }
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
}
