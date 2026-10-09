use crate::commands::{
    CommandError, CommandOutcome, CoordinateToolCommand, HistoryPolicy, LauncherCommandHost,
    ToastPolicy,
};

pub(crate) fn handle_coordinate_tool<H: LauncherCommandHost + ?Sized>(
    host: &mut H,
    command: &CoordinateToolCommand,
) -> Result<CommandOutcome, CommandError> {
    let copied = match command {
        CoordinateToolCommand::Settings => {
            host.open_mouse_settings().map_err(|message| {
                let mut error = CommandError::new("mouse", message);
                error.toast = true;
                error
            })?;
            return Ok(CommandOutcome::default());
        }
        CoordinateToolCommand::Help => {
            return Ok(help_outcome(MOUSE_HELP));
        }
        CoordinateToolCommand::HudHelp => {
            return Ok(help_outcome(COORDINATE_HELP));
        }
        CoordinateToolCommand::CrosshairHelp => {
            return Ok(help_outcome(CROSSHAIR_HELP));
        }
        CoordinateToolCommand::HaloHelp => {
            return Ok(help_outcome(HALO_HELP));
        }
        CoordinateToolCommand::ZoomHelp => {
            return Ok(help_outcome(ZOOM_HELP));
        }
        CoordinateToolCommand::Invalid { error, .. } => {
            let mut error = CommandError::new("mouse", error.clone());
            error.toast = true;
            return Err(error);
        }
        _ => host
            .execute_coordinate_tool_command(command)
            .map_err(|message| {
                let mut error = CommandError::new("mouse", message);
                error.toast = true;
                error
            })?,
    };

    let toasts = match command {
        CoordinateToolCommand::Pick => copied.map(ToastPolicy::Info).into_iter().collect(),
        CoordinateToolCommand::Cancel => copied.map(ToastPolicy::Info).into_iter().collect(),
        CoordinateToolCommand::Copy => copied.into_iter().map(ToastPolicy::Copied).collect(),
        _ => Vec::new(),
    };

    Ok(CommandOutcome {
        history: HistoryPolicy::Skip,
        toasts,
        ..CommandOutcome::default()
    })
}

fn help_outcome(text: &str) -> CommandOutcome {
    CommandOutcome {
        history: HistoryPolicy::Skip,
        toasts: vec![ToastPolicy::Info(text.into())],
        ..CommandOutcome::default()
    }
}

const MOUSE_HELP: &str = "Mouse controls: `mouse settings` opens the saved appearance UI. Quick actions: `mouse coords toggle`, `mouse coords copy`, `mouse coords pick`, and `mouse coords cancel`; crosshair controls are `mouse crosshair toggle|on|off`, halo controls are `mouse halo toggle|on|off`, and magnifier controls are `mouse zoom toggle|on|off`. Zoom is a live magnifier of actual desktop content on supported composited Windows surfaces. `mouse effects off` disables the crosshair, halo, and magnifier while preserving the coordinate HUD and an active pick. Enablement is session-only; appearance is saved in Mouse Settings. Halo inversion requires supported composited Windows content and may use a non-inverting outline fallback. Advanced commands: `mouse coords space desktop|monitor|client`, `mouse coords compact|detailed`, `mouse coords offset <signed-x> <signed-y>` (-512..512), `mouse coords freeze|unfreeze`, `mouse crosshair color #rrggbb`, `mouse crosshair thickness` (1..16), `mouse crosshair length` (2..256), `mouse crosshair gap <0..128>` (per-arm physical pixels), `mouse crosshair opacity` (0.1..1.0), `mouse crosshair guides on|off`, and `mouse crosshair contrast on|off`. Coordinates are signed physical pixels. A pick consumes a fresh left press and release; Escape cancels without changing the clipboard. Picking uses its click-time sample even while the HUD is frozen. Client coordinates use the foreground client origin, or the last external active window while Multi Launcher is foreground. See `mouse coords help`, `mouse crosshair help`, `mouse halo help`, and `mouse zoom help` for details.";

