use image::{Rgba, RgbaImage};

use super::controller::CoordinateRenderFrame;
use super::model::{
    CoordinateSample, CoordinateSpace, CoordinateUnavailable, PhysicalPoint, PhysicalRect,
    PhysicalSize,
};
use super::settings::{CrosshairColor, CrosshairPreferences, HudDetail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn expanded(self, amount: u32) -> Self {
        Self {
            x: self.x.saturating_sub(amount),
            y: self.y.saturating_sub(amount),
            width: self.width.saturating_add(amount.saturating_mul(2)),
            height: self.height.saturating_add(amount.saturating_mul(2)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CrosshairGeometry {
    width: u32,
    height: u32,
    colored_strokes: [PixelRect; 4],
    outline_strokes: Option<[PixelRect; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GuideGeometry {
    pub origin: PhysicalPoint,
    pub width: u32,
    pub height: u32,
    colored_stroke: PixelRect,
    outline: Option<PixelRect>,
}

fn crosshair_geometry(preferences: &CrosshairPreferences) -> CrosshairGeometry {
    let arm = preferences.arm_length.max(2) as u32;
    let thickness = preferences.thickness.clamp(1, 16) as u32;
    let outline = u32::from(preferences.high_contrast_outline);
    let gap = thickness.max(3);
    let extent = arm * 2 + gap;
    let width = extent + outline * 2;
    let height = width;
    let center_x = outline + arm + gap / 2;
    let center_y = center_x;
    let horizontal_y = center_y.saturating_sub(thickness / 2);
    let vertical_x = center_x.saturating_sub(thickness / 2);
    let colored_strokes = [
        PixelRect {
            x: outline,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: outline + arm + gap,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: vertical_x,
            y: outline,
            width: thickness,
            height: arm,
        },
        PixelRect {
            x: vertical_x,
            y: outline + arm + gap,
            width: thickness,
            height: arm,
        },
    ];
    let outline_strokes = preferences
        .high_contrast_outline
        .then(|| colored_strokes.map(|stroke| stroke.expanded(outline)));
    CrosshairGeometry {
        width,
        height,
        colored_strokes,
        outline_strokes,
    }
}

pub(crate) fn crosshair_bitmap(preferences: &CrosshairPreferences) -> RgbaImage {
    let geometry = crosshair_geometry(preferences);
    let mut image = RgbaImage::new(geometry.width, geometry.height);
    let alpha = opacity_alpha(preferences.opacity);
    if let Some(outlines) = geometry.outline_strokes {
        let outline = contrasting_color(preferences.color);
        for stroke in outlines {
            fill_rect(&mut image, stroke, color(outline, alpha));
        }
    }
    for stroke in geometry.colored_strokes {
        fill_rect(&mut image, stroke, color(preferences.color, alpha));
    }
    image
}

pub(crate) fn guide_geometry(
    orientation: GuideOrientation,
    desktop: PhysicalRect,
    cursor: PhysicalPoint,
    preferences: &CrosshairPreferences,
) -> Option<GuideGeometry> {
    let thickness = preferences.thickness.clamp(1, 16) as u32;
    let outline_width = u32::from(preferences.high_contrast_outline);
    let total = thickness + outline_width * 2;
    match orientation {
        GuideOrientation::Horizontal => {
            let width = u32::try_from(desktop.width()).ok()?;
            let y_offset = i32::try_from(total / 2).ok()?;
            let y = cursor.y.checked_sub(y_offset)?;
            Some(GuideGeometry {
                origin: PhysicalPoint::new(desktop.left(), y),
                width,
                height: total,
                colored_stroke: PixelRect {
                    x: 0,
                    y: outline_width,
                    width,
                    height: thickness,
                },
                outline: preferences.high_contrast_outline.then_some(PixelRect {
                    x: 0,
                    y: 0,
                    width,
                    height: total,
                }),
            })
        }
        GuideOrientation::Vertical => {
            let height = u32::try_from(desktop.height()).ok()?;
            let x_offset = i32::try_from(total / 2).ok()?;
            let x = cursor.x.checked_sub(x_offset)?;
            Some(GuideGeometry {
                origin: PhysicalPoint::new(x, desktop.top()),
                width: total,
                height,
                colored_stroke: PixelRect {
                    x: outline_width,
                    y: 0,
                    width: thickness,
                    height,
                },
                outline: preferences.high_contrast_outline.then_some(PixelRect {
                    x: 0,
                    y: 0,
                    width: total,
                    height,
                }),
            })
        }
    }
}

pub(crate) fn guide_bitmap(
    geometry: GuideGeometry,
    preferences: &CrosshairPreferences,
) -> RgbaImage {
    let mut image = RgbaImage::new(geometry.width, geometry.height);
    let alpha = opacity_alpha(preferences.opacity);
    if let Some(outline) = geometry.outline {
        fill_rect(
            &mut image,
            outline,
            color(contrasting_color(preferences.color), alpha),
        );
    }
    fill_rect(
        &mut image,
        geometry.colored_stroke,
        color(preferences.color, alpha),
    );
    image
}

fn fill_rect(image: &mut RgbaImage, rect: PixelRect, value: Rgba<u8>) {
    let right = rect.x.saturating_add(rect.width).min(image.width());
    let bottom = rect.y.saturating_add(rect.height).min(image.height());
    for y in rect.y.min(bottom)..bottom {
        for x in rect.x.min(right)..right {
            image.put_pixel(x, y, value);
        }
    }
}

fn color(color: CrosshairColor, alpha: u8) -> Rgba<u8> {
    Rgba([color.red, color.green, color.blue, alpha])
}

fn opacity_alpha(opacity: f32) -> u8 {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.1, 1.0)
    } else {
        1.0
    };
    (opacity * 255.0).round() as u8
}

fn contrasting_color(color: CrosshairColor) -> CrosshairColor {
    let luminance =
        u32::from(color.red) * 299 + u32::from(color.green) * 587 + u32::from(color.blue) * 114;
    if luminance >= 128_000 {
        CrosshairColor::new(0, 0, 0)
    } else {
        CrosshairColor::new(255, 255, 255)
    }
}

pub(crate) fn hud_lines(frame: &CoordinateRenderFrame) -> Vec<String> {
    let Some(sample) = frame.displayed_sample.as_ref() else {
        let mut lines = vec!["Coordinates unavailable".to_string()];
        if let Some(error) = frame.sample_error.as_deref() {
            lines.push(format!("Sampling: {}", concise_error(error)));
        }
        if frame.preferences.hud_detail == HudDetail::Detailed {
            lines.push("Help: mouse help".into());
        }
        return lines;
    };

    let selected_space = frame.preferences.space;
    let selected = format_coordinate_value(sample, selected_space);
    if frame.preferences.hud_detail == HudDetail::Compact {
        let mut lines = vec![format!("{}: {selected}", space_name(selected_space))];
        if let Some(monitor) = sample.monitor.as_ref() {
            lines.push(format!("Monitor: {}", monitor.id.0));
        }
        if let Some(error) = frame.sample_error.as_deref() {
            lines.push(format!("Sampling: {}", concise_error(error)));
        }
        return lines;
    }

    let mut lines = vec!["Coordinate Inspector".into()];
    lines.push(format!(
        "Selected: {} {selected}",
        space_name(selected_space)
    ));
    lines.push(format!(
        "Desktop: {},{}",
        sample.desktop_point.x, sample.desktop_point.y
    ));
    if let Some(monitor) = sample.monitor.as_ref() {
        lines.push(format!("Monitor: {}", monitor.id.0));
        lines.push(format!(
            "Monitor origin: {},{}",
            monitor.bounds.left(),
            monitor.bounds.top()
        ));
        lines.push(format!(
            "Work area: {},{} .. {},{}",
            monitor.work_area.left(),
            monitor.work_area.top(),
            monitor.work_area.right(),
            monitor.work_area.bottom()
        ));
        if let Some((dpi_x, dpi_y)) = monitor.effective_dpi {
            lines.push(format!("Effective DPI: {dpi_x}x{dpi_y}"));
        } else {
            lines.push("Effective DPI: unavailable".into());
        }
    } else {
        lines.push("Monitor: unavailable".into());
    }
    if let Some(client) = sample.foreground_client {
        lines.push(format!(
            "Client origin: {},{}",
            client.origin.x, client.origin.y
        ));
        if let Some(bounds) = client.bounds {
            lines.push(format!(
                "Client bounds: {},{} .. {},{}",
                bounds.left(),
                bounds.top(),
                bounds.right(),
                bounds.bottom()
            ));
        }
    } else {
        lines.push("Foreground client: unavailable".into());
    }
    if let Some(bounds) = sample.virtual_desktop_bounds {
        lines.push(format!(
            "Virtual desktop: {},{} .. {},{}",
            bounds.left(),
            bounds.top(),
            bounds.right(),
            bounds.bottom()
        ));
    } else {
        lines.push("Virtual desktop: unavailable".into());
    }
    lines.push(format!(
        "Freeze: {}",
        if frame.runtime_state.is_frozen() {
            "on"
        } else {
            "off"
        }
    ));
    if let Some(copied) = frame.runtime_state.last_successful_copy() {
        lines.push(format!(
            "Last copy: {} ({})",
            copied.text,
            space_name(copied.space)
        ));
    } else {
        lines.push("Last copy: none".into());
    }
    if let Some(error) = frame.sample_error.as_deref() {
        lines.push(format!("Sampling: {}", concise_error(error)));
    }
    lines.push("Help: mouse help".into());
    lines
}

pub(crate) fn hud_font_size(effective_dpi_x: Option<u32>) -> u32 {
    let dpi = effective_dpi_x.filter(|dpi| *dpi > 0).unwrap_or(96);
    (14_u32.saturating_mul(dpi).saturating_add(48) / 96).clamp(12, 32)
}

pub(crate) fn hud_dimensions(lines: &[String], font_size: u32) -> (u32, u32) {
    let longest = lines
        .iter()
        .map(|line| line.chars().count().min(96) as u32)
        .max()
        .unwrap_or(1);
    let approximate_char_width = (font_size * 3 + 2) / 5;
    let width = (longest
        .saturating_mul(approximate_char_width)
        .saturating_add(24))
    .max(220);
    let line_height = font_size.saturating_add(5);
    let height = (lines.len() as u32)
        .saturating_mul(line_height)
        .saturating_add(18)
        .max(36);
    (width, height)
}

pub(crate) fn hud_layout(
    lines: &[String],
    preferred_font_size: u32,
    work_area: Option<PhysicalSize>,
) -> (u32, u32, u32) {
    let mut font_size = preferred_font_size.clamp(12, 32);
    let max_width = work_area.map(|area| u32::try_from(area.width()).unwrap_or(u32::MAX).max(1));
    let max_height = work_area.map(|area| u32::try_from(area.height()).unwrap_or(u32::MAX).max(1));

    loop {
        let (width, height) = hud_dimensions(lines, font_size);
        let fits_width = max_width.is_none_or(|limit| width <= limit);
        let fits_height = max_height.is_none_or(|limit| height <= limit);
        if (fits_width && fits_height) || font_size == 12 {
            return (
                font_size,
                max_width.map_or(width, |limit| width.min(limit)),
                max_height.map_or(height, |limit| height.min(limit)),
            );
        }
        font_size -= 1;
    }
}

fn format_coordinate_value(sample: &CoordinateSample, space: CoordinateSpace) -> String {
    match sample.point_in(space) {
        Ok(point) => format!("{},{}", point.x, point.y),
        Err(CoordinateUnavailable::MonitorUnavailable) => "unavailable".into(),
        Err(CoordinateUnavailable::ForegroundClientUnavailable) => "unavailable".into(),
        Err(CoordinateUnavailable::ArithmeticOverflow) => "unavailable (range overflow)".into(),
    }
}

fn space_name(space: CoordinateSpace) -> &'static str {
    match space {
        CoordinateSpace::Desktop => "Desktop",
        CoordinateSpace::Monitor => "Monitor",
        CoordinateSpace::ForegroundClient => "Foreground client",
    }
}

fn concise_error(error: &str) -> String {
    let mut concise: String = error.chars().take(72).collect();
    if error.chars().count() > 72 {
        concise.push_str("...");
    }
    concise
}

#[cfg(test)]
mod tests {
    use super::{CrosshairGeometry, GuideOrientation, crosshair_geometry, guide_geometry};
    use crate::coordinate_tool::controller::CoordinateRenderFrame;
    use crate::coordinate_tool::model::{
        CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, ForegroundClientGeometry,
        MonitorGeometry, MonitorId,
    };
    use crate::coordinate_tool::model::{PhysicalPoint, PhysicalRect, PhysicalSize};
    use crate::coordinate_tool::settings::{CrosshairColor, CrosshairPreferences};

    #[test]
    fn crosshair_geometry_keeps_four_arms_and_outline_around_each_arm() {
        let preferences = CrosshairPreferences::default();
        let CrosshairGeometry {
            width,
            height,
            colored_strokes,
            outline_strokes,
        } = crosshair_geometry(&preferences);

        assert_eq!((width, height), (29, 29));
        assert!(
            colored_strokes
                .iter()
                .all(|stroke| stroke.width > 0 && stroke.height > 0)
        );
        let outlines = outline_strokes.expect("default crosshair has contrast outline");
        for (stroke, outline) in colored_strokes.iter().zip(outlines) {
            assert!(outline.x <= stroke.x);
            assert!(outline.y <= stroke.y);
            assert!(outline.x + outline.width >= stroke.x + stroke.width);
            assert!(outline.y + outline.height >= stroke.y + stroke.height);
        }
    }

    #[test]
    fn guide_geometry_spans_signed_virtual_desktop_with_narrow_bands() {
        let desktop = PhysicalRect::new(-1920, -200, 2560, 1240).unwrap();
        let cursor = PhysicalPoint::new(-640, 333);
        let preferences = CrosshairPreferences::default();

        let horizontal =
            guide_geometry(GuideOrientation::Horizontal, desktop, cursor, &preferences).unwrap();
        assert_eq!(horizontal.origin, PhysicalPoint::new(-1920, 331));
        assert_eq!((horizontal.width, horizontal.height), (4480, 4));

        let vertical =
            guide_geometry(GuideOrientation::Vertical, desktop, cursor, &preferences).unwrap();
        assert_eq!(vertical.origin, PhysicalPoint::new(-642, -200));
        assert_eq!((vertical.width, vertical.height), (4, 1440));
    }

    #[test]
    fn contrast_outline_switches_to_the_opposite_luminance_family() {
        assert_eq!(
            super::contrasting_color(CrosshairColor::new(255, 0, 0)),
            CrosshairColor::new(255, 255, 255)
        );
        assert_eq!(
            super::contrasting_color(CrosshairColor::new(255, 255, 255)),
            CrosshairColor::new(0, 0, 0)
        );
    }

    #[test]
    fn detailed_hud_reports_origins_freeze_copy_and_help_context() {
        let monitor = PhysicalRect::new(-1920, 0, 0, 1080).unwrap();
        let sample = CoordinateSample::new(
            PhysicalPoint::new(-1732, 215),
            Some(PhysicalRect::new(-1920, 0, 3840, 2160).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY_LEFT"),
                bounds: monitor,
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((144, 144)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        );
        let mut runtime_state = CoordinateToolRuntimeState::default();
        runtime_state.freeze(&sample);
        runtime_state.record_successful_copy(
            super::super::model::format_coordinate(&sample, CoordinateSpace::Desktop).unwrap(),
        );
        let frame = CoordinateRenderFrame {
            preferences: crate::coordinate_tool::CoordinateToolPreferences {
                space: CoordinateSpace::ForegroundClient,
                hud_detail: crate::coordinate_tool::HudDetail::Detailed,
                ..Default::default()
            },
            runtime_state,
            current_sample: Some(sample.clone()),
            displayed_sample: Some(sample.clone()),
            placement_sample: Some(sample.clone()),
            sample_error: None,
        };

        let lines = super::hud_lines(&frame).join("\n");
        assert!(lines.contains("Selected: Foreground client 68,175"));
        assert!(lines.contains("Monitor: DISPLAY_LEFT"));
        assert!(lines.contains("Monitor origin: -1920,0"));
        assert!(lines.contains("Client origin: -1800,40"));
        assert!(lines.contains("Freeze: on"));
        assert!(lines.contains("Last copy: -1732,215 (Desktop)"));
        assert!(lines.contains("Help: mouse help"));
        assert_eq!(super::hud_font_size(Some(144)), 21);
        assert_eq!(super::hud_font_size(None), 14);
    }

    #[test]
    fn high_dpi_frozen_hud_fits_sampling_error_and_final_help_row_in_work_area() {
        let sample = CoordinateSample::new(
            PhysicalPoint::new(-1732, 215),
            Some(PhysicalRect::new(-1920, 0, 1920, 2160).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY_LEFT"),
                bounds: PhysicalRect::new(-1920, 0, 0, 1080).unwrap(),
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((288, 288)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        );
        let mut runtime_state = CoordinateToolRuntimeState::default();
        runtime_state.freeze(&sample);
        runtime_state.record_successful_copy(
            super::super::model::format_coordinate(&sample, CoordinateSpace::Desktop).unwrap(),
        );
        let frame = CoordinateRenderFrame {
            preferences: crate::coordinate_tool::CoordinateToolPreferences {
                hud_detail: crate::coordinate_tool::HudDetail::Detailed,
                ..Default::default()
            },
            runtime_state,
            current_sample: Some(sample.clone()),
            displayed_sample: Some(sample.clone()),
            placement_sample: Some(sample),
            sample_error: Some("sample backend is temporarily unavailable".into()),
        };
        let lines = super::hud_lines(&frame);
        assert!(lines.iter().any(|line| line.starts_with("Sampling: ")));
        assert_eq!(lines.last().map(String::as_str), Some("Help: mouse help"));

        let uncapped = super::hud_layout(&lines, 32, None);
        assert!(uncapped.2 > 480);

        let work_area = PhysicalSize::new(1920, 520).unwrap();
        let (font_size, width, height) = super::hud_layout(&lines, 32, Some(work_area));
        assert!(
            font_size < 32,
            "font should shrink to fit the work-area height"
        );
        assert!(width <= 1920);
        assert!(
            width > 640,
            "long signed coordinates and sampling details must not be clipped by a fixed width cap"
        );
        assert!(height <= 520);
        let last_row_bottom =
            9 + u32::try_from(lines.len() - 1).unwrap() * (font_size + 5) + font_size;
        assert!(
            last_row_bottom <= height,
            "final HUD row must be fully inside the allocated surface"
        );
    }
}
