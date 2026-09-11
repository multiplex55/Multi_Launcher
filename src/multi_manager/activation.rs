use crate::multi_manager::model::{MmRect, MmWorkspace};
use crate::multi_manager::runtime::{WinWindowOps, WindowOps};
use crate::multi_manager::win;
use crate::virtual_desktop::{VirtualDesktopBinding, VirtualDesktopId, VirtualDesktopService};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOperation {
    Toggle,
    SendHome,
    SendTarget,
    Rotate,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ActivationResult {
    pub moved: usize,
    pub already_bound: usize,
    pub closed: usize,
    pub missing: usize,
    pub ambiguous: usize,
    pub metadata_mismatch: usize,
    pub unresolved_labels: Vec<String>,
    pub bindings_changed: bool,
    pub movement_errors: Vec<String>,
}

pub struct ActivationDeps<'a, O: WindowOps> {
    pub window_ops: &'a O,
    pub is_window: &'a dyn Fn(usize) -> bool,
    pub desktop_ops: &'a dyn DesktopOps,
}

pub trait DesktopOps {
    fn resolve_binding(&self, binding: &VirtualDesktopBinding) -> Result<VirtualDesktopId, String>;
    fn move_window(&self, hwnd: usize, desktop: &VirtualDesktopId) -> Result<(), String>;
    fn switch(&self, desktop: &VirtualDesktopId) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WinDesktopOps;

impl DesktopOps for WinDesktopOps {
    fn resolve_binding(&self, binding: &VirtualDesktopBinding) -> Result<VirtualDesktopId, String> {
        let snapshot = VirtualDesktopService
            .snapshot()
            .map_err(|error| error.to_string())?;
        snapshot
            .resolve_binding(binding)
            .map(|desktop| desktop.id.clone())
            .map_err(|error| error.to_string())
    }

    fn move_window(&self, hwnd: usize, desktop: &VirtualDesktopId) -> Result<(), String> {
        #[cfg(windows)]
        {
            VirtualDesktopService
                .move_window_to_desktop(windows::Win32::Foundation::HWND(hwnd as *mut _), desktop)
                .map_err(|error| error.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = (hwnd, desktop);
            Err("Virtual desktop window movement is available only on Windows".into())
        }
    }

    fn switch(&self, desktop: &VirtualDesktopId) -> Result<(), String> {
        VirtualDesktopService
            .switch_to_id(desktop)
            .map_err(|error| error.to_string())
    }
}

impl ActivationResult {
    pub fn all_active_handled(&self) -> bool {
        self.movement_errors.is_empty()
            && self.closed == 0
            && self.missing == 0
            && self.ambiguous == 0
            && self.metadata_mismatch == 0
    }

    pub fn has_unresolved(&self) -> bool {
        self.closed > 0 || self.missing > 0 || self.ambiguous > 0 || self.metadata_mismatch > 0
    }

