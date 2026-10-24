//! Owned native fixture for the production Duplicate Finder dialog.
//!
//! The fixture accepts only caller-supplied generated roots. It performs no
//! scan until the real Start response is clicked and routes result actions to
//! an inert recording sink rather than Explorer or the system clipboard.

use super::duplicate_finder_dialog::{
    CandidateProof, DuplicateFinderActions, DuplicateFinderDialogState, SmokeResponseCandidate,
    SmokeResponseKind,
};
use crate::duplicate_finder::FirstReadCheckpointGate;
use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const WINDOW_TITLE: &str = "Duplicate Finder M5 Fixture";
const DIALOG_INSTANCE: u64 = 0x4D31_1C01;

struct FixtureArguments {
    identity: String,
    roots: Vec<PathBuf>,
    output: PathBuf,
}

impl FixtureArguments {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut identity = None;
        let mut roots = Vec::new();
        let mut output = None;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--fixture-identity" => {
                    identity = Some(next_value(&mut arguments, &argument)?);
                }
                "--root" => roots.push(PathBuf::from(next_value(&mut arguments, &argument)?)),
                "--out" => output = Some(PathBuf::from(next_value(&mut arguments, &argument)?)),
                _ => {
                    return Err(format!(
                        "unsupported Duplicate Finder fixture option {argument}"
                    ));
                }
            }
        }
        let identity = identity
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "--fixture-identity is required".to_owned())?;
        if roots.is_empty() || roots.len() > 32 {
            return Err("provide between one and 32 repeated --root options".into());
        }
        let output =
            output.ok_or_else(|| "--out owned observation directory is required".to_owned())?;
        std::fs::create_dir_all(&output)
            .map_err(|error| format!("create owned observation directory: {error}"))?;
        Ok(Self {
            identity,
            roots,
            output,
        })
    }
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("missing value for {option}"))
}

#[derive(Default)]
struct FixtureActionSink {
    calls: Mutex<Vec<serde_json::Value>>,
}

impl FixtureActionSink {
    fn push(&self, value: serde_json::Value) {
        let mut calls = self.calls.lock().unwrap();
        if calls.len() < 64 {
            calls.push(value);
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::Value::Array(self.calls.lock().unwrap().clone())
    }
}

impl DuplicateFinderActions for FixtureActionSink {
    fn revalidate(&self, proof: &CandidateProof) -> Result<(), String> {
        super::duplicate_finder_dialog::SystemDuplicateFinderActions.revalidate(proof)?;
        let path = proof
            .fixture_path()
            .to_str()
            .ok_or_else(|| "fixture path cannot be represented as UTF-8".to_owned())?;
        let identity = proof.fixture_identity();
        self.push(serde_json::json!({
            "operation": "revalidate",
            "generation": proof.fixture_generation(),
            "identity_volume": identity.volume_serial_number(),
            "identity_file_id": hex::encode(identity.file_id()),
            "path": path,
        }));
        Ok(())
    }

    fn open(&self, path: &std::path::Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| "fixture open path cannot be represented as UTF-8".to_owned())?;
        self.push(serde_json::json!({
            "operation": "open",
            "sink": "fixture_only",
            "path": path,
        }));
        Ok(())
    }

    fn reveal(&self, path: &std::path::Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or_else(|| "fixture reveal path cannot be represented as UTF-8".to_owned())?;
        self.push(serde_json::json!({
            "operation": "reveal",
            "sink": "fixture_only",
            "path": path,
        }));
        Ok(())
    }

    fn copy_paths(&self, text: &str) -> Result<(), String> {
        self.push(serde_json::json!({
            "operation": "copy_paths",
            "sink": "fixture_only",
            "copied_text": text,
        }));
        Ok(())
    }
}

struct DuplicateFinderFixtureApp {
    dialog: DuplicateFinderDialogState,
    actions: Arc<FixtureActionSink>,
    identity: String,
    observation_path: PathBuf,
    retirement_path: PathBuf,
    worker_joined: bool,
    pending_frame: Option<PendingFrame>,
    last_error: Option<String>,
}

#[derive(Clone)]
struct PendingFrame {
    observation: serde_json::Value,
    root_frame: u64,
    root_pass: u64,
    root_pass_index: usize,
    dialog_instance: u64,
    session_generation: u64,
    dialog_open: bool,
    owner_layer: egui::LayerId,
    owner_layer_current: bool,
    responses: Vec<SmokeResponseCandidate>,
}

