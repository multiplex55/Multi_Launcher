use std::io;
use std::path::Path;

pub struct JsonWatcher;

/// Stubbed file watcher that does nothing.
pub fn watch_json<F, P>(_path: P, _callback: F) -> io::Result<JsonWatcher>
where
    F: FnMut() + Send + 'static,
    P: AsRef<Path>,
{
    Ok(JsonWatcher)
}
