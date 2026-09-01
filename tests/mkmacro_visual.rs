use multi_launcher::mkmacro::{executor::fake::FakeBackend, *};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
fn s(action: MkAction) -> MkStep {
    MkStep {
        id: 1,
        enabled: true,
        breakpoint: false,
        repeat: 1,
        delay_after_ms: 0,
        on_error: Default::default(),
        action,
    }
}
fn plan(action: MkAction) -> MkExecutionPlan {
    compile(&MkMacro {
        id: 1,
        name: "visual".into(),
        description: String::new(),
        enabled: true,
        hotkey: None,
        hotkey_scope: Default::default(),
        folder_id: None,
        playback: Default::default(),
        steps: vec![s(action)],
        image_assets: vec![],
    })
    .unwrap()
}

#[test]
fn image_catalog_defaults_require_assets_and_configured_payload_round_trips() {
    use multi_launcher::gui::mkmacro_dialog::action_catalog::{self, DraftValidationContract};
    let image_entries: Vec<_> = action_catalog::visible_descriptors()
        .filter(|descriptor| matches!(descriptor.name, "Find Image" | "Click Image"))
        .collect();
    assert_eq!(
        image_entries.len(),
        2,
        "both production image paths are intentionally visible"
    );
    for descriptor in image_entries {
        let action = (descriptor.make_default)();
        assert_eq!(
            action_catalog::draft_validation_contract(&action),
            DraftValidationContract::AwaitingRequiredAsset,
            "{} must not apply the asset_id == 0 draft sentinel",
            descriptor.name
        );
        assert!(multi_launcher::mkmacro::executor::has_runtime_support(
            &action
        ));
        assert!(descriptor.editor.contract().is_some());
    }

    let payload = MkImagePayload {
        asset_id: 42,
        wait: MkWaitOptions {
            timeout_ms: 12_345,
            poll_interval_ms: 77,
        },
        region: SearchRegion::Rectangle {
            rect: ScreenRect::new(-10, 20, 640, 480),
        },
        tolerance: 9,
        alpha: AlphaPolicy::Ignore,
        return_point: ReturnPoint::TopLeft,
        not_found_policy: MkImageNotFoundPolicy::Fail,
        outputs: MkImageOutputs::default(),
    };
    let action = MkAction::ImageClick(payload.clone());
    assert_eq!(
        action_catalog::draft_validation_contract(&action),
        DraftValidationContract::CommitReady
    );
    let json = serde_json::to_string(&action).unwrap();
    assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), action);
    assert!(
        matches!(serde_json::from_str::<MkAction>(&json).unwrap(), MkAction::ImageClick(saved) if saved == payload)
    );
}
#[test]
fn image_wait_cancels_promptly_without_real_screen_access() {
    let f = Arc::new(FakeBackend::default());
    let c = Arc::new(RunControl::default());
    c.reset();
    let worker = {
        let c = c.clone();
        thread::spawn(move || {
            Executor::new(f.backends(), c).execute(
                &plan(MkAction::ImageFind(MkImagePayload {
                    asset_id: 2,
                    wait: MkWaitOptions {
                        timeout_ms: 60_000,
                        poll_interval_ms: 10_000,
                    },
                    region: SearchRegion::Desktop,
                    tolerance: 0,
                    alpha: AlphaPolicy::Compare,
                    return_point: ReturnPoint::Center,
                    not_found_policy: MkImageNotFoundPolicy::Fail,
                    outputs: MkImageOutputs::default(),
                })),
                ExecutionOptions::normal(),
                &|_| {},
            )
        })
    };
    thread::sleep(Duration::from_millis(10));
    let at = Instant::now();
    c.stop();
    assert_eq!(
        worker.join().unwrap().unwrap_err().kind,
        DiagnosticKind::Cancelled
    );
    assert!(at.elapsed() < Duration::from_millis(500));
}
#[test]
fn pixel_wait_cancels_promptly_without_real_screen_access() {
    let f = Arc::new(FakeBackend::default());
    let c = Arc::new(RunControl::default());
    c.reset();
    let target = MkCoordinateTarget::Screen {
        point: MkPoint { x: -10, y: 20 },
    };
    let action = MkAction::WaitUntil {
        condition: MkCondition::PixelResult {
            target,
            color: "#ffffff".into(),
            tolerance: 0,
        },
        wait: MkWaitOptions {
            timeout_ms: 60_000,
            poll_interval_ms: 10_000,
        },
    };
    let worker = {
        let c = c.clone();
        thread::spawn(move || {
            Executor::new(f.backends(), c).execute(
                &plan(action),
                ExecutionOptions::normal(),
                &|_| {},
            )
        })
    };
    thread::sleep(Duration::from_millis(10));
    c.stop();
    assert_eq!(
        worker.join().unwrap().unwrap_err().kind,
        DiagnosticKind::Cancelled
    );
}
