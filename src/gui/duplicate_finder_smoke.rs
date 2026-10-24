//! Owned native fixture for the production Duplicate Finder dialog.
//!
//! The fixture accepts only caller-supplied generated roots. It performs no
//! scan until the real Start response is clicked and routes result actions to
//! an inert recording sink rather than Explorer or the system clipboard.

use super::duplicate_finder_dialog::{
    CandidateProof, DuplicateFinderActions, DuplicateFinderDialogState,
};
use crate::duplicate_finder::FirstReadCheckpointGate;
use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const WINDOW_TITLE: &str = "Duplicate Finder M11-C Fixture";
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
    last_error: Option<String>,
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
            last_error: None,
        })
    }

    fn write_observation(&self, ctx: &egui::Context) {
        let mut observation = self
            .dialog
            .smoke_observation(&self.identity, ctx, DIALOG_INSTANCE);
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
        self.write_observation(&ctx);
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
