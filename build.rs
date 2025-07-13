#[cfg(target_os = "windows")]
fn main() {
    use image::imageops::FilterType;
    use std::env;
    use std::fs::File;
    use std::path::Path;

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let ico_path = Path::new(&out_dir).join("app.ico");

    let bytes = include_bytes!("Resources/Green_MultiLauncher.png");
    let img = image::load_from_memory(bytes).expect("load icon");
    let icon = img.resize(256, 256, FilterType::Lanczos3);
    icon.write_to(
        &mut File::create(&ico_path).expect("create ico"),
        image::ImageOutputFormat::Ico,
    )
    .expect("write ico");

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    res.compile().expect("failed to compile resources");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
