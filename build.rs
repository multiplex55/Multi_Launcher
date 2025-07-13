#[cfg(target_os = "windows")]
fn main() {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::imageops::FilterType;
    use image::ColorType;
    use std::env;
    use std::fs::File;
    use std::path::Path;

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let ico_path = Path::new(&out_dir).join("app.ico");

    let bytes = include_bytes!("Resources/Green_MultiLauncher.png");
    let img = image::load_from_memory(bytes).expect("load icon");

    let mut frames = Vec::new();
    for &size in &[16u32, 32, 48, 256] {
        let resized = img.resize_exact(size, size, FilterType::Lanczos3).to_rgba8();
        frames.push(
            IcoFrame::as_png(resized.as_raw(), size, size, ColorType::Rgba8)
                .expect("create ico frame"),
        );
    }

    let mut file = File::create(&ico_path).expect("create ico");
    IcoEncoder::new(&mut file)
        .encode_images(&frames)
        .expect("write ico");

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    res.compile().expect("failed to compile resources");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
