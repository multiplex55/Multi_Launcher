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
    #[cfg(test)]
    test_fake_hardware_value: Option<u8>,
    #[cfg(test)]
    test_hardware_queries: usize,
    #[cfg(test)]
    test_set_actions: usize,
    #[cfg(test)]
    test_level_response: Option<egui::Response>,
}

impl BrightnessDialog {
    #[cfg(test)]
    pub(crate) fn test_seed_fake_hardware_value(&mut self, value: u8) {
        self.test_fake_hardware_value = Some(value);
        self.value = value;
        self.value_loaded = false;
        self.test_hardware_queries = 0;
        self.test_set_actions = 0;
        self.test_level_response = None;
    }

    #[cfg(test)]
    pub(crate) fn test_level_response(&self) -> Option<&egui::Response> {
        self.test_level_response.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_hardware_query_count(&self) -> usize {
        self.test_hardware_queries
    }

    #[cfg(test)]
    pub(crate) fn test_set_action_count(&self) -> usize {
        self.test_set_actions
    }

    pub fn open(&mut self) {
        self.open = true;
        self.value_loaded = false;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        if !self.value_loaded {
            #[cfg(test)]
            {
                if let Some(value) = self.test_fake_hardware_value {
                    self.value = value;
                } else {
                    self.test_hardware_queries = self.test_hardware_queries.saturating_add(1);
                    self.value = get_main_display_brightness().unwrap_or(50);
                }
            }
            #[cfg(not(test))]
            {
                self.value = get_main_display_brightness().unwrap_or(50);
            }
            self.value_loaded = true;
        }
        let mut close = false;
        egui::Window::new("Brightness")
            .resizable(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                let response = ui.add(egui::Slider::new(&mut self.value, 0..=100).text("Level"));
                #[cfg(test)]
                {
                    self.test_level_response = Some(response.clone());
                }
                crate::settings_editor::render::observe_scoped_settings_navigation_response(
                    ui,
                    &response,
                    "brightness.level",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        #[cfg(test)]
                        {
                            if self.test_fake_hardware_value.is_some() {
                                self.test_set_actions = self.test_set_actions.saturating_add(1);
                            } else {
                                let _ = launch_action(&Action {
                                    label: String::new(),
                                    desc: "Brightness".into(),
                                    action: format!("brightness:set:{}", self.value),
                                    args: None,
                                });
                            }
                        }
                        #[cfg(not(test))]
                        {
                            let _ = launch_action(&Action {
                                label: String::new(),
                                desc: "Brightness".into(),
                                action: format!("brightness:set:{}", self.value),
                                args: None,
                            });
                        }
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
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count) }.is_ok() {
            let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if unsafe { GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors) }.is_ok() {
                if let Some(m) = monitors.first() {
                    let mut min = 0u32;
                    let mut cur = 0u32;
                    let mut max = 0u32;
                    if unsafe {
                        GetMonitorBrightness(m.hPhysicalMonitor, &mut min, &mut cur, &mut max)
                    } != 0
                    {
                        if max > min {
                            unsafe {
                                *percent_ptr = (cur - min) * 100 / (max - min);
                            }
                        } else {
                            unsafe {
                                *percent_ptr = 0;
                            }
                        }
                    }
                }
                let _ = unsafe { DestroyPhysicalMonitors(&monitors) };
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
