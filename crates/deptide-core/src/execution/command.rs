use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::process_tree::ProcessTree;
use crate::util::text::strip_ansi;

const NPM_ENVIRONMENT: &[(&str, &str)] = &[
    ("NPM_CONFIG_FUND", "false"),
    ("NPM_CONFIG_COLOR", "false"),
    ("NO_COLOR", "1"),
    ("FORCE_COLOR", "0"),
];

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

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
    pub capture_stdout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub code: Option<i32>,
    pub killed: bool,
    pub spawn_error: Option<String>,
    pub stdout: Vec<String>,
}

pub fn describe_exit_code(code: Option<i32>) -> String {
    code.map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Default)]
pub struct ProcessRegistry {
    aborted: AtomicBool,
    trees: Mutex<HashMap<u32, (usize, Arc<ProcessTree>)>>,
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

        let trees: Vec<Arc<ProcessTree>> = self
            .lock_trees()
            .values()
            .map(|(_, tree)| tree.clone())
            .collect();
        for tree in trees {
            tree.stop();
        }
    }

    pub fn abort_job(&self, job: usize) {
        if !self.lock_aborted_jobs().insert(job) {
            return;
        }

        let trees: Vec<Arc<ProcessTree>> = self
            .lock_trees()
            .values()
            .filter(|(owner, _)| *owner == job)
            .map(|(_, tree)| tree.clone())
            .collect();
        for tree in trees {
            tree.stop();
        }
    }

    fn register(&self, pid: u32, job: usize, tree: Arc<ProcessTree>) {
        self.lock_trees().insert(pid, (job, tree.clone()));
        if self.is_job_aborted(job) {
            tree.stop();
        }
    }

    fn unregister(&self, pid: u32) {
        self.lock_trees().remove(&pid);
    }

    fn lock_trees(&self) -> std::sync::MutexGuard<'_, HashMap<u32, (usize, Arc<ProcessTree>)>> {
        self.trees
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_aborted_jobs(&self) -> std::sync::MutexGuard<'_, HashSet<usize>> {
        self.aborted_jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

struct TreeEndedOnDrop<'a> {
    pid: u32,
    tree: Option<Arc<ProcessTree>>,
    registry: Option<&'a ProcessRegistry>,
    child: Option<Child>,
}

impl TreeEndedOnDrop<'_> {
    fn child_exited(&mut self) -> Option<Child> {
        let Some(tree) = self.release() else {
            return self.child.take();
        };
        match tree.leave_running() {
            Ok(()) => self.child.take(),
            Err(_) => {
                std::mem::forget(self.child.take());
                None
            }
        }
    }

    fn release(&mut self) -> Option<Arc<ProcessTree>> {
        let tree = self.tree.take()?;
        if let Some(registry) = self.registry {
            registry.unregister(self.pid);
        }
        Some(tree)
    }
}

impl Drop for TreeEndedOnDrop<'_> {
    fn drop(&mut self) {
        if let Some(tree) = self.release() {
            if tree.kill().is_err() {
                std::mem::forget(self.child.take());
            }
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
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
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

fn forward_lines<R>(reader: R, captured: bool, sender: mpsc::UnboundedSender<(bool, String)>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let cleaned = strip_ansi(line.trim_end());
            if !cleaned.trim().is_empty() && sender.send((captured, cleaned)).is_err() {
                break;
            }
        }
    });
}

pub async fn run_command<F>(
    spec: &CommandSpec,
    registry: &ProcessRegistry,
    job: usize,
    on_line: F,
) -> CommandOutcome
where
    F: FnMut(String),
{
    if registry.is_job_aborted(job) {
        return CommandOutcome {
            code: None,
            killed: true,
            spawn_error: None,
            stdout: Vec::new(),
        };
    }

    wait_for_command(spec, Some((registry, job)), on_line).await
}

async fn wait_for_command<F>(
    spec: &CommandSpec,
    owner: Option<(&ProcessRegistry, usize)>,
    mut on_line: F,
) -> CommandOutcome
where
    F: FnMut(String),
{
    let admission = match ProcessTree::admit() {
        Ok(admission) => admission,
        Err(error) => return not_started(error),
    };
    let mut command = build_command(spec);
    #[cfg(unix)]
    admission.watch_start(command.as_std_mut());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return not_started(error),
    };
    let Some(pid) = child.id() else {
        return not_started(std::io::ErrorKind::NotFound.into());
    };
    let tree = match ProcessTree::adopt(pid, admission) {
        Ok(tree) => Arc::new(tree),
        Err(error) => return not_started(error),
    };

    if let Some((registry, job)) = owner {
        registry.register(pid, job, tree.clone());
    }
    let (sender, mut receiver) = mpsc::unbounded_channel::<(bool, String)>();
    if let Some(stdout) = child.stdout.take() {
        forward_lines(stdout, spec.capture_stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        forward_lines(stderr, false, sender.clone());
    }
    drop(sender);
    let mut tree = TreeEndedOnDrop {
        pid,
        tree: Some(tree),
        registry: owner.map(|(registry, _)| registry),
        child: Some(child),
    };

    let mut stdout = Vec::new();
    while let Some((captured, line)) = receiver.recv().await {
        if captured {
            stdout.push(line);
        } else {
            on_line(line);
        }
    }

    if let Some(child) = tree.child.as_mut() {
        wait_until_exited(child).await;
    }
    let status = match tree.child_exited() {
        Some(mut child) => child.wait().await,
        None => Err(std::io::Error::other(
            "Deptide is closing after an internal error",
        )),
    };

    let code = status.as_ref().ok().and_then(|status| status.code());
    let ended_by_signal = status
        .as_ref()
        .map(|status| status.code().is_none())
        .unwrap_or(false);
    let job_aborted = owner.is_some_and(|(registry, job)| registry.is_job_aborted(job));

    CommandOutcome {
        code,
        killed: job_aborted || ended_by_signal,
        spawn_error: None,
        stdout,
    }
}

#[cfg(windows)]
async fn wait_until_exited(child: &mut Child) {
    let _ = child.wait().await;
}

#[cfg(unix)]
async fn wait_until_exited(child: &mut Child) {
    let Some(pid) = child.id() else {
        return;
    };
    let _ = tokio::task::spawn_blocking(move || loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let waited = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if waited == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
        {
            return;
        }
    })
    .await;
}

fn not_started(error: std::io::Error) -> CommandOutcome {
    CommandOutcome {
        code: None,
        killed: false,
        spawn_error: Some(error.to_string()),
        stdout: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn ends_within(child: &mut std::process::Child, limit: Duration) -> bool {
        let since = Instant::now();
        while since.elapsed() < limit {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn registered_after(abort: impl Fn(&ProcessRegistry)) -> bool {
        let mut child = std::process::Command::new("node")
            .args(["-e", "setInterval(() => {}, 1000)"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let registry = ProcessRegistry::new();

        abort(&registry);
        let admission = ProcessTree::admit().unwrap();
        let tree = Arc::new(ProcessTree::adopt(child.id(), admission).unwrap());
        registry.register(child.id(), 5, tree);
        let ended = ends_within(&mut child, Duration::from_secs(5));
        let _ = child.kill();
        let _ = child.wait();
        ended
    }

    #[test]
    fn a_process_registered_after_its_job_was_aborted_is_stopped() {
        assert!(registered_after(|registry| registry.abort_job(5)));
    }

    #[test]
    fn a_process_registered_after_abort_all_is_stopped() {
        assert!(registered_after(|registry| registry.abort_all()));
    }
}
