use crate::multi_manager::model::MmWindow;
use crate::multi_manager::win::WindowIdentitySnapshot;

fn trimmed(value: &str) -> &str {
    value.trim()
}

pub fn normalize_path(value: &str) -> String {
    trimmed(value).replace('/', "\\").to_ascii_lowercase()
}

pub fn normalize_value(value: &str) -> String {
    trimmed(value).to_ascii_lowercase()
}

pub fn same_process_path(stored: &str, live: &str) -> bool {
    let stored = normalize_path(stored);
    !stored.is_empty() && stored == normalize_path(live)
}

pub fn same_value(stored: &str, live: &str) -> bool {
    let stored = normalize_value(stored);
    !stored.is_empty() && stored == normalize_value(live)
}

pub fn has_stable_metadata(window: &MmWindow) -> bool {
    !window.process_path.trim().is_empty()
        || !window.executable.trim().is_empty()
        || !window.class_name.trim().is_empty()
}

pub fn stable_identity_matches(window: &MmWindow, live: &WindowIdentitySnapshot) -> bool {
    if !live.is_window {
        return false;
    }

    let stored_process_path = window.process_path.trim();
    let stored_class_name = window.class_name.trim();
    let stored_executable = window.executable.trim();

    if stored_process_path.is_empty()
        && stored_class_name.is_empty()
        && stored_executable.is_empty()
    {
        return false;
    }

    if !stored_process_path.is_empty()
        && !same_process_path(stored_process_path, &live.process_path)
    {
        return false;
    }

    if !stored_class_name.is_empty() && !same_value(stored_class_name, &live.class_name) {
        return false;
    }

    if stored_process_path.is_empty()
        && !stored_executable.is_empty()
        && !same_value(stored_executable, &live.executable)
    {
        return false;
    }

    true
}

pub fn stable_identity_matches_enumerated(
    window: &MmWindow,
    live: &crate::multi_manager::win::EnumeratedWindow,
) -> bool {
    stable_identity_matches(
        window,
        &WindowIdentitySnapshot {
            hwnd: live.hwnd,
            is_window: true,
            live_title: live.title.clone(),
            process_path: live.process_path.clone(),
            executable: live.executable.clone(),
            class_name: live.class_name.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(process_path: &str, executable: &str, class_name: &str, title: &str) -> MmWindow {
        MmWindow {
            process_path: process_path.into(),
            executable: executable.into(),
            class_name: class_name.into(),
            captured_title: title.into(),
            hwnd: 10,
            ..Default::default()
        }
    }

    fn live(
        process_path: &str,
        executable: &str,
        class_name: &str,
        title: &str,
    ) -> WindowIdentitySnapshot {
        WindowIdentitySnapshot {
            hwnd: 10,
            is_window: true,
            live_title: title.into(),
            process_path: process_path.into(),
            executable: executable.into(),
            class_name: class_name.into(),
        }
    }

    #[test]
    fn case_insensitive_process_path_comparison() {
        assert!(same_process_path(
            r" C:\Apps\EDITOR.EXE ",
            r"c:\apps\editor.exe"
        ));
    }

    #[test]
    fn slash_normalization() {
        assert!(same_process_path(
            "C:/Apps/editor.exe",
            r"c:\apps\EDITOR.exe"
        ));
    }

    #[test]
    fn title_ignored_during_existing_hwnd_verification() {
        let stored = window("C:/Apps/editor.exe", "", "Editor", "Original Title");
        let live = live(
            "c:/apps/editor.exe",
            "editor.exe",
            "editor",
            "Completely Different",
        );
        assert!(stable_identity_matches(&stored, &live));
    }

    #[test]
    fn process_path_mismatch_rejection() {
        let stored = window("C:/Apps/editor.exe", "editor.exe", "Editor", "Doc");
        let live = live("C:/Other/other.exe", "editor.exe", "Editor", "Doc");
        assert!(!stable_identity_matches(&stored, &live));
    }

    #[test]
    fn executable_class_fallback_when_process_path_is_absent() {
        let stored = window("", "EDITOR.EXE", " EditorClass ", "Doc");
        let live = live("", "editor.exe", "editorclass", "Other");
        assert!(stable_identity_matches(&stored, &live));
    }

    #[test]
    fn missing_stable_metadata_cannot_safely_verify_hwnd_across_restart() {
        let stored = window("", "", "", "Doc");
        let live = live("", "", "", "Doc");
        assert!(!stable_identity_matches(&stored, &live));
    }
}
