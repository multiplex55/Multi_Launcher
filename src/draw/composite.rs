use crate::draw::model::CanvasModel;
use crate::draw::render::{render_canvas_to_rgba, RenderSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaBuffer {
    pub fn new(width: u32, height: u32, fill: Rgba) -> Self {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = fill.r;
            chunk[1] = fill.g;
            chunk[2] = fill.b;
            chunk[3] = fill.a;
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Rgba {
        let idx = ((y * self.width + x) * 4) as usize;
        Rgba {
            r: self.pixels[idx],
            g: self.pixels[idx + 1],
            b: self.pixels[idx + 2],
            a: self.pixels[idx + 3],
        }
    }
}

pub fn annotation_from_canvas(canvas: &CanvasModel, width: u32, height: u32) -> RgbaBuffer {
    let mut rgba = render_canvas_to_rgba(canvas, RenderSettings::default(), (width, height));
    let key = crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY;
    for px in rgba.chunks_exact_mut(4) {
        if px[0] == key.r && px[1] == key.g && px[2] == key.b {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        }
    }
    RgbaBuffer::from_pixels(width, height, rgba)
}

pub fn composite_annotation_over_desktop(
    desktop: &RgbaBuffer,
    annotation: &RgbaBuffer,
) -> RgbaBuffer {
    assert_eq!(desktop.width, annotation.width);
    assert_eq!(desktop.height, annotation.height);

    let mut output = desktop.clone();
    blend_in_place(&mut output, annotation);
    output
}

pub fn composite_annotation_over_blank(annotation: &RgbaBuffer, background: Rgba) -> RgbaBuffer {
    let mut output = RgbaBuffer::new(annotation.width, annotation.height, background);
    blend_in_place(&mut output, annotation);
    output
}

fn blend_in_place(base: &mut RgbaBuffer, top: &RgbaBuffer) {
    assert_eq!(base.width, top.width);
    assert_eq!(base.height, top.height);

    for (dst, src) in base
        .pixels
        .chunks_exact_mut(4)
        .zip(top.pixels.chunks_exact(4))
    {
        let blended = blend_pixel(
            Rgba {
                r: dst[0],
                g: dst[1],
                b: dst[2],
                a: dst[3],
            },
            Rgba {
                r: src[0],
                g: src[1],
                b: src[2],
                a: src[3],
            },
        );
        dst[0] = blended.r;
        dst[1] = blended.g;
        dst[2] = blended.b;
        dst[3] = blended.a;
    }
}

fn blend_pixel(bottom: Rgba, top: Rgba) -> Rgba {
    let sa = top.a as f32 / 255.0;
    let da = bottom.a as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);

    if out_a <= f32::EPSILON {
        return Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }

    let blend = |s: u8, d: u8| -> u8 {
        (((s as f32 * sa) + (d as f32 * da * (1.0 - sa))) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8
    };

    Rgba {
        r: blend(top.r, bottom.r),
        g: blend(top.g, bottom.g),
        b: blend(top.b, bottom.b),
        a: (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        annotation_from_canvas, composite_annotation_over_blank, composite_annotation_over_desktop,
        Rgba, RgbaBuffer,
    };
    use crate::draw::model::{
        CanvasModel, Color, DrawObject, Geometry, ObjectStyle, StrokeStyle, Tool,
    };

    #[test]
    fn annotation_over_desktop_blends_expected_pixel() {
        let desktop = RgbaBuffer::from_pixels(1, 1, vec![100, 100, 100, 255]);
        let annotation = RgbaBuffer::from_pixels(1, 1, vec![200, 0, 0, 128]);

        let out = composite_annotation_over_desktop(&desktop, &annotation);
        assert_eq!(
            out.pixel(0, 0),
            Rgba {
                r: 150,
                g: 50,
                b: 50,
                a: 255
            }
        );
    }

    #[test]
    fn blank_background_export_uses_configured_color() {
        let annotation = RgbaBuffer::from_pixels(2, 1, vec![255, 255, 255, 0, 0, 255, 0, 255]);
        let bg = Rgba {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        };

        let out = composite_annotation_over_blank(&annotation, bg);
        assert_eq!(out.pixel(0, 0), bg);
        assert_eq!(
            out.pixel(1, 0),
            Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
    }

    #[test]
    fn canvas_annotation_maps_colorkey_to_transparent_alpha() {
        let canvas = CanvasModel {
            objects: vec![DrawObject {
                tool: Tool::Line,
                style: ObjectStyle {
                    stroke: StrokeStyle {
                        width: 1,
                        color: Color::rgba(10, 20, 30, 255),
                    },
                    fill: None,
                },
                geometry: Geometry::Line {
                    start: (0, 0),
                    end: (0, 0),
                },
            }],
        };

        let out = annotation_from_canvas(&canvas, 2, 1);
        assert_eq!(
            out.pixel(0, 0),
            Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255
            }
        );
        assert_eq!(
            out.pixel(1, 0),
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            }
        );
    }
}
