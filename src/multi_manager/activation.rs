use crate::multi_manager::model::{MmRect, MmWorkspace};
use crate::multi_manager::runtime::{WinWindowOps, WindowOps};
use crate::multi_manager::win;

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
    let deps = ActivationDeps {
        window_ops: &ops,
        is_window: &|hwnd| win::is_valid_window(hwnd),
    };
    activate_workspace_with_deps(workspaces, workspace_id, operation, &deps)
}

pub fn activate_all_home(workspaces: &mut [MmWorkspace]) -> ActivationResult {
    let ops = WinWindowOps;
    let deps = ActivationDeps {
        window_ops: &ops,
        is_window: &|hwnd| win::is_valid_window(hwnd),
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

    match operation {
        ActivationOperation::Toggle => toggle(workspace, deps.window_ops, &mut result),
        ActivationOperation::SendHome => {
            move_kind(workspace, RectKind::Home, deps.window_ops, &mut result)
        }
        ActivationOperation::SendTarget => {
            move_kind(workspace, RectKind::Target, deps.window_ops, &mut result)
        }
        ActivationOperation::Rotate => rotate(workspace, deps.window_ops, &mut result),
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

fn toggle<O: WindowOps>(workspace: &mut MmWorkspace, ops: &O, result: &mut ActivationResult) {
    if workspace.rotate {
        rotate(workspace, ops, result);
        return;
    }
    let all_at_home = workspace
        .windows
        .iter()
        .filter(|w| w.can_activate())
        .all(|w| {
            w.home_rect
                .is_some_and(|rect| ops.is_window_at_rect(w.hwnd, rect))
        });
    if all_at_home {
        move_kind(workspace, RectKind::Target, ops, result);
    } else {
        move_kind(workspace, RectKind::Home, ops, result);
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
        let mut workspaces = vec![ws(vec![win(1, "one"), win(2, "two"), win(3, "three")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd != 2,
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
        let mut workspaces = vec![ws(vec![win(9, "gone")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|_| false,
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
        let mut workspaces = vec![ws(vec![win(0, "App")])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd == 42,
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
        let mut missing = win(0, "Captured");
        missing.alias = "Alias".into();
        let mut workspaces = vec![ws(vec![win(1, "ok"), missing])];
        let deps = ActivationDeps {
            window_ops: &ops,
            is_window: &|hwnd| hwnd == 1,
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
    fn zero_handle_statuses_remain_disconnected_and_distinguishable() {
        let ops = FakeOps::default();
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
        let deps = accepts_activation_deps(ActivationDeps {
            window_ops: &ops,
            is_window: &|_| true,
        });
        assert!((deps.is_window)(1));
    }
}
