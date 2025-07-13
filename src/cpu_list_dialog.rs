use crate::gui::LauncherApp;
use eframe::egui;
use std::time::Instant;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, MINIMUM_CPU_UPDATE_INTERVAL};

#[derive(Default)]
pub struct CpuListDialog {
    pub open: bool,
    count: usize,
    system: System,
    last_refresh: Option<Instant>,
}

impl CpuListDialog {
    pub fn open(&mut self, count: usize) {
        self.count = count;
        self.system = System::new_all();
        self.last_refresh = Some(Instant::now() - MINIMUM_CPU_UPDATE_INTERVAL);
        self.open = true;
    }

    fn refresh(&mut self) {
        if let Some(last) = self.last_refresh {
            if last.elapsed() >= MINIMUM_CPU_UPDATE_INTERVAL {
                self.system.refresh_processes_specifics(
                    ProcessesToUpdate::All,
                    true,
                    ProcessRefreshKind::everything(),
                );
                self.system.refresh_cpu_usage();
                self.last_refresh = Some(Instant::now());
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        self.refresh();
        let mut close = false;
        egui::Window::new("CPU Usage")
            .resizable(true)
            .open(&mut self.open)
            .show(ctx, |ui| {
                let mut procs: Vec<_> = self.system.processes().values().collect();
                procs.sort_by(|a, b| {
                    b.cpu_usage()
                        .partial_cmp(&a.cpu_usage())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for p in procs.into_iter().take(self.count) {
                    ui.label(format!(
                        "{:.1}% - {}",
                        p.cpu_usage(),
                        p.name().to_string_lossy()
                    ));
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            self.open = false;
        }
    }
}
