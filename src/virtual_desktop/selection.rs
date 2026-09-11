use super::{VirtualDesktopError, VirtualDesktopErrorKind, VirtualDesktopSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjacentDirection {
    Previous,
    Next,
}

pub fn adjacent_index(
    snapshot: &VirtualDesktopSnapshot,
    direction: AdjacentDirection,
) -> Result<Option<usize>, VirtualDesktopError> {
    let current = snapshot.current()?;
    let index = snapshot
        .desktops
        .iter()
        .position(|desktop| desktop.id == current.id)
        .expect("current desktop originates from the snapshot");
    Ok(match direction {
        AdjacentDirection::Previous => index.checked_sub(1),
        AdjacentDirection::Next if index + 1 < snapshot.desktops.len() => Some(index + 1),
        AdjacentDirection::Next => None,
    })
}

pub fn close_current_plan(
    snapshot: &VirtualDesktopSnapshot,
) -> Result<(&super::VirtualDesktopInfo, &super::VirtualDesktopInfo), VirtualDesktopError> {
    if snapshot.desktops.len() <= 1 {
        return Err(VirtualDesktopError::new(
            VirtualDesktopErrorKind::InvalidSelector,
            "close current desktop",
            "The final virtual desktop cannot be closed",
        ));
    }
    let current = snapshot.current()?;
    let index = snapshot
        .desktops
        .iter()
        .position(|desktop| desktop.id == current.id)
        .expect("current desktop originates from the snapshot");
    let fallback = if index > 0 {
        &snapshot.desktops[index - 1]
    } else {
        &snapshot.desktops[1]
    };
    Ok((current, fallback))
}
