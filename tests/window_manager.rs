use multi_launcher::window_manager::{
    clear_mock_mouse_position, current_mouse_position, mock_mouse_position_is_set,
    set_mock_mouse_position, virtual_key_from_string, MOCK_MOUSE_LOCK,
};

#[test]
fn mock_mouse_position_override_and_clear() {
    let _lock = MOCK_MOUSE_LOCK.lock().unwrap();
    // Set a custom mouse position and confirm it is returned
    set_mock_mouse_position(Some((10.0, 20.0)));
    assert!(mock_mouse_position_is_set());
    assert_eq!(current_mouse_position(), Some((10.0, 20.0)));

    // Clear the mock and ensure the mock state is cleared
    clear_mock_mouse_position();
    assert!(!mock_mouse_position_is_set());
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
