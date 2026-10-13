use eframe::egui;
use multi_launcher::{
    actions::Action, gui::LauncherApp, plugin::PluginManager, settings::Settings,
};
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Debug)]
struct TestDroppedFile(std::path::PathBuf);

impl egui::DroppedFile for TestDroppedFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

fn new_app(ctx: &egui::Context) -> LauncherApp {
    let actions: Vec<Action> = Vec::new();
    let custom_len = actions.len();
    LauncherApp::new(
        ctx,
        Arc::new(actions),
        custom_len,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn dropping_file_opens_add_dialog() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let tmp = std::env::temp_dir().join("dummy.txt");
    let dropped = Arc::new(TestDroppedFile(tmp));
    app.handle_dropped_files(vec![dropped]);
    assert!(app.show_editor);
    assert!(app.editor.is_dialog_open());
}
