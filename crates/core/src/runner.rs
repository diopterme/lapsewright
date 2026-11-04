use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::progress::{Progress, ProgressParser};

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to run {path}: {source}")]
    Spawn { path: PathBuf, source: io::Error },
    /// `stderr`: everything ffmpeg wrote there, blank lines left out.
    #[error("ffmpeg exited with code {code:?}")]
    Failed { code: Option<i32>, stderr: Vec<String> },
    #[error("cancelled")]
    Cancelled,
    #[error("reading ffmpeg output: {0}")]
    Io(#[from] io::Error),
}

/// A running ffmpeg. `progress` closes when ffmpeg closes stdout; then call `wait`.
pub struct Job {
    pub progress: Receiver<Progress>,
    control: Canceller,
    stdout: JoinHandle<io::Result<()>>,
    stderr: JoinHandle<io::Result<Vec<String>>>,
}

/// Can be sent to another thread (Ctrl+C handler, UI) to stop the job.
#[derive(Clone)]
pub struct Canceller {
    child: Arc<Mutex<Child>>,
    is_cancelled: Arc<AtomicBool>,
}

/// Both pipes are drained on their own threads: reading only one lets the other fill and stall ffmpeg.
pub fn start<I, S>(ffmpeg: &Path, args: I) -> Result<Job, RunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| RunError::Spawn { path: ffmpeg.to_path_buf(), source })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (sender, progress) = mpsc::channel();
    Ok(Job {
        progress,
        control: Canceller { child: Arc::new(Mutex::new(child)), is_cancelled: Arc::new(AtomicBool::new(false)) },
        stdout: thread::spawn(move || read_progress(stdout, sender)),
        stderr: thread::spawn(move || read_lines(stderr)),
    })
}

impl Job {
    pub fn canceller(&self) -> Canceller {
        self.control.clone()
    }

    /// Decides the outcome only after both pipes have closed. `Ok`: what ffmpeg wrote to stderr.
    pub fn wait(self) -> Result<Vec<String>, RunError> {
        let stdout = self.stdout.join().expect("stdout reader panicked");
        let stderr = self.stderr.join().expect("stderr reader panicked");
        let status = self.control.child.lock().expect("child lock").wait()?;
        if self.control.is_cancelled.load(Ordering::SeqCst) {
            return Err(RunError::Cancelled);
        }
        stdout?;
        let stderr = stderr?;
        if !status.success() {
            return Err(RunError::Failed { code: status.code(), stderr });
        }
        Ok(stderr)
    }
}

impl Canceller {
    /// Partial output is deleted anyway, so a hard kill is enough.
    pub fn cancel(&self) -> io::Result<()> {
        self.is_cancelled.store(true, Ordering::SeqCst);
        self.child.lock().expect("child lock").kill()
    }
}

fn read_progress(stdout: impl Read, sender: Sender<Progress>) -> io::Result<()> {
    let mut parser = ProgressParser::default();
    for_each_line(stdout, |line| {
        // A dropped receiver only means nobody is watching; keep draining.
        if let Some(update) = parser.feed(line) {
            let _ = sender.send(update);
        }
    })
}

/// `-nostats` keeps this short: stream info, warnings and errors (D11: shown in full).
fn read_lines(stderr: impl Read) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    for_each_line(stderr, |line| {
        if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    })?;
    Ok(lines)
}

/// Lossy decoding: ffmpeg echoes file names and tags that need not be UTF-8.
fn for_each_line(reader: impl Read, mut on_line: impl FnMut(&str)) -> io::Result<()> {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            return Ok(());
        }
        on_line(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']));
    }
}
