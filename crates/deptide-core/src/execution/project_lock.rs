use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(windows)]
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::process_tree::crash_started;
use crate::error::{AppError, AppResult};
use crate::util::time::{iso_timestamp, now_ms};
use crate::workspace::normalize_directory;

const LOCK_EXTENSION: &str = "lock";
const CLAIM_ATTEMPTS: usize = 5;
const HELD_BY_ANOTHER_PROCESS: &str = "Another Deptide process is working in this project";
#[cfg(windows)]
const RETRY_DELAY: Duration = Duration::from_millis(20);
#[cfg(windows)]
const UNREADABLE_LOCK_AGE: Duration = Duration::from_secs(10);

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockHolder {
    id: String,
    pid: u32,
    #[serde(default)]
    process_started: Option<u64>,
    owner: String,
    since: String,
    directory: String,
}

impl LockHolder {
    fn describe(&self) -> String {
        format!(
            "{} is working in this project (process {}, since {})",
            self.owner, self.pid, self.since
        )
    }
}

pub struct ProjectLock {
    path: PathBuf,
    id: String,
    file: Option<File>,
}

impl ProjectLock {
    fn give_back(&mut self, crashing: bool) {
        if crashing {
            std::mem::forget(self.file.take());
            return;
        }
        if cfg!(windows) {
            drop(self.file.take());
        }
        if read_holder(&self.path).is_some_and(|holder| holder.id == self.id) {
            let _ = fs::remove_file(&self.path);
        }
        drop(self.file.take());
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        self.give_back(crash_started());
    }
}

fn lock_file(locks_directory: &Path, project_directory: &Path) -> PathBuf {
    let digest = Sha256::digest(normalize_directory(project_directory).as_bytes());
    let name: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    locks_directory.join(format!("{name}.{LOCK_EXTENSION}"))
}

fn read_holder(path: &Path) -> Option<LockHolder> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(windows)]
fn create_lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
}

#[cfg(windows)]
enum ProcessState {
    Gone,
    Running { started: Option<u64> },
}

#[cfg(windows)]
fn process_state(pid: u32) -> ProcessState {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => handle,
        Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => {
            return ProcessState::Gone;
        }
        Err(_) => return ProcessState::Running { started: None },
    };
    let mut exit_code = 0;
    let exited =
        unsafe { GetExitCodeProcess(handle, &mut exit_code) }.is_ok() && exit_code != STILL_ACTIVE;
    let mut created = FILETIME::default();
    let mut unused = [FILETIME::default(); 3];
    let [exit, kernel, user] = &mut unused;
    let started = unsafe { GetProcessTimes(handle, &mut created, exit, kernel, user) }
        .ok()
        .map(|()| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime));
    let _ = unsafe { CloseHandle(handle) };

    if exited {
        ProcessState::Gone
    } else {
        ProcessState::Running { started }
    }
}

#[cfg(windows)]
fn this_process_started() -> Option<u64> {
    match process_state(std::process::id()) {
        ProcessState::Running { started } => started,
        ProcessState::Gone => None,
    }
}

#[cfg(unix)]
fn this_process_started() -> Option<u64> {
    None
}

#[cfg(windows)]
fn is_stale(holder: &LockHolder) -> bool {
    match process_state(holder.pid) {
        ProcessState::Gone => true,
        ProcessState::Running { started } => {
            matches!((started, holder.process_started), (Some(now), Some(then)) if now != then)
        }
    }
}

#[cfg(windows)]
fn is_abandoned(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > UNREADABLE_LOCK_AGE)
}

#[cfg(windows)]
fn is_taken(error: &std::io::Error, path: &Path) -> bool {
    match error.kind() {
        ErrorKind::AlreadyExists => true,
        ErrorKind::PermissionDenied => {
            !fs::symlink_metadata(path).is_err_and(|missing| missing.kind() == ErrorKind::NotFound)
        }
        _ => false,
    }
}

#[cfg(windows)]
fn claim_lock_file(path: &Path) -> AppResult<File> {
    claim_lock_file_with(path, create_lock_file)
}

#[cfg(windows)]
fn claim_lock_file_with(
    path: &Path,
    mut create: impl FnMut(&Path) -> std::io::Result<File>,
) -> AppResult<File> {
    let mut refused = None;
    for _ in 0..CLAIM_ATTEMPTS {
        refused = match create(path) {
            Ok(file) => return Ok(file),
            Err(error) if is_taken(&error, path) => {
                match read_holder(path) {
                    Some(current) if !is_stale(&current) => {
                        return Err(AppError::new(current.describe()));
                    }
                    Some(_) => {
                        let _ = fs::remove_file(path);
                    }
                    None if is_abandoned(path) => {
                        let _ = fs::remove_file(path);
                    }
                    None => std::thread::sleep(RETRY_DELAY),
                }
                None
            }
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                std::thread::sleep(RETRY_DELAY);
                Some(error)
            }
            Err(error) => return Err(error.into()),
        };
    }

    Err(refused.map_or_else(|| AppError::new(HELD_BY_ANOTHER_PROCESS), AppError::from))
}

#[cfg(unix)]
fn open_lock_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

#[cfg(unix)]
fn claim_lock_file(path: &Path) -> AppResult<File> {
    claim_lock_file_with(path, open_lock_file)
}

