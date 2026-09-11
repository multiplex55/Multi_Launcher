mod model;
mod selection;
#[cfg(windows)]
mod windows;

pub use model::*;
pub use selection::{AdjacentDirection, adjacent_index, close_current_plan};

#[derive(Debug, Clone, Copy, Default)]
pub struct VirtualDesktopService;

impl VirtualDesktopService {
    pub fn snapshot(&self) -> Result<VirtualDesktopSnapshot, VirtualDesktopError> {
        #[cfg(windows)]
        {
            windows::snapshot()
        }
        #[cfg(not(windows))]
        {
            Err(VirtualDesktopError::unsupported(
                "snapshot virtual desktops",
                VirtualDesktopCapability::Enumeration,
            ))
        }
    }

    pub fn current(&self) -> Result<VirtualDesktopInfo, VirtualDesktopError> {
        self.snapshot()?.current().cloned()
    }

    pub fn switch(&self, selector: &VirtualDesktopSelector) -> Result<(), VirtualDesktopError> {
        if matches!(selector, VirtualDesktopSelector::Number(0)) {
            return Err(VirtualDesktopError::new(
                VirtualDesktopErrorKind::InvalidSelector,
                "switch virtual desktop",
                "Virtual desktop number must be at least 1",
            )
            .context("requested_selector", "0"));
        }
        let snapshot = self.snapshot()?;
        let target = snapshot.resolve(selector)?.clone();
        if target.is_current {
            return Ok(());
        }
        #[cfg(windows)]
        {
            windows::switch_to(&target.id)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(VirtualDesktopError::unsupported(
                "switch virtual desktop",
                VirtualDesktopCapability::DirectSwitching,
            ))
        }
    }

    /// Switch by stable identity without taking a second discovery snapshot.
    pub fn switch_to_id(&self, target: &VirtualDesktopId) -> Result<(), VirtualDesktopError> {
        #[cfg(windows)]
        {
            windows::switch_to(target)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(VirtualDesktopError::unsupported(
                "switch virtual desktop",
                VirtualDesktopCapability::DirectSwitching,
            ))
        }
    }

    pub fn switch_adjacent(
        &self,
        direction: AdjacentDirection,
    ) -> Result<bool, VirtualDesktopError> {
        let snapshot = self.snapshot()?;
        let Some(index) = adjacent_index(&snapshot, direction)? else {
            return Ok(false);
        };
        let target = snapshot.desktops[index].id.clone();
        #[cfg(windows)]
        windows::switch_to(&target)?;
        #[cfg(not(windows))]
        return Err(VirtualDesktopError::unsupported(
            "switch adjacent virtual desktop",
            VirtualDesktopCapability::DirectSwitching,
        ));
        Ok(true)
    }

    pub fn create(&self) -> Result<VirtualDesktopInfo, VirtualDesktopError> {
        #[cfg(windows)]
        {
            windows::create()
        }
        #[cfg(not(windows))]
        {
            Err(VirtualDesktopError::unsupported(
                "create virtual desktop",
                VirtualDesktopCapability::Creation,
            ))
        }
    }

    pub fn close_current(&self) -> Result<(), VirtualDesktopError> {
        let snapshot = self.snapshot()?;
        let (current, fallback) = close_current_plan(&snapshot)?;
        #[cfg(windows)]
        {
            windows::close(&current.id, &fallback.id)
        }
        #[cfg(not(windows))]
        {
            let _ = (current, fallback);
            Err(VirtualDesktopError::unsupported(
                "close current desktop",
                VirtualDesktopCapability::Closing,
            ))
        }
    }

