use crate::actions::Action;
use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use eframe::egui;
#[cfg(target_os = "windows")]
use sysinfo::{Pid, System};

pub struct VolumeDialog {
    pub open: bool,
    value: u8,
    #[cfg(target_os = "windows")]
    processes: Vec<ProcessVolume>,
}

impl Default for VolumeDialog {
    fn default() -> Self {
        Self {
            open: false,
            value: 50,
            #[cfg(target_os = "windows")]
            processes: Vec::new(),
        }
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Clone)]
struct ProcessVolume {
    pid: u32,
    name: String,
    value: u8,
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value = get_system_volume().unwrap_or(50);
        #[cfg(target_os = "windows")]
        {
            self.processes = get_process_volumes();
        }
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
                    if ui.button("Mute").clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Volume".into(),
                            action: "volume:mute_active".into(),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                #[cfg(target_os = "windows")]
                {
                    ui.separator();
                    for proc in &mut self.processes {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} (PID {})", proc.name, proc.pid));
                            ui.add(egui::Slider::new(&mut proc.value, 0..=100).text("Level"));
                            if ui.button("Set").clicked() {
                                let _ = launch_action(&Action {
                                    label: String::new(),
                                    desc: "Volume".into(),
                                    action: format!("volume:pid:{}:{}", proc.pid, proc.value),
                                    args: None,
                                });
                            }
                        });
                    }
                }
            });
        if close { self.open = false; }
    }
}

#[cfg(target_os = "windows")]
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

#[cfg(not(target_os = "windows"))]
fn get_system_volume() -> Option<u8> {
    None
}

#[cfg(target_os = "windows")]
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
                                                entries.push(ProcessVolume {
                                                    pid,
                                                    name,
                                                    value: (val * 100.0).round() as u8,
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

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[cfg(not(target_os = "windows"))]
fn get_process_volumes() -> Vec<ProcessVolume> {
    Vec::new()
}
