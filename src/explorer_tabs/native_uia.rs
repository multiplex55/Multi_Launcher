//! Target-rooted, bounded UI Automation observations for Explorer tabs.
//! All interfaces stay on the COM worker and are never portable identities.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, SAFEARRAY};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, IsChild};
use windows::core::Interface;

const MAX_UIA_NODES: usize = 1024;
const MAX_UIA_DEPTH: usize = 24;
const MAX_RUNTIME_ID_PARTS: usize = 64;

/// A root-wide HWND does not establish which tab owns a view. The caller
/// supplies live descendant checks; equality with the content pane is valid.
pub(super) fn pane_view_relation(
    root: usize,
    pane: usize,
    view: usize,
    pane_under_root: bool,
    view_under_pane: bool,
) -> bool {
    root != 0
        && pane != 0
        && view != 0
        && pane != root
        && pane_under_root
        && (view == pane || view_under_pane)
}

fn invoke_with_marker<T>(
    acquire: impl FnOnce() -> Result<T, String>,
    check: impl FnOnce() -> Result<(), String>,
    mark_attempted: impl FnOnce(),
    invoke: impl FnOnce(T) -> Result<(), String>,
) -> Result<(), String> {
    let pattern = acquire()?;
    check()?;
    mark_attempted();
    invoke(pattern)
}

fn merge_related_pane(panes: &mut Vec<usize>, matches: &[usize]) -> Result<(), String> {
    if matches.len() != 1 {
        return Err("pane relation is missing or ambiguous in target tree".into());
    }
    if !panes.contains(&matches[0]) {
        panes.push(matches[0]);
    }
    Ok(())
}

