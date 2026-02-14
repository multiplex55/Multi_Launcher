use std::any::TypeId;
use std::time::Instant;

#[test]
fn draw_runtime_is_service_backed() {
    assert_eq!(
        TypeId::of::<multi_launcher::draw::DrawRuntime>(),
        TypeId::of::<multi_launcher::draw::service::DrawRuntime>(),
        "multi_launcher::draw::DrawRuntime must re-export draw::service::DrawRuntime",
    );

    let runtime = multi_launcher::draw::runtime();
    runtime
        .tick(Instant::now())
        .expect("service-backed draw runtime tick should be callable");
}