    fn merge(&mut self, other: ActivationResult) {
        self.moved += other.moved;
        self.already_bound += other.already_bound;
        self.closed += other.closed;
        self.missing += other.missing;
        self.ambiguous += other.ambiguous;
        self.metadata_mismatch += other.metadata_mismatch;
        self.unresolved_labels.extend(other.unresolved_labels);
        self.bindings_changed |= other.bindings_changed;
        self.movement_errors.extend(other.movement_errors);
    }
}

pub fn activate_workspace(
    workspaces: &mut [MmWorkspace],
    workspace_id: &str,
    operation: ActivationOperation,
) -> Option<ActivationResult> {
    let ops = WinWindowOps;
    let desktop_ops = WinDesktopOps;
    let deps = ActivationDeps {
        window_ops: &ops,
        is_window: &|hwnd| win::is_valid_window(hwnd),
        desktop_ops: &desktop_ops,
    };
    activate_workspace_with_deps(workspaces, workspace_id, operation, &deps)
}

pub fn activate_all_home(workspaces: &mut [MmWorkspace]) -> ActivationResult {
    let ops = WinWindowOps;
    let desktop_ops = WinDesktopOps;
    let deps = ActivationDeps {
        window_ops: &ops,
        is_window: &|hwnd| win::is_valid_window(hwnd),
        desktop_ops: &desktop_ops,
    };
    activate_all_home_with_deps(workspaces, &deps)
}

pub fn activate_workspace_with_deps<O: WindowOps>(
    workspaces: &mut [MmWorkspace],
    workspace_id: &str,
    operation: ActivationOperation,
    deps: &ActivationDeps<'_, O>,
) -> Option<ActivationResult> {
    let workspace = workspaces
        .iter_mut()
        .find(|workspace| workspace.id == workspace_id)?;
    Some(activate_one_workspace(workspace, operation, deps))
}

pub fn activate_all_home_with_deps<O: WindowOps>(
    workspaces: &mut [MmWorkspace],
    deps: &ActivationDeps<'_, O>,
) -> ActivationResult {
    let mut result = ActivationResult::default();
    for workspace in workspaces.iter_mut() {
        result.merge(activate_one_workspace(
            workspace,
            ActivationOperation::SendHome,
            deps,
        ));
    }
    result
}

/// Copy activation-owned runtime fields back only if they did not change while
/// the unlocked native work was running.
pub struct ActivationStateBaseline {
    rotation_offset: usize,
    windows: Vec<WindowActivationBaseline>,
}

struct WindowActivationBaseline {
    hwnd: usize,
    valid: bool,
    binding_status: crate::multi_manager::model::MmBindingStatus,
    binding_verified: bool,
    live_title: String,
}

pub fn capture_activation_state(workspace: &MmWorkspace) -> ActivationStateBaseline {
    ActivationStateBaseline {
        rotation_offset: workspace.rotation_offset,
        windows: workspace
            .windows
            .iter()
            .map(|window| WindowActivationBaseline {
                hwnd: window.hwnd,
                valid: window.valid,
                binding_status: window.binding_status,
                binding_verified: window.binding_verified,
                live_title: window.live_title.clone(),
            })
            .collect(),
    }
}

pub fn merge_activation_state_if_unchanged(
    target: &mut MmWorkspace,
    before: &ActivationStateBaseline,
    activated: &MmWorkspace,
) {
    if target.rotation_offset == before.rotation_offset {
        target.rotation_offset = activated.rotation_offset;
    }
    for ((target_window, before_window), activated_window) in target
        .windows
        .iter_mut()
        .zip(&before.windows)
        .zip(&activated.windows)
    {
        let unchanged = target_window.hwnd == before_window.hwnd
            && target_window.valid == before_window.valid
            && target_window.binding_status == before_window.binding_status
            && target_window.binding_verified == before_window.binding_verified
            && target_window.live_title == before_window.live_title;
        if unchanged && target_window.captured_title == activated_window.captured_title {
            target_window.hwnd = activated_window.hwnd;
            target_window.valid = activated_window.valid;
            target_window.binding_status = activated_window.binding_status;
            target_window.binding_verified = activated_window.binding_verified;
            target_window.live_title = activated_window.live_title.clone();
        }
    }
}

fn activate_one_workspace<O: WindowOps>(
    workspace: &mut MmWorkspace,
    operation: ActivationOperation,
    deps: &ActivationDeps<'_, O>,
) -> ActivationResult {
    let mut result = ActivationResult::default();
    if workspace.disabled || !workspace.valid {
        return result;
    }

    let before: Vec<(usize, bool)> = workspace
        .windows
        .iter()
        .map(|w| (w.hwnd, w.binding_verified))
        .collect();

    for window in workspace.windows.iter_mut().filter(|w| !w.disabled) {
        if window.hwnd != 0 {
            if (deps.is_window)(window.hwnd) {
                result.already_bound += 1;
            } else {
                window.mark_closed();
                window.live_title.clear();
                result.closed += 1;
                result.unresolved_labels.push(window_label(window));
            }
            continue;
        }

        match window.binding_status {
            crate::multi_manager::model::MmBindingStatus::Closed => result.closed += 1,
            crate::multi_manager::model::MmBindingStatus::Ambiguous => result.ambiguous += 1,
            crate::multi_manager::model::MmBindingStatus::MetadataMismatch => {
                result.metadata_mismatch += 1;
            }
            _ => result.missing += 1,
        }
        result.unresolved_labels.push(window_label(window));
    }

    let placement = resolve_placement(workspace, operation, deps.window_ops);
    let mut placement_ready = true;
    if placement.requires_target_desktop()
        && let Some(binding) = &workspace.virtual_desktop
    {
        match deps.desktop_ops.resolve_binding(binding) {
            Ok(desktop_id) => {
                let errors_before = result.movement_errors.len();
                for window in workspace
                    .windows
                    .iter()
                    .filter(|window| window.can_activate())
                {
                    if let Err(error) = deps.desktop_ops.move_window(window.hwnd, &desktop_id) {
                        result
                            .movement_errors
                            .push(format!("{} desktop: {error}", window.hwnd));
                    }
                }
                if result.movement_errors.len() == errors_before {
                    if let Err(error) = deps.desktop_ops.switch(&desktop_id) {
                        placement_ready = false;
                        result
                            .movement_errors
                            .push(format!("desktop switch: {error}"));
                    }
                } else {
                    placement_ready = false;
                }
            }
            Err(error) => {
                placement_ready = false;
                result
                    .movement_errors
                    .push(format!("desktop binding: {error}"));
            }
        }
    }

    match placement {
        _ if !placement_ready => {}
        PlacementKind::Home => move_kind(workspace, RectKind::Home, deps.window_ops, &mut result),
        PlacementKind::Target => {
            move_kind(workspace, RectKind::Target, deps.window_ops, &mut result)
        }
        PlacementKind::Rotate => rotate(workspace, deps.window_ops, &mut result),
    }

    let after: Vec<(usize, bool)> = workspace
        .windows
        .iter()
        .map(|w| (w.hwnd, w.binding_verified))
        .collect();
    result.bindings_changed = before != after;
    result
}

fn window_label(window: &crate::multi_manager::model::MmWindow) -> String {
    let alias = window.alias.trim();
    if !alias.is_empty() {
        alias.to_string()
    } else {
        window.current_display_title().to_string()
    }
}

#[derive(Clone, Copy)]
enum RectKind {
    Home,
    Target,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlacementKind {
    Home,
    Target,
    Rotate,
}

impl PlacementKind {
    fn requires_target_desktop(self) -> bool {
        matches!(self, Self::Target | Self::Rotate)
    }
}

fn resolve_placement<O: WindowOps>(
    workspace: &MmWorkspace,
    operation: ActivationOperation,
    ops: &O,
) -> PlacementKind {
    match operation {
        ActivationOperation::SendHome => PlacementKind::Home,
        ActivationOperation::SendTarget => PlacementKind::Target,
        ActivationOperation::Rotate => PlacementKind::Rotate,
        ActivationOperation::Toggle if workspace.rotate => PlacementKind::Rotate,
        ActivationOperation::Toggle => {
            let all_at_home = workspace
                .windows
                .iter()
                .filter(|window| window.can_activate())
                .all(|window| {
                    window
                        .home_rect
                        .is_some_and(|rect| ops.is_window_at_rect(window.hwnd, rect))
                });
            if all_at_home {
                PlacementKind::Target
            } else {
                PlacementKind::Home
            }
        }
    }
}

fn move_kind<O: WindowOps>(
    workspace: &MmWorkspace,
    kind: RectKind,
    ops: &O,
    result: &mut ActivationResult,
) {
    for window in workspace.windows.iter().filter(|w| w.can_activate()) {
        let rect = match kind {
            RectKind::Home => window.home_rect.or(workspace.home_rect),
            RectKind::Target => window.target_rect.or(workspace.target_rect),
        };
        if let Some(rect) = rect {
            move_one(window.hwnd, rect, ops, result);
        }
    }
}

fn rotate<O: WindowOps>(workspace: &mut MmWorkspace, ops: &O, result: &mut ActivationResult) {
    let valid_indices: Vec<usize> = workspace
        .windows
        .iter()
        .enumerate()
        .filter_map(|(idx, window)| window.can_activate().then_some(idx))
        .collect();
    if valid_indices.is_empty() {
        return;
    }
    let primary = workspace.windows[valid_indices[0]].target_rect;
    let slots: Vec<MmRect> = valid_indices
        .iter()
        .filter_map(|&idx| workspace.windows[idx].home_rect)
        .collect();
    if slots.is_empty() {
        return;
    }
    let offset = workspace.rotation_offset % valid_indices.len();
    for (slot_idx, &window_idx) in valid_indices
        .iter()
        .cycle()
        .skip(offset)
        .take(valid_indices.len())
        .enumerate()
    {
        let target = if slot_idx == 0 {
            primary
        } else {
            slots.get(slot_idx - 1).copied()
        };
        if let Some(rect) = target {
            move_one(workspace.windows[window_idx].hwnd, rect, ops, result);
        }
    }
    workspace.rotation_offset = workspace.rotation_offset.wrapping_add(1);
}

fn move_one<O: WindowOps>(hwnd: usize, rect: MmRect, ops: &O, result: &mut ActivationResult) {
    match ops.move_window_to_rect(hwnd, rect) {
        Ok(()) => result.moved += 1,
        Err(err) => result.movement_errors.push(format!("{hwnd}: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmBindingStatus, MmRect, MmWindow};
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeOps {
        moves: RefCell<Vec<(usize, MmRect)>>,
    }
    impl WindowOps for FakeOps {
        fn is_window_at_rect(&self, _hwnd: usize, _rect: MmRect) -> bool {
            false
        }
        fn move_window_to_rect(&self, hwnd: usize, rect: MmRect) -> anyhow::Result<()> {
            self.moves.borrow_mut().push((hwnd, rect));
            Ok(())
        }
    }
    #[derive(Default)]
    struct FakeDesktopOps {
        moves: RefCell<Vec<(usize, VirtualDesktopId)>>,
        switches: RefCell<Vec<VirtualDesktopId>>,
    }
    impl DesktopOps for FakeDesktopOps {
        fn resolve_binding(
            &self,
            binding: &VirtualDesktopBinding,
        ) -> Result<VirtualDesktopId, String> {
            Ok(binding.id.clone())
        }
        fn move_window(&self, hwnd: usize, desktop: &VirtualDesktopId) -> Result<(), String> {
            self.moves.borrow_mut().push((hwnd, desktop.clone()));
            Ok(())
        }
        fn switch(&self, desktop: &VirtualDesktopId) -> Result<(), String> {
            self.switches.borrow_mut().push(desktop.clone());
            Ok(())
        }
    }

    fn rect(x: i32) -> MmRect {
        MmRect {
            x,
            y: 0,
            w: 10,
            h: 10,
        }
    }
    fn win(hwnd: usize, title: &str) -> MmWindow {
        MmWindow {
            hwnd,
            valid: hwnd != 0,
            binding_verified: hwnd != 0,
            captured_title: title.into(),
            alias: title.into(),
            home_rect: Some(rect(hwnd as i32)),
            target_rect: Some(rect(hwnd as i32 + 10)),
            ..MmWindow::default()
        }
    }
    fn ws(windows: Vec<MmWindow>) -> MmWorkspace {
        MmWorkspace {
            id: "ws".into(),
            windows,
            ..MmWorkspace::default()
        }
    }

    #[test]
    fn closing_one_of_three_windows_still_moves_other_two() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let mut workspaces = vec![ws(vec![win(1, "one"), win(2, "two"), win(3, "three")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd != 2,
            desktop_ops: &desktop_ops,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendTarget,
            &deps,
        )
        .unwrap();
        assert_eq!(result.moved, 2);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(11)), (3, rect(13))]);
    }

    #[test]
    fn closed_binding_is_cleared() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let mut workspaces = vec![ws(vec![win(9, "gone")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| false,
            desktop_ops: &desktop_ops,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendHome,
            &deps,
        )
        .unwrap();
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert_eq!(
            workspaces[0].windows[0].binding_status,
            MmBindingStatus::Closed
        );
        assert_eq!(result.unresolved_labels, vec!["gone"]);
        assert!(result.bindings_changed);
    }

    #[test]
    fn matching_live_candidate_is_not_auto_reconnected_during_activation() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let mut workspaces = vec![ws(vec![win(0, "App")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd == 42,
            desktop_ops: &desktop_ops,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendTarget,
            &deps,
        )
        .unwrap();
        assert_eq!(result.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert!(ops.moves.borrow().is_empty());
    }

    #[test]
    fn warning_result_includes_unresolved_labels() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let mut missing = win(0, "Captured");
        missing.alias = "Alias".into();
        let mut workspaces = vec![ws(vec![win(1, "ok"), missing])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd == 1,
            desktop_ops: &desktop_ops,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendHome,
            &deps,
        )
        .unwrap();
        assert_eq!(result.moved, 1);
        assert_eq!(result.unresolved_labels, vec!["Alias"]);
    }

    #[test]
    fn target_and_rotate_share_bound_desktop_placement() {
        for operation in [ActivationOperation::SendTarget, ActivationOperation::Rotate] {
            let ops = FakeOps::default();
            let desktop_ops = FakeDesktopOps::default();
            let desktop_id =
                VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
            let mut workspace = ws(vec![win(1, "one"), win(2, "two")]);
            workspace.virtual_desktop = Some(crate::virtual_desktop::VirtualDesktopBinding {
                id: desktop_id.clone(),
                cached_name: Some("Work".into()),
            });
            let mut workspaces = vec![workspace];
            let deps = ActivationDeps {
                window_ops: &ops,
                is_window: &|_| true,
                desktop_ops: &desktop_ops,
            };
            activate_workspace_with_deps(&mut workspaces, "ws", operation, &deps).unwrap();
            assert_eq!(
                *desktop_ops.moves.borrow(),
                vec![(1, desktop_id.clone()), (2, desktop_id.clone())]
            );
            assert_eq!(*desktop_ops.switches.borrow(), vec![desktop_id]);
        }
    }

    #[test]
    fn toggle_uses_desktop_only_for_home_to_target_half() {
        struct ToggleOps {
            at_home: bool,
            moves: RefCell<Vec<(usize, MmRect)>>,
        }
        impl WindowOps for ToggleOps {
            fn is_window_at_rect(&self, _: usize, _: MmRect) -> bool {
                self.at_home
            }
            fn move_window_to_rect(&self, hwnd: usize, rect: MmRect) -> anyhow::Result<()> {
                self.moves.borrow_mut().push((hwnd, rect));
                Ok(())
            }
        }
        for (at_home, expect_desktop, expected_rect) in [(true, true, 11), (false, false, 1)] {
            let ops = ToggleOps {
                at_home,
                moves: RefCell::new(Vec::new()),
            };
            let desktop_ops = FakeDesktopOps::default();
            let desktop_id =
                VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
            let mut workspace = ws(vec![win(1, "one")]);
            workspace.virtual_desktop = Some(VirtualDesktopBinding {
                id: desktop_id,
                cached_name: Some("Work".into()),
            });
            let mut workspaces = vec![workspace];
            let deps = ActivationDeps {
                window_ops: &ops,
                is_window: &|_| true,
                desktop_ops: &desktop_ops,
            };
            activate_workspace_with_deps(&mut workspaces, "ws", ActivationOperation::Toggle, &deps)
                .unwrap();
            assert_eq!(!desktop_ops.switches.borrow().is_empty(), expect_desktop);
            assert_eq!(*ops.moves.borrow(), vec![(1, rect(expected_rect))]);
        }
    }

    #[test]
    fn bound_target_orders_desktop_move_switch_then_geometry() {
        use std::rc::Rc;
        struct OrderedWindowOps(Rc<RefCell<Vec<&'static str>>>);
        impl WindowOps for OrderedWindowOps {
            fn is_window_at_rect(&self, _: usize, _: MmRect) -> bool {
                false
            }
            fn move_window_to_rect(&self, _: usize, _: MmRect) -> anyhow::Result<()> {
                self.0.borrow_mut().push("geometry");
                Ok(())
            }
        }
        struct OrderedDesktopOps(Rc<RefCell<Vec<&'static str>>>);
        impl DesktopOps for OrderedDesktopOps {
            fn resolve_binding(
                &self,
                binding: &VirtualDesktopBinding,
            ) -> Result<VirtualDesktopId, String> {
                Ok(binding.id.clone())
            }
            fn move_window(&self, _: usize, _: &VirtualDesktopId) -> Result<(), String> {
                self.0.borrow_mut().push("desktop_move");
                Ok(())
            }
            fn switch(&self, _: &VirtualDesktopId) -> Result<(), String> {
                self.0.borrow_mut().push("switch");
                Ok(())
            }
        }
        let effects = Rc::new(RefCell::new(Vec::new()));
        let window_ops = OrderedWindowOps(Rc::clone(&effects));
        let desktop_ops = OrderedDesktopOps(Rc::clone(&effects));
        let mut workspace = ws(vec![win(1, "one")]);
        workspace.virtual_desktop = Some(VirtualDesktopBinding {
            id: VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            cached_name: None,
        });
        let mut workspaces = vec![workspace];
        let deps = ActivationDeps {
            window_ops: &window_ops,
            is_window: &|_| true,
            desktop_ops: &desktop_ops,
        };
        activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendTarget,
            &deps,
        )
        .unwrap();
        assert_eq!(*effects.borrow(), ["desktop_move", "switch", "geometry"]);
    }

    #[test]
    fn failed_desktop_association_prevents_switch_and_geometry() {
        struct FailingDesktopOps;
        impl DesktopOps for FailingDesktopOps {
            fn resolve_binding(
                &self,
                binding: &VirtualDesktopBinding,
            ) -> Result<VirtualDesktopId, String> {
                Ok(binding.id.clone())
            }
            fn move_window(&self, _: usize, _: &VirtualDesktopId) -> Result<(), String> {
                Err("association failed".into())
            }
            fn switch(&self, _: &VirtualDesktopId) -> Result<(), String> {
                panic!("failed association must prevent switching")
            }
        }
        let ops = FakeOps::default();
        let mut workspace = ws(vec![win(1, "one")]);
        workspace.virtual_desktop = Some(VirtualDesktopBinding {
            id: VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            cached_name: None,
        });
        let mut workspaces = vec![workspace];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
            desktop_ops: &FailingDesktopOps,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendTarget,
            &deps,
        )
        .unwrap();
        assert!(result.movement_errors[0].contains("association failed"));
        assert!(ops.moves.borrow().is_empty());
    }

    #[test]
    fn home_and_all_home_never_move_or_switch_virtual_desktops() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let mut workspace = ws(vec![win(1, "one")]);
        workspace.virtual_desktop = Some(crate::virtual_desktop::VirtualDesktopBinding {
            id: VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            cached_name: Some("Work".into()),
        });
        let mut workspaces = vec![workspace];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
            desktop_ops: &desktop_ops,
        };
        activate_all_home_with_deps(&mut workspaces, &deps);
        assert!(desktop_ops.moves.borrow().is_empty());
        assert!(desktop_ops.switches.borrow().is_empty());
    }

    #[test]
    fn stale_binding_reports_error_without_retargeting_or_desktop_effects() {
        struct StaleDesktopOps;
        impl DesktopOps for StaleDesktopOps {
            fn resolve_binding(
                &self,
                _binding: &VirtualDesktopBinding,
            ) -> Result<VirtualDesktopId, String> {
                Err("bound desktop no longer exists".into())
            }
            fn move_window(&self, _: usize, _: &VirtualDesktopId) -> Result<(), String> {
                panic!("stale binding must not move a window")
            }
            fn switch(&self, _: &VirtualDesktopId) -> Result<(), String> {
                panic!("stale binding must not switch desktops")
            }
        }
        let ops = FakeOps::default();
        let mut workspace = ws(vec![win(1, "one")]);
        workspace.virtual_desktop = Some(VirtualDesktopBinding {
            id: VirtualDesktopId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            cached_name: Some("Work".into()),
        });
        let mut workspaces = vec![workspace];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
            desktop_ops: &StaleDesktopOps,
        };
        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendTarget,
            &deps,
        )
        .unwrap();
        assert!(result.movement_errors[0].contains("no longer exists"));
        assert!(ops.moves.borrow().is_empty());
    }