const COORDINATE_HELP: &str = "Coordinate HUD: `mouse coords toggle` toggles the HUD; use `mouse coords on|off`, `mouse coords space desktop|monitor|client`, `mouse coords compact|detailed`, or `mouse coords offset <signed-x> <signed-y>` (-512..512). Use `mouse coords freeze|unfreeze`; `mouse coords copy` writes the displayed signed physical-pixel position as x,y. `mouse coords pick` copies the next click's physical x,y in the selected space after consuming its press and release; Escape cancels without changing the clipboard. Pick uses its click-time sample even while the HUD is frozen. `mouse coords cancel` requests safe cancellation and drains any consumed click release. Desktop uses signed virtual-desktop coordinates, monitor is relative to the cursor monitor origin, and client is relative to the foreground client origin. While Multi Launcher is foreground, client coordinates use the last external active window. For display preferences, use `mouse settings`. The crosshair is independent; see `mouse crosshair help`.";

const CROSSHAIR_HELP: &str = "Crosshair controls: `mouse crosshair toggle|on|off`; `mouse crosshair color #rrggbb`; `mouse crosshair thickness <1..16>`; `mouse crosshair length <2..256>`; `mouse crosshair gap <0..128>` sets the per-arm physical-pixel clearance from the cursor hotspot, including the outline; `mouse crosshair opacity <0.1..1.0>`; `mouse crosshair guides on|off`; `mouse crosshair contrast on|off`. The crosshair is independent from the coordinate HUD; `mouse effects off` disables the crosshair and cursor effects while leaving the HUD and active pick alone.";

const HALO_HELP: &str = "Cursor halo controls: `mouse halo toggle|on|off` changes session enablement immediately. Its radius, inversion strength, and outline are saved in `mouse settings`. On supported composited Windows content the halo inverts desktop colors; if native inversion is unavailable, the status reports a non-inverting outline fallback. See `mouse help` for `mouse effects off`, which preserves the coordinate HUD and active pick.";

