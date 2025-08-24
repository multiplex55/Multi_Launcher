use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use eframe::egui;
use sysinfo::System;

pub struct VolumeDialog {
    pub open: bool,
    value: u8,
    processes: Vec<ProcessVolume>,
}

impl Default for VolumeDialog {
    fn default() -> Self {
        Self { open: false, value: 50, processes: Vec::new() }
    }
}

#[derive(Clone)]
struct ProcessVolume {
    pid: u32,
    name: String,
    value: u8,
    muted: bool,
}

impl ProcessVolume {
    /// Returns an action string to toggle mute if the process is currently muted.
    /// The caller is responsible for dispatching the action if returned.
    fn slider_changed(&mut self) -> Option<String> {
        if self.muted {
            self.muted = false;
            Some(format!("volume:pid_toggle_mute:{}", self.pid))
        } else {
            None
        }
    }
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value = get_system_volume().unwrap_or(50);
        self.processes = get_process_volumes();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        egui::Window::new("Volume")
            .resizable(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.add(egui::Slider::new(&mut self.value, 0..=100).text("Level"));
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Volume".into(),
                            action: format!("volume:set:{}", self.value),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                ui.separator();
                for proc in &mut self.processes {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} (PID {})", proc.name, proc.pid));
                        let resp = ui.add(egui::Slider::new(&mut proc.value, 0..=100).text("Level"));
                        if resp.changed() {
                            if let Some(action) = proc.slider_changed() {
                                let _ = launch_action(&Action {
                                    label: String::new(),
                                    desc: "Volume".into(),
                                    action,
                                    args: None,
                                });
                            }
                        }
                        if ui.button("Set").clicked() {
                            let _ = launch_action(&Action {
                                label: String::new(),
                                desc: "Volume".into(),
                                action: format!("volume:pid:{}:{}", proc.pid, proc.value),
                                args: None,
                            });
                        }
                        if ui.button("Mute").clicked() {
                            let _ = launch_action(&Action {
                                label: String::new(),
                                desc: "Volume".into(),
                                action: format!("volume:pid_toggle_mute:{}", proc.pid),
                                args: None,
                            });
                            proc.muted = !proc.muted;
                        }
                        if proc.muted {
                            ui.colored_label(egui::Color32::RED, "muted");
                        }
                    });
                }
            });
        if close { self.open = false; }
    }
}

fn get_system_volume() -> Option<u8> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let mut percent = None;
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    if let Ok(val) = vol.GetMasterVolumeLevelScalar() {
                        percent = Some((val * 100.0).round() as u8);
                    }
                }
            }
        }
        CoUninitialize();
        percent
    }
}

fn get_process_volumes() -> Vec<ProcessVolume> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    let mut entries = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    if let Ok(list) = manager.GetSessionEnumerator() {
                        let count = list.GetCount().unwrap_or(0);
                        let sys = System::new_all();
                        for i in 0..count {
                            if let Ok(ctrl) = list.GetSession(i) {
                                if let Ok(c2) = ctrl.cast::<IAudioSessionControl2>() {
                                    if let Ok(pid) = c2.GetProcessId() {
                                        if pid == 0 {
                                            continue;
                                        }
                                        if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                                            if let Ok(val) = vol.GetMasterVolume() {
                                                let name = sys
                                                    .process(sysinfo::Pid::from_u32(pid))
                                                    .map(|p| p.name().to_string_lossy().to_string())
                                                    .unwrap_or_else(|| format!("PID {pid}"));
                                                let muted = vol
                                                    .GetMute()
                                                    .map(|m| m.as_bool())
                                                    .unwrap_or(false);
                                                entries.push(ProcessVolume {
                                                    pid,
                                                    name,
                                                    value: (val * 100.0).round() as u8,
                                                    muted,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_change_unmutes() {
        let mut proc = ProcessVolume {
            pid: 1,
            name: "test".into(),
            value: 50,
            muted: true,
        };
        let action = proc.slider_changed();
        assert_eq!(
            action,
            Some("volume:pid_toggle_mute:1".to_string())
        );
        assert!(!proc.muted);
    }
}
