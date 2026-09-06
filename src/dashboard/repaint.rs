//! Dashboard repaint cadence policy.

use std::time::Duration;

pub const FAST_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
pub const SLOW_REPAINT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepaintDemand {
    #[default]
    EventDriven,
    Slow,
    Fast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepaintPolicyInput {
    pub launcher_visible: bool,
    pub dashboard_active: bool,
    pub viewport_focused: bool,
    pub reduce_when_unfocused: bool,
    pub demand: RepaintDemand,
}

pub fn dashboard_should_render(launcher_visible: bool, dashboard_active: bool) -> bool {
    launcher_visible && dashboard_active
}

pub fn repaint_interval(input: RepaintPolicyInput) -> Option<Duration> {
    if !dashboard_should_render(input.launcher_visible, input.dashboard_active)
        || input.demand == RepaintDemand::EventDriven
    {
        return None;
    }

    if !input.viewport_focused && input.reduce_when_unfocused {
        return (input.demand == RepaintDemand::Fast).then_some(SLOW_REPAINT_INTERVAL);
    }

    Some(match input.demand {
        RepaintDemand::EventDriven => unreachable!(),
        RepaintDemand::Slow => SLOW_REPAINT_INTERVAL,
        RepaintDemand::Fast => FAST_REPAINT_INTERVAL,
    })
}

pub fn aggregate_demands(demands: impl IntoIterator<Item = RepaintDemand>) -> RepaintDemand {
    demands.into_iter().max().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(demand: RepaintDemand) -> RepaintPolicyInput {
        RepaintPolicyInput {
            launcher_visible: true,
            dashboard_active: true,
            viewport_focused: true,
            reduce_when_unfocused: true,
            demand,
        }
    }

    #[test]
    fn policy_matrix_suppresses_inactive_and_event_driven_frames() {
        for demand in [
            RepaintDemand::EventDriven,
            RepaintDemand::Slow,
            RepaintDemand::Fast,
        ] {
            let mut case = input(demand);
            case.launcher_visible = false;
            assert_eq!(repaint_interval(case), None);

            let mut case = input(demand);
            case.dashboard_active = false;
            assert_eq!(repaint_interval(case), None);
        }
        assert_eq!(repaint_interval(input(RepaintDemand::EventDriven)), None);
        assert!(dashboard_should_render(true, true));
        assert!(!dashboard_should_render(false, true));
        assert!(!dashboard_should_render(true, false));
    }

    #[test]
    fn policy_matrix_uses_fast_and_slow_cadences() {
        assert_eq!(
            repaint_interval(input(RepaintDemand::Slow)),
            Some(SLOW_REPAINT_INTERVAL)
        );
        assert_eq!(
            repaint_interval(input(RepaintDemand::Fast)),
            Some(FAST_REPAINT_INTERVAL)
        );

        let mut reduced_slow = input(RepaintDemand::Slow);
        reduced_slow.viewport_focused = false;
        assert_eq!(repaint_interval(reduced_slow), None);

        let mut reduced_fast = input(RepaintDemand::Fast);
        reduced_fast.viewport_focused = false;
        assert_eq!(repaint_interval(reduced_fast), Some(SLOW_REPAINT_INTERVAL));

        reduced_fast.reduce_when_unfocused = false;
        assert_eq!(repaint_interval(reduced_fast), Some(FAST_REPAINT_INTERVAL));
    }

    #[test]
    fn aggregation_uses_the_highest_active_demand() {
        assert_eq!(aggregate_demands([]), RepaintDemand::EventDriven);
        assert_eq!(
            aggregate_demands([RepaintDemand::Slow, RepaintDemand::EventDriven]),
            RepaintDemand::Slow
        );
        assert_eq!(
            aggregate_demands([RepaintDemand::Slow, RepaintDemand::Fast]),
            RepaintDemand::Fast
        );
    }
}
