use multi_launcher::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY;
use multi_launcher::draw::render::{
    convert_rgba_to_dib_bgra_with_transparency, render_canvas_to_rgba, RenderSettings,
};
use multi_launcher::draw::{model::CanvasModel, settings::TransparencyMethod};

#[test]
fn transparent_regions_map_to_exact_colorkey_in_colorkey_mode() {
    let settings = RenderSettings {
        transparency_method: TransparencyMethod::Colorkey,
        ..RenderSettings::default()
    };
    let rgba = render_canvas_to_rgba(&CanvasModel::default(), settings, (4, 4));
    let mut bgra = vec![0_u8; rgba.len()];
    convert_rgba_to_dib_bgra_with_transparency(&rgba, &mut bgra, settings.transparency_method);

    for px in bgra.chunks_exact(4) {
        assert_eq!(px[0], FIRST_PASS_TRANSPARENCY_COLORKEY.b);
        assert_eq!(px[1], FIRST_PASS_TRANSPARENCY_COLORKEY.g);
        assert_eq!(px[2], FIRST_PASS_TRANSPARENCY_COLORKEY.r);
    }
}