#[derive(Clone, Copy)]
struct HitWidget {
    id: egui::Id,
    layer_id: egui::LayerId,
    rect: egui::Rect,
    interact_rect: egui::Rect,
    enabled: bool,
}

fn rect_is_finite_positive(rect: egui::Rect) -> bool {
    rect.is_positive()
        && [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .into_iter()
            .all(f32::is_finite)
}

fn layer_json(layer: egui::LayerId) -> serde_json::Value {
    serde_json::json!({
        "order": format!("{:?}", layer.order),
        "id": layer.id.value(),
    })
}

fn update_response_json(
    observation: &mut serde_json::Value,
    response: &SmokeResponseCandidate,
    current: bool,
    eligible: bool,
    input_hit_eligible: bool,
    completed_frame: u64,
    layer_current: bool,
) {
    let target = match &response.kind {
        SmokeResponseKind::Control(key) => observation
            .get_mut("responses")
            .and_then(|responses| responses.get_mut(*key)),
        SmokeResponseKind::ResultAction {
            action,
            group_index,
            member_index,
        } => observation
            .get_mut("result_actions")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|actions| {
                actions.iter_mut().find(|candidate| {
                    candidate.get("id").and_then(serde_json::Value::as_u64)
                        == Some(response.id.value())
                        && candidate.get("action").and_then(serde_json::Value::as_str)
                            == Some(*action)
                        && candidate
                            .get("group_index")
                            .and_then(serde_json::Value::as_u64)
                            == Some(*group_index as u64)
                        && candidate.get("member_index").and_then(|value| {
                            if value.is_null() {
                                None
                            } else {
                                value.as_u64()
                            }
                        }) == member_index.map(|index| index as u64)
                        && candidate
                            .get("generation")
                            .and_then(serde_json::Value::as_u64)
                            == response.generation
                })
            }),
    };
    if let Some(target) = target {
        target["response_current"] = serde_json::json!(current);
        target["interaction_eligible"] = serde_json::json!(eligible);
        target["input_hit_eligible"] = serde_json::json!(input_hit_eligible);
        target["painted_frame"] = serde_json::json!(completed_frame);
        target["layer"] = layer_json(response.layer_id);
        target["layer_current"] = serde_json::json!(layer_current);
    }
}

fn validate_completed_frame(
    ctx: &egui::Context,
    candidate: &mut PendingFrame,
    current_generation: u64,
    current_dialog_open: bool,
) -> bool {
    let completed_frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
    let completed_pass = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
    let (pointer, click, widgets) = ctx.viewport_for(egui::ViewportId::ROOT, |viewport| {
        let widgets = candidate
            .responses
            .iter()
            .map(|response| {
                viewport
                    .prev_pass
                    .widgets
                    .get(response.id)
                    .map(|widget| HitWidget {
                        id: widget.id,
                        layer_id: widget.layer_id,
                        rect: widget.rect,
                        interact_rect: widget.interact_rect,
                        enabled: widget.enabled,
                    })
            })
            .collect::<Vec<_>>();
        let click = viewport.hits.click.map(|widget| HitWidget {
            id: widget.id,
            layer_id: widget.layer_id,
            rect: widget.rect,
            interact_rect: widget.interact_rect,
            enabled: widget.enabled,
        });
        (viewport.input.pointer.latest_pos(), click, widgets)
    });
    let exact_completion = completed_frame == candidate.root_frame.saturating_add(1)
        && completed_pass == candidate.root_pass.saturating_add(1)
        && candidate.dialog_instance == DIALOG_INSTANCE;
    let generation_current = current_generation == candidate.session_generation;
    let owner_layer_current = candidate.owner_layer_current
        && crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == Some(candidate.owner_layer);
    let frame_valid = exact_completion && generation_current;
    let mut response_failures = Vec::new();

    for (response, widget) in candidate.responses.iter().zip(widgets) {
        let widget_matches = widget.is_some_and(|widget| {
            widget.id == response.id
                && widget.layer_id == response.layer_id
                && widget.enabled == response.enabled
                && widget.rect == response.response_rect
                && widget.interact_rect == response.rect
        });
        let geometry_valid = rect_is_finite_positive(response.response_rect)
            && rect_is_finite_positive(response.rect)
            && response.response_rect.contains_rect(response.rect);
        let current = frame_valid
            && owner_layer_current
            && widget_matches
            && geometry_valid
            && response.layer_id == candidate.owner_layer
            && response
                .generation
                .is_none_or(|generation| generation == candidate.session_generation);
        let eligible = current
            && candidate.dialog_open
            && current_dialog_open
            && response.enabled
            && response.interaction_eligible;
        let input_hit_eligible = eligible
            && pointer.is_some_and(|pointer| response.rect.contains(pointer))
            && click.is_some_and(|click| {
                click.id == response.id
                    && click.layer_id == response.layer_id
                    && click.enabled == response.enabled
                    && click.rect == response.response_rect
                    && click.interact_rect == response.rect
            });
        if !current {
            response_failures.push(format!("response:{}", response.id.value()));
        }
        update_response_json(
            &mut candidate.observation,
            response,
            current,
            eligible,
            input_hit_eligible,
            completed_frame,
            owner_layer_current,
        );
    }

    candidate.observation["frame_id"] = serde_json::json!(completed_frame);
    candidate.observation["candidate_root_frame"] = serde_json::json!(candidate.root_frame);
    candidate.observation["candidate_root_pass"] = serde_json::json!(candidate.root_pass);
    candidate.observation["completed_root_frame"] = serde_json::json!(completed_frame);
    candidate.observation["completed_root_pass"] = serde_json::json!(completed_pass);
    candidate.observation["candidate_root_pass_index"] =
        serde_json::json!(candidate.root_pass_index);
    candidate.observation["frame_complete_valid"] = serde_json::json!(frame_valid);
    candidate.observation["owner_layer_current"] = serde_json::json!(owner_layer_current);
    candidate.observation["response_validation_failures"] = serde_json::json!(response_failures);
    candidate.observation["processed_pointer"] = pointer.map_or(
        serde_json::Value::Null,
        |pointer| serde_json::json!({"x":pointer.x,"y":pointer.y}),
    );
    candidate.observation["pointer_hit"] = click.map_or(serde_json::Value::Null, |click| {
        serde_json::json!({
            "id": click.id.value(),
            "layer": layer_json(click.layer_id),
            "enabled": click.enabled,
            "rect": {"min_x":click.rect.min.x,"min_y":click.rect.min.y,"max_x":click.rect.max.x,"max_y":click.rect.max.y},
            "interact_rect": {"min_x":click.interact_rect.min.x,"min_y":click.interact_rect.min.y,"max_x":click.interact_rect.max.x,"max_y":click.interact_rect.max.y},
        })
    });
    frame_valid
}

