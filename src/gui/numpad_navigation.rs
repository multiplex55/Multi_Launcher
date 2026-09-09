use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LauncherNumpadNavigation {
    Up,
    Down,
    Left,
    Right,
}

impl LauncherNumpadNavigation {
    pub(crate) fn navigation_key(self) -> egui::Key {
        match self {
            Self::Up => egui::Key::ArrowUp,
            Self::Down => egui::Key::ArrowDown,
            Self::Left => egui::Key::ArrowLeft,
            Self::Right => egui::Key::ArrowRight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhysicalNumpadKey {
    Num2,
    Num4,
    Num6,
    Num8,
}

pub(crate) trait NumpadKeyStateProbe {
    fn is_down(&self, key: PhysicalNumpadKey) -> bool;
}

pub(crate) struct NativeNumpadKeyStateProbe;

impl NumpadKeyStateProbe for NativeNumpadKeyStateProbe {
    fn is_down(&self, key: PhysicalNumpadKey) -> bool {
        native_numpad_key_is_down(key)
    }
}

#[cfg(windows)]
fn native_numpad_key_is_down(key: PhysicalNumpadKey) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_NUMPAD2, VK_NUMPAD4, VK_NUMPAD6, VK_NUMPAD8,
    };

    let virtual_key = match key {
        PhysicalNumpadKey::Num2 => VK_NUMPAD2,
        PhysicalNumpadKey::Num4 => VK_NUMPAD4,
        PhysicalNumpadKey::Num6 => VK_NUMPAD6,
        PhysicalNumpadKey::Num8 => VK_NUMPAD8,
    };
    unsafe { (GetAsyncKeyState(virtual_key.0 as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn native_numpad_key_is_down(_key: PhysicalNumpadKey) -> bool {
    false
}

fn candidate_for_key(
    key: egui::Key,
) -> Option<(PhysicalNumpadKey, LauncherNumpadNavigation, &'static str)> {
    match key {
        egui::Key::Num2 => Some((PhysicalNumpadKey::Num2, LauncherNumpadNavigation::Down, "2")),
        egui::Key::Num4 => Some((PhysicalNumpadKey::Num4, LauncherNumpadNavigation::Left, "4")),
        egui::Key::Num6 => Some((
            PhysicalNumpadKey::Num6,
            LauncherNumpadNavigation::Right,
            "6",
        )),
        egui::Key::Num8 => Some((PhysicalNumpadKey::Num8, LauncherNumpadNavigation::Up, "8")),
        _ => None,
    }
}

/// Claims one physical keypad navigation press before the query editor sees it.
///
/// The native state is queried only after focus, logical-key, and modifier checks
/// have identified a candidate. A following text event is removed only when it
/// is immediately adjacent and contains the digit emitted by the claimed key.
pub(crate) fn consume_physical_numpad_navigation(
    query_has_focus: bool,
    input: &mut egui::InputState,
    probe: &impl NumpadKeyStateProbe,
) -> Vec<LauncherNumpadNavigation> {
    if !query_has_focus {
        return Vec::new();
    }

    let mut navigation = Vec::new();
    let mut event_index = 0;
    while event_index < input.events.len() {
        let candidate = match &input.events[event_index] {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if *modifiers == egui::Modifiers::NONE => candidate_for_key(*key),
            _ => None,
        };
        let Some((physical_key, direction, digit)) = candidate else {
            event_index += 1;
            continue;
        };
        if !probe.is_down(physical_key) {
            event_index += 1;
            continue;
        }

        input.events.remove(event_index);
        if matches!(
            input.events.get(event_index),
            Some(egui::Event::Text(text)) if text == digit
        ) {
            input.events.remove(event_index);
        }
        if !navigation.contains(&direction) {
            navigation.push(direction);
        }
    }
    navigation
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeProbe {
        down: Option<PhysicalNumpadKey>,
        calls: Cell<usize>,
    }

    impl NumpadKeyStateProbe for FakeProbe {
        fn is_down(&self, key: PhysicalNumpadKey) -> bool {
            self.calls.set(self.calls.get() + 1);
            self.down == Some(key)
        }
    }

    fn key_press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn route(
        focused: bool,
        events: Vec<egui::Event>,
        down: Option<PhysicalNumpadKey>,
    ) -> (Vec<LauncherNumpadNavigation>, Vec<egui::Event>, usize) {
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events,
            ..Default::default()
        });
        let probe = FakeProbe {
            down,
            calls: Cell::new(0),
        };
        let navigation =
            ctx.input_mut(|input| consume_physical_numpad_navigation(focused, input, &probe));
        let remaining = ctx.input(|input| input.events.clone());
        let _ = ctx.end_frame();
        (navigation, remaining, probe.calls.get())
    }

    #[test]
    fn routes_all_physical_numpad_directions_and_removes_paired_text() {
        for (key, physical, digit, expected) in [
            (
                egui::Key::Num8,
                PhysicalNumpadKey::Num8,
                "8",
                LauncherNumpadNavigation::Up,
            ),
            (
                egui::Key::Num2,
                PhysicalNumpadKey::Num2,
                "2",
                LauncherNumpadNavigation::Down,
            ),
            (
                egui::Key::Num4,
                PhysicalNumpadKey::Num4,
                "4",
                LauncherNumpadNavigation::Left,
            ),
            (
                egui::Key::Num6,
                PhysicalNumpadKey::Num6,
                "6",
                LauncherNumpadNavigation::Right,
            ),
        ] {
            let (navigation, remaining, calls) = route(
                true,
                vec![
                    key_press(key, egui::Modifiers::NONE),
                    egui::Event::Text(digit.into()),
                ],
                Some(physical),
            );
            assert_eq!(navigation, vec![expected]);
            assert!(remaining.is_empty());
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn focus_modifiers_and_irrelevant_events_avoid_native_probe() {
        let cases = [
            (false, key_press(egui::Key::Num8, egui::Modifiers::NONE)),
            (true, key_press(egui::Key::Num8, egui::Modifiers::CTRL)),
            (true, key_press(egui::Key::Num8, egui::Modifiers::ALT)),
            (true, key_press(egui::Key::Num8, egui::Modifiers::SHIFT)),
            (true, key_press(egui::Key::Num8, egui::Modifiers::COMMAND)),
            (true, key_press(egui::Key::ArrowUp, egui::Modifiers::NONE)),
            (true, key_press(egui::Key::ArrowDown, egui::Modifiers::NONE)),
            (true, key_press(egui::Key::ArrowLeft, egui::Modifiers::NONE)),
            (
                true,
                key_press(egui::Key::ArrowRight, egui::Modifiers::NONE),
            ),
            (true, key_press(egui::Key::PageUp, egui::Modifiers::NONE)),
            (true, key_press(egui::Key::PageDown, egui::Modifiers::NONE)),
        ];
        for (focused, event) in cases {
            let (navigation, remaining, calls) = route(focused, vec![event], None);
            assert!(navigation.is_empty());
            assert_eq!(remaining.len(), 1);
            assert_eq!(calls, 0);
        }

        let (navigation, remaining, calls) = route(true, vec![], None);
        assert!(navigation.is_empty());
        assert!(remaining.is_empty());
        assert_eq!(calls, 0);
    }

    #[test]
    fn top_row_candidate_is_preserved_when_physical_key_is_not_down() {
        let events = vec![
            key_press(egui::Key::Num8, egui::Modifiers::NONE),
            egui::Event::Text("8".into()),
        ];
        let (navigation, remaining, calls) = route(true, events.clone(), None);
        assert!(navigation.is_empty());
        assert_eq!(remaining, events);
        assert_eq!(calls, 1);
    }

    #[test]
    fn only_immediately_adjacent_matching_text_is_removed() {
        let unrelated_text = vec![
            key_press(egui::Key::Num8, egui::Modifiers::NONE),
            egui::Event::Text("2".into()),
            egui::Event::Text("8".into()),
        ];
        let (navigation, remaining, _) = route(true, unrelated_text, Some(PhysicalNumpadKey::Num8));
        assert_eq!(navigation, vec![LauncherNumpadNavigation::Up]);
        assert_eq!(
            remaining,
            vec![egui::Event::Text("2".into()), egui::Event::Text("8".into())]
        );
    }

    #[test]
    fn one_repeat_press_is_claimed_per_frame_without_reordering_other_events() {
        let events = vec![
            egui::Event::Text("a".into()),
            egui::Event::Key {
                key: egui::Key::Num2,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("2".into()),
            key_press(egui::Key::ArrowDown, egui::Modifiers::NONE),
        ];
        let (navigation, remaining, _) = route(true, events, Some(PhysicalNumpadKey::Num2));
        assert_eq!(navigation, vec![LauncherNumpadNavigation::Down]);
        assert_eq!(
            remaining,
            vec![
                egui::Event::Text("a".into()),
                key_press(egui::Key::ArrowDown, egui::Modifiers::NONE),
            ]
        );
    }

    #[test]
    fn repeated_pairs_are_all_removed_but_navigate_once_per_direction() {
        let events = vec![
            key_press(egui::Key::Num8, egui::Modifiers::NONE),
            egui::Event::Text("8".into()),
            egui::Event::Key {
                key: egui::Key::Num8,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("8".into()),
        ];
        let (navigation, remaining, calls) = route(true, events, Some(PhysicalNumpadKey::Num8));
        assert_eq!(navigation, vec![LauncherNumpadNavigation::Up]);
        assert!(remaining.is_empty());
        assert_eq!(calls, 2);
    }
}
