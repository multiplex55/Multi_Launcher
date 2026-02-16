use multi_launcher::draw::keyboard_hook::{KeyCode, KeyEvent, KeyModifiers};
use multi_launcher::draw::overlay::{
    modal_action_for_key_event, modal_action_for_pointer_event, ExitDialogState, ExitModalAction,
    OverlayPointerEvent,
};
use multi_launcher::draw::save::SaveChoice;

#[test]
fn key_shortcuts_route_to_expected_modal_actions() {
    let no_mods = KeyModifiers::default();
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Num1,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Select(SaveChoice::Desktop))
    );
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Num2,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Select(SaveChoice::Blank))
    );
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Num3,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Select(SaveChoice::Both))
    );
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Num4,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Select(SaveChoice::Discard))
    );
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Enter,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Select(SaveChoice::Desktop))
    );
    assert_eq!(
        modal_action_for_key_event(
            ExitDialogState::PromptVisible,
            KeyEvent {
                key: KeyCode::Escape,
                modifiers: no_mods,
            }
        ),
        Some(ExitModalAction::Cancel)
    );
}

#[test]
fn pointer_hits_route_to_expected_modal_actions() {
    let size = (800, 600);
    let panel_x = (size.0 as i32 - 300) / 2;
    let panel_y = (size.1 as i32 - 224) / 2;
    let first_button_y = panel_y + 48;
    let button_h = 28;
    let spacing = 8;
    let click_x = panel_x + 16 + 20;

    let expected = [
        ExitModalAction::Select(SaveChoice::Desktop),
        ExitModalAction::Select(SaveChoice::Blank),
        ExitModalAction::Select(SaveChoice::Both),
        ExitModalAction::Select(SaveChoice::Discard),
        ExitModalAction::Cancel,
    ];

    for (idx, action) in expected.into_iter().enumerate() {
        let y = first_button_y + (idx as i32 * (button_h + spacing)) + (button_h / 2);
        assert_eq!(
            modal_action_for_pointer_event(
                ExitDialogState::PromptVisible,
                (click_x, y),
                OverlayPointerEvent::LeftDown {
                    modifiers: Default::default(),
                },
                size,
            ),
            Some(action),
            "unexpected modal action for button {idx}"
        );
    }

    assert_eq!(
        modal_action_for_pointer_event(
            ExitDialogState::Saving,
            (click_x, first_button_y),
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default(),
            },
            size,
        ),
        None
    );
}
