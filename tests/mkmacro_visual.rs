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
        playback: Default::default(),
        steps: vec![s(action)],
    })
    .unwrap()
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
                    confidence: 0.9,
                    wait: MkWaitOptions {
                        timeout_ms: 60_000,
                        poll_interval_ms: 10_000,
                    },
                })),
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
        thread::spawn(move || Executor::new(f.backends(), c).execute(&plan(action), &|_| {}))
    };
    thread::sleep(Duration::from_millis(10));
    c.stop();
    assert_eq!(
        worker.join().unwrap().unwrap_err().kind,
        DiagnosticKind::Cancelled
    );
}
