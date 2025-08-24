use std::io::{self, ErrorKind};
use std::path::Path;

pub struct JsonWatcher;

/// Stubbed file watcher that simply validates the path exists.
///
/// Real file watching was previously powered by the `notify` crate but has
/// been removed in favour of a minimal cross platform implementation. The
/// current stub only checks that the provided path exists and returns an
/// [`io::ErrorKind::NotFound`] otherwise. This behaviour allows tests to
/// simulate watcher failures without relying on platform specific tooling.
pub fn watch_json<F, P>(path: P, _callback: F) -> io::Result<JsonWatcher>
where
    F: FnMut() + Send + 'static,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if !path.exists() {
        return Err(io::Error::new(ErrorKind::NotFound, "path not found"));
    }
    Ok(JsonWatcher)
}
