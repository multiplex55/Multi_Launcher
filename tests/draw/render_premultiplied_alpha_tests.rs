use multi_launcher::draw::render::convert_rgba_to_dib_bgra_with_transparency;
use multi_launcher::draw::settings::TransparencyMethod;

#[test]
fn alpha_mode_uses_premultiplied_rgb_and_preserves_zero_alpha_pixels() {
    let rgba = vec![
        200, 100, 50, 128, // partially transparent
        255, 10, 220, 0, // fully transparent
        30, 60, 90, 255, // fully opaque
    ];
    let mut bgra = vec![0_u8; rgba.len()];

    convert_rgba_to_dib_bgra_with_transparency(&rgba, &mut bgra, TransparencyMethod::Alpha);

    assert_eq!(bgra[0], 25); // 50 * 128 / 255 (rounded)
    assert_eq!(bgra[1], 50); // 100 * 128 / 255
    assert_eq!(bgra[2], 100); // 200 * 128 / 255
    assert_eq!(bgra[3], 128);

    assert_eq!(&bgra[4..8], &[0, 0, 0, 0]);
    assert_eq!(&bgra[8..12], &[90, 60, 30, 255]);
}
