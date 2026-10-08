use image::{Rgba, RgbaImage};

use super::controller::CoordinateRenderFrame;
use super::model::{
    CoordinateSample, CoordinateSpace, CoordinateUnavailable, PhysicalPoint, PhysicalRect,
    PhysicalSize,
};
use super::settings::{CrosshairColor, CrosshairPreferences, HaloPreferences, HudDetail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn expanded(self, amount: u32) -> Self {
        let expansion = amount
            .checked_mul(2)
            .expect("bounded pixel rectangle expansion fits u32");
        Self {
            x: self
                .x
                .checked_sub(amount)
                .expect("crosshair outline remains inside its bitmap"),
            y: self
                .y
                .checked_sub(amount)
                .expect("crosshair outline remains inside its bitmap"),
            width: self
                .width
                .checked_add(expansion)
                .expect("bounded pixel rectangle width fits u32"),
            height: self
                .height
                .checked_add(expansion)
                .expect("bounded pixel rectangle height fits u32"),
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

/// Physical desktop geometry for a circular halo centered on the sampled
/// cursor hotspot. Source and destination have identical extents at 1x.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HaloGeometry {
    pub origin: PhysicalPoint,
    pub source: PhysicalRect,
    pub diameter: i32,
}

/// Five-by-five Magnification color matrix and equivalent RGB pixel blend.
/// Windows applies input channel vectors across matrix rows, so the additive
/// offset belongs in the final row (indices 20..23). Alpha is passed through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HaloColorTransform {
    slope: f32,
    additive: f32,
}

impl HaloColorTransform {
    pub(crate) fn from_strength(strength: f32) -> Self {
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            HaloPreferences::default().inversion_strength
        };
        Self {
            slope: 1.0 - 2.0 * strength,
            additive: strength,
        }
    }

    pub(crate) fn matrix(self) -> [f32; 25] {
        [
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            self.additive,
            self.additive,
            self.additive,
            0.0,
            1.0,
        ]
    }

    #[cfg(test)]
    pub(crate) fn apply_rgba(self, pixel: [u8; 4]) -> [u8; 4] {
        let mut result = pixel;
        for channel in &mut result[..3] {
            let normalized = f32::from(*channel) / 255.0;
            *channel = ((normalized * self.slope + self.additive) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        result
    }
}

/// Return the exact square source and placement rectangle for a 1x circular
/// halo. The window spans `[hotspot-radius, hotspot+radius)` on each axis;
/// coordinates are never shifted inward to accommodate a monitor edge.
pub(crate) fn halo_geometry(cursor: PhysicalPoint, radius: i32) -> Option<HaloGeometry> {
    let radius = radius.clamp(8, 256);
    let diameter = radius.checked_mul(2)?;
    let left = cursor.x.checked_sub(radius)?;
    let top = cursor.y.checked_sub(radius)?;
    let right = left.checked_add(diameter)?;
    let bottom = top.checked_add(diameter)?;
    Some(HaloGeometry {
        origin: PhysicalPoint::new(left, top),
        source: PhysicalRect::new(left, top, right, bottom)?,
        diameter,
    })
}

/// Build the optional crisp outline as a separate transparent-ring bitmap.
/// Pixel centers are measured from the same physical hotspot as the native
/// magnifier. At the minimum radius, rendered thickness is capped at radius-1
/// so at least the center pixels remain transparent even when the preference
/// requests the maximum eight-pixel border.
pub(crate) fn halo_outline_bitmap(preferences: HaloPreferences) -> Option<RgbaImage> {
    let preferences = preferences.normalized();
    if !preferences.outline_enabled {
        return None;
    }

    let radius = preferences.radius;
    let diameter = radius.checked_mul(2)? as u32;
    let thickness = preferences.outline_thickness.min(radius - 1);
    let outer_radius = radius as f32;
    let inner_radius = (radius - thickness) as f32;
    let outer_squared = outer_radius * outer_radius;
    let inner_squared = inner_radius * inner_radius;
    let mut image = RgbaImage::new(diameter, diameter);
    let outline = Rgba([
        preferences.outline_color.red,
        preferences.outline_color.green,
        preferences.outline_color.blue,
        255,
    ]);
    for y in 0..diameter {
        let dy = y as f32 + 0.5 - outer_radius;
        for x in 0..diameter {
            let dx = x as f32 + 0.5 - outer_radius;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared <= outer_squared && distance_squared > inner_squared {
                image.put_pixel(x, y, outline);
            }
        }
    }
    Some(image)
}

fn crosshair_geometry(preferences: &CrosshairPreferences) -> CrosshairGeometry {
    // Bound raw/deserialized and directly constructed preferences before any
    // sizing arithmetic so malformed values cannot request a huge bitmap.
    let arm = preferences.arm_length.clamp(2, 256) as u32;
    let thickness = preferences.thickness.clamp(1, 16) as u32;
    let gap = preferences.center_gap.clamp(0, 128) as u32;
    let outline = u32::from(preferences.high_contrast_outline);
    let outline_expansion = outline
        .checked_mul(2)
        .expect("bounded crosshair outline expansion fits u32");
    let arm_radius = gap
        .checked_add(arm)
        .and_then(|extent| extent.checked_sub(1))
        .and_then(|extent| extent.checked_add(outline_expansion))
        .expect("bounded crosshair arm extent fits u32");
    let thickness_radius = (thickness / 2)
        .checked_add(outline)
        .expect("bounded crosshair thickness extent fits u32");
    let radius = arm_radius.max(thickness_radius);
    let extent = radius
        .checked_mul(2)
        .and_then(|diameter| diameter.checked_add(1))
        .expect("bounded crosshair bitmap dimensions fit u32");
    let width = extent;
    let height = extent;
    // Odd dimensions keep this exact center pixel aligned with the hotspot
    // when native.rs places the image at hotspot - (width / 2, height / 2).
    let center_x = width / 2;
    let center_y = height / 2;
    let inset = gap
        .checked_add(outline)
        .expect("bounded crosshair center inset fits u32");
    let negative_end_x = center_x
        .checked_sub(inset)
        .expect("crosshair negative arm remains inside its bitmap");
    let negative_start_x = negative_end_x
        .checked_sub(arm - 1)
        .expect("crosshair negative arm start remains inside its bitmap");
    let positive_start_x = center_x
        .checked_add(inset)
        .expect("crosshair positive arm remains inside its bitmap");
    let negative_end_y = center_y
        .checked_sub(inset)
        .expect("crosshair negative arm remains inside its bitmap");
    let negative_start_y = negative_end_y
        .checked_sub(arm - 1)
        .expect("crosshair negative arm start remains inside its bitmap");
    let positive_start_y = center_y
        .checked_add(inset)
        .expect("crosshair positive arm remains inside its bitmap");
    let horizontal_y = center_y
        .checked_sub(thickness / 2)
        .expect("crosshair horizontal stroke remains inside its bitmap");
    let vertical_x = center_x
        .checked_sub(thickness / 2)
        .expect("crosshair vertical stroke remains inside its bitmap");
    let colored_strokes = [
        PixelRect {
            x: negative_start_x,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: positive_start_x,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: vertical_x,
            y: negative_start_y,
            width: thickness,
            height: arm,
        },
        PixelRect {
            x: vertical_x,
            y: positive_start_y,
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
    use super::{
        CrosshairGeometry, GuideOrientation, HaloColorTransform, crosshair_bitmap,
        crosshair_geometry, guide_geometry, halo_geometry, halo_outline_bitmap,
    };
    use crate::coordinate_tool::controller::CoordinateRenderFrame;
    use crate::coordinate_tool::model::{
        CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, ForegroundClientGeometry,
        MonitorGeometry, MonitorId,
    };
    use crate::coordinate_tool::model::{PhysicalPoint, PhysicalRect, PhysicalSize};
    use crate::coordinate_tool::settings::{CrosshairColor, CrosshairPreferences, HaloPreferences};

    #[test]
    fn halo_blend_preserves_input_full_inversion_and_expected_partial_rgb_values() {
        let unchanged = HaloColorTransform::from_strength(0.0);
        assert_eq!(unchanged.apply_rgba([0, 255, 128, 77]), [0, 255, 128, 77]);

        let partial = HaloColorTransform::from_strength(0.4);
        assert_eq!(partial.apply_rgba([0, 0, 0, 255]), [102, 102, 102, 255]);
        assert_eq!(
            partial.apply_rgba([255, 255, 255, 255]),
            [153, 153, 153, 255]
        );
        assert_eq!(partial.apply_rgba([128, 128, 128, 64]), [128, 128, 128, 64]);
        assert_eq!(partial.apply_rgba([255, 0, 0, 19]), [153, 102, 102, 19]);
        assert_eq!(partial.apply_rgba([17, 93, 241, 201]), [105, 121, 150, 201]);

        let full = HaloColorTransform::from_strength(1.0);
        assert_eq!(full.apply_rgba([0, 127, 255, 33]), [255, 128, 0, 33]);
    }

    #[test]
    fn halo_native_matrix_uses_rgb_slopes_last_row_offsets_and_unchanged_alpha() {
        let matrix = HaloColorTransform::from_strength(0.4).matrix();
        assert!((matrix[0] - 0.2).abs() < f32::EPSILON);
        assert!((matrix[6] - 0.2).abs() < f32::EPSILON);
        assert!((matrix[12] - 0.2).abs() < f32::EPSILON);
        assert_eq!(matrix[18], 1.0);
        assert_eq!(&matrix[20..23], &[0.4, 0.4, 0.4]);
        assert_eq!(matrix[24], 1.0);
        for index in [1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 19, 23] {
            assert_eq!(matrix[index], 0.0);
        }
        assert_eq!(HaloColorTransform::from_strength(0.0).matrix()[0], 1.0);
        assert_eq!(HaloColorTransform::from_strength(1.0).matrix()[0], -1.0);
        assert_eq!(HaloColorTransform::from_strength(1.0).matrix()[20], 1.0);
    }

    #[test]
    fn halo_geometry_centers_signed_source_and_destination_on_the_hotspot() {
        let geometry = halo_geometry(PhysicalPoint::new(-1920, 250), 60).unwrap();
        assert_eq!(geometry.origin, PhysicalPoint::new(-1980, 190));
        assert_eq!(
            geometry.source,
            PhysicalRect::new(-1980, 190, -1860, 310).unwrap()
        );
        assert_eq!(geometry.diameter, 120);

        assert!(halo_geometry(PhysicalPoint::new(i32::MIN, 0), 60).is_none());
        assert!(halo_geometry(PhysicalPoint::new(i32::MAX, 0), 60).is_none());
        assert_eq!(
            halo_geometry(PhysicalPoint::new(10, 20), i32::MIN)
                .unwrap()
                .diameter,
            16
        );
        assert_eq!(
            halo_geometry(PhysicalPoint::new(10, 20), i32::MAX)
                .unwrap()
                .diameter,
            512
        );
    }

    #[test]
    fn halo_outline_is_separate_transparent_and_keeps_a_hole_at_minimum_radius() {
        let mut preferences = HaloPreferences {
            outline_enabled: true,
            outline_color: CrosshairColor::new(31, 121, 223),
            outline_thickness: 1,
            ..HaloPreferences::default()
        };
        let thin = halo_outline_bitmap(preferences).unwrap();
        assert_eq!(thin.dimensions(), (120, 120));
        assert_eq!(thin.get_pixel(60, 0).0, [31, 121, 223, 255]);
        assert_eq!(thin.get_pixel(0, 0).0[3], 0);
        for (x, y) in [(59, 59), (60, 59), (59, 60), (60, 60)] {
            assert_eq!(thin.get_pixel(x, y).0[3], 0);
        }
        let thin_pixels = thin.pixels().filter(|pixel| pixel.0[3] > 0).count();

        preferences.outline_thickness = 4;
        let thick = halo_outline_bitmap(preferences).unwrap();
        let thick_pixels = thick.pixels().filter(|pixel| pixel.0[3] > 0).count();
        assert!(thick_pixels > thin_pixels);
        assert_eq!(thick.get_pixel(60, 0).0, [31, 121, 223, 255]);
        assert!(halo_outline_bitmap(HaloPreferences::default()).is_none());

        preferences.radius = 8;
        preferences.outline_thickness = 8;
        let minimum = halo_outline_bitmap(preferences).unwrap();
        assert_eq!(minimum.dimensions(), (16, 16));
        assert!(minimum.pixels().any(|pixel| pixel.0[3] == 255));
        assert_eq!(minimum.get_pixel(7, 7).0[3], 0);
        assert_eq!(minimum.get_pixel(8, 7).0[3], 0);
        assert_eq!(minimum.get_pixel(7, 8).0[3], 0);
        assert_eq!(minimum.get_pixel(8, 8).0[3], 0);
    }

    #[test]
    fn crosshair_geometry_keeps_four_arms_and_outline_around_each_arm() {
        let preferences = CrosshairPreferences::default();
        let CrosshairGeometry {
            width,
            height,
            colored_strokes,
            outline_strokes,
        } = crosshair_geometry(&preferences);

        assert_eq!((width, height), (59, 59));
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

        let changed_gap = CrosshairPreferences {
            center_gap: 128,
            ..preferences
        };
        assert_eq!(
            horizontal,
            guide_geometry(GuideOrientation::Horizontal, desktop, cursor, &changed_gap).unwrap()
        );
        assert_eq!(
            vertical,
            guide_geometry(GuideOrientation::Vertical, desktop, cursor, &changed_gap).unwrap()
        );
    }

    #[test]
    fn crosshair_pixel_clearance_is_symmetric_for_gaps_thicknesses_and_outlines() {
        let color = CrosshairColor::new(200, 30, 40);
        const ARM_LENGTH: u32 = 9;
        for gap in [0, 1, 16, 128] {
            for thickness in [3, 4] {
                for high_contrast_outline in [false, true] {
                    let preferences = CrosshairPreferences {
                        color,
                        thickness,
                        arm_length: ARM_LENGTH as i32,
                        center_gap: gap,
                        opacity: 1.0,
                        high_contrast_outline,
                        ..Default::default()
                    };
                    let geometry = crosshair_geometry(&preferences);
                    let image = crosshair_bitmap(&preferences);
                    let center = (image.width() / 2, image.height() / 2);
                    assert_eq!(image.width() % 2, 1);
                    assert_eq!(image.height() % 2, 1);

                    for (horizontal, positive) in
                        [(true, false), (true, true), (false, false), (false, true)]
                    {
                        let visible_distance =
                            first_visible_axis_pixel(&image, center, horizontal, positive);
                        assert_eq!(
                            visible_distance, gap as u32,
                            "gap={gap}, thickness={thickness}, outline={high_contrast_outline}, horizontal={horizontal}, positive={positive}"
                        );

                        let colored_length =
                            colored_axis_pixels(&image, center, horizontal, positive, color);
                        assert_eq!(
                            colored_length, ARM_LENGTH,
                            "colored arm length changed for gap={gap}, thickness={thickness}, outline={high_contrast_outline}, horizontal={horizontal}, positive={positive}"
                        );
                    }

                    let expected_rgba = [color.red, color.green, color.blue, 255];
                    for stroke in &geometry.colored_strokes[..2] {
                        let endpoint_x = if stroke.x < center.0 {
                            stroke.x
                        } else {
                            stroke.x + stroke.width - 1
                        };
                        let colored_pixels = (0..image.height())
                            .filter(|y| image.get_pixel(endpoint_x, *y).0 == expected_rgba)
                            .count();
                        assert_eq!(colored_pixels, thickness as usize);
                    }
                    for stroke in &geometry.colored_strokes[2..] {
                        let endpoint_y = if stroke.y < center.1 {
                            stroke.y
                        } else {
                            stroke.y + stroke.height - 1
                        };
                        let colored_pixels = (0..image.width())
                            .filter(|x| image.get_pixel(*x, endpoint_y).0 == expected_rgba)
                            .count();
                        assert_eq!(colored_pixels, thickness as usize);
                    }
                }
            }
        }
    }

    #[test]
    fn crosshair_geometry_bounds_raw_dimensions_before_sizing() {
        let minimum = CrosshairPreferences {
            arm_length: i32::MIN,
            thickness: i32::MIN,
            center_gap: i32::MIN,
            high_contrast_outline: false,
            ..Default::default()
        };
        let minimum_geometry = crosshair_geometry(&minimum);
        assert_eq!((minimum_geometry.width, minimum_geometry.height), (3, 3));
        assert_eq!(minimum_geometry.colored_strokes[0].width, 2);
        assert_eq!(minimum_geometry.colored_strokes[0].height, 1);

        let maximum = CrosshairPreferences {
            arm_length: i32::MAX,
            thickness: i32::MAX,
            center_gap: i32::MAX,
            high_contrast_outline: true,
            ..Default::default()
        };
        let maximum_geometry = crosshair_geometry(&maximum);
        assert_eq!(
            (maximum_geometry.width, maximum_geometry.height),
            (771, 771)
        );
        assert_eq!(maximum_geometry.colored_strokes[0].width, 256);
        assert_eq!(maximum_geometry.colored_strokes[0].height, 16);
    }

    fn first_visible_axis_pixel(
        image: &image::RgbaImage,
        center: (u32, u32),
        horizontal: bool,
        positive: bool,
    ) -> u32 {
        let axis_length = if horizontal {
            image.width()
        } else {
            image.height()
        };
        let center_position = if horizontal { center.0 } else { center.1 };
        let ray_length = if positive {
            axis_length - center_position
        } else {
            center_position + 1
        };
        (0..ray_length)
            .find(|distance| {
                let x = if horizontal {
                    if positive {
                        center.0 + *distance
                    } else {
                        center.0 - *distance
                    }
                } else {
                    center.0
                };
                let y = if horizontal {
                    center.1
                } else if positive {
                    center.1 + *distance
                } else {
                    center.1 - *distance
                };
                x < image.width() && y < image.height() && image.get_pixel(x, y).0[3] > 0
            })
            .expect("crosshair has visible pixels on all four axes")
    }

    fn colored_axis_pixels(
        image: &image::RgbaImage,
        center: (u32, u32),
        horizontal: bool,
        positive: bool,
        color: CrosshairColor,
    ) -> u32 {
        let axis_length = if horizontal {
            image.width()
        } else {
            image.height()
        };
        let center_position = if horizontal { center.0 } else { center.1 };
        let ray_length = if positive {
            axis_length - center_position
        } else {
            center_position + 1
        };
        let expected = [color.red, color.green, color.blue, 255];
        (0..ray_length)
            .filter(|distance| {
                let x = if horizontal {
                    if positive {
                        center.0 + *distance
                    } else {
                        center.0 - *distance
                    }
                } else {
                    center.0
                };
                let y = if horizontal {
                    center.1
                } else if positive {
                    center.1 + *distance
                } else {
                    center.1 - *distance
                };
                x < image.width() && y < image.height() && image.get_pixel(x, y).0 == expected
            })
            .count() as u32
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
