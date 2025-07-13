#[cfg(target_os = "windows")]
fn main() {
    embed_resource::compile("Resources/windows.rc");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
