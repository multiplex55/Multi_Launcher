fn main() {
    // no custom macros are passed to the resource compiler
    println!("cargo:rerun-if-changed=Resources/windows.rc");
    println!("cargo:rerun-if-changed=Resources/Green_MultiLauncher.ico");
    embed_resource::compile("Resources/windows.rc", embed_resource::NONE);
}
