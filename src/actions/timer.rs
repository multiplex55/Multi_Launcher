pub fn cancel(id: u64) {
    crate::plugins::timer::cancel_timer(id);
}

pub fn pause(id: u64) {
    crate::plugins::timer::pause_timer(id);
}

pub fn resume(id: u64) {
    crate::plugins::timer::resume_timer(id);
}

pub fn cancel_checked(id: u64) -> anyhow::Result<()> {
    checked(id, crate::plugins::timer::try_cancel_timer(id), "cancel")
}

pub fn pause_checked(id: u64) -> anyhow::Result<()> {
    checked(id, crate::plugins::timer::try_pause_timer(id), "pause")
}

pub fn resume_checked(id: u64) -> anyhow::Result<()> {
    checked(id, crate::plugins::timer::try_resume_timer(id), "resume")
}

fn checked(
    id: u64,
    result: crate::plugins::timer::TimerMutation,
    operation: &str,
) -> anyhow::Result<()> {
    use crate::plugins::timer::TimerMutation;
    match result {
        TimerMutation::Updated => Ok(()),
        TimerMutation::Missing => {
            anyhow::bail!("could not {operation} timer: timer {id} is no longer available")
        }
        TimerMutation::AlreadyPaused => {
            anyhow::bail!("could not {operation} timer: timer {id} is already paused")
        }
        TimerMutation::AlreadyRunning => {
            anyhow::bail!("could not {operation} timer: timer {id} is already running")
        }
    }
}

pub fn start(dur: &str, name: &str) {
    if let Some(d) = crate::plugins::timer::parse_duration(dur) {
        if name.is_empty() {
            crate::plugins::timer::start_timer(d, "None".to_string());
        } else {
            crate::plugins::timer::start_timer_named(d, Some(name.to_string()), "None".to_string());
        }
    }
}

pub fn set_alarm(time: &str, name: &str) {
    if let Some((h, m, date)) = crate::plugins::timer::parse_hhmm(time) {
        if name.is_empty() {
            crate::plugins::timer::start_alarm(h, m, date, "None".to_string());
        } else {
            crate::plugins::timer::start_alarm_named(
                h,
                m,
                date,
                Some(name.to_string()),
                "None".to_string(),
            );
        }
    }
}