    #[test]
    fn zero_handle_statuses_remain_disconnected_and_distinguishable() {
        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let missing = win(0, "Missing");
        let mut closed = win(0, "Closed");
        closed.binding_status = MmBindingStatus::Closed;
        let mut ambiguous = win(0, "Ambiguous");
        ambiguous.binding_status = MmBindingStatus::Ambiguous;
        let mut mismatch = win(0, "Mismatch");
        mismatch.binding_status = MmBindingStatus::MetadataMismatch;
        let mut workspaces = vec![ws(vec![missing, closed, ambiguous, mismatch])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
            desktop_ops: &desktop_ops,
        };

        let result = activate_workspace_with_deps(
            &mut workspaces,
            "ws",
            ActivationOperation::SendHome,
            &deps,
        )
        .unwrap();

        assert_eq!(
            (
                result.missing,
                result.closed,
                result.ambiguous,
                result.metadata_mismatch
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(
            result.unresolved_labels,
            vec!["Missing", "Closed", "Ambiguous", "Mismatch"]
        );
        assert!(workspaces[0].windows.iter().all(|window| window.hwnd == 0));
        assert_eq!(
            workspaces[0].windows[0].binding_status,
            MmBindingStatus::Missing
        );
        assert_eq!(
            workspaces[0].windows[1].binding_status,
            MmBindingStatus::Closed
        );
        assert_eq!(
            workspaces[0].windows[2].binding_status,
            MmBindingStatus::Ambiguous
        );
        assert_eq!(
            workspaces[0].windows[3].binding_status,
            MmBindingStatus::MetadataMismatch
        );
    }

    #[test]
    fn activation_deps_has_no_enumeration_dependency() {
        fn accepts_activation_deps<'a, O: WindowOps>(
            deps: ActivationDeps<'a, O>,
        ) -> ActivationDeps<'a, O> {
            deps
        }

        let ops = FakeOps::default();
        let desktop_ops = FakeDesktopOps::default();
        let deps = accepts_activation_deps(ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
            desktop_ops: &desktop_ops,
        });
        assert!((deps.is_window)(1));
    }

    #[test]
    fn unlocked_activation_merge_does_not_overwrite_concurrent_rebind() {
        let before = ws(vec![win(1, "Editor")]);
        let baseline = capture_activation_state(&before);
        let mut activated = before.clone();
        activated.windows[0].mark_closed();
        let mut current = before.clone();
        current.windows[0].mark_reconnected(99);

        merge_activation_state_if_unchanged(&mut current, &baseline, &activated);

        assert_eq!(current.windows[0].hwnd, 99);
    }
}
