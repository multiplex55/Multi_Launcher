//! Shared, side-effect-free OCR editor widgets. Capability lookup and test OCR
//! are separate explicit jobs; rendering these controls never calls a backend.

use crate::mkmacro::*;
use eframe::egui;

pub fn search_ui(ui: &mut egui::Ui, search: &mut MkOcrSearchSpec) {
    search_match_ui(ui, search);
    language_ui(ui, &mut search.language);
    region_ui(ui, &mut search.region);
}

/// Search fields whose capability-backed language and region controls are
/// owned by the action editor host.
pub fn search_match_ui(ui: &mut egui::Ui, search: &mut MkOcrSearchSpec) {
    ui.label("Text or template");
    ui.text_edit_singleline(&mut search.text);
    egui::ComboBox::from_label("Match")
        .selected_text(format!("{:?}", search.match_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut search.match_mode, MkOcrMatchMode::Contains, "Contains");
            ui.selectable_value(
                &mut search.match_mode,
                MkOcrMatchMode::WholeWordPhrase,
                "Whole word phrase",
            );
            ui.selectable_value(
                &mut search.match_mode,
                MkOcrMatchMode::Regex,
                "Regular expression",
            );
        });
    ui.checkbox(&mut search.case_sensitive, "Case sensitive");
    let mut nth = match search.occurrence {
        MkOcrOccurrence::First => 1,
        MkOcrOccurrence::Nth(n) => n,
    };
    ui.horizontal(|ui| {
        ui.label("Occurrence");
        ui.add(egui::DragValue::new(&mut nth).clamp_range(1..=u32::MAX));
    });
    search.occurrence = if nth == 1 {
        MkOcrOccurrence::First
    } else {
        MkOcrOccurrence::Nth(nth)
    };
}

pub fn read_ui(ui: &mut egui::Ui, read: &mut MkOcrReadPayload) {
    language_ui(ui, &mut read.language);
    region_ui(ui, &mut read.region);
    read_output_ui(ui, read);
}

pub fn read_output_ui(ui: &mut egui::Ui, read: &mut MkOcrReadPayload) {
    ui.horizontal(|ui| {
        ui.label("Output variable");
        ui.text_edit_singleline(&mut read.output_variable);
    });
}

pub fn installed_language_ui(
    ui: &mut egui::Ui,
    language: &mut MkOcrLanguage,
    languages: &[OcrLanguageInfo],
) {
    let selected = match language {
        MkOcrLanguage::Auto => "Auto (Windows profile)".to_owned(),
        MkOcrLanguage::LanguageTag(tag) => languages
            .iter()
            .find(|candidate| candidate.tag.eq_ignore_ascii_case(tag))
            .map(|candidate| format!("{} ({})", candidate.display_name, candidate.tag))
            .unwrap_or_else(|| tag.clone()),
    };
    egui::ComboBox::from_label("Installed OCR language")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(language, MkOcrLanguage::Auto, "Auto (Windows profile)");
            for candidate in languages {
                ui.selectable_value(
                    language,
                    MkOcrLanguage::LanguageTag(candidate.tag.clone()),
                    format!("{} ({})", candidate.display_name, candidate.tag),
                );
            }
        });
}

pub fn preview_ui(
    ui: &mut egui::Ui,
    preview: &super::ocr_test_job::OcrAuthoringPreview,
    texture_key: &str,
    texture_cache: &mut Option<egui::TextureHandle>,
) {
    let capture = &preview.recognized.capture;
    let size = [
        capture.image.width() as usize,
        capture.image.height() as usize,
    ];
    let texture = texture_cache.get_or_insert_with(|| {
        let color = egui::ColorImage::from_rgba_unmultiplied(size, capture.image.as_raw());
        ui.ctx().load_texture(
            format!("ocr-preview-{texture_key}"),
            color,
            egui::TextureOptions::LINEAR,
        )
    });
    let available = ui.available_width().max(1.0);
    let scale = (available / capture.image.width().max(1) as f32).min(1.0);
    let display_size = egui::vec2(
        capture.image.width() as f32 * scale,
        capture.image.height() as f32 * scale,
    );
    let response = ui.add(egui::Image::new((texture.id(), display_size)));
    let to_screen_rect = |bounds: ScreenRect| {
        let x = (bounds.x - capture.origin.0) as f32 * scale;
        let y = (bounds.y - capture.origin.1) as f32 * scale;
        egui::Rect::from_min_size(
            response.rect.min + egui::vec2(x, y),
            egui::vec2(bounds.width as f32 * scale, bounds.height as f32 * scale),
        )
    };
    let painter = ui.painter();
    for line in &preview.recognized.document.lines {
        if let Some(bounds) = line.bounds() {
            painter.rect_stroke(
                to_screen_rect(bounds),
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::YELLOW),
            );
        }
        for word in &line.words {
            painter.rect_stroke(
                to_screen_rect(word.bounds),
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(80, 190, 255)),
            );
        }
    }
    if let Some(selected) = preview
        .search
        .as_ref()
        .and_then(|result| result.selected.as_ref())
    {
        painter.rect_stroke(
            to_screen_rect(selected.bounds),
            0.0,
            egui::Stroke::new(3.0_f32, egui::Color32::GREEN),
        );
    }
    ui.label(format!(
        "Recognized language: {}",
        preview
            .recognized
            .document
            .recognized_language
            .as_deref()
            .unwrap_or("unknown")
    ));
    if let Some(search) = &preview.search {
        ui.label(format!("Matches: {}", search.match_count));
        if let Some(selected) = &search.selected {
            ui.label(format!(
                "Selected #{} at ({}, {}): {}",
                selected.occurrence, selected.center.x, selected.center.y, selected.text
            ));
        } else {
            ui.label("No selected match");
        }
    }
    ui.label("Recognized text");
    let mut text = preview.recognized.document.recognized_text();
    ui.add(
        egui::TextEdit::multiline(&mut text)
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
}