    pub fn rename(
        &self,
        selector: &VirtualDesktopSelector,
        name: &str,
    ) -> Result<(), VirtualDesktopError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(VirtualDesktopError::new(
                VirtualDesktopErrorKind::InvalidSelector,
                "rename virtual desktop",
                "Virtual desktop name cannot be empty",
            ));
        }
        let snapshot = self.snapshot()?;
        let target = snapshot.resolve(selector)?;
        if snapshot.desktops.iter().any(|desktop| {
            desktop.id != target.id
                && desktop
                    .name
                    .as_deref()
                    .is_some_and(|current| current.eq_ignore_ascii_case(name))
        }) {
            return Err(VirtualDesktopError::new(
                VirtualDesktopErrorKind::AmbiguousSelector,
                "rename virtual desktop",
                format!("Another virtual desktop is already named {name:?}"),
            ));
        }
        let target_id = target.id.clone();
        #[cfg(windows)]
        {
            windows::rename(&target_id, name)
        }
        #[cfg(not(windows))]
        {
            let _ = target_id;
            Err(VirtualDesktopError::unsupported(
                "rename virtual desktop",
                VirtualDesktopCapability::Renaming,
            ))
        }
    }

    #[cfg(windows)]
    pub fn desktop_for_window(
        &self,
        hwnd: ::windows::Win32::Foundation::HWND,
    ) -> Result<VirtualDesktopId, VirtualDesktopError> {
        windows::desktop_for_window(hwnd)
    }

    #[cfg(windows)]
    pub fn is_window_on_current_desktop(
        &self,
        hwnd: ::windows::Win32::Foundation::HWND,
    ) -> Result<bool, VirtualDesktopError> {
        windows::is_window_on_current_desktop(hwnd)
    }

    #[cfg(windows)]
    pub fn move_window_to_desktop(
        &self,
        hwnd: ::windows::Win32::Foundation::HWND,
        desktop: &VirtualDesktopId,
    ) -> Result<(), VirtualDesktopError> {
        windows::move_window_to_desktop(hwnd, desktop)
    }

    #[cfg(windows)]
    pub(crate) fn id_from_native(&self, guid: &::windows::core::GUID) -> VirtualDesktopId {
        windows::id_from_guid(*guid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{value:08x}-0000-0000-0000-000000000000")).unwrap()
    }

    fn snapshot(current: usize, names: &[Option<&str>]) -> VirtualDesktopSnapshot {
        VirtualDesktopSnapshot {
            desktops: names
                .iter()
                .enumerate()
                .map(|(index, name)| VirtualDesktopInfo {
                    id: id((index + 1) as u128),
                    index: (index + 1) as u32,
                    name: name.map(str::to_string),
                    is_current: index == current,
                })
                .collect(),
            capabilities: VirtualDesktopCapabilities::default(),
        }
    }

    #[test]
    fn desktop_ids_are_normalized_and_human_readable() {
        let id = VirtualDesktopId::parse("{550E8400-E29B-41D4-A716-446655440000}").unwrap();
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            "\"550e8400-e29b-41d4-a716-446655440000\""
        );
    }

    #[test]
    fn selectors_resolve_number_name_and_guid() {
        let snapshot = snapshot(0, &[Some("Work"), None]);
        assert_eq!(
            snapshot
                .resolve(&VirtualDesktopSelector::Number(2))
                .unwrap()
                .index,
            2
        );
        assert_eq!(
            snapshot
                .resolve(&VirtualDesktopSelector::Name("work".into()))
                .unwrap()
                .index,
            1
        );
        assert_eq!(
            snapshot
                .resolve(&VirtualDesktopSelector::Id(id(2)))
                .unwrap()
                .index,
            2
        );
        assert!(matches!(
            VirtualDesktopSelector::parse("2").unwrap(),
            VirtualDesktopSelector::Number(2)
        ));
        assert!(matches!(
            VirtualDesktopSelector::parse(id(1).as_str()).unwrap(),
            VirtualDesktopSelector::Id(_)
        ));
    }

    #[test]
    fn duplicate_names_are_ambiguous() {
        let snapshot = snapshot(0, &[Some("Work"), Some("work")]);
        assert_eq!(
            snapshot
                .resolve(&VirtualDesktopSelector::Name("WORK".into()))
                .unwrap_err()
                .kind,
            VirtualDesktopErrorKind::AmbiguousSelector
        );
    }

    #[test]
    fn stale_binding_never_retargets_by_cached_name() {
        let snapshot = snapshot(0, &[Some("Work")]);
        let binding = VirtualDesktopBinding {
            id: id(99),
            cached_name: Some("Work".into()),
        };
        assert_eq!(
            snapshot.resolve_binding(&binding).unwrap_err().kind,
            VirtualDesktopErrorKind::StaleBinding
        );
    }

    #[test]
    fn adjacent_navigation_does_not_wrap() {
        let first = snapshot(0, &[None, None, None]);
        assert_eq!(
            adjacent_index(&first, AdjacentDirection::Previous).unwrap(),
            None
        );
        assert_eq!(
            adjacent_index(&first, AdjacentDirection::Next).unwrap(),
            Some(1)
        );
        let last = snapshot(2, &[None, None, None]);
        assert_eq!(
            adjacent_index(&last, AdjacentDirection::Next).unwrap(),
            None
        );
    }

    #[test]
    fn close_plan_prefers_previous_then_next_and_rejects_last() {
        let middle = snapshot(1, &[None, None, None]);
        assert_eq!(close_current_plan(&middle).unwrap().1.index, 1);
        let first = snapshot(0, &[None, None]);
        assert_eq!(close_current_plan(&first).unwrap().1.index, 2);
        assert!(close_current_plan(&snapshot(0, &[None])).is_err());
    }

    #[test]
    fn service_rejects_zero_before_opening_the_platform_backend() {
        let error = VirtualDesktopService
            .switch(&VirtualDesktopSelector::Number(0))
            .unwrap_err();
        assert_eq!(error.kind, VirtualDesktopErrorKind::InvalidSelector);
        assert_eq!(error.context["requested_selector"], "0");
    }
}
