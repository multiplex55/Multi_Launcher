use crate::actions::Action;
use crate::plugin::Plugin;
use std::sync::{Arc, Mutex};
use reqwest::blocking::Client;

#[derive(Default)]
pub struct VoiceEngine {
    pub sensitivity: f32,
    listening: bool,
    last_result: Option<String>,
}

impl VoiceEngine {
    pub fn new(sensitivity: f32) -> Self {
        Self {
            sensitivity,
            listening: false,
            last_result: None,
        }
    }

    pub fn start_listening(&mut self) {
        if self.listening {
            return;
        }
        self.listening = true;
        // Example request to a local Vosk/Whisper server. In a real
        // implementation microphone audio would be streamed to the server
        // for transcription and the returned text stored as the query.
        let client = Client::new();
        let _ = client
            .post("http://localhost:2700")
            .body(Vec::new())
            .send()
            .ok()
            .and_then(|resp| resp.text().ok())
            .map(|text| self.last_result = Some(text));
    }

    pub fn stop_listening(&mut self) {
        self.listening = false;
    }

    pub fn take_result(&mut self) -> Option<String> {
        self.listening = false;
        self.last_result.take()
    }
}

pub struct VoicePlugin {
    engine: Arc<Mutex<VoiceEngine>>,
}

impl VoicePlugin {
    pub fn new(engine: Arc<Mutex<VoiceEngine>>) -> Self {
        Self { engine }
    }
}

impl Plugin for VoicePlugin {
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "voice"
    }

    fn description(&self) -> &str {
        "Capture audio and convert speech to queries"
    }

    fn capabilities(&self) -> &[&str] {
        &[]
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Some(s) = value.get("sensitivity").and_then(|v| v.as_f64()) {
            if let Ok(mut eng) = self.engine.lock() {
                eng.sensitivity = s as f32;
            }
        }
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        self.engine
            .lock()
            .ok()
            .map(|eng| serde_json::json!({"sensitivity": eng.sensitivity}))
    }
}
