//! Opt-in native fixture for the production image comparison dialog.
//!
//! It loads only explicitly supplied generated inputs and publishes redacted
//! current-response geometry so an owned native harness can exercise the real
//! mode and wipe controls. It never reads the system clipboard or captures the
//! desktop.

use super::ImageCompareDialogState;
use eframe::egui;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

const WINDOW_TITLE: &str = "Image Comparison M7-B Fixture";
const DIALOG_INSTANCE: u64 = 0x4D37_B001;

struct FixtureArguments {
    fixture_identity: String,
    input_a: PathBuf,
    input_b: PathBuf,
    output_dir: PathBuf,
}

impl FixtureArguments {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut fixture_identity = None;
        let mut input_a = None;
        let mut input_b = None;
        let mut output_dir = None;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {argument}"))?;
            match argument.as_str() {
                "--fixture-identity" => fixture_identity = Some(value),
                "--a" => input_a = Some(PathBuf::from(value)),
                "--b" => input_b = Some(PathBuf::from(value)),
                "--out" => output_dir = Some(PathBuf::from(value)),
                _ => {
                    return Err(format!(
                        "unsupported image comparison fixture option {argument}"
                    ));
                }
            }
        }
        let fixture_identity = fixture_identity
            .filter(|identity| !identity.trim().is_empty())
            .ok_or_else(|| "--fixture-identity is required".to_owned())?;
        let input_a = input_a.ok_or_else(|| "--a generated input is required".to_owned())?;
        let input_b = input_b.ok_or_else(|| "--b generated input is required".to_owned())?;
        let output_dir =
            output_dir.ok_or_else(|| "--out owned output directory is required".to_owned())?;
        if !input_a.is_file() || !input_b.is_file() {
            return Err("both image inputs must be existing generated files".into());
        }
        std::fs::create_dir_all(&output_dir)
            .map_err(|error| format!("create owned observation directory: {error}"))?;
        Ok(Self {
            fixture_identity,
            input_a,
            input_b,
            output_dir,
        })
    }
}

struct ImageCompareFixtureApp {
    dialog: ImageCompareDialogState,
    fixture_identity: String,
    observation_path: PathBuf,
    output_dir: PathBuf,
    captures: Vec<serde_json::Value>,
    pending_capture: Option<PendingCapture>,
    closing_after_captures: bool,
    exit_after_retirement: bool,
    last_error: Option<String>,
}

struct PendingCapture {
    index: u8,
    view: String,
    wipe_fraction: f32,
    requested_frame: u64,
}

impl ImageCompareFixtureApp {
    fn new(
        ctx: &egui::Context,
        arguments: FixtureArguments,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut dialog = ImageCompareDialogState::new(ctx);
        dialog
            .load_smoke_inputs(arguments.input_a, arguments.input_b)
            .map_err(std::io::Error::other)?;
        Ok(Self {
            dialog,
            fixture_identity: arguments.fixture_identity,
            observation_path: arguments.output_dir.join("observation.json"),
            output_dir: arguments.output_dir,
            captures: Vec::new(),
            pending_capture: None,
            closing_after_captures: false,
            exit_after_retirement: false,
            last_error: None,
        })
    }

