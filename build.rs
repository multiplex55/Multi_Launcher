fn main() {
    // no custom macros are passed to the resource compiler
    embed_resource::compile("Resources/windows.rc", embed_resource::NONE);
}
