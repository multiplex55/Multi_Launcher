use eframe::egui::{Color32, Pos2, Rect};
use image::RgbaImage;
use multi_launcher::gui::{
    render_markup_layers, MarkupArrow, MarkupHistory, MarkupLayer, MarkupRect, MarkupStroke,
};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::screenshot::ScreenshotPlugin;

#[test]
fn search_lists_screenshot_actions() {
    let plugin = ScreenshotPlugin;
    let results = plugin.search("ss");
    assert!(!results.is_empty());
    let prefixes = [
        "screenshot:window",
        "screenshot:region",
        "screenshot:region_markup",
        "screenshot:desktop",
        "screenshot:window_clip",
        "screenshot:region_clip",
        "screenshot:desktop_clip",
    ];
    for prefix in prefixes.iter() {
        assert!(
            results.iter().any(|a| a.action == *prefix),
            "missing action {}",
            prefix
        );
    }
}

#[test]
fn markup_layers_render_on_image() {
    let base = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
    let stroke = MarkupLayer::Stroke(MarkupStroke {
        points: vec![Pos2::new(1.0, 1.0), Pos2::new(1.0, 8.0)],
        color: Color32::from_rgb(255, 0, 0),
        thickness: 2.0,
    });
    let rect = MarkupLayer::Rectangle(MarkupRect {
        rect: Rect::from_min_max(Pos2::new(3.0, 3.0), Pos2::new(8.0, 8.0)),
        color: Color32::from_rgb(0, 0, 255),
        thickness: 1.0,
    });
    let arrow = MarkupLayer::Arrow(MarkupArrow {
        start: Pos2::new(8.0, 1.0),
        end: Pos2::new(2.0, 1.0),
        color: Color32::from_rgb(0, 255, 0),
        thickness: 1.0,
    });
    let rendered = render_markup_layers(&base, &[stroke, rect, arrow]);
    assert_ne!(rendered.get_pixel(1, 5).0, [255, 255, 255, 255]);
    assert_ne!(rendered.get_pixel(3, 3).0, [255, 255, 255, 255]);
    assert_ne!(rendered.get_pixel(6, 1).0, [255, 255, 255, 255]);
}

#[test]
fn markup_history_undo_redo() {
    let mut history = MarkupHistory::default();
    let layer_a = MarkupLayer::Rectangle(MarkupRect {
        rect: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(2.0, 2.0)),
        color: Color32::from_rgb(0, 0, 0),
        thickness: 1.0,
    });
    let layer_b = MarkupLayer::Highlight(MarkupRect {
        rect: Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(4.0, 4.0)),
        color: Color32::from_rgba_unmultiplied(255, 255, 0, 96),
        thickness: 1.0,
    });
    history.push(layer_a.clone());
    history.push(layer_b.clone());
    assert_eq!(history.layers().len(), 2);
    assert!(history.undo());
    assert_eq!(history.layers().len(), 1);
    assert!(history.redo());
    assert_eq!(history.layers().len(), 2);
    assert_eq!(history.layers()[0], layer_a);
    assert_eq!(history.layers()[1], layer_b);
}
