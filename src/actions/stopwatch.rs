pub fn start(name: &str) {
    if name.is_empty() {
        crate::plugins::stopwatch::start_stopwatch_named(None);
    } else {
        crate::plugins::stopwatch::start_stopwatch_named(Some(name.to_string()));
    }
}

pub fn pause(id: u64) {
    crate::plugins::stopwatch::pause_stopwatch(id);
}

pub fn resume(id: u64) {
    crate::plugins::stopwatch::resume_stopwatch(id);
}

pub fn stop(id: u64) {
    crate::plugins::stopwatch::stop_stopwatch(id);
}

pub fn pause_checked(id: u64) -> anyhow::Result<()> {
    checked(
        id,
        crate::plugins::stopwatch::try_pause_stopwatch(id),
        "pause",
    )
}

pub fn resume_checked(id: u64) -> anyhow::Result<()> {
    checked(
        id,
        crate::plugins::stopwatch::try_resume_stopwatch(id),
        "resume",
    )
}

pub fn stop_checked(id: u64) -> anyhow::Result<()> {
    checked(
        id,
        crate::plugins::stopwatch::try_stop_stopwatch(id),
        "stop",
    )
}

fn checked(
    id: u64,
    result: crate::plugins::stopwatch::StopwatchMutation,
    operation: &str,
) -> anyhow::Result<()> {
    use crate::plugins::stopwatch::StopwatchMutation;
    match result {
        StopwatchMutation::Updated => Ok(()),
        StopwatchMutation::Missing => {
            anyhow::bail!("could not {operation} stopwatch: stopwatch {id} is no longer available")
        }
        StopwatchMutation::AlreadyPaused => {
            anyhow::bail!("could not {operation} stopwatch: stopwatch {id} is already paused")
        }
        StopwatchMutation::AlreadyRunning => {
            anyhow::bail!("could not {operation} stopwatch: stopwatch {id} is already running")
        }
    }
}
