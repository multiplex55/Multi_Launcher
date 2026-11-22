use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    rc::Rc,
};

use gpui_kit::component::label::Label;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Frontend {
    #[default]
    Egui,
    Gpui,
}

pub(crate) fn selection_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Frontend, String> {
    let mut selection = Frontend::Egui;
    let mut has_explicit_selection = false;

    for arg in args {
        match arg.as_os_str() {
            value if value == OsStr::new("--ui=egui") => {
                if has_explicit_selection {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = Frontend::Egui;
                has_explicit_selection = true;
            }
            value if value == OsStr::new("--ui=gpui") => {
                if has_explicit_selection {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = Frontend::Gpui;
                has_explicit_selection = true;
            }
            value if value == OsStr::new("--ui") => {
                return Err("Choose --ui=egui or --ui=gpui.".to_owned());
            }
            value if value.to_string_lossy().starts_with("--ui=") => {
                return Err("Choose --ui=egui or --ui=gpui.".to_owned());
            }
            _ => {}
        }
    }

    Ok(selection)
}

pub(crate) fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows::{
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            },
            core::PCWSTR,
        };

        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Multi Launcher".encode_utf16().chain(Some(0)).collect();

        // SAFETY: both strings remain alive and NUL-terminated throughout the call.
        unsafe {
            MessageBoxW(
                HWND::default(),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    eprintln!("Multi Launcher: {message}");
}

struct FoundationWindow;

impl Render for FoundationWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(Label::new("GPUI Kit foundation is running"))
    }
}

pub(crate) fn run_foundation_window() -> Result<(), String> {
    let startup_error = Rc::new(RefCell::new(None));
    let startup_error_sink = Rc::clone(&startup_error);

    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Foundation".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Err(error) = open_window(options, cx, |_, cx| cx.new(|_| FoundationWindow)) {
            *startup_error_sink.borrow_mut() = Some(error.to_string());
            cx.quit();
        }
    });

    match startup_error.borrow_mut().take() {
        Some(error) => Err(format!(
            "GPUI could not open its window: {error}. Restart with --ui=egui to use the fallback frontend."
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Frontend, selection_from_args};
    use std::ffi::OsString;

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|arg| OsString::from(*arg)).collect()
    }

    #[test]
    fn defaults_to_egui_and_preserves_other_arguments() {
        assert_eq!(
            selection_from_args(args(&["--open", "settings"])),
            Ok(Frontend::Egui)
        );
    }

    #[test]
    fn selects_each_explicit_frontend() {
        assert_eq!(
            selection_from_args(args(&["--ui=egui"])),
            Ok(Frontend::Egui)
        );
        assert_eq!(
            selection_from_args(args(&["--ui=gpui"])),
            Ok(Frontend::Gpui)
        );
    }

    #[test]
    fn rejects_invalid_and_repeated_frontend_selectors() {
        assert!(selection_from_args(args(&["--ui=unknown"])).is_err());
        assert!(selection_from_args(args(&["--ui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=gpui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=egui"])).is_err());
    }
}