    fn write_observation(&self, ctx: &egui::Context) {
        let mut observation = self.dialog.smoke_observation(ctx, &self.fixture_identity);
        observation["captures"] = serde_json::Value::Array(self.captures.clone());
        observation["pending_capture"] =
            self.pending_capture
                .as_ref()
                .map_or(serde_json::Value::Null, |capture| {
                    serde_json::json!({
                        "index": capture.index,
                        "view": capture.view,
                        "wipe_fraction": capture.wipe_fraction,
                        "requested_frame": capture.requested_frame,
                    })
                });
        observation["last_error"] = self
            .last_error
            .as_ref()
            .map_or(serde_json::Value::Null, |error| serde_json::json!(error));
        match serde_json::to_vec(&observation) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(&self.observation_path, bytes) {
                    tracing::error!(%error, "image comparison smoke observation could not be written");
                }
            }
            Err(error) => {
                tracing::error!(%error, "image comparison smoke observation could not be serialized")
            }
        }
    }

    fn consume_screenshot(&mut self, user_data: egui::UserData, image: Arc<egui::ColorImage>) {
        let Some(index) = user_data
            .data
            .as_ref()
            .and_then(|value| value.downcast_ref::<u8>())
            .copied()
        else {
            self.last_error = Some("screenshot reply had no fixture request identity".into());
            return;
        };
        let Some(pending) = self
            .pending_capture
            .take()
            .filter(|pending| pending.index == index)
        else {
            self.last_error =
                Some("screenshot reply did not match the pending fixture request".into());
            return;
        };
        let Some((&width, &height)) = image.size.first().zip(image.size.get(1)) else {
            self.last_error = Some("screenshot reply had invalid dimensions".into());
            return;
        };
        let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
            self.last_error = Some("screenshot dimensions exceeded the PNG format bounds".into());
            return;
        };
        let rgba = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect::<Vec<_>>();
        let Some(bitmap) = image::RgbaImage::from_raw(width, height, rgba) else {
            self.last_error = Some("screenshot reply had an inconsistent RGBA buffer".into());
            return;
        };
        let file_name = format!("capture-{:03}.png", index + 1);
        let path = self.output_dir.join(&file_name);
        if let Err(error) = bitmap.save(&path) {
            self.last_error = Some(format!("write owned framebuffer image: {error}"));
            return;
        }
        let digest = match std::fs::read(&path) {
            Ok(bytes) => hex::encode(Sha256::digest(bytes)),
            Err(error) => {
                self.last_error = Some(format!("read back owned framebuffer image: {error}"));
                return;
            }
        };
        self.captures.push(serde_json::json!({
            "index": index,
            "file": file_name,
            "sha256": digest,
            "width": width,
            "height": height,
            "view": pending.view,
            "wipe_fraction": pending.wipe_fraction,
            "requested_frame": pending.requested_frame,
        }));
        self.last_error = None;
        if self.captures.len() == 3 {
            self.closing_after_captures = true;
        }
    }

    fn request_screenshot(&mut self, ctx: &egui::Context) {
        if self.pending_capture.is_some() || self.captures.len() >= 3 {
            return;
        }
        let observation = self.dialog.smoke_observation(ctx, &self.fixture_identity);
        if observation["presentation_current"] != true {
            self.last_error =
                Some("capture requires the current fixture image pair to be presented".into());
            return;
        }
        let Some(view) = observation["view"].as_str() else {
            self.last_error = Some("current view was not reported".into());
            return;
        };
        if view != "wipe" && self.captures.len() < 2 {
            self.last_error = Some("capture the A and B wipe endpoints before Difference".into());
            return;
        }
        if self.captures.len() == 2 && view != "difference" {
            self.last_error = Some("the third capture must show Difference".into());
            return;
        }
        if self.captures.len() < 2 {
            let expected_fraction = if self.captures.is_empty() { 0.0 } else { 1.0 };
            let actual_fraction = observation["wipe_fraction"].as_f64().unwrap_or(f64::NAN);
            if actual_fraction != expected_fraction {
                self.last_error = Some(format!(
                    "wipe endpoint capture expected fraction {expected_fraction}, got {actual_fraction}"
                ));
                return;
            }
        }
        let index = self.captures.len() as u8;
        let wipe_fraction = observation["wipe_fraction"].as_f64().unwrap_or(0.5) as f32;
        let requested_frame = observation["frame_id"].as_u64().unwrap_or_default();
        self.pending_capture = Some(PendingCapture {
            index,
            view: view.to_owned(),
            wipe_fraction,
            requested_frame,
        });
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Screenshot(egui::UserData::new(index)),
        );
        self.last_error = None;
    }
}

impl eframe::App for ImageCompareFixtureApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let first_ui_pass = ctx.current_pass_index() == 0;
        if first_ui_pass {
            let events = ctx.input(|input| input.raw.events.clone());
            for event in events {
                if let egui::Event::Screenshot {
                    viewport_id,
                    user_data,
                    image,
                } = event
                    && viewport_id == egui::ViewportId::ROOT
                {
                    self.consume_screenshot(user_data, image);
                }
            }
            if ctx.input(|input| input.key_pressed(egui::Key::F11)) {
                self.request_screenshot(&ctx);
            }
            if ctx.input(|input| input.key_pressed(egui::Key::Escape))
                && self.closing_after_captures
            {
                self.dialog.close();
                self.exit_after_retirement = true;
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.dialog.show_with_jump_mode(
                    ui.ctx(),
                    None,
                    Some(DIALOG_INSTANCE),
                    crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                        kind: crate::gui::jump_mode::ui::NavigationDialogKind::ImageCompare,
                        instance: DIALOG_INSTANCE,
                    },
                    true,
                    true,
                    first_ui_pass,
                );
                self.dialog.paint_retirement_fence(ui.painter());
            });
        let retirement = self.dialog.smoke_retirement_observation();
        if self.exit_after_retirement
            && retirement["budget_used_bytes"] == 0
            && retirement["retired_uploads"] == 0
            && retirement["session_closed"] == true
            && retirement["presented_pair_retained"] == false
        {
            let mut retirement = retirement;
            retirement["fixture_identity"] = serde_json::json!(self.fixture_identity);
            retirement["capture_count"] = serde_json::json!(self.captures.len());
            retirement["process_id"] = serde_json::json!(std::process::id());
            retirement["frame_id"] = serde_json::json!(ctx.cumulative_frame_nr());
            retirement["captures"] = serde_json::Value::Array(self.captures.clone());
            if let Err(error) = serde_json::to_vec(&retirement)
                .map_err(std::io::Error::other)
                .and_then(|bytes| std::fs::write(self.output_dir.join("retirement.json"), bytes))
            {
                self.last_error = Some(format!("write retirement receipt: {error}"));
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
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title(WINDOW_TITLE),
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |creation| {
            ImageCompareFixtureApp::new(&creation.egui_ctx, arguments)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|error| error.to_string())
}
