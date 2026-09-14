use super::model::{ContextRule, ContextRuleId, MenuId};

const MAX_MATCH_TEXT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowIdentity {
    pub hwnd: usize,
    pub pid: u32,
    pub process_name: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationContext {
    pub token: u64,
    pub foreground: Option<WindowIdentity>,
    pub under_pointer: Option<WindowIdentity>,
    pub last_external: Option<WindowIdentity>,
    pub monitor_id: String,
    pub pointer_physical: (i32, i32),
}

impl InvocationContext {
    pub fn empty(token: u64) -> Self {
        Self {
            token,
            foreground: None,
            under_pointer: None,
            last_external: None,
            monitor_id: "monitor:unknown".into(),
            pointer_physical: (0, 0),
        }
    }

    pub fn capture_current(
        token: u64,
        last_external: Option<WindowIdentity>,
        _own_process_id: u32,
    ) -> Self {
        capture_current(token, last_external)
    }

    pub fn preferred_external(&self) -> Option<&WindowIdentity> {
        self.foreground
            .as_ref()
            .or(self.last_external.as_ref())
            .or(self.under_pointer.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledContextRule {
    pub id: ContextRuleId,
    pub menu_id: MenuId,
    priority: i32,
    configured_order: usize,
    process_name: Option<String>,
    title_contains: Option<String>,
    monitor_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextSelection {
    pub menu_id: MenuId,
    pub matched_rule: Option<ContextRuleId>,
    pub explanation: String,
}

#[derive(Clone, Debug, Default)]
pub struct CompiledContextRules {
    rules: Vec<CompiledContextRule>,
}

impl CompiledContextRules {
    pub fn compile(rules: &[ContextRule]) -> Result<Self, String> {
        let mut compiled = Vec::new();
        for (configured_order, rule) in rules.iter().enumerate().filter(|(_, rule)| rule.enabled) {
            for (field, value) in [
                ("process_name", rule.process_name.as_deref()),
                (
                    "window_title_contains",
                    rule.window_title_contains.as_deref(),
                ),
                ("monitor_id", rule.monitor_id.as_deref()),
            ] {
                if value.is_some_and(|value| value.len() > MAX_MATCH_TEXT) {
                    return Err(format!("context rule {} {field} is too long", rule.id));
                }
            }
            compiled.push(CompiledContextRule {
                id: rule.id.clone(),
                menu_id: rule.menu_id.clone(),
                priority: rule.priority,
                configured_order,
                process_name: normalize(rule.process_name.as_deref()),
                title_contains: normalize(rule.window_title_contains.as_deref()),
                monitor_id: rule.monitor_id.clone(),
            });
        }
        compiled.sort_by_key(|rule| (std::cmp::Reverse(rule.priority), rule.configured_order));
        Ok(Self { rules: compiled })
    }

    pub fn select(&self, context: &InvocationContext, default_menu: &MenuId) -> ContextSelection {
        for rule in &self.rules {
            if rule.matches(context) {
                return ContextSelection {
                    menu_id: rule.menu_id.clone(),
                    matched_rule: Some(rule.id.clone()),
                    explanation: format!("matched context rule {}", rule.id),
                };
            }
        }
        ContextSelection {
            menu_id: default_menu.clone(),
            matched_rule: None,
            explanation: "no enabled context rule matched".into(),
        }
    }

    pub fn select_menu<'a>(&'a self, context: &InvocationContext) -> Option<&'a MenuId> {
        self.rules
            .iter()
            .find(|rule| rule.matches(context))
            .map(|rule| &rule.menu_id)
    }
}

impl CompiledContextRule {
    fn matches(&self, context: &InvocationContext) -> bool {
        let window = context.preferred_external();
        self.process_name.as_ref().is_none_or(|expected| {
            window
                .and_then(|window| normalize(window.process_name.as_deref()))
                .as_ref()
                == Some(expected)
        }) && self.title_contains.as_ref().is_none_or(|expected| {
            window.is_some_and(|window| window.title.to_lowercase().contains(expected.as_str()))
        }) && self
            .monitor_id
            .as_ref()
            .is_none_or(|expected| expected == &context.monitor_id)
    }
}

fn normalize(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub fn capture_current(token: u64, last_external: Option<WindowIdentity>) -> InvocationContext {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, GetForegroundWindow, WindowFromPoint,
        };
        fn identity(hwnd: HWND) -> Option<WindowIdentity> {
            if hwnd.0.is_null() {
                return None;
            }
            let descriptor = crate::window_catalog::describe_window(hwnd.0 as usize)?;
            if descriptor.pid == std::process::id() {
                return None;
            }
            Some(WindowIdentity {
                hwnd: descriptor.hwnd,
                pid: descriptor.pid,
                process_name: descriptor.executable,
                title: descriptor.title,
            })
        }
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let foreground = identity(GetForegroundWindow())
            .or_else(|| last_external.clone())
            .or_else(|| {
                crate::active_window::resolve_previous_active_window()
                    .ok()
                    .and_then(|hwnd| identity(HWND(hwnd as *mut _)))
            });
        let under_pointer = identity(WindowFromPoint(point));
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        InvocationContext {
            token,
            foreground,
            under_pointer,
            last_external,
            monitor_id: format!("monitor:{:x}", monitor.0 as usize),
            pointer_physical: (point.x, point.y),
        }
    }
    #[cfg(not(windows))]
    {
        InvocationContext {
            token,
            foreground: last_external.clone(),
            under_pointer: None,
            last_external,
            monitor_id: "monitor:unknown".into(),
            pointer_physical: (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(process: &str, title: &str, monitor: &str) -> InvocationContext {
        InvocationContext {
            token: 7,
            foreground: Some(WindowIdentity {
                hwnd: 11,
                pid: 22,
                process_name: Some(process.into()),
                title: title.into(),
            }),
            under_pointer: None,
            last_external: None,
            monitor_id: monitor.into(),
            pointer_physical: (-20, 40),
        }
    }

    fn rule(id: &str, priority: i32, process: Option<&str>, title: Option<&str>) -> ContextRule {
        ContextRule {
            id: ContextRuleId::new(id),
            enabled: true,
            priority,
            process_name: process.map(str::to_owned),
            window_title_contains: title.map(str::to_owned),
            monitor_id: None,
            menu_id: MenuId::new(id),
        }
    }

    #[test]
    fn priority_then_configured_order_is_deterministic() {
        let rules = CompiledContextRules::compile(&[
            rule("first", 5, Some("CODE.EXE"), None),
            rule("second", 8, None, Some("project")),
            rule("third", 8, None, Some("project")),
        ])
        .unwrap();
        let selected = rules.select(
            &context("code.exe", "Project Alpha", "monitor:1"),
            &MenuId::new("default"),
        );
        assert_eq!(selected.menu_id.as_str(), "second");
        assert_eq!(selected.matched_rule.unwrap().as_str(), "second");
    }

    #[test]
    fn own_process_absence_uses_last_external_without_guessing() {
        let external = WindowIdentity {
            hwnd: 3,
            pid: 4,
            process_name: Some("editor.exe".into()),
            title: "Draft".into(),
        };
        let mut value = context("ignored.exe", "Ignored", "monitor:1");
        value.foreground = None;
        value.last_external = Some(external.clone());
        assert_eq!(value.preferred_external(), Some(&external));
    }
}
