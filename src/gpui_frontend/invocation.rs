//! One owned generation for launcher and scoped Quick Help polling edges.
use super::*;

#[derive(Default)]
pub(super) struct Owner {
    revision: AtomicU64,
    installed: Mutex<Option<Installed>>,
}

pub(super) struct Installed {
    pub revision: u64,
    pub listener: HotkeyListener,
    pub launcher: Arc<HotkeyTrigger>,
    pub quick_help: Option<Arc<HotkeyTrigger>>,
}

#[derive(Clone)]
pub(super) struct Turn {
    owner: Arc<Owner>,
    revision: u64,
    launcher: Arc<HotkeyTrigger>,
    quick_help: Option<Arc<HotkeyTrigger>>,
}

impl Owner {
    pub fn with_generation<T>(&self, generation: u64, claim: impl FnOnce(Hotkey) -> T) -> Option<T> {
        let installed = self.installed.lock().ok()?;
        let active = installed.as_ref()?;
        if active.revision != generation || self.revision.load(Ordering::Acquire) != generation { return None; }
        Some(claim(active.launcher.hotkey()))
    }
    pub fn install(&self, listener: HotkeyListener, launcher: Arc<HotkeyTrigger>, quick_help: Option<Arc<HotkeyTrigger>>) -> Result<(), String> {
        let mut installed = self.installed.lock().map_err(|_| "hotkey owner lock poisoned".to_owned())?;
        if installed.is_some() { return Err("hotkey owner already installed".into()); }
        let revision = self.revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        listener.activate();
        *installed = Some(Installed { revision, listener, launcher, quick_help });
        Ok(())
    }

    pub fn turn(self: &Arc<Self>) -> Option<Turn> {
        let installed = self.installed.lock().ok()?;
        let state = installed.as_ref()?;
        Some(Turn { owner: self.clone(), revision: state.revision, launcher: state.launcher.clone(), quick_help: state.quick_help.clone() })
    }

    pub fn stop_checked(&self) -> Result<(), String> {
        let mut state = self.installed.lock().map_err(|_| "hotkey owner lock poisoned".to_owned())?;
        self.revision.fetch_add(1, Ordering::AcqRel);
        if let Some(mut installed) = state.take() { installed.listener.stop_checked()?; }
        Ok(())
    }

    #[cfg(windows)]
    pub(super) fn active_routes(&self) -> Option<(u64, Hotkey, Option<Hotkey>)> {
        let installed = self.installed.lock().ok()?;
        let active = installed.as_ref()?;
        Some((active.revision, active.launcher.hotkey(), active.quick_help.as_ref().map(|trigger| trigger.hotkey())))
    }

    #[cfg(windows)]
    pub(super) fn replace(&self, expected: u64, candidate: &mut Option<Installed>, fallback: &mut Option<Installed>, claim_current: impl FnOnce(&dyn Fn() -> u64) -> Option<u64>) -> Result<Option<u64>, (bool, String)> {
        let mut installed = self.installed.lock().map_err(|_| (false, "hotkey owner lock poisoned".to_owned()))?;
        if installed.as_ref().is_none_or(|active| active.revision != expected) { return Err((false, "hotkey generation changed".into())); }
        let active = installed.as_ref().ok_or_else(|| (false, "hotkey owner unavailable".to_owned()))?;
        let claim = active.launcher.with_publication_guard(|launcher_pending| {
            if let Some(help) = &active.quick_help {
                help.with_publication_guard(|help_pending| {
                    if launcher_pending || help_pending { None } else { claim_current(&|| self.revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1)) }
                })
            } else if launcher_pending { Ok(None) }
            else { Ok(claim_current(&|| self.revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1))) }
        }).map_err(|error| (false, error))?.map_err(|error| (false, error))?;
        let Some(next) = claim else { return Ok(None); };
        let mut previous = installed.take().ok_or_else(|| (false, "hotkey owner unavailable".to_owned()))?;
        let mut candidate = candidate.take().ok_or_else(|| (false, "prepared hotkey listener unavailable".to_owned()))?;
        let mut fallback = fallback.take().ok_or_else(|| (false, "fallback hotkey listener unavailable".to_owned()))?;
        if let Err(error) = previous.listener.stop_checked() {
            fallback.revision = next;
            fallback.listener.activate();
            *installed = Some(fallback);
            return Err((true, format!("old polling listener failed to stop: {error}")));
        }
        candidate.revision = next;
        candidate.listener.activate();
        *installed = Some(candidate);
        Ok(Some(next))
    }
}

impl Turn {
    pub fn generation(&self) -> u64 { self.revision }
    pub fn hotkey(&self) -> Option<Hotkey> { self.current().then(|| self.launcher.hotkey()) }
    pub fn current(&self) -> bool { self.owner.revision.load(Ordering::Acquire) == self.revision }
    pub fn pending(&self) -> bool {
        if !self.current() { return true; }
        self.launcher.open.lock().map(|pending| *pending || !self.current()).unwrap_or(true)
    }
    pub fn help_pending(&self) -> bool {
        if !self.current() { return true; }
        self.quick_help.as_ref().is_some_and(|trigger| trigger.with_publication_guard(|pending| pending || !self.current()).unwrap_or(true))
    }
    pub fn take(&self) -> bool { self.current() && self.launcher.take() && self.current() }
    pub fn quick_help(&self) -> Option<Option<u64>> {
        if !self.current() { return None; }
        let stamp = self.quick_help.as_ref()?.take_scoped_stamp();
        if self.current() { stamp } else { None }
    }
    pub fn clear_quick_help(&self) {
        if self.current() && let Some(trigger) = &self.quick_help { trigger.clear_pending(); }
    }
    pub fn rollback(&self, visibility: &RootVisibility, revision: u64) -> bool {
        if !self.current() { return false; }
        if let Ok(pending) = self.launcher.open.lock() {
            if !*pending && self.current() && visibility.is_current(revision, true) {
                visibility.revert_if_current(revision, true);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
pub(super) fn test_turn() -> (Turn, Arc<HotkeyTrigger>) {
    let owner = Arc::new(Owner::default());
    owner.revision.store(1, Ordering::Release);
    let trigger = Arc::new(HotkeyTrigger::new(multi_launcher::hotkey::parse_hotkey("F2").unwrap()));
    (Turn { owner, revision: 1, launcher: trigger.clone(), quick_help: None }, trigger)
}

#[cfg(test)]
mod tests {
    use super::{Owner, Turn};
    use multi_launcher::hotkey::{parse_hotkey, runtime::HotkeyTrigger};
    use std::sync::{Arc, atomic::Ordering};

    #[test]
    fn current_turn_observes_pending_without_consuming_and_retired_turn_cannot_take() {
        let owner = Arc::new(Owner::default());
        owner.revision.store(1, Ordering::Release);
        let launcher = Arc::new(HotkeyTrigger::new(parse_hotkey("F2").unwrap()));
        let turn = Turn { owner: owner.clone(), revision: 1, launcher: launcher.clone(), quick_help: None };
        assert!(!turn.pending());
        *launcher.open.lock().unwrap() = true;
        assert!(turn.pending());
        assert!(turn.pending());
        assert!(turn.take());
        *launcher.open.lock().unwrap() = true;
        owner.revision.fetch_add(1, Ordering::AcqRel);
        assert!(!turn.take());
        assert!(*launcher.open.lock().unwrap());
    }
}
