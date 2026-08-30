//! Platform-independent selection of a one-based virtual-desktop number.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopSelectionError {
    Zero,
    BeyondCount { requested: u32, count: u32 },
}

/// Validate a one-based desktop number and convert it to the zero-based COM index.
pub(crate) fn select_virtual_desktop_index(
    requested: u32,
    count: u32,
) -> Result<u32, DesktopSelectionError> {
    if requested == 0 {
        return Err(DesktopSelectionError::Zero);
    }
    if requested > count {
        return Err(DesktopSelectionError::BeyondCount { requested, count });
    }
    Ok(requested - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_before_index_conversion() {
        assert_eq!(
            select_virtual_desktop_index(0, 3),
            Err(DesktopSelectionError::Zero)
        );
    }

    #[test]
    fn converts_one_based_numbers_to_zero_based_indices() {
        assert_eq!(select_virtual_desktop_index(1, 3), Ok(0));
        assert_eq!(select_virtual_desktop_index(3, 3), Ok(2));
    }

    #[test]
    fn accepts_first_and_last_desktops() {
        assert_eq!(select_virtual_desktop_index(1, 1), Ok(0));
        assert_eq!(select_virtual_desktop_index(4, 4), Ok(3));
    }

    #[test]
    fn rejects_desktops_beyond_the_enumerated_count() {
        assert_eq!(
            select_virtual_desktop_index(4, 3),
            Err(DesktopSelectionError::BeyondCount {
                requested: 4,
                count: 3,
            })
        );
    }

    #[test]
    fn internal_com_vtables_have_one_definition_in_the_crate() {
        let windows_module = include_str!("windows_virtual_desktop.rs");
        let window_manager = include_str!("window_manager.rs");

        assert_eq!(
            windows_module
                .matches("struct IVirtualDesktop_Vtbl")
                .count(),
            1
        );
        assert_eq!(
            windows_module
                .matches("struct IVirtualDesktopManagerInternal_Vtbl")
                .count(),
            1
        );
        assert!(!window_manager.contains("struct IVirtualDesktop_Vtbl"));
        assert!(!window_manager.contains("struct IVirtualDesktopManagerInternal_Vtbl"));
    }
}
