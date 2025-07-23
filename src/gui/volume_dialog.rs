use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use crate::actions::Action;
use eframe::egui;

#[derive(Default)]
pub struct VolumeDialog {
    pub open: bool,
    value: u8,
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value = get_system_volume().unwrap_or(50);
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
                    let mut val = 0f32;
                    if vol.GetMasterVolumeLevelScalar(&mut val).is_ok() {
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