#[cfg(unix)]
fn claim_lock_file_with(
    path: &Path,
    mut open: impl FnMut(&Path) -> std::io::Result<File>,
) -> AppResult<File> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::io::AsRawFd;

    for _ in 0..CLAIM_ATTEMPTS {
        let file = open(path)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != ErrorKind::WouldBlock {
                return Err(error.into());
            }
            return Err(AppError::new(read_holder(path).map_or_else(
                || HELD_BY_ANOTHER_PROCESS.to_string(),
                |holder| holder.describe(),
            )));
        }

        let held = file.metadata()?;
        let still_named = fs::metadata(path)
            .is_ok_and(|named| named.dev() == held.dev() && named.ino() == held.ino());
        if still_named {
            file.set_len(0)?;
            return Ok(file);
        }
    }

    Err(AppError::new(HELD_BY_ANOTHER_PROCESS))
}

fn this_program() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "Deptide".to_string())
}

pub(crate) fn run_owner(label: &str) -> String {
    format!("the run \"{label}\" in {}", this_program())
}

pub(crate) fn restore_owner() -> String {
    format!("a restore in {}", this_program())
}

pub fn lock_project(
    locks_directory: &Path,
    project_directory: &Path,
    owner: &str,
) -> AppResult<ProjectLock> {
    fs::create_dir_all(locks_directory)?;
    let path = lock_file(locks_directory, project_directory);
    let holder = LockHolder {
        id: format!(
            "{}-{}-{}",
            std::process::id(),
            now_ms(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ),
        pid: std::process::id(),
        process_started: this_process_started(),
        owner: owner.to_string(),
        since: iso_timestamp(Utc::now()),
        directory: project_directory.to_string_lossy().to_string(),
    };

    let mut file = claim_lock_file(&path)?;
    file.write_all(serde_json::to_string(&holder)?.as_bytes())?;
    file.sync_all()?;
    Ok(ProjectLock {
        path,
        id: holder.id,
        file: Some(file),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_folder() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "deptide-project-lock-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn a_lock_given_back_during_a_crash_stays_behind_for_a_stale_takeover() {
        let directory = temporary_folder();
        let project = directory.join("web");
        fs::create_dir_all(&project).unwrap();
        let mut lock = lock_project(&directory.join("locks"), &project, "test").unwrap();
        let path = lock.path.clone();

        lock.give_back(true);
        let stayed = path.exists();
        std::mem::forget(lock);

        assert!(stayed);
    }

    #[cfg(windows)]
    #[test]
    fn a_lock_file_gone_after_access_was_denied_is_claimed_on_the_next_attempt() {
        let directory = temporary_folder();
        let path = directory.join("project.lock");
        let mut attempts = 0;

        let claimed = claim_lock_file_with(&path, |path| {
            attempts += 1;
            if attempts == 1 {
                Err(std::io::Error::from_raw_os_error(5))
            } else {
                create_lock_file(path)
            }
        })
        .map(drop)
        .map_err(|error| error.to_string());
        fs::remove_dir_all(&directory).unwrap();

        assert_eq!((claimed, attempts), (Ok(()), 2));
    }

    #[cfg(windows)]
    #[test]
    fn access_denied_on_every_attempt_reports_the_system_error_after_the_last_attempt() {
        let directory = temporary_folder();
        let path = directory.join("project.lock");
        let mut attempts = 0;

        let refused = claim_lock_file_with(&path, |_| {
            attempts += 1;
            Err(std::io::Error::from_raw_os_error(5))
        })
        .map(drop)
        .map_err(|error| error.to_string());
        fs::remove_dir_all(&directory).unwrap();

        let refused = refused.expect_err("access is never granted");
        assert!(refused.contains("(os error 5)"), "{refused}");
        assert_eq!(attempts, CLAIM_ATTEMPTS);
    }

    #[cfg(windows)]
    #[test]
    fn a_lock_file_found_taken_after_access_was_denied_reports_the_holder_not_the_denial() {
        let directory = temporary_folder();
        let path = directory.join("project.lock");
        let mut attempts = 0;

        let refused = claim_lock_file_with(&path, |path| {
            attempts += 1;
            if attempts == 1 {
                return Err(std::io::Error::from_raw_os_error(5));
            }
            fs::write(path, "not a holder")?;
            Err(ErrorKind::AlreadyExists.into())
        })
        .map(drop)
        .map_err(|error| error.to_string());
        fs::remove_dir_all(&directory).unwrap();

        assert_eq!(
            (refused, attempts),
            (Err(HELD_BY_ANOTHER_PROCESS.to_string()), CLAIM_ATTEMPTS)
        );
    }

    #[cfg(windows)]
    #[test]
    fn access_denied_waits_between_attempts() {
        let directory = temporary_folder();
        let path = directory.join("project.lock");
        let started = std::time::Instant::now();

        let refused = claim_lock_file_with(&path, |_| Err(std::io::Error::from_raw_os_error(5)));
        let elapsed = started.elapsed();
        fs::remove_dir_all(&directory).unwrap();

        assert!(refused.is_err());
        assert!(
            elapsed >= RETRY_DELAY * (CLAIM_ATTEMPTS as u32 - 1),
            "{elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_file_unlinked_before_its_flock_is_claimed_again_by_its_path() {
        use std::os::unix::fs::MetadataExt;

        let directory = temporary_folder();
        let path = directory.join("project.lock");
        let mut opened = 0;

        let claimed = claim_lock_file_with(&path, |path| {
            opened += 1;
            let file = open_lock_file(path)?;
            if opened == 1 {
                fs::remove_file(path)?;
                open_lock_file(path)?;
            }
            Ok(file)
        })
        .expect("the file now at the path is claimed");
        let held = claimed.metadata().unwrap().ino();
        let named = fs::metadata(&path).unwrap().ino();
        drop(claimed);
        fs::remove_dir_all(&directory).unwrap();

        assert_eq!((opened, held), (2, named));
    }
}
