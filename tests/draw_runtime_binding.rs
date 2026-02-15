use std::any::TypeId;
use std::path::Path;
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

#[test]
fn draw_module_entrypoint_is_directory_mod_rs() {
    let lib_rs = include_str!("../src/lib.rs");
    let canonical_wiring_present = lib_rs
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["#[path = \"draw/mod.rs\"]", "pub mod draw;"]);
    assert!(
        canonical_wiring_present,
        "src/lib.rs must keep draw wired through #[path = \"draw/mod.rs\"] pub mod draw;",
    );
    assert!(
        !Path::new("src/draw.rs").exists(),
        "legacy src/draw.rs should not exist; keep draw implementation under src/draw/mod.rs",
    );
}
