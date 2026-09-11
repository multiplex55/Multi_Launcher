use image::RgbaImage;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub const fn from_corners(first: Point, second: Point) -> Self {
        Self {
            min: Point::new(
                if first.x < second.x {
                    first.x
                } else {
                    second.x
                },
                if first.y < second.y {
                    first.y
                } else {
                    second.y
                },
            ),
            max: Point::new(
                if first.x > second.x {
                    first.x
                } else {
                    second.x
                },
                if first.y > second.y {
                    first.y
                } else {
                    second.y
                },
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub [u8; 4]);

pub fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, color: Color) {
    if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
        return;
    }
    let [red, green, blue, alpha] = color.0;
    if alpha == 0 {
        return;
    }
    let destination = image.get_pixel(x as u32, y as u32).0;
    let source_alpha = alpha as f32 / 255.0;
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= 0.0 {
        return;
    }
    let blend = |source: u8, destination: u8| {
        let source = source as f32 / 255.0;
        let destination = destination as f32 / 255.0;
        ((source * source_alpha + destination * destination_alpha * (1.0 - source_alpha))
            / output_alpha
            * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    image.put_pixel(
        x as u32,
        y as u32,
        image::Rgba([
            blend(red, destination[0]),
            blend(green, destination[1]),
            blend(blue, destination[2]),
            (output_alpha * 255.0) as u8,
        ]),
    );
}

pub fn draw_circle(image: &mut RgbaImage, center: Point, radius: f32, color: Color) {
    if radius <= 0.0 || image.width() == 0 || image.height() == 0 {
        return;
    }
    let radius_squared = radius * radius;
    let min_x = (center.x - radius).floor().max(0.0) as i32;
    let max_x = (center.x + radius).ceil().min((image.width() - 1) as f32) as i32;
    let min_y = (center.y - radius).floor().max(0.0) as i32;
    let max_y = (center.y + radius).ceil().min((image.height() - 1) as f32) as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - center.x;
            let dy = y as f32 + 0.5 - center.y;
            if dx * dx + dy * dy <= radius_squared {
                blend_pixel(image, x, y, color);
            }
        }
    }
}

pub fn draw_line(image: &mut RgbaImage, start: Point, end: Point, color: Color, thickness: f32) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
    let radius = (thickness / 2.0).max(0.5);
    for step in 0..=steps {
        let amount = step as f32 / steps as f32;
        draw_circle(
            image,
            Point::new(start.x + dx * amount, start.y + dy * amount),
            radius,
            color,
        );
    }
}

pub fn draw_rect_outline(image: &mut RgbaImage, rect: Rect, color: Color, thickness: f32) {
    draw_line(
        image,
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        color,
        thickness,
    );
    draw_line(
        image,
        Point::new(rect.max.x, rect.min.y),
        rect.max,
        color,
        thickness,
    );
    draw_line(
        image,
        rect.max,
        Point::new(rect.min.x, rect.max.y),
        color,
        thickness,
    );
    draw_line(
        image,
        Point::new(rect.min.x, rect.max.y),
        rect.min,
        color,
        thickness,
    );
}

pub fn draw_rect_fill(image: &mut RgbaImage, rect: Rect, color: Color) {
    if image.width() == 0 || image.height() == 0 {
        return;
    }
    let min_x = rect.min.x.floor().max(0.0) as i32;
    let max_x = rect.max.x.ceil().min((image.width() - 1) as f32) as i32;
    let min_y = rect.min.y.floor().max(0.0) as i32;
    let max_y = rect.max.y.ceil().min((image.height() - 1) as f32) as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            blend_pixel(image, x, y, color);
        }
    }
}

pub fn draw_arrow(image: &mut RgbaImage, start: Point, end: Point, color: Color, thickness: f32) {
    draw_line(image, start, end, color, thickness);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    if length <= 0.5 {
        return;
    }
    let unit = Point::new(dx / length, dy / length);
    let head_length = (10.0 + thickness * 2.0).min(length * 0.5);
    let angle = 30.0_f32.to_radians();
    let (sin, cos) = angle.sin_cos();
    let rotate = |direction: Point, sine: f32| {
        Point::new(
            direction.x * cos - direction.y * sine,
            direction.x * sine + direction.y * cos,
        )
    };
    for direction in [rotate(unit, sin), rotate(unit, -sin)] {
        draw_line(
            image,
            end,
            Point::new(
                end.x - direction.x * head_length,
                end.y - direction.y * head_length,
            ),
            color,
            thickness,
        );
    }
}

pub fn draw_ellipse(image: &mut RgbaImage, rect: Rect, color: Color, thickness: f32) {
    let radius_x = (rect.max.x - rect.min.x) / 2.0;
    let radius_y = (rect.max.y - rect.min.y) / 2.0;
    if radius_x <= 0.0 || radius_y <= 0.0 {
        return;
    }
    let center = Point::new(rect.min.x + radius_x, rect.min.y + radius_y);
    let circumference = std::f32::consts::TAU * radius_x.max(radius_y);
    let steps = circumference.ceil().max(12.0) as usize;
    let point_at = |step: usize| {
        let angle = std::f32::consts::TAU * step as f32 / steps as f32;
        Point::new(
            center.x + radius_x * angle.cos(),
            center.y + radius_y * angle.sin(),
        )
    };
    let mut previous = point_at(0);
    for step in 1..=steps {
        let current = point_at(step);
        draw_line(image, previous, current, color, thickness);
        previous = current;
    }
}

pub fn default_font_arc() -> Option<(ab_glyph::FontArc, eframe::egui::FontTweak)> {
    let definitions = eframe::egui::FontDefinitions::default();
    let family = definitions
        .families
        .get(&eframe::egui::FontFamily::Proportional)?;
    let font_name = family.first()?;
    let data = definitions.font_data.get(font_name)?.clone();
    let font = match data.font {
        std::borrow::Cow::Borrowed(bytes) => {
            ab_glyph::FontRef::try_from_slice_and_index(bytes, data.index)
                .map(ab_glyph::FontArc::from)
                .ok()
        }
        std::borrow::Cow::Owned(bytes) => {
            ab_glyph::FontVec::try_from_vec_and_index(bytes, data.index)
                .map(ab_glyph::FontArc::from)
                .ok()
        }
    }?;
    Some((font, data.tweak))
}

pub fn draw_text(
    image: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    tweak: eframe::egui::FontTweak,
    position: Point,
    text: &str,
    color: Color,
    size: f32,
) {
    use ab_glyph::{Font, ScaleFont, point};
    if text.is_empty() {
        return;
    }
    let scaled = font.as_scaled(size * tweak.scale);
    let line_height = scaled.height();
    let mut caret = point(
        position.x,
        position.y + scaled.ascent() + tweak.y_offset * size,
    );
    for character in text.chars() {
        if character == '\n' {
            caret.x = position.x;
            caret.y += line_height;
            continue;
        }
        let mut glyph = scaled.scaled_glyph(character);
        glyph.position = caret;
        caret.x += scaled.h_advance(glyph.id);
        if let Some(outlined) = scaled.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, coverage| {
                let pixel_x = x as i32 + bounds.min.x as i32;
                let pixel_y = y as i32 + bounds.min.y as i32;
                let alpha = (color.0[3] as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                blend_pixel(
                    image,
                    pixel_x,
                    pixel_y,
                    Color([color.0[0], color.0[1], color.0[2], alpha]),
                );
            });
        }
    }
}
