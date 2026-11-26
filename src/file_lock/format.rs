use super::FileLockInspection;

pub fn copy_snapshot(inspection: &FileLockInspection) -> String {
    let inspected_at = chrono::DateTime::<chrono::Local>::from(inspection.inspected_at);
    let mut output = format!(
        "Windows reported users for {}\nInspected {}\n",
        inspection.path.display(),
        inspected_at.format("%Y-%m-%d %H:%M:%S %Z"),
    );
    for process in &inspection.processes {
        use std::fmt::Write as _;
        let _ = writeln!(
            output,
            "PID {}\tFILETIME {}\t{}\t{}\t{}",
            process.process_id,
            process.creation_filetime,
            process.application_name,
            process.service_name,
            process
                .executable_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "executable unavailable".into()),
        );
    }
    if inspection.processes.is_empty() {
        output.push_str("No processes reported.\n");
    }
    output.push_str(
        "These are processes Windows reported; this list does not prove which process is denying access.\n",
    );
    output
}

#[cfg(test)]
mod tests {
    use super::copy_snapshot;
    use crate::file_lock::{FileLockInspection, FileLockProcess};
    use std::{path::PathBuf, time::UNIX_EPOCH};

    #[test]
    fn snapshot_preserves_stock_bytes_for_empty_and_unicode_rows() {
        let mut inspection = FileLockInspection {
            path: PathBuf::from("fixture/é.txt"),
            identity: crate::file_operations::NativeFileIdentity::default(),
            inspected_at: UNIX_EPOCH,
            processes: Vec::new(),
        };
        let time = chrono::DateTime::<chrono::Local>::from(UNIX_EPOCH).format("%Y-%m-%d %H:%M:%S %Z").to_string();
        let prefix = format!("Windows reported users for {}\nInspected {time}\n", inspection.path.display());
        let suffix = "These are processes Windows reported; this list does not prove which process is denying access.\n";
        assert_eq!(copy_snapshot(&inspection), format!("{prefix}No processes reported.\n{suffix}"));
        inspection.processes.push(FileLockProcess { process_id: 17, creation_filetime: 42,
            application_name: "café".into(), service_name: "svc".into(), executable_path: None });
        assert_eq!(copy_snapshot(&inspection), format!("{prefix}PID 17\tFILETIME 42\tcafé\tsvc\texecutable unavailable\n{suffix}"));
    }
}
