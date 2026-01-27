use sysinfo::System;

#[derive(Clone)]
pub struct ProcessVolume {
    pub pid: u32,
    pub name: String,
    pub value: u8,
    pub muted: bool,
}

impl ProcessVolume {
    /// Returns an action string to toggle mute if the process is currently muted.
    /// The caller is responsible for dispatching the action if returned.
    pub fn slider_changed(&mut self) -> Option<String> {
        if self.muted {
            self.muted = false;
            Some(format!("volume:pid_toggle_mute:{}", self.pid))
        } else {
            None
        }
    }
}

pub fn get_system_volume() -> Option<u8> {
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

pub fn get_process_volumes() -> Vec<ProcessVolume> {
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
        assert_eq!(action, Some("volume:pid_toggle_mute:1".to_string()));
        assert!(!proc.muted);
    }
}
