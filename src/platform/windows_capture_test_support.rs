//! Shared launcher/handshake for the disposable WGC HWND fixture.

use crate::mkmacro::ScreenRect;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) struct SourceReady {
    pub(crate) nonce: String,
    pub(crate) hwnd: usize,
    pub(crate) process_id: u32,
    pub(crate) thread_id: u32,
    pub(crate) bounds: ScreenRect,
}

impl SourceReady {
    fn parse(line: &str) -> Result<Self, String> {
        let fields: Vec<_> = line.split('|').collect();
        if fields.len() != 9 || fields[0] != "READY" {
            return Err(format!("invalid source fixture handshake: {line}"));
        }
        let parse = |value: &str, name: &str| {
            value
                .parse::<u64>()
                .map_err(|error| format!("invalid {name} in fixture handshake: {error}"))
        };
        let hwnd = usize::try_from(parse(fields[2], "HWND")?)
            .map_err(|_| "fixture HWND does not fit usize".to_owned())?;
        let process_id = u32::try_from(parse(fields[3], "PID")?)
            .map_err(|_| "fixture PID does not fit u32".to_owned())?;
        let thread_id = u32::try_from(parse(fields[4], "thread ID")?)
            .map_err(|_| "fixture thread ID does not fit u32".to_owned())?;
        let x = i32::try_from(
            fields[5]
                .parse::<i64>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|_| "fixture X coordinate does not fit i32".to_owned())?;
        let y = i32::try_from(
            fields[6]
                .parse::<i64>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|_| "fixture Y coordinate does not fit i32".to_owned())?;
        let width = u32::try_from(parse(fields[7], "width")?)
            .map_err(|_| "fixture width does not fit u32".to_owned())?;
        let height = u32::try_from(parse(fields[8], "height")?)
            .map_err(|_| "fixture height does not fit u32".to_owned())?;
        Ok(Self {
            nonce: fields[1].to_owned(),
            hwnd,
            process_id,
            thread_id,
            bounds: ScreenRect::new(x, y, width, height),
        })
    }
}

pub(crate) struct OwnedSourceFixture {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    reader: Option<thread::JoinHandle<()>>,
    next_command_id: u64,
    finished: bool,
}

impl OwnedSourceFixture {
    pub(crate) fn spawn(nonce: &str) -> Result<Self, String> {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("pinboard_live_source.ps1");
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-STA", "-File"])
            .arg(script)
            .arg(nonce)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("could not start the owned PowerShell fixture: {error}"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "PowerShell fixture has no stdout pipe".to_owned()
        })?;
        let (line_tx, lines) = mpsc::sync_channel(32);
        let reader = match thread::Builder::new()
            .name("pinboard-live-fixture-output".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    match line {
                        Ok(line) => {
                            if line_tx.try_send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("could not read fixture output: {error}"));
            }
        };
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            lines,
            reader: Some(reader),
            next_command_id: 1,
            finished: false,
        })
    }

    pub(crate) fn process_id(&self) -> u32 {
        self.child.id()
    }

    fn next_line(&self, timeout: Duration) -> Result<String, String> {
        self.lines
            .recv_timeout(timeout)
            .map_err(|error| format!("timed out waiting for the owned source fixture: {error}"))
    }

    pub(crate) fn wait_ready(&self, nonce: &str) -> Result<SourceReady, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("owned source fixture did not become ready within 10 seconds".into());
        }
        let line = self.next_line(remaining)?;
        if line.starts_with("ERROR|") {
            return Err(format!("owned source fixture reported {line}"));
        }
        let ready = SourceReady::parse(&line)?;
        if ready.nonce != nonce {
            return Err("owned source fixture nonce did not match the test".into());
        }
        Ok(ready)
    }

    pub(crate) fn command(&mut self, command: &str) -> Result<(), String> {
        let id = self.next_command_id;
        self.next_command_id = self
            .next_command_id
            .checked_add(1)
            .ok_or_else(|| "fixture command identity exhausted".to_owned())?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "owned source fixture stdin is closed".to_owned())?;
        writeln!(stdin, "{command} {id}")
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("could not send command to owned fixture: {error}"))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "owned source fixture did not acknowledge {command}"
            ));
        }
        let line = self.next_line(remaining)?;
        if line == format!("ACK|{id}") {
            return Ok(());
        }
        if line.starts_with(&format!("ERROR|{id}|")) || line.starts_with("ERROR|") {
            return Err(format!("owned source fixture rejected {command}: {line}"));
        }
        Err(format!(
            "unexpected output from owned source fixture: {line}"
        ))
    }

    pub(crate) fn finish(&mut self) -> Result<(), String> {
        if self
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            self.command("QUIT")?;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                self.child
                    .kill()
                    .map_err(|error| format!("could not reap the owned fixture: {error}"))?;
                break self.child.wait().map_err(|error| error.to_string())?;
            }
            thread::yield_now();
        };
        self.stdin.take();
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| "owned fixture stdout reader terminated unexpectedly".to_owned())?;
        }
        self.finished = true;
        if !status.success() {
            return Err(format!("owned PowerShell fixture exited with {status}"));
        }
        Ok(())
    }
}

impl Drop for OwnedSourceFixture {
    fn drop(&mut self) {
        if !self.finished && self.child.try_wait().ok().flatten().is_none() {
            if let Some(stdin) = self.stdin.as_mut() {
                let id = self.next_command_id;
                let _ = writeln!(stdin, "QUIT {id}").and_then(|()| stdin.flush());
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                thread::yield_now();
            }
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
        self.stdin.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
