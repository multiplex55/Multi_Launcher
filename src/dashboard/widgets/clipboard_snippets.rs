use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::clipboard::{load_history, CLIPBOARD_FILE};
use crate::plugins::snippets::{load_snippets, SNIPPETS_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};

fn default_clipboard_count() -> usize {
    5
}

fn default_snippet_count() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardSnippetsConfig {
    #[serde(default = "default_clipboard_count")]
    pub clipboard_count: usize,
    #[serde(default = "default_snippet_count")]
    pub snippet_count: usize,
    #[serde(default)]
    pub show_system: bool,
}

impl Default for ClipboardSnippetsConfig {
    fn default() -> Self {
        Self {
            clipboard_count: default_clipboard_count(),
            snippet_count: default_snippet_count(),
            show_system: true,
        }
    }
}

pub struct ClipboardSnippetsWidget {
    cfg: ClipboardSnippetsConfig,
    cached_history: Vec<String>,
    cached_snippets: Vec<crate::plugins::snippets::SnippetEntry>,
    last_clipboard_version: u64,
    last_snippets_version: u64,
}

impl ClipboardSnippetsWidget {
    pub fn new(cfg: ClipboardSnippetsConfig) -> Self {
        Self {
            cfg,
            cached_history: Vec::new(),
            cached_snippets: Vec::new(),
            last_clipboard_version: u64::MAX,
            last_snippets_version: u64::MAX,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut ClipboardSnippetsConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Clipboard items");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.clipboard_count).clamp_range(1..=50))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Snippets");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.snippet_count).clamp_range(0..=50))
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut cfg.show_system, "Show system snapshot")
                    .changed();
                changed
            },
        )
    }

    fn shorten(text: &str, len: usize) -> String {
        let trimmed = text.trim();
        if trimmed.len() > len {
            format!("{}…", &trimmed[..len])
        } else {
            trimmed.to_string()
        }
    }

    fn system_snapshot() -> Option<(f32, f32, f32)> {
        let mut system = System::new_all();
        system.refresh_cpu_usage();
        system.refresh_memory();
        let disks = Disks::new_with_refreshed_list();

        let cpu = system.global_cpu_usage();
        let total_mem = system.total_memory() as f32;
        let used_mem = system.used_memory() as f32;
        let mem = if total_mem > 0.0 {
            used_mem / total_mem * 100.0
        } else {
            0.0
        };

        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;
        for d in disks.list() {
            total_disk += d.total_space();
            avail_disk += d.available_space();
        }
        let disk = if total_disk > 0 {
            (total_disk.saturating_sub(avail_disk)) as f32 / total_disk as f32 * 100.0
        } else {
            0.0
        };
        Some((cpu, mem, disk))
    }

    fn refresh_data(&mut self, ctx: &DashboardContext<'_>) {
        if self.last_clipboard_version != ctx.clipboard_version {
            self.cached_history = load_history(CLIPBOARD_FILE)
                .unwrap_or_default()
                .into_iter()
                .collect();
            self.last_clipboard_version = ctx.clipboard_version;
        }
        if self.last_snippets_version != ctx.snippets_version {
            self.cached_snippets = load_snippets(SNIPPETS_FILE).unwrap_or_default();
            self.last_snippets_version = ctx.snippets_version;
        }
    }
}

impl Default for ClipboardSnippetsWidget {
    fn default() -> Self {
        Self {
            cfg: ClipboardSnippetsConfig::default(),
            cached_history: Vec::new(),
            cached_snippets: Vec::new(),
            last_clipboard_version: u64::MAX,
            last_snippets_version: u64::MAX,
        }
    }
}

impl Widget for ClipboardSnippetsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_data(ctx);
        let mut clicked = None;
        if !self.cached_history.is_empty() {
            ui.label("Clipboard");
            for (idx, entry) in self
                .cached_history
                .iter()
                .enumerate()
                .take(self.cfg.clipboard_count)
            {
                if ui
                    .button(Self::shorten(entry, 60))
                    .on_hover_text(entry)
                    .clicked()
                {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: "Copy from clipboard history".into(),
                            desc: "Clipboard".into(),
                            action: format!("clipboard:copy:{idx}"),
                            args: None,
                        },
                        query_override: Some("cb list".into()),
                    });
                }
            }
        }

        if self.cfg.snippet_count > 0 && !self.cached_snippets.is_empty() {
            ui.separator();
            ui.label("Snippets");
            for snippet in self.cached_snippets.iter().take(self.cfg.snippet_count) {
                if ui
                    .button(format!(
                        "{} — {}",
                        snippet.alias,
                        Self::shorten(&snippet.text, 40)
                    ))
                    .on_hover_text(&snippet.text)
                    .clicked()
                {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: snippet.alias.clone(),
                            desc: "Snippet".into(),
                            action: format!("clipboard:{}", snippet.text),
                            args: None,
                        },
                        query_override: Some(format!("cs {}", snippet.alias)),
                    });
                }
            }
        }

        if self.cfg.show_system {
            if let Some((cpu, mem, disk)) = Self::system_snapshot() {
                ui.separator();
                ui.label("System snapshot");
                ui.label(format!("CPU: {:.0}%", cpu));
                ui.label(format!("Mem: {:.0}%", mem));
                ui.label(format!("Disk: {:.0}%", disk));
            }
        }

        clicked
    }
}