fn language_ui(ui: &mut egui::Ui, language: &mut MkOcrLanguage) {
    let mut explicit = matches!(language, MkOcrLanguage::LanguageTag(_));
    ui.horizontal(|ui| {
        ui.checkbox(&mut explicit, "Explicit language");
        if explicit {
            let tag = match language {
                MkOcrLanguage::LanguageTag(tag) => tag,
                MkOcrLanguage::Auto => {
                    *language = MkOcrLanguage::LanguageTag("en-US".into());
                    let MkOcrLanguage::LanguageTag(tag) = language else {
                        unreachable!()
                    };
                    tag
                }
            };
            ui.text_edit_singleline(tag);
        } else {
            *language = MkOcrLanguage::Auto;
            ui.label("Auto (Windows profile)");
        }
    });
}

pub fn region_ui(ui: &mut egui::Ui, region: &mut SearchRegion) {
    let mut kind = match region {
        SearchRegion::Desktop => 0,
        SearchRegion::Monitor { .. } => 1,
        SearchRegion::Rectangle { .. } => 2,
        SearchRegion::Window { .. } => 3,
        SearchRegion::ClientArea { .. } => 4,
    };
    egui::ComboBox::from_label("Region")
        .selected_text(["Desktop", "Monitor", "Rectangle", "Window", "Client area"][kind])
        .show_ui(ui, |ui| {
            for (index, label) in ["Desktop", "Monitor", "Rectangle", "Window", "Client area"]
                .into_iter()
                .enumerate()
            {
                ui.selectable_value(&mut kind, index, label);
            }
        });
    let old = match region {
        SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
            Some(matcher.clone())
        }
        _ => None,
    };
    if kind
        != match region {
            SearchRegion::Desktop => 0,
            SearchRegion::Monitor { .. } => 1,
            SearchRegion::Rectangle { .. } => 2,
            SearchRegion::Window { .. } => 3,
            SearchRegion::ClientArea { .. } => 4,
        }
    {
        *region = match kind {
            0 => SearchRegion::Desktop,
            1 => SearchRegion::Monitor { index: 0 },
            2 => SearchRegion::Rectangle {
                rect: ScreenRect::new(0, 0, 640, 480),
            },
            3 => SearchRegion::Window {
                matcher: old.unwrap_or_else(default_matcher),
            },
            _ => SearchRegion::ClientArea {
                matcher: old.unwrap_or_else(default_matcher),
            },
        };
    }
    match region {
        SearchRegion::Monitor { index } => {
            ui.horizontal(|ui| {
                ui.label("Monitor index");
                ui.add(egui::DragValue::new(index));
            });
        }
        SearchRegion::Rectangle { rect } => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut rect.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut rect.y));
                ui.label("W");
                ui.add(egui::DragValue::new(&mut rect.width).clamp_range(1..=u32::MAX));
                ui.label("H");
                ui.add(egui::DragValue::new(&mut rect.height).clamp_range(1..=u32::MAX));
            });
        }
        SearchRegion::Window { matcher } | SearchRegion::ClientArea { matcher } => {
            ui.label("Window matcher");
            option(ui, "Process", &mut matcher.process);
            option(ui, "Title", &mut matcher.title);
            option(ui, "Title regex", &mut matcher.title_regex);
            option(ui, "Class", &mut matcher.class);
        }
        SearchRegion::Desktop => {}
    }
}

fn default_matcher() -> MkWindowMatcher {
    MkWindowMatcher {
        title: Some("Window".into()),
        ..Default::default()
    }
}
fn option(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let value = value.get_or_insert_with(String::new);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
    if value.trim().is_empty() {
        *value = String::new();
    }
}