pub(super) struct UiaTab {
    pub element: IUIAutomationElement,
    pub runtime: Vec<i32>,
    pub selected: bool,
}
pub(super) struct UiaSnapshot {
    pub tabs: Vec<UiaTab>,
    pub selected_index: usize,
    pub selected_pane: HWND,
    pub new_tab: IUIAutomationElement,
}
struct Node {
    element: IUIAutomationElement,
    parent: Option<usize>,
    kind: UIA_CONTROLTYPE_ID,
}
pub(super) struct ExplorerUia {
    automation: IUIAutomation,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl ExplorerUia {
    pub fn connect(cancelled: Arc<AtomicBool>, deadline: Instant) -> Result<Self, String> {
        let check = || {
            if cancelled.load(Ordering::Acquire) {
                Err("cancelled".to_owned())
            } else if Instant::now() >= deadline {
                Err("deadline expired".to_owned())
            } else {
                Ok(())
            }
        };
        check()?;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("UI Automation unavailable: {e}"))?;
        check()?;
        let v2: IUIAutomation2 = automation
            .cast()
            .map_err(|_| "UI Automation timeout capability unavailable".to_owned())?;
        check()?;
        unsafe { v2.SetConnectionTimeout(500) }
            .map_err(|e| format!("UI Automation connection timeout: {e}"))?;
        check()?;
        unsafe { v2.SetTransactionTimeout(500) }
            .map_err(|e| format!("UI Automation transaction timeout: {e}"))?;
        check()?;
        Ok(Self {
            automation,
            cancelled,
            deadline,
        })
    }
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("cancelled".into())
        } else if Instant::now() >= self.deadline {
            Err("deadline expired".into())
        } else {
            Ok(())
        }
    }
    fn call<T>(&self, f: impl FnOnce() -> windows::core::Result<T>) -> Result<T, String> {
        self.check()?;
        let result = f();
        self.check()?;
        result.map_err(|e| format!("UI Automation provider: {e}"))
    }
    fn call_value<T>(&self, f: impl FnOnce() -> T) -> Result<T, String> {
        self.check()?;
        let result = f();
        self.check()?;
        Ok(result)
    }
    // The generated wrappers convert a successful null COM out-pointer into
    // an error. Inspect HRESULT and the pointer separately so only a genuine
    // successful null means "no child/relation"; provider errors fail closed.
    fn optional_interface<T: Interface>(
        &self,
        f: impl FnOnce(*mut *mut core::ffi::c_void) -> windows::core::HRESULT,
    ) -> Result<Option<T>, String> {
        self.check()?;
        let mut raw = std::ptr::null_mut();
        let result = f(&mut raw);
        let owned = if raw.is_null() {
            None
        } else {
            Some(unsafe { T::from_raw(raw) })
        };
        self.check()?;
        result
            .ok()
            .map_err(|error| format!("UI Automation provider: {error}"))?;
        Ok(owned)
    }
    fn collect(
        &self,
        walker: &IUIAutomationTreeWalker,
        element: IUIAutomationElement,
        parent: Option<usize>,
        depth: usize,
        nodes: &mut Vec<Node>,
    ) -> Result<(), String> {
        self.check()?;
        if depth > MAX_UIA_DEPTH || nodes.len() >= MAX_UIA_NODES {
            return Err("UI Automation tree limit exceeded".into());
        }
        let kind = self.call(|| unsafe { element.CurrentControlType() })?;
        let index = nodes.len();
        nodes.push(Node {
            element: element.clone(),
            parent,
            kind,
        });
        let mut child = self.optional_interface::<IUIAutomationElement>(|out| unsafe {
            (Interface::vtable(walker).GetFirstChildElement)(
                Interface::as_raw(walker),
                Interface::as_raw(&element),
                out,
            )
        })?;
        while let Some(current) = child {
            self.collect(walker, current.clone(), Some(index), depth + 1, nodes)?;
            child = self.optional_interface::<IUIAutomationElement>(|out| unsafe {
                (Interface::vtable(walker).GetNextSiblingElement)(
                    Interface::as_raw(walker),
                    Interface::as_raw(&current),
                    out,
                )
            })?;
        }
        Ok(())
    }
    fn runtime(&self, element: &IUIAutomationElement) -> Result<Vec<i32>, String> {
        self.check()?;
        let array = unsafe { element.GetRuntimeId() }
            .map_err(|e| format!("UI Automation provider: {e}"))?;
        struct Guard(*mut SAFEARRAY);
        impl Drop for Guard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    let _ = unsafe { SafeArrayDestroy(self.0) };
                }
            }
        }
        let guard = Guard(array);
        self.check()?;
        if guard.0.is_null() || unsafe { SafeArrayGetDim(guard.0) } != 1 {
            return Err("invalid UI Automation runtime ID".into());
        }
        let lower = self.call(|| unsafe { SafeArrayGetLBound(guard.0, 1) })?;
        let upper = self.call(|| unsafe { SafeArrayGetUBound(guard.0, 1) })?;
        let count = upper
            .checked_sub(lower)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n > 0 && *n <= MAX_RUNTIME_ID_PARTS)
            .ok_or("UI Automation runtime ID limit")?;
        let mut result = Vec::with_capacity(count);
        for index in lower..=upper {
            let mut value = 0i32;
            self.call(|| unsafe {
                SafeArrayGetElement(guard.0, &index, (&mut value as *mut i32).cast())
            })?;
            result.push(value);
        }
        Ok(result)
    }
    fn is_descendant(nodes: &[Node], mut index: usize, ancestor: usize) -> bool {
        while let Some(parent) = nodes[index].parent {
            if parent == ancestor {
                return true;
            }
            index = parent;
        }
        false
    }
    fn related_panes(
        &self,
        nodes: &[Node],
        tab_node: usize,
        tab: &IUIAutomationElement,
    ) -> Result<Vec<usize>, String> {
        let mut panes = nodes
            .iter()
            .enumerate()
            .filter(|(index, node)| {
                node.kind == UIA_PaneControlTypeId && Self::is_descendant(nodes, *index, tab_node)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if let Some(associated) =
            self.optional_interface::<IUIAutomationElementArray>(|out| unsafe {
                (Interface::vtable(tab).CurrentControllerFor)(Interface::as_raw(tab), out)
            })?
        {
            let count = self.call(|| unsafe { associated.Length() })?;
            if !(0..=MAX_UIA_NODES as i32).contains(&count) {
                return Err("pane relation limit exceeded".into());
            }
            for index in 0..count {
                let element = self.call(|| unsafe { associated.GetElement(index) })?;
                let mut matches = Vec::new();
                for (candidate, node) in nodes.iter().enumerate() {
                    if node.kind == UIA_PaneControlTypeId
                        && self.same_element(&element, &node.element)?
                    {
                        matches.push(candidate);
                    }
                }
                merge_related_pane(&mut panes, &matches)?;
            }
        }
        Ok(panes)
    }
    pub fn same_element(
        &self,
        a: &IUIAutomationElement,
        b: &IUIAutomationElement,
    ) -> Result<bool, String> {
        Ok(self
            .call(|| unsafe { self.automation.CompareElements(a, b) })?
            .as_bool())
    }
    pub fn snapshot(&self, root: HWND, process_id: u32) -> Result<UiaSnapshot, String> {
        let root_element = self.call(|| unsafe { self.automation.ElementFromHandle(root) })?;
        let walker = self.call(|| unsafe { self.automation.ControlViewWalker() })?;
        let mut nodes = Vec::new();
        self.collect(&walker, root_element, None, 0, &mut nodes)?;
        let mut candidates = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            if node.kind != UIA_TabControlTypeId {
                continue;
            }
            let children = nodes
                .iter()
                .enumerate()
                .filter(|(_, child)| {
                    child.parent == Some(index) && child.kind == UIA_TabItemControlTypeId
                })
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if !children.is_empty() {
                candidates.push((index, children));
            }
        }
        if candidates.len() != 1 {
            return Err("Explorer tab container unavailable or ambiguous".into());
        }
        let (container, tab_nodes) = candidates.pop().unwrap();
        let mut tabs = Vec::with_capacity(tab_nodes.len());
        for index in &tab_nodes {
            let element = &nodes[*index].element;
            if self.call(|| unsafe { element.CurrentProcessId() })? != process_id as i32 {
                return Err("foreign tab process".into());
            }
            let selection: IUIAutomationSelectionItemPattern =
                self.call(|| unsafe { element.GetCurrentPatternAs(UIA_SelectionItemPatternId) })?;
            let selected = self
                .call(|| unsafe { selection.CurrentIsSelected() })?
                .as_bool();
            let runtime = self.runtime(element)?;
            if tabs.iter().any(|tab: &UiaTab| tab.runtime == runtime) {
                return Err("duplicate tab runtime ID".into());
            }
            tabs.push(UiaTab {
                element: element.clone(),
                runtime,
                selected,
            });
        }
        let selected = tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.selected)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err("selected Explorer tab unavailable or ambiguous".into());
        }
        let selected_index = selected[0];
        let selected_node = tab_nodes[selected_index];
        let panes = self.related_panes(&nodes, selected_node, &tabs[selected_index].element)?;
        let mut handles = Vec::new();
        for index in panes {
            let element = &nodes[index].element;
            if self.call(|| unsafe { element.CurrentProcessId() })? != process_id as i32 {
                continue;
            }
            let hwnd = self.call(|| unsafe { element.CurrentNativeWindowHandle() })?;
            if !hwnd.0.is_null()
                && hwnd != root
                && self.call_value(|| unsafe { GetAncestor(hwnd, GA_ROOT) })? == root
                && self
                    .call_value(|| unsafe { IsChild(root, hwnd) })?
                    .as_bool()
                && !handles.contains(&hwnd)
            {
                handles.push(hwnd);
            }
        }
        if handles.len() != 1 {
            return Err("selected tab pane unavailable or ambiguous".into());
        }
        for (index, tab_node) in tab_nodes.iter().enumerate() {
            if index == selected_index {
                continue;
            }
            for pane in self.related_panes(&nodes, *tab_node, &tabs[index].element)? {
                let hwnd =
                    self.call(|| unsafe { nodes[pane].element.CurrentNativeWindowHandle() })?;
                let overlaps = hwnd == handles[0]
                    || self
                        .call_value(|| unsafe { IsChild(hwnd, handles[0]) })?
                        .as_bool()
                    || self
                        .call_value(|| unsafe { IsChild(handles[0], hwnd) })?
                        .as_bool();
                if overlaps {
                    return Err("content pane is shared by multiple tabs".into());
                }
            }
        }
        let parent = nodes[container].parent;
        let mut buttons = Vec::new();
        for node in &nodes {
            if node.kind != UIA_ButtonControlTypeId
                || (node.parent != Some(container) && node.parent != parent)
            {
                continue;
            }
            if self.call(|| unsafe { node.element.CurrentProcessId() })? != process_id as i32 {
                continue;
            }
            let accelerator = self
                .call(|| unsafe { node.element.CurrentAcceleratorKey() })?
                .to_string()
                .replace(' ', "")
                .to_ascii_lowercase();
            if accelerator != "ctrl+t" && accelerator != "control+t" {
                continue;
            }
            if !self
                .call(|| unsafe { node.element.CurrentIsEnabled() })?
                .as_bool()
            {
                continue;
            }
            let pattern = self.optional_interface::<windows::core::IUnknown>(|out| unsafe {
                (Interface::vtable(&node.element).GetCurrentPattern)(
                    Interface::as_raw(&node.element),
                    UIA_InvokePatternId,
                    out,
                )
            })?;
            if let Some(pattern) = pattern {
                let _: IUIAutomationInvokePattern = self.call(|| pattern.cast())?;
                buttons.push(node.element.clone());
            }
        }
        if buttons.len() != 1 {
            return Err("Explorer New Tab control unavailable or ambiguous".into());
        }
        Ok(UiaSnapshot {
            tabs,
            selected_index,
            selected_pane: handles[0],
            new_tab: buttons.remove(0),
        })
    }
    pub fn invoke_new_tab(
        &self,
        element: &IUIAutomationElement,
        mark_attempted: impl FnOnce(),
    ) -> Result<(), String> {
        invoke_with_marker(
            || {
                self.call(|| unsafe {
                    element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                })
            },
            || self.check(),
            mark_attempted,
            |pattern| {
                let result = unsafe { pattern.Invoke() };
                self.check()?;
                result.map_err(|error| format!("UI Automation provider: {error}"))
            },
        )
    }
    pub fn select(&self, element: &IUIAutomationElement) -> Result<(), String> {
        let pattern: IUIAutomationSelectionItemPattern =
            self.call(|| unsafe { element.GetCurrentPatternAs(UIA_SelectionItemPatternId) })?;
        self.call(|| unsafe { pattern.Select() })
    }
}

