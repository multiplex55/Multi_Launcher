use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use eframe::egui;
use std::sync::atomic::{AtomicUsize, Ordering};

pub static BRIGHTNESS_QUERIES: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
pub struct BrightnessDialog {
    pub open: bool,
    value: u8,
    value_loaded: bool,
}

impl BrightnessDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value_loaded = false;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        if !self.value_loaded {
            self.value = get_main_display_brightness().unwrap_or(50);
            self.value_loaded = true;
        }
        let mut close = false;
        egui::Window::new("Brightness")
            .resizable(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.add(egui::Slider::new(&mut self.value, 0..=100).text("Level"));
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Brightness".into(),
                            action: format!("brightness:set:{}", self.value),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.open = false;
        }
    }
}

fn get_main_display_brightness() -> Option<u8> {
    use windows::Win32::Devices::Display::{
        DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
    };
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn enum_monitors(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let percent_ptr = lparam.0 as *mut u32;
        let mut count: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_ok() {
            let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors).is_ok() {
                if let Some(m) = monitors.first() {
                    let mut min = 0u32;
                    let mut cur = 0u32;
                    let mut max = 0u32;
                    if GetMonitorBrightness(m.hPhysicalMonitor, &mut min, &mut cur, &mut max) != 0 {
                        if max > min {
                            *percent_ptr = ((cur - min) * 100 / (max - min)) as u32;
                        } else {
                            *percent_ptr = 0;
                        }
                    }
                }
                let _ = DestroyPhysicalMonitors(&monitors);
            }
        }
        false.into()
    }

    let mut percent: u32 = 50;
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC(std::ptr::null_mut()),
            None,
            Some(enum_monitors),
            LPARAM(&mut percent as *mut u32 as isize),
        );
    }
    BRIGHTNESS_QUERIES.fetch_add(1, Ordering::Relaxed);
    Some(percent as u8)
}
