use crate::commands::{CommandOutcome, JsonUtilityCommand, JsonUtilityCommandHost};

pub(crate) fn handle_json_utility<H>(host: &mut H, command: &JsonUtilityCommand) -> CommandOutcome
where
    H: JsonUtilityCommandHost + ?Sized,
{
    match command {
        JsonUtilityCommand::Open { intent } => host.open_json_utility(*intent),
    }

    CommandOutcome::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        HistoryPolicy, JsonUtilityIntent, PendingQueryPolicy, QueryPolicy, ResultsPolicy,
        VisibilityPolicy,
    };

    #[derive(Default)]
    struct Host {
        intents: Vec<JsonUtilityIntent>,
    }

    impl JsonUtilityCommandHost for Host {
        fn open_json_utility(&mut self, intent: JsonUtilityIntent) {
            self.intents.push(intent);
        }
    }

    #[test]
    fn open_dispatch_carries_intent_and_leaves_launcher_outcome_unchanged() {
        for intent in [
            JsonUtilityIntent::General,
            JsonUtilityIntent::Format,
            JsonUtilityIntent::Minify,
        ] {
            let mut host = Host::default();
            let outcome = handle_json_utility(&mut host, &JsonUtilityCommand::Open { intent });

            assert_eq!(host.intents, [intent]);
            assert_eq!(outcome.query, QueryPolicy::Keep);
            assert_eq!(outcome.pending_query, PendingQueryPolicy::Keep);
            assert_eq!(outcome.results, ResultsPolicy::Keep);
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
            assert_eq!(outcome.history, HistoryPolicy::Skip);
            assert!(outcome.toasts.is_empty());
        }
    }
}
