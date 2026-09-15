//! Typed launcher-to-main control requests for radial runtime sessions.
//!
//! The GUI can request a presentation or close, but only the process main loop
//! owns menu resolution and the native controller.

use super::model::{MenuId, RadialDocument};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RadialMenuSelector {
    Default,
    IdOrName(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RadialControlRequest {
    Show(RadialMenuSelector),
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RadialControlError {
    Disabled,
    Missing(String),
    Ambiguous { name: String, matches: Vec<MenuId> },
    Unavailable,
}

impl fmt::Display for RadialControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => formatter.write_str("radial menus are disabled in settings"),
            Self::Missing(target) => write!(formatter, "no radial menu matches `{target}`"),
            Self::Ambiguous { name, matches } => write!(
                formatter,
                "radial menu name `{name}` is ambiguous; matching IDs: {}",
                matches
                    .iter()
                    .map(MenuId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Unavailable => formatter.write_str("radial runtime service is unavailable"),
        }
    }
}

impl std::error::Error for RadialControlError {}

#[derive(Clone)]
pub struct RadialControlClient {
    request_tx: mpsc::Sender<RadialControlRequest>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: Arc<AtomicBool>,
}

pub struct RadialControlMainEndpoint {
    pub request_rx: mpsc::Receiver<RadialControlRequest>,
    enabled: Arc<AtomicBool>,
}

pub fn radial_control_service_with_wake(
    enabled: bool,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (RadialControlClient, RadialControlMainEndpoint) {
    let (request_tx, request_rx) = mpsc::channel();
    let enabled = Arc::new(AtomicBool::new(enabled));
    (
        RadialControlClient {
            request_tx,
            wake,
            enabled: Arc::clone(&enabled),
        },
        RadialControlMainEndpoint {
            request_rx,
            enabled,
        },
    )
}

impl RadialControlClient {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn send(&self, request: RadialControlRequest) -> Result<(), RadialControlError> {
        if matches!(request, RadialControlRequest::Show(_)) && !self.is_enabled() {
            return Err(RadialControlError::Disabled);
        }
        self.request_tx
            .send(request)
            .map_err(|_| RadialControlError::Unavailable)?;
        if let Some(wake) = &self.wake {
            wake();
        }
        Ok(())
    }
}

impl RadialControlMainEndpoint {
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }
}

pub fn resolve_menu(
    document: &RadialDocument,
    selector: &RadialMenuSelector,
) -> Result<MenuId, RadialControlError> {
    let RadialMenuSelector::IdOrName(target) = selector else {
        return Ok(document.default_menu_id.clone());
    };
    if let Some(menu) = document
        .menus
        .iter()
        .find(|menu| menu.id.as_str() == target)
    {
        return Ok(menu.id.clone());
    }
    let matches = document
        .menus
        .iter()
        .filter(|menu| menu.name.eq_ignore_ascii_case(target))
        .map(|menu| menu.id.clone())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(RadialControlError::Missing(target.clone())),
        _ => Err(RadialControlError::Ambiguous {
            name: target.clone(),
            matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_id_wins_before_unique_case_insensitive_name() {
        let mut document = RadialDocument::starter();
        // Keep the ambiguous-name fixture distinct from the starter menu's
        // exact ID so this assertion continues to exercise name resolution.
        let shared_name = "Shared Name".to_owned();
        document.menus[0].name = shared_name.clone();
        let mut second = document.menus[0].clone();
        second.id = MenuId::new("Tools");
        second.name = shared_name.clone();
        document.menus.push(second);
        assert_eq!(
            resolve_menu(&document, &RadialMenuSelector::IdOrName("Tools".into())).unwrap(),
            MenuId::new("Tools")
        );
        assert!(matches!(
            resolve_menu(
                &document,
                &RadialMenuSelector::IdOrName(shared_name.to_ascii_lowercase())
            ),
            Err(RadialControlError::Ambiguous { matches, .. }) if matches.len() == 2
        ));
        assert!(matches!(
            resolve_menu(&document, &RadialMenuSelector::IdOrName("missing".into())),
            Err(RadialControlError::Missing(_))
        ));
    }

    #[test]
    fn disabled_service_rejects_show_but_close_remains_idempotent() {
        let (client, endpoint) = radial_control_service_with_wake(false, None);
        assert_eq!(
            client.send(RadialControlRequest::Show(RadialMenuSelector::Default)),
            Err(RadialControlError::Disabled)
        );
        client.send(RadialControlRequest::Close).unwrap();
        client.send(RadialControlRequest::Close).unwrap();
        assert_eq!(
            endpoint.request_rx.try_recv().unwrap(),
            RadialControlRequest::Close
        );
        assert_eq!(
            endpoint.request_rx.try_recv().unwrap(),
            RadialControlRequest::Close
        );
    }
}
