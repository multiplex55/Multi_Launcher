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
