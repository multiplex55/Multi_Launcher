use chrono::Local;
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub const TOAST_LOG_FILE: &str = "toast.log";
pub const TOAST_LOG_TAIL_LINES: usize = 20;

#[derive(Debug)]
pub enum ToastLogOpenError {
    Create(io::Error),
    Open(String),
}

impl fmt::Display for ToastLogOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(error) => write!(formatter, "Failed to create log: {error}"),
            Self::Open(error) => write!(formatter, "Failed to open log: {error}"),
        }
    }
}

/// Read the last `count` lines using the same empty behavior as the egui viewer.
/// Missing files, invalid UTF-8 and other read errors produce an empty list.
pub fn read_last_lines(path: impl AsRef<Path>, count: usize) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    if lines.len() > count {
        lines.drain(0..lines.len() - count);
    }
    lines
}

/// Create the canonical log without truncating an existing file, then hand it
/// to the operating system's associated application.
pub fn open_toast_log() -> Result<(), ToastLogOpenError> {
    let path = Path::new(TOAST_LOG_FILE);
    ensure_log_file(path).map_err(ToastLogOpenError::Create)?;
    open::that(path).map_err(|error| ToastLogOpenError::Open(error.to_string()))
}

fn ensure_log_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map(drop)
}

pub fn append_toast_log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(TOAST_LOG_FILE)
    {
        let _ = writeln!(file, "{} - {}", Local::now().to_rfc3339(), msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_last_lines_returns_the_ordered_tail_without_mutating_the_log() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(TOAST_LOG_FILE);
        let content = (1..=23)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();

        let rows = read_last_lines(&path, TOAST_LOG_TAIL_LINES);

        assert_eq!(
            rows,
            (4..=23)
                .map(|line| format!("line-{line}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(fs::read(&path).unwrap(), content.as_bytes());
    }

    #[test]
    fn missing_or_invalid_utf8_log_reads_as_empty_without_creating_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.log");
        assert!(read_last_lines(&missing, TOAST_LOG_TAIL_LINES).is_empty());
        assert!(!missing.exists());

        let invalid = directory.path().join("invalid.log");
        fs::write(&invalid, [0xff, 0xfe]).unwrap();
        assert!(read_last_lines(&invalid, TOAST_LOG_TAIL_LINES).is_empty());
        assert_eq!(fs::read(&invalid).unwrap(), [0xff, 0xfe]);
    }

    #[test]
    fn ensure_log_file_creates_missing_and_preserves_existing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let existing = directory.path().join("existing.log");
        fs::write(&existing, b"preserve these bytes\n").unwrap();
        ensure_log_file(&existing).unwrap();
        assert_eq!(fs::read(&existing).unwrap(), b"preserve these bytes\n");

        let missing = directory.path().join("new.log");
        ensure_log_file(&missing).unwrap();
        assert_eq!(fs::read(&missing).unwrap(), b"");
    }
}
