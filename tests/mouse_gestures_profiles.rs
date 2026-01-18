use std::collections::HashMap;

use multi_launcher::plugins::mouse_gestures::db::{
    select_binding, select_profile, ForegroundWindowInfo, MouseGestureBinding, MouseGestureDb,
    MouseGestureProfile, MouseGestureProfileRule, MouseGestureRuleField, MouseGestureRuleType,
};

fn profile(
    id: &str,
    priority: i32,
    rules: Vec<MouseGestureProfileRule>,
) -> MouseGestureProfile {
    MouseGestureProfile {
        id: id.to_string(),
        label: id.to_string(),
        enabled: true,
        priority,
        rules,
        bindings: Vec::new(),
    }
}

#[test]
fn selects_profile_by_rules_and_priority() {
    let db = MouseGestureDb {
        profiles: vec![
            profile(
                "editor",
                2,
                vec![
                    MouseGestureProfileRule {
                        field: MouseGestureRuleField::Exe,
                        matcher: MouseGestureRuleType::Contains,
                        value: "code".to_string(),
                    },
                    MouseGestureProfileRule {
                        field: MouseGestureRuleField::Title,
                        matcher: MouseGestureRuleType::Regex,
                        value: "Workspace".to_string(),
                    },
                ],
            ),
            profile(
                "browser",
                1,
                vec![MouseGestureProfileRule {
                    field: MouseGestureRuleField::Exe,
                    matcher: MouseGestureRuleType::StartsWith,
                    value: "chrome".to_string(),
                }],
            ),
            profile(
                "fallback",
                0,
                vec![MouseGestureProfileRule {
                    field: MouseGestureRuleField::Class,
                    matcher: MouseGestureRuleType::Contains,
                    value: "Window".to_string(),
                }],
            ),
        ],
        ..MouseGestureDb::default()
    };

    let window = ForegroundWindowInfo {
        exe: Some("code.exe".to_string()),
        class: Some("WindowClass".to_string()),
        title: Some("Workspace - Multi".to_string()),
    };

    let selected = select_profile(&db, &window).expect("profile match");
    assert_eq!(selected.id, "editor");
}

#[test]
fn selects_binding_with_threshold_and_tiebreaks() {
    let mut profile = MouseGestureProfile {
        id: "profile".to_string(),
        label: "profile".to_string(),
        enabled: true,
        priority: 0,
        rules: Vec::new(),
        bindings: vec![
            MouseGestureBinding {
                gesture_id: "one".to_string(),
                action: "action:one".to_string(),
                args: None,
                priority: 0,
            },
            MouseGestureBinding {
                gesture_id: "two".to_string(),
                action: "action:two".to_string(),
                args: None,
                priority: 1,
            },
            MouseGestureBinding {
                gesture_id: "three".to_string(),
                action: "action:three".to_string(),
                args: None,
                priority: 1,
            },
        ],
    };

    let mut distances = HashMap::new();
    distances.insert("one".to_string(), 0.4);
    distances.insert("two".to_string(), 0.3);
    distances.insert("three".to_string(), 0.3);

    let selected = select_binding(&profile, &distances, 0.5).expect("binding match");
    assert_eq!(selected.binding.gesture_id, "two");

    profile.bindings.swap(1, 2);
    let selected = select_binding(&profile, &distances, 0.5).expect("binding match");
    assert_eq!(selected.binding.gesture_id, "three");

    assert!(select_binding(&profile, &distances, 0.2).is_none());
}
