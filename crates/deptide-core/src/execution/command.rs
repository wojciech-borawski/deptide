use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::util::text::strip_ansi;

const NPM_ENVIRONMENT: &[(&str, &str)] = &[
    ("NPM_CONFIG_FUND", "false"),
    ("NPM_CONFIG_COLOR", "false"),
    ("NO_COLOR", "1"),
    ("FORCE_COLOR", "0"),
];

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn npm_program() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub shell: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub code: Option<i32>,
    pub killed: bool,
    pub spawn_error: Option<String>,
}

pub fn describe_exit_code(code: Option<i32>) -> String {
    code.map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Default)]
pub struct ProcessRegistry {
    aborted: AtomicBool,
    pids: Mutex<HashMap<u32, usize>>,
    aborted_jobs: Mutex<HashSet<usize>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    pub fn is_job_aborted(&self, job: usize) -> bool {
        self.is_aborted() || self.lock_aborted_jobs().contains(&job)
    }

    pub fn abort_all(&self) {
        if self.aborted.swap(true, Ordering::SeqCst) {
            return;
        }

        let pids: Vec<u32> = self.lock_pids().keys().copied().collect();
        for pid in pids {
            kill_tree(pid);
        }
    }

    pub fn abort_job(&self, job: usize) {
        if !self.lock_aborted_jobs().insert(job) {
            return;
        }

        let pids: Vec<u32> = self
            .lock_pids()
            .iter()
            .filter(|(_, owner)| **owner == job)
            .map(|(pid, _)| *pid)
            .collect();
        for pid in pids {
            kill_tree(pid);
        }
    }

    fn register(&self, pid: u32, job: usize) {
        self.lock_pids().insert(pid, job);
    }

    fn unregister(&self, pid: u32) {
        self.lock_pids().remove(&pid);
    }

    fn lock_pids(&self) -> std::sync::MutexGuard<'_, HashMap<u32, usize>> {
        self.pids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_aborted_jobs(&self) -> std::sync::MutexGuard<'_, HashSet<usize>> {
        self.aborted_jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;

    let _ = std::process::Command::new("taskkill")
        .args(["/pid", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(unix)]
fn kill_tree(pid: u32) {
    unsafe {
        if libc::kill(-(pid as i32), libc::SIGTERM) != 0 {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

fn build_command(spec: &CommandSpec) -> Command {
    let mut command = match &spec.shell {
        Some(line) => shell_command(line),
        None => {
            let mut direct = std::process::Command::new(&spec.program);
            direct.args(&spec.args);
            direct
        }
    };
    command
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in NPM_ENVIRONMENT {
        command.env(key, value);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut command = Command::from(command);
    command.kill_on_drop(true);
    command
}

#[cfg(windows)]
fn shell_command(line: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    let mut command = std::process::Command::new("cmd.exe");
    command.arg("/D").arg("/C");
    command.raw_arg(line);
    command
}

#[cfg(not(windows))]
fn shell_command(line: &str) -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(line);
    command
}

fn forward_lines<R>(reader: R, sender: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let cleaned = strip_ansi(line.trim_end());
            if !cleaned.trim().is_empty() && sender.send(cleaned).is_err() {
                break;
            }
        }
    });
}

pub async fn run_command<F>(
    spec: &CommandSpec,
    registry: &ProcessRegistry,
    job: usize,
    mut on_line: F,
) -> CommandOutcome
where
    F: FnMut(String),
{
    if registry.is_job_aborted(job) {
        return CommandOutcome {
            code: None,
            killed: true,
            spawn_error: None,
        };
    }

    let mut child = match build_command(spec).spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandOutcome {
                code: None,
                killed: false,
                spawn_error: Some(error.to_string()),
            }
        }
    };

    let pid = child.id();
    if let Some(pid) = pid {
        registry.register(pid, job);
    }

    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        forward_lines(stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        forward_lines(stderr, sender.clone());
    }
    drop(sender);

    while let Some(line) = receiver.recv().await {
        on_line(line);
    }

    let status = child.wait().await;
    if let Some(pid) = pid {
        registry.unregister(pid);
    }

    let code = status.as_ref().ok().and_then(|status| status.code());
    let ended_by_signal = status
        .as_ref()
        .map(|status| status.code().is_none())
        .unwrap_or(false);

    CommandOutcome {
        code,
        killed: registry.is_job_aborted(job) || ended_by_signal,
        spawn_error: None,
    }
}
