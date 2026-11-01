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

pub fn start(dur: &str, name: &str) -> anyhow::Result<()> {
    let duration = crate::plugins::timer::parse_duration(dur)
        .ok_or_else(|| anyhow::anyhow!("invalid or non-positive timer duration"))?;
    crate::plugins::timer::try_start_timer_named(
        duration,
        (!name.is_empty()).then(|| name.to_string()),
        "None".to_string(),
    )?;
    Ok(())
}

pub fn set_alarm(time: &str, name: &str) -> anyhow::Result<()> {
    let spec = crate::plugins::timer::parse_alarm_spec(time)?;
    crate::plugins::timer::try_start_alarm_named(
        spec,
        (!name.is_empty()).then(|| name.to_string()),
        "None".to_string(),
    )?;
    Ok(())
}