const ZOOM_HELP: &str = "Cursor magnifier controls: `mouse zoom toggle|on|off` changes session enablement immediately. The live screen lens follows the current cursor; its factor, centered/offset destination, diameter, and outline are saved in `mouse settings`. Magnification depends on supported composited Windows desktop content. See `mouse help` for `mouse effects off`, which preserves the coordinate HUD and active pick.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CoordinateToolCommand;

    #[derive(Default)]
    struct Host {
        calls: Vec<CoordinateToolCommand>,
        copy: Option<String>,
        error: Option<String>,
        settings_opened: usize,
    }

    impl LauncherCommandHost for Host {
        fn launcher_is_visible(&self) -> bool {
            true
        }

        fn open_mouse_settings(&mut self) -> Result<(), String> {
            self.settings_opened += 1;
            Ok(())
        }

        fn execute_coordinate_tool_command(
            &mut self,
            command: &CoordinateToolCommand,
        ) -> Result<Option<String>, String> {
            self.calls.push(command.clone());
            self.error.clone().map_or(Ok(self.copy.clone()), Err)
        }
    }

    #[test]
    fn passive_controls_dispatch_once_without_query_visibility_or_focus_effects() {
        let mut host = Host::default();
        let outcome = handle_coordinate_tool(&mut host, &CoordinateToolCommand::ToggleHud).unwrap();
        assert_eq!(outcome, CommandOutcome::default());

        let gap = CoordinateToolCommand::SetCrosshairGap(16);
        let outcome = handle_coordinate_tool(&mut host, &gap).unwrap();
        assert_eq!(host.calls, [CoordinateToolCommand::ToggleHud, gap]);
        assert_eq!(outcome, CommandOutcome::default());
    }

    #[test]
    fn halo_zoom_and_effects_off_dispatch_once_without_copy_feedback() {
        let mut host = Host {
            copy: Some("stale copied value".into()),
            ..Host::default()
        };
        let commands = [
            CoordinateToolCommand::ToggleHalo,
            CoordinateToolCommand::SetHaloEnabled(true),
            CoordinateToolCommand::ToggleZoom,
            CoordinateToolCommand::SetZoomEnabled(false),
            CoordinateToolCommand::EffectsOff,
        ];
        for command in &commands {
            let outcome = handle_coordinate_tool(&mut host, command).unwrap();
            assert_eq!(outcome, CommandOutcome::default());
            assert!(outcome.toasts.is_empty());
        }
        assert_eq!(host.calls, commands.to_vec());
        assert_eq!(host.settings_opened, 0);
    }

    #[test]
    fn copy_feedback_is_reported_only_after_host_success() {
        let mut host = Host {
            copy: Some("-20,14".into()),
            ..Host::default()
        };
        let outcome = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Copy).unwrap();
        assert_eq!(outcome.toasts, [ToastPolicy::Copied("-20,14".into())]);
        assert_eq!(outcome.history, HistoryPolicy::Skip);

        host.error = Some("clipboard unavailable".into());
        let error = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Copy).unwrap_err();
        assert!(error.toast);
        assert!(!error.refocus);
    }

    #[test]
    fn pick_feedback_is_informational_and_cancel_is_a_noop_without_a_host_message() {
        let mut host = Host {
            copy: Some("Click once to copy coordinates; press Escape to cancel.".into()),
            ..Host::default()
        };
        let picked = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Pick).unwrap();
        assert_eq!(
            picked.toasts,
            [ToastPolicy::Info(
                "Click once to copy coordinates; press Escape to cancel.".into()
            )]
        );
        assert_eq!(host.calls, [CoordinateToolCommand::Pick]);

        host.calls.clear();
        host.copy = None;
        let cancelled = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Cancel).unwrap();
        assert!(cancelled.toasts.is_empty());
        assert_eq!(host.calls, [CoordinateToolCommand::Cancel]);
    }

    #[test]
    fn help_is_pure_and_invalid_commands_do_not_reach_the_host() {
        let mut host = Host::default();
        let general = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Help).unwrap();
        assert!(matches!(
            general.toasts.as_slice(),
            [ToastPolicy::Info(text)] if text.contains("mouse settings") && text.contains("mouse coords pick") && text.contains("mouse crosshair gap <0..128>") && text.contains("mouse halo toggle|on|off") && text.contains("mouse zoom toggle|on|off") && text.contains("mouse effects off") && text.contains("preserving the coordinate HUD and an active pick") && text.contains("non-inverting outline fallback")
        ));
        let crosshair_help =
            handle_coordinate_tool(&mut host, &CoordinateToolCommand::CrosshairHelp).unwrap();
        assert!(matches!(
            crosshair_help.toasts.as_slice(),
            [ToastPolicy::Info(text)] if text.contains("mouse crosshair gap <0..128>") && text.contains("per-arm physical-pixel clearance")
        ));
        let halo_help =
            handle_coordinate_tool(&mut host, &CoordinateToolCommand::HaloHelp).unwrap();
        assert!(matches!(
            halo_help.toasts.as_slice(),
            [ToastPolicy::Info(text)] if text.contains("mouse halo toggle|on|off") && text.contains("non-inverting outline fallback") && text.contains("mouse settings")
        ));
        let zoom_help =
            handle_coordinate_tool(&mut host, &CoordinateToolCommand::ZoomHelp).unwrap();
        assert!(matches!(
            zoom_help.toasts.as_slice(),
            [ToastPolicy::Info(text)] if text.contains("live screen lens") && text.contains("mouse zoom toggle|on|off") && text.contains("mouse settings")
        ));
        let help = handle_coordinate_tool(&mut host, &CoordinateToolCommand::HudHelp).unwrap();
        assert!(
            matches!(help.toasts.as_slice(), [ToastPolicy::Info(text)] if text.contains("last external active window") && text.contains("mouse coords pick") && text.contains("Escape cancels"))
        );
        assert_eq!(
            help,
            CommandOutcome {
                history: HistoryPolicy::Skip,
                toasts: help.toasts.clone(),
                ..CommandOutcome::default()
            }
        );
        assert!(host.calls.is_empty());

        let invalid = CoordinateToolCommand::Invalid {
            raw: "mouse:coords:offset:513:0".into(),
            error: "mouse coords offset x must be in the range -512..512".into(),
        };
        let error = handle_coordinate_tool(&mut host, &invalid).unwrap_err();
        assert_eq!(error.domain, "mouse");
        assert!(error.toast);
        assert!(!error.refocus);
        assert!(host.calls.is_empty());
        assert_eq!(host.settings_opened, 0);
    }

    #[test]
    fn settings_routes_to_launcher_host_without_mutating_coordinate_runtime() {
        let mut host = Host::default();
        let outcome = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Settings).unwrap();
        assert_eq!(host.settings_opened, 1);
        assert!(host.calls.is_empty());
        assert_eq!(outcome, CommandOutcome::default());
    }
}
