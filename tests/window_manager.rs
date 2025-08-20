use multi_launcher::window_manager::{
    clear_mock_mouse_position, current_mouse_position, set_mock_mouse_position,
    virtual_key_from_string,
};
use serial_test::serial;

#[test]
#[serial]
fn mock_mouse_position_override_and_clear() {
    // Set a custom mouse position and confirm it is returned
    set_mock_mouse_position(Some((10.0, 20.0)));
    assert_eq!(current_mouse_position(), Some((10.0, 20.0)));

    // Clear the mock and ensure the default is returned
    clear_mock_mouse_position();
    let pos = current_mouse_position();
    assert!(pos.is_some());
    assert_ne!(pos, Some((10.0, 20.0)));
}

#[test]
fn virtual_key_from_string_cases() {
    let cases = [
        ("A", Some(0x41)),
        ("F1", Some(0x70)),
        ("LEFTALT", Some(0xA4)),
        ("INVALID", None),
    ];

    for (input, expected) in cases {
        assert_eq!(virtual_key_from_string(input), expected, "input: {input}");
    }
}
