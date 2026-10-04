//! GUI-owned publication history for the current Screen Draw parking lifecycle.

use crate::screen_draw::{
    ScreenDrawGeneration, ScreenDrawRecoveryIntent, ScreenDrawRestoreCause,
    ScreenDrawRestoreOutcome,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RestoreScope {
    pub generation: u64,
    pub lifecycle: u64,
    pub parking_cycle: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RestoreKey {
    pub scope: RestoreScope,
    pub cause: ScreenDrawRestoreCause,
    pub intent: Option<ScreenDrawRecoveryIntent>,
}

#[derive(Debug, Default)]
pub(super) struct ScreenDrawRestorePublication {
    lifecycle: u64,
    generation: u64,
    parking_cycle: u64,
    latest_admission: Option<(ScreenDrawRecoveryIntent, RestoreScope)>,
    completed: [Option<RestoreKey>; ScreenDrawRestoreCause::COUNT],
}

impl ScreenDrawRestorePublication {
    pub fn advance(&mut self) -> Result<(), String> {
        self.lifecycle = self
            .lifecycle
            .checked_add(1)
            .ok_or_else(|| "Screen Draw parking lifecycle identity exhausted".to_string())?;
        self.completed.fill(None);
        self.parking_cycle = 0;
        Ok(())
    }

    pub fn scope(
        &mut self,
        generation: Option<ScreenDrawGeneration>,
        parking_cycle: Option<u64>,
    ) -> Result<RestoreScope, String> {
        if let Some(generation) = generation
            && generation.get() != self.generation
        {
            self.advance()?;
            self.generation = generation.get();
        }
        if let Some(cycle) = parking_cycle {
            self.parking_cycle = cycle;
        }
        Ok(RestoreScope {
            generation: self.generation,
            lifecycle: self.lifecycle,
            parking_cycle: self.parking_cycle,
        })
    }

    pub fn admission_is_stale(
        &mut self,
        intent: ScreenDrawRecoveryIntent,
        scope: RestoreScope,
    ) -> bool {
        match self.latest_admission {
            Some((latest, _)) if intent.id() < latest.id() => true,
            Some((latest, admitted_scope)) if intent.id() == latest.id() => {
                latest != intent || admitted_scope != scope
            }
            _ => {
                self.latest_admission = Some((intent, scope));
                false
            }
        }
    }

    pub fn previously_admitted(&self, intent: ScreenDrawRecoveryIntent) -> bool {
        self.latest_admission
            .is_some_and(|(latest, _)| latest == intent)
    }

    pub fn decision(&mut self, key: RestoreKey) -> Option<ScreenDrawRestoreOutcome> {
        if let Some(intent) = key.intent
            && self.admission_is_stale(intent, key.scope)
        {
            return Some(ScreenDrawRestoreOutcome::StaleIntent);
        }
        (self.completed[key.cause.slot()] == Some(key))
            .then_some(ScreenDrawRestoreOutcome::Duplicate)
    }

    pub fn complete(&mut self, key: RestoreKey) {
        self.completed[key.cause.slot()] = Some(key);
    }
}