#[cfg(test)]
mod tests {
    use super::{invoke_with_marker, merge_related_pane, pane_view_relation};

    #[test]
    fn explorer_tabs_backend_shared_root_pane_cannot_authorize_unique_shell_view() {
        assert!(!pane_view_relation(10, 10, 11, true, true));
        assert!(!pane_view_relation(10, 12, 11, false, true));
        assert!(pane_view_relation(10, 12, 11, true, true));
        let mut panes = vec![4];
        assert!(merge_related_pane(&mut panes, &[]).is_err());
        assert!(merge_related_pane(&mut panes, &[4, 5]).is_err());
        merge_related_pane(&mut panes, &[4]).unwrap();
        assert_eq!(panes, [4]);
        merge_related_pane(&mut panes, &[5]).unwrap();
        assert_eq!(panes, [4, 5]);
    }

    #[test]
    fn explorer_tabs_backend_marks_attempt_only_at_invoke_boundary() {
        use std::cell::Cell;
        let marked = Cell::new(false);
        let invoked = Cell::new(false);
        let result = invoke_with_marker::<()>(
            || Err("pattern failed".into()),
            || Ok(()),
            || marked.set(true),
            |_| {
                invoked.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!marked.get() && !invoked.get());
        let result = invoke_with_marker(
            || Ok(()),
            || Err("cancelled".into()),
            || marked.set(true),
            |_| {
                invoked.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!marked.get() && !invoked.get());
        let result = invoke_with_marker(
            || Ok(()),
            || Ok(()),
            || marked.set(true),
            |_| {
                invoked.set(true);
                Err("uncertain return".into())
            },
        );
        assert!(result.is_err());
        assert!(marked.get() && invoked.get());
    }
}
