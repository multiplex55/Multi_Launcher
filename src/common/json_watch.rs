use std::io;
use std::path::Path;

pub struct JsonWatcher;

/// Stubbed file watcher that validates the path exists.
///
/// The real project uses an async file watcher to reload JSON config files when
/// they change. For the Windows-only build used in these exercises we replace
/// that machinery with a synchronous stub. The tests still expect an error when
/// a watched path is missing, so this function performs a simple existence
/// check and returns [`io::ErrorKind::NotFound`] if the path does not exist.
/// Otherwise it succeeds with a no-op [`JsonWatcher`].
pub fn watch_json<F, P>(path: P, _callback: F) -> io::Result<JsonWatcher>
where
    F: FnMut() + Send + 'static,
    P: AsRef<Path>,
{
    if !path.as_ref().exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "path does not exist"));
    }
    Ok(JsonWatcher)
}
