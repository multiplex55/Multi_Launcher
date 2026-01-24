use multi_launcher::mouse_gestures::service::{
    HookEvent, MockHookBackend, MouseGestureConfig, MouseGestureService,
};

#[test]
fn start_stop_installs_and_uninstalls_once() {
    let (backend, handle) = MockHookBackend::new();
    let mut service = MouseGestureService::new_with_backend(Box::new(backend));

    service.start();
    service.start();
    service.stop();
    service.stop();

    assert_eq!(handle.install_count(), 1);
    assert_eq!(handle.uninstall_count(), 1);
}

#[test]
fn disabling_config_stops_worker_and_blocks_hook_events() {
    let (backend, handle) = MockHookBackend::new();
    let mut service = MouseGestureService::new_with_backend(Box::new(backend));
    let mut config = MouseGestureConfig::default();

    config.enabled = true;
    service.update_config(config.clone());
    assert!(service.is_running());
    assert!(handle.emit(HookEvent::RButtonDown));

    config.enabled = false;
    service.update_config(config);

    assert!(!service.is_running());
    assert!(!handle.emit(HookEvent::RButtonDown));
}
