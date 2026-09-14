use image::RgbaImage;

/// Converts straight-alpha RGBA into the premultiplied BGRA required by
/// `UpdateLayeredWindow(ULW_ALPHA)`.
pub(crate) fn premultiplied_bgra(image: &RgbaImage, output: &mut [u8]) -> Result<(), &'static str> {
    if output.len() != image.as_raw().len() {
        return Err("pixel buffer length does not match image");
    }
    for (source, destination) in image
        .as_raw()
        .chunks_exact(4)
        .zip(output.chunks_exact_mut(4))
    {
        let alpha = u16::from(source[3]);
        let premultiply = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
        destination.copy_from_slice(&[
            premultiply(source[2]),
            premultiply(source[1]),
            premultiply(source[0]),
            source[3],
        ]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn translucent_edges_are_premultiplied_without_black_halo_channels() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([240, 120, 60, 128]));
        let mut output = [0; 4];
        premultiplied_bgra(&image, &mut output).unwrap();
        assert_eq!(output, [30, 60, 120, 128]);
        assert!(premultiplied_bgra(&image, &mut []).is_err());
    }
}
