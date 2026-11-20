use eframe::egui::{Event, InputState, Key, Modifiers};

/// Claim only result navigation, leaving the remaining events for the editor.
/// The native backend preserves keypad positions independently of Num Lock.
pub(super) fn take_query_navigation(input: &mut InputState, grid: bool) -> Vec<Key> {
    let mut navigation = Vec::new();
    let mut paired_digit = None;
    input.events.retain(|event| {
        if let Some(digit) = paired_digit.take()
            && matches!(event, Event::Text(text) if text == digit)
        {
            return false;
        }
        let Event::Key {
            key,
            physical_key,
            pressed: true,
            modifiers: Modifiers::NONE,
            ..
        } = *event
        else {
            return true;
        };
        let direction = match (key, physical_key) {
            (Key::Num2, Some(Key::Numpad2)) => (Key::ArrowDown, "2"),
            (Key::Num4, Some(Key::Numpad4)) => (Key::ArrowLeft, "4"),
            (Key::Num6, Some(Key::Numpad6)) => (Key::ArrowRight, "6"),
            (Key::Num8, Some(Key::Numpad8)) => (Key::ArrowUp, "8"),
            (Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown, _) => {
                navigation.push(key);
                return false;
            }
            (Key::ArrowLeft | Key::ArrowRight, _) if grid => {
                navigation.push(key);
                return false;
            }
            _ => return true,
        };
        paired_digit = Some(direction.1);
        if grid || matches!(direction.0, Key::ArrowUp | Key::ArrowDown) {
            navigation.push(direction.0);
        }
        false
    });
    navigation
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn only_the_adjacent_digit_is_claimed_and_modified_keys_stay_with_the_editor() {
        let digit = |physical_key, modifiers| Event::Key {
            key: Key::Num2,
            physical_key: Some(physical_key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        let events = vec![
            digit(Key::Numpad2, Modifiers::NONE),
            Event::Text("x".into()),
            Event::Text("2".into()),
            digit(Key::Numpad2, Modifiers::CTRL),
            digit(Key::Num2, Modifiers::NONE),
            Event::Text("2".into()),
        ];
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            events,
            ..Default::default()
        });
        // egui has already normalized repeat flags by the time we route input.
        let expected = ctx.input(|input| input.events[1..].to_vec());
        assert_eq!(
            ctx.input_mut(|input| take_query_navigation(input, false)),
            [Key::ArrowDown]
        );
        ctx.input(|input| assert_eq!(input.events, expected));
        ctx.end_pass().drop_without_applying_deltas();
    }
}
