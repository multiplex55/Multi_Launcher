use multi_launcher::draw::overlay::select_layered_window_attributes;
use multi_launcher::draw::settings::TransparencyMethod;

#[derive(Default)]
struct MockPlatform {
    last_flags: Option<u32>,
    last_method: Option<TransparencyMethod>,
}

impl MockPlatform {
    fn configure(&mut self, method: TransparencyMethod) {
        let attrs = select_layered_window_attributes(method);
        self.last_flags = Some(attrs.flags);
        self.last_method = Some(method);
    }
}

#[test]
fn colorkey_mode_selects_colorkey_flag() {
    let mut platform = MockPlatform::default();
    platform.configure(TransparencyMethod::Colorkey);

    assert_eq!(platform.last_method, Some(TransparencyMethod::Colorkey));
    assert_eq!(platform.last_flags, Some(0x0000_0001));
}

#[test]
fn alpha_mode_selects_alpha_flag() {
    let mut platform = MockPlatform::default();
    platform.configure(TransparencyMethod::Alpha);

    assert_eq!(platform.last_method, Some(TransparencyMethod::Alpha));
    assert_eq!(platform.last_flags, Some(0x0000_0002));
}
