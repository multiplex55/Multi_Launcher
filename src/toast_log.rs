use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

pub const TOAST_LOG_FILE: &str = "toast.log";

pub fn append_toast_log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(TOAST_LOG_FILE)
    {
        let _ = writeln!(file, "{} - {}", Local::now().to_rfc3339(), msg);
    }
}