impl DuplicateFinderFixtureApp {
    fn new(
        ctx: &egui::Context,
        arguments: FixtureArguments,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let wake_context = ctx.clone();
        let gate = Arc::new(FirstReadCheckpointGate::with_wake(Arc::new(move || {
            wake_context.request_repaint();
        })));
        let mut dialog = DuplicateFinderDialogState::new_with_first_read_checkpoint(ctx, gate);
        for root in arguments.roots {
            dialog
                .add_fixture_root(root)
                .map_err(std::io::Error::other)?;
        }
        dialog.open();
        let actions = Arc::new(FixtureActionSink::default());
        dialog.set_action_sink_for_fixture(actions.clone());
        Ok(Self {
            dialog,
            actions,
            identity: arguments.identity,
            observation_path: arguments.output.join("observation.json"),
            retirement_path: arguments.output.join("retirement.json"),
            worker_joined: false,
            pending_frame: None,
            last_error: None,
        })
    }

    fn write_observation(&self, mut observation: serde_json::Value) {
        observation["fixture_action_sink"] =
            serde_json::json!("recording_only_no_os_actions_or_clipboard");
        observation["fixture_action_sink_calls"] = self.actions.snapshot();
        observation["worker_joined"] = serde_json::json!(self.worker_joined);
        observation["last_error"] = self
            .last_error
            .as_ref()
            .map_or(serde_json::Value::Null, |error| serde_json::json!(error));
        match serde_json::to_vec(&observation) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(&self.observation_path, bytes) {
                    tracing::error!(%error, "duplicate finder fixture observation could not be written");
                }
            }
            Err(error) => {
                tracing::error!(%error, "duplicate finder fixture observation could not be serialized")
            }
        }
    }
}

impl eframe::App for DuplicateFinderFixtureApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let first_ui_pass = ctx.current_pass_index() == 0;
        self.pending_frame = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.dialog.show_with_jump_mode(
                    ui.ctx(),
                    None,
                    Some(DIALOG_INSTANCE),
                    crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                        kind: crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder,
                        instance: DIALOG_INSTANCE,
                    },
                    true,
                    true,
                    first_ui_pass,
                );
            });
        if !self.worker_joined && self.dialog.finish_fixture_close() {
            self.worker_joined = true;
            let mut retirement =
                self.dialog
                    .smoke_observation(&self.identity, &ctx, DIALOG_INSTANCE);
            retirement["worker_joined"] = serde_json::json!(true);
            retirement["fixture_action_sink_calls"] = self.actions.snapshot();
            if let Err(error) = serde_json::to_vec(&retirement)
                .map_err(std::io::Error::other)
                .and_then(|bytes| std::fs::write(&self.retirement_path, bytes))
            {
                self.last_error = Some(format!("write owned retirement receipt: {error}"));
            } else {
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            }
        }
        let mut observation = self
            .dialog
            .smoke_observation(&self.identity, &ctx, DIALOG_INSTANCE);
        let owner_layer = crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder
            .layer_id()
            .expect("Duplicate Finder is a root dialog");
        let responses = self.dialog.smoke_response_candidates();
        observation["candidate_root_frame"] =
            serde_json::json!(ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT));
        observation["candidate_root_pass"] =
            serde_json::json!(ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT));
        observation["candidate_root_pass_index"] = serde_json::json!(ctx.current_pass_index());
        self.pending_frame = Some(PendingFrame {
            dialog_open: self.dialog.open,
            session_generation: self.dialog.smoke_generation(),
            dialog_instance: DIALOG_INSTANCE,
            root_frame: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
            root_pass: ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT),
            root_pass_index: ctx.current_pass_index(),
            owner_layer_current: crate::gui::jump_mode::ui::visible_top_middle_layer(&ctx)
                == Some(owner_layer),
            owner_layer,
            responses,
            observation,
        });
    }

    fn on_ui_frame_complete(&mut self, ctx: &egui::Context) {
        if let Some(mut candidate) = self.pending_frame.take() {
            validate_completed_frame(
                ctx,
                &mut candidate,
                self.dialog.smoke_generation(),
                self.dialog.open,
            );
            self.write_observation(candidate.observation);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.dialog.shutdown();
    }
}

pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = FixtureArguments::parse(arguments)?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 860.0])
            .with_min_inner_size([900.0, 620.0])
            .with_title(WINDOW_TITLE),
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |creation| {
            DuplicateFinderFixtureApp::new(&creation.egui_ctx, arguments)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Id, Pos2, RawInput, Rect, Vec2};

    const TEST_GENERATION: u64 = 7;

    fn rect_json(rect: Rect) -> serde_json::Value {
        serde_json::json!({
            "min_x": rect.min.x,
            "min_y": rect.min.y,
            "max_x": rect.max.x,
            "max_y": rect.max.y,
        })
    }

    fn frame_input(events: Vec<Event>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn draw_fixture_window(
        ctx: &Context,
        pointer: Option<Pos2>,
        cover_target: bool,
        request_discard: bool,
    ) -> (PendingFrame, Option<PendingFrame>, bool) {
        let mut final_candidate = None;
        let mut first_candidate = None;
        let mut in_pass_authorized = false;
        let events = pointer
            .map(|pos| vec![Event::PointerMoved(pos)])
            .unwrap_or_default();
        ctx.run_ui(frame_input(events), |root| {
            let root_frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
            let root_pass = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
            let root_pass_index = ctx.current_pass_index();
            let owner_layer = crate::gui::jump_mode::ui::NavigationDialogKind::DuplicateFinder
                .layer_id()
                .expect("Duplicate Finder has a root layer");
            let start = egui::Window::new("Duplicate Finder")
                .id(Id::new(Some("Duplicate Finder")))
                .collapsible(false)
                .resizable(false)
                .default_pos(Pos2::new(30.0, 30.0))
                .default_size(Vec2::new(260.0, 120.0))
                .show(root.ctx(), |ui| {
                    let response = ui.button("Start");
                    if cover_target {
                        let _ = ui.put(response.rect, egui::Button::new("Cover"));
                    }
                    response
                })
                .expect("test dialog is visible")
                .inner
                .expect("test dialog closure returns its Start response");
            assert_eq!(start.layer_id, owner_layer);
            let response_candidate = SmokeResponseCandidate {
                kind: SmokeResponseKind::Control("start"),
                id: start.id,
                layer_id: start.layer_id,
                rect: start.interact_rect,
                response_rect: start.rect,
                enabled: start.enabled(),
                interaction_eligible: true,
                generation: None,
            };
            let observation = serde_json::json!({
                "frame_id": root_frame,
                "dialog_instance": DIALOG_INSTANCE,
                "session_generation": TEST_GENERATION,
                "responses": {
                    "start": {
                        "id": start.id.value(),
                        "enabled": start.enabled(),
                        "visible": start.interact_rect.is_positive(),
                        "response_current": false,
                        "interaction_eligible": false,
                        "input_hit_eligible": false,
                        "layer": layer_json(start.layer_id),
                        "painted_frame": root_frame,
                        "rect": rect_json(start.interact_rect),
                        "response_rect": rect_json(start.rect),
                        "layer_current": false,
                    }
                },
                "result_actions": []
            });
            let candidate = PendingFrame {
                observation,
                root_frame,
                root_pass,
                root_pass_index,
                dialog_instance: DIALOG_INSTANCE,
                session_generation: TEST_GENERATION,
                dialog_open: true,
                owner_layer,
                owner_layer_current: crate::gui::jump_mode::ui::visible_top_middle_layer(ctx)
                    == Some(owner_layer),
                responses: vec![response_candidate],
            };
            let mut unfinished = candidate.clone();
            let _ = validate_completed_frame(ctx, &mut unfinished, TEST_GENERATION, true);
            in_pass_authorized |= unfinished.observation["responses"]["start"]["response_current"]
                .as_bool()
                .unwrap_or(false)
                || unfinished.observation["responses"]["start"]["input_hit_eligible"]
                    .as_bool()
                    .unwrap_or(false);
            if first_candidate.is_none() {
                first_candidate = Some(candidate.clone());
            }
            if request_discard && root_pass_index == 0 {
                ctx.request_discard("M5 completed-frame receipt test");
            }
            final_candidate = Some(candidate);
        })
        .drop_without_applying_deltas();
        (
            final_candidate.expect("the fixture window was rendered"),
            first_candidate,
            in_pass_authorized,
        )
    }

    #[test]
    fn dff_dialog_completed_response_receipt_rejects_stale_and_covering_widgets() {
        let ctx = Context::default();
        let (discarded_pass_candidate, first, in_pass_authorized) =
            draw_fixture_window(&ctx, None, false, true);
        assert!(
            !in_pass_authorized,
            "an in-progress pass cannot authorize input"
        );
        let mut discarded = first.expect("discard request captured first candidate");
        assert!(
            !validate_completed_frame(&ctx, &mut discarded, TEST_GENERATION, true),
            "a discarded first pass cannot authorize input after the final pass"
        );

        let target = discarded_pass_candidate.responses[0].response_rect.center();
        let (mut current, _, in_pass_authorized) =
            draw_fixture_window(&ctx, Some(target), false, false);
        assert!(
            !in_pass_authorized,
            "the candidate remains pending until frame completion"
        );
        assert!(validate_completed_frame(
            &ctx,
            &mut current,
            TEST_GENERATION,
            true
        ));
        assert!(
            current.observation["responses"]["start"]["response_current"]
                .as_bool()
                .unwrap()
        );
        assert!(
            current.observation["responses"]["start"]["input_hit_eligible"]
                .as_bool()
                .unwrap()
        );

        let mut wrong_generation = current.clone();
        assert!(!validate_completed_frame(
            &ctx,
            &mut wrong_generation,
            TEST_GENERATION + 1,
            true
        ));
        assert!(
            !wrong_generation.observation["responses"]["start"]["response_current"]
                .as_bool()
                .unwrap()
        );
        let mut wrong_owner = current.clone();
        wrong_owner.owner_layer_current = false;
        assert!(validate_completed_frame(
            &ctx,
            &mut wrong_owner,
            TEST_GENERATION,
            true
        ));
        assert!(
            !wrong_owner.observation["responses"]["start"]["response_current"]
                .as_bool()
                .unwrap()
        );

        let mut stale = current.clone();
        let (mut next, _, _) = draw_fixture_window(&ctx, Some(target), false, false);
        assert!(validate_completed_frame(
            &ctx,
            &mut next,
            TEST_GENERATION,
            true
        ));
        assert!(
            !validate_completed_frame(&ctx, &mut stale, TEST_GENERATION, true),
            "a prior completed-frame receipt cannot be reused"
        );

        let _ = draw_fixture_window(&ctx, Some(target), true, false);
        let (mut covered, _, _) = draw_fixture_window(&ctx, Some(target), true, false);
        assert!(validate_completed_frame(
            &ctx,
            &mut covered,
            TEST_GENERATION,
            true
        ));
        assert!(
            covered.observation["responses"]["start"]["response_current"]
                .as_bool()
                .unwrap()
        );
        assert!(
            !covered.observation["responses"]["start"]["input_hit_eligible"]
                .as_bool()
                .unwrap(),
            "a covering same-layer widget refuses the target input"
        );
    }
}
