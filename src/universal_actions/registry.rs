use std::collections::HashSet;

use super::provider::{ActionResolutionContext, UniversalActionProvider};
use super::providers;
use super::{ActionGroup, ResolvedActionTarget, UniversalAction};

/// Fixed-order registry for all built-in Universal Action providers.
#[derive(Clone, Copy, Debug, Default)]
pub struct UniversalActionRegistry;

impl UniversalActionRegistry {
    pub fn resolve(
        &self,
        target: &ResolvedActionTarget,
        context: &ActionResolutionContext<'_>,
    ) -> Vec<UniversalAction> {
        const PROVIDERS: [&dyn UniversalActionProvider; 13] = [
            &providers::GENERIC,
            &providers::FOLDER,
            &providers::BOOKMARK,
            &providers::TIMER,
            &providers::STOPWATCH,
            &providers::SNIPPET,
            &providers::TEMPFILE,
            &providers::NOTE,
            &providers::CLIPBOARD,
            &providers::TODO,
            &providers::WINDOW,
            &providers::MKMACRO,
            &providers::BROWSER_TAB,
        ];

        resolve_from_providers(&PROVIDERS, target, context)
    }
}

fn resolve_from_providers(
    providers: &[&dyn UniversalActionProvider],
    target: &ResolvedActionTarget,
    context: &ActionResolutionContext<'_>,
) -> Vec<UniversalAction> {
    let mut seen = HashSet::new();
    let mut actions = providers
        .iter()
        .flat_map(|provider| provider.actions(target, context))
        .filter(|action| seen.insert(action.id.clone()))
        .collect::<Vec<_>>();

    // Stable sorting preserves provider order among equally ranked actions.
    actions.sort_by_key(|action| {
        (
            action.presentation.priority.sort_rank(),
            group_rank(action.presentation.group),
        )
    });
    actions
}

const fn group_rank(group: ActionGroup) -> u8 {
    match group {
        ActionGroup::Primary => 0,
        ActionGroup::OpenEdit => 1,
        ActionGroup::Navigation => 2,
        ActionGroup::Window => 3,
        ActionGroup::Automation => 4,
        ActionGroup::CopyShare => 5,
        ActionGroup::Organization => 6,
        ActionGroup::Other => 7,
        ActionGroup::Destructive => 8,
    }
}
