use std::io;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[cfg(windows)]
pub(super) use windows_tree::ProcessTree;

#[cfg(unix)]
pub(super) use unix_tree::ProcessTree;

const MOST_RUNNING_TREES: usize = 1024;
const NOT_RUNNING: usize = usize::MAX;
const EMPTY: isize = 0;
const RESERVED: isize = -1;
const CLAIMED: isize = -2;
const ADMISSION_WAIT: Duration = Duration::from_secs(2);
const CLEANUP_WAIT: Duration = Duration::from_secs(5);

static RUNNING_TREES: CrashTable = CrashTable::new();

pub fn install_crash_hook() {
    if cfg!(panic = "unwind") {
        return;
    }

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        RUNNING_TREES.kill_all(ADMISSION_WAIT, ProcessTree::kill_remembered);
        previous(info);
    }));
}

impl ProcessTree {
    pub(in crate::execution) fn admit() -> io::Result<Admission> {
        RUNNING_TREES.admit()
    }
}

pub(super) fn crash_started() -> bool {
    RUNNING_TREES.crashing.load(Ordering::SeqCst)
}

fn closing() -> io::Error {
    io::Error::other("not started because Deptide is closing after an internal error")
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::execution) struct Crashing;

struct CrashTable {
    slots: [AtomicIsize; MOST_RUNNING_TREES],
    crashing: AtomicBool,
    admitting: AtomicUsize,
    cleaning: AtomicUsize,
    #[cfg(unix)]
    starts: std::sync::atomic::AtomicPtr<unix_tree::Starts>,
}

impl std::fmt::Debug for CrashTable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CrashTable")
            .field("crashing", &self.crashing)
            .finish_non_exhaustive()
    }
}

impl CrashTable {
    const fn new() -> Self {
        Self {
            slots: [const { AtomicIsize::new(EMPTY) }; MOST_RUNNING_TREES],
            crashing: AtomicBool::new(false),
            admitting: AtomicUsize::new(0),
            cleaning: AtomicUsize::new(0),
            #[cfg(unix)]
            starts: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    fn admit(&'static self) -> io::Result<Admission> {
        #[cfg(unix)]
        let starts = unix_tree::shared_starts(&self.starts)?;
        self.admitting.fetch_add(1, Ordering::SeqCst);
        let mut admission = Admission {
            table: self,
            index: NOT_RUNNING,
            #[cfg(unix)]
            starts,
        };
        if self.crashing.load(Ordering::SeqCst) {
            return Err(closing());
        }
        admission.index = self
            .slots
            .iter()
            .position(|slot| {
                slot.compare_exchange(EMPTY, RESERVED, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            })
            .ok_or_else(|| {
                io::Error::other(format!(
                    "not started because {MOST_RUNNING_TREES} commands are already running"
                ))
            })?;
        Ok(admission)
    }

    fn kill_all(&self, wait: Duration, kill: impl Fn(isize)) {
        self.cleaning.fetch_add(1, Ordering::SeqCst);
        self.crashing.store(true, Ordering::SeqCst);
        wait_until_zero(&self.admitting, wait);
        #[cfg(unix)]
        unix_tree::kill_starting(&self.starts);
        for slot in &self.slots {
            if let Some(tree) = claim(slot) {
                kill(tree);
            }
        }
        self.cleaning.fetch_sub(1, Ordering::SeqCst);
        wait_until_zero(&self.cleaning, CLEANUP_WAIT);
    }
}

fn wait_until_zero(count: &AtomicUsize, limit: Duration) {
    let since = Instant::now();
    while count.load(Ordering::SeqCst) > 0 && since.elapsed() < limit {
        std::thread::yield_now();
    }
}

fn claim(slot: &AtomicIsize) -> Option<isize> {
    let tree = slot.swap(CLAIMED, Ordering::SeqCst);
    (tree != EMPTY && tree != RESERVED && tree != CLAIMED).then_some(tree)
}

#[derive(Debug)]
pub(in crate::execution) struct Admission {
    table: &'static CrashTable,
    index: usize,
    #[cfg(unix)]
    starts: &'static unix_tree::Starts,
}

impl Admission {
    fn hold(self, tree: isize) -> Result<CrashSlot, Crashing> {
        self.table.slots[self.index]
            .compare_exchange(RESERVED, tree, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Crashing)?;
        #[cfg(unix)]
        self.starts.forget(self.index);
        Ok(CrashSlot {
            table: self.table,
            index: AtomicUsize::new(self.index),
            tree,
        })
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(slot) = self.table.slots.get(self.index) {
            #[cfg(unix)]
            self.starts.forget(self.index);
            let _ = slot.compare_exchange(RESERVED, EMPTY, Ordering::SeqCst, Ordering::SeqCst);
        }
        self.table.admitting.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct CrashSlot {
    table: &'static CrashTable,
    index: AtomicUsize,
    tree: isize,
}

impl CrashSlot {
    fn forget(&self) -> Result<(), Crashing> {
        let index = self.index.load(Ordering::SeqCst);
        let Some(slot) = self.table.slots.get(index) else {
            return Ok(());
        };
        if self.table.crashing.load(Ordering::SeqCst) {
            return Err(Crashing);
        }
        slot.compare_exchange(self.tree, EMPTY, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Crashing)?;
        self.index.store(NOT_RUNNING, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(windows)]
mod windows_tree {
    use std::io;
    use std::mem::ManuallyDrop;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::time::{Duration, Instant};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, OpenThread, ResumeThread, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        THREAD_SUSPEND_RESUME,
    };

    use super::{closing, Admission, CrashSlot, Crashing};

    const KILL_WAIT: Duration = Duration::from_secs(5);
    const KILL_POLL: Duration = Duration::from_millis(10);

    #[derive(Debug)]
    pub(in crate::execution) struct ProcessTree {
        job: ManuallyDrop<OwnedHandle>,
        crash_slot: CrashSlot,
    }

    impl ProcessTree {
        pub(in crate::execution) fn adopt(pid: u32, admission: Admission) -> io::Result<Self> {
            let job = owned(unsafe { CreateJobObjectW(None, PCWSTR::null()) }?);
            set_limits(&job, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
            let process =
                owned(unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }?);
            unsafe { AssignProcessToJobObject(raw(&job), raw(&process)) }?;
            let crash_slot = admission
                .hold(job.as_raw_handle() as isize)
                .map_err(|Crashing| closing())?;
            let tree = Self {
                job: ManuallyDrop::new(job),
                crash_slot,
            };
            resume_threads(pid)?;
            Ok(tree)
        }

        pub(super) fn kill_remembered(job: isize) {
            let _ = unsafe { TerminateJobObject(HANDLE(job as *mut _), 1) };
        }

        pub(in crate::execution) fn stop(&self) {
            let _ = unsafe { TerminateJobObject(self.handle(), 1) };
        }

        pub(in crate::execution) fn kill(&self) -> Result<(), Crashing> {
            self.stop();
            let since = Instant::now();
            while self.has_running_processes() && since.elapsed() < KILL_WAIT {
                std::thread::sleep(KILL_POLL);
            }
            self.crash_slot.forget()
        }

        pub(in crate::execution) fn leave_running(&self) -> Result<(), Crashing> {
            self.crash_slot.forget()?;
            let _ = set_limits(&self.job, JOB_OBJECT_LIMIT(0));
            Ok(())
        }

        fn handle(&self) -> HANDLE {
            raw(&self.job)
        }

        fn has_running_processes(&self) -> bool {
            let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            let queried = unsafe {
                QueryInformationJobObject(
                    Some(self.handle()),
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut _,
                    std::mem::size_of_val(&accounting) as u32,
                    None,
                )
            };
            queried.is_ok() && accounting.ActiveProcesses > 0
        }
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            if self.crash_slot.forget().is_ok() {
                unsafe { ManuallyDrop::drop(&mut self.job) };
            }
        }
    }

    fn set_limits(job: &OwnedHandle, flags: JOB_OBJECT_LIMIT) -> io::Result<()> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = flags;
        unsafe {
            SetInformationJobObject(
                raw(job),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        }?;
        Ok(())
    }

    fn resume_threads(pid: u32) -> io::Result<()> {
        let snapshot = owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }?);
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut listed = unsafe { Thread32First(raw(&snapshot), &mut entry) };
        while listed.is_ok() {
            if entry.th32OwnerProcessID == pid {
                let thread =
                    owned(unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }?);
                if unsafe { ResumeThread(raw(&thread)) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
            }
            listed = unsafe { Thread32Next(raw(&snapshot), &mut entry) };
        }
        Ok(())
    }

    fn owned(handle: HANDLE) -> OwnedHandle {
        unsafe { OwnedHandle::from_raw_handle(handle.0) }
    }

    fn raw(handle: &OwnedHandle) -> HANDLE {
        HANDLE(handle.as_raw_handle())
    }

    #[cfg(test)]
    mod tests {
        use std::os::windows::io::AsRawHandle;
        use std::process::{Child, Command, Stdio};
        use std::sync::atomic::Ordering;

        use windows::Win32::System::JobObjects::IsProcessInJob;

        use super::super::{claim, CrashTable};
        use super::*;

        fn new_job() -> OwnedHandle {
            owned(unsafe { CreateJobObjectW(None, PCWSTR::null()) }.unwrap())
        }

        fn run_in(job: &OwnedHandle) -> Child {
            let child = Command::new("node")
                .args(["-e", "setInterval(() => {}, 1000)"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let process = owned(
                unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id()) }
                    .unwrap(),
            );
            unsafe { AssignProcessToJobObject(raw(job), raw(&process)) }.unwrap();
            child
        }

        fn still_running(child: &mut Child) -> bool {
            std::thread::sleep(Duration::from_millis(300));
            child.try_wait().unwrap().is_none()
        }

        fn table() -> &'static CrashTable {
            Box::leak(Box::new(CrashTable::new()))
        }

        fn idle_node(flags: u32) -> Child {
            use std::os::windows::process::CommandExt;
            Command::new("node")
                .args(["-e", "setInterval(() => {}, 1000)"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(flags)
                .spawn()
                .unwrap()
        }

        fn ends_within(child: &mut Child, limit: Duration) -> bool {
            let since = Instant::now();
            while since.elapsed() < limit {
                if child.try_wait().unwrap().is_some() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            false
        }

        #[test]
        fn a_tree_whose_command_ends_while_the_crash_waits_is_still_killed() {
            let table = table();
            let starting = table.admit().unwrap();
            let mut leftover = idle_node(0);
            let tree = ProcessTree::adopt(leftover.id(), table.admit().unwrap()).unwrap();
            let crash = std::thread::spawn(move || {
                table.kill_all(Duration::from_secs(10), ProcessTree::kill_remembered)
            });
            std::thread::sleep(Duration::from_millis(100));

            std::thread::spawn(move || {
                let _ = tree.leave_running();
                drop(tree);
            });
            std::thread::sleep(Duration::from_millis(300));
            drop(starting);
            crash.join().unwrap();

            let killed = ends_within(&mut leftover, Duration::from_secs(3));
            let _ = leftover.kill();
            let _ = leftover.wait();
            assert!(killed);
        }

        #[test]
        fn a_tree_kept_for_the_crash_holds_its_handle_so_the_crash_cannot_hit_a_reused_one() {
            let table = table();
            let starting = table.admit().unwrap();
            let mut kept = idle_node(0);
            let tree = ProcessTree::adopt(kept.id(), table.admit().unwrap()).unwrap();
            let crash = std::thread::spawn(move || {
                table.kill_all(Duration::from_secs(10), ProcessTree::kill_remembered)
            });
            std::thread::sleep(Duration::from_millis(100));

            let token = tree.crash_slot.tree;
            let _ = tree.leave_running();
            drop(tree);
            let mut still_its_job = windows::core::BOOL(0);
            let _ = unsafe {
                IsProcessInJob(
                    HANDLE(kept.as_raw_handle()),
                    Some(HANDLE(token as *mut _)),
                    &mut still_its_job,
                )
            };
            let unrelated_job = new_job();
            let mut unrelated = run_in(&unrelated_job);
            drop(starting);
            crash.join().unwrap();

            let survived = still_running(&mut unrelated);
            for child in [&mut unrelated, &mut kept] {
                let _ = child.kill();
                let _ = child.wait();
            }
            assert!(still_its_job.as_bool());
            assert!(survived);
        }

        #[test]
        fn a_tree_retiring_after_the_crash_returns_so_a_chained_hook_can_take_its_lock() {
            let table = table();
            let mut command = idle_node(0);
            let tree = ProcessTree::adopt(command.id(), table.admit().unwrap()).unwrap();
            table.kill_all(Duration::ZERO, ProcessTree::kill_remembered);
            let shared = std::sync::Arc::new(std::sync::Mutex::new(()));
            let held = shared.clone();
            let (locked, holding) = std::sync::mpsc::channel();

            std::thread::spawn(move || {
                let _guard = held.lock().unwrap();
                locked.send(()).unwrap();
                let _ = tree.leave_running();
                drop(tree);
            });
            holding.recv().unwrap();
            let since = Instant::now();
            let mut taken = false;
            while !taken && since.elapsed() < Duration::from_secs(3) {
                taken = shared.try_lock().is_ok();
                std::thread::sleep(Duration::from_millis(20));
            }

            let _ = command.kill();
            let _ = command.wait();
            assert!(taken);
        }

        #[test]
        fn a_suspended_command_refused_by_the_crash_is_killed_not_left_suspended() {
            const CREATE_SUSPENDED: u32 = 0x0000_0004;
            let table = table();
            let admission = table.admit().unwrap();
            let mut suspended = idle_node(CREATE_SUSPENDED);
            table.kill_all(Duration::ZERO, ProcessTree::kill_remembered);

            let pid = suspended.id();
            std::thread::spawn(move || ProcessTree::adopt(pid, admission).is_err());

            let ended = ends_within(&mut suspended, Duration::from_secs(3));
            let _ = suspended.kill();
            let _ = suspended.wait();
            assert!(ended);
        }

        #[test]
        fn the_crash_never_kills_a_job_that_reused_a_released_handle() {
            let table: &'static CrashTable = Box::leak(Box::new(CrashTable::new()));
            let released_job = new_job();
            let owner = table
                .admit()
                .unwrap()
                .hold(released_job.as_raw_handle() as isize)
                .unwrap();

            let claimed = claim(&table.slots[owner.index.load(Ordering::SeqCst)]);
            let kept_job = owner.forget().is_err().then_some(released_job);
            let unrelated_job = new_job();
            let mut unrelated = run_in(&unrelated_job);
            if let Some(job) = claimed {
                ProcessTree::kill_remembered(job);
            }

            let survived = still_running(&mut unrelated);
            let _ = unrelated.kill();
            let _ = unrelated.wait();
            drop(kept_job);
            assert!(survived);
        }
    }
}

#[cfg(unix)]
mod unix_tree {
    use std::io;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};

    use super::{closing, Admission, CrashSlot, Crashing, MOST_RUNNING_TREES};

    pub(super) struct Starts {
        crashing: AtomicBool,
        pids: [AtomicI32; MOST_RUNNING_TREES],
    }

    impl std::fmt::Debug for Starts {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("Starts")
                .field("crashing", &self.crashing)
                .finish_non_exhaustive()
        }
    }

    impl Starts {
        pub(super) fn forget(&self, index: usize) {
            self.pids[index].store(0, Ordering::SeqCst);
        }
    }

    pub(super) fn shared_starts(cell: &AtomicPtr<Starts>) -> io::Result<&'static Starts> {
        let current = cell.load(Ordering::SeqCst);
        if !current.is_null() {
            return Ok(unsafe { &*current });
        }
        let size = std::mem::size_of::<Starts>();
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if mapped == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let mapped = mapped.cast::<Starts>();
        match cell.compare_exchange(
            std::ptr::null_mut(),
            mapped,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => Ok(unsafe { &*mapped }),
            Err(existing) => {
                unsafe { libc::munmap(mapped.cast(), size) };
                Ok(unsafe { &*existing })
            }
        }
    }

    pub(super) fn kill_starting(cell: &AtomicPtr<Starts>) {
        let starts = cell.load(Ordering::SeqCst);
        if starts.is_null() {
            return;
        }
        let starts = unsafe { &*starts };
        starts.crashing.store(true, Ordering::SeqCst);
        for pid in &starts.pids {
            let pid = pid.swap(0, Ordering::SeqCst);
            if pid != 0 {
                unsafe { libc::kill(-pid, libc::SIGKILL) };
            }
        }
    }

    impl Admission {
        pub(in crate::execution) fn watch_start(&self, command: &mut std::process::Command) {
            use std::os::unix::process::CommandExt;

            let starts = self.starts as *const Starts as usize;
            let index = self.index;
            unsafe {
                command.pre_exec(move || publish_start(starts, index));
            }
        }
    }

    fn publish_start(starts: usize, index: usize) -> io::Result<()> {
        let starts = unsafe { &*(starts as *const Starts) };
        starts.pids[index].store(unsafe { libc::getpid() }, Ordering::SeqCst);
        if starts.crashing.load(Ordering::SeqCst) {
            return Err(io::Error::from_raw_os_error(libc::ECANCELED));
        }
        Ok(())
    }

    #[derive(Debug)]
    pub(in crate::execution) struct ProcessTree {
        pid: u32,
        crash_slot: CrashSlot,
    }

    impl ProcessTree {
        pub(in crate::execution) fn adopt(pid: u32, admission: Admission) -> io::Result<Self> {
            match admission.hold(pid as isize) {
                Ok(crash_slot) => Ok(Self { pid, crash_slot }),
                Err(Crashing) => {
                    signal(pid as i32, libc::SIGKILL);
                    Err(closing())
                }
            }
        }

        pub(super) fn kill_remembered(pid: isize) {
            signal(pid as i32, libc::SIGKILL);
        }

        pub(in crate::execution) fn stop(&self) {
            signal(self.pid as i32, libc::SIGTERM);
        }

        pub(in crate::execution) fn kill(&self) -> Result<(), Crashing> {
            signal(self.pid as i32, libc::SIGKILL);
            self.crash_slot.forget()
        }

        pub(in crate::execution) fn leave_running(&self) -> Result<(), Crashing> {
            self.crash_slot.forget()
        }
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            let _ = self.crash_slot.forget();
        }
    }

    fn signal(pid: i32, signal: libc::c_int) {
        unsafe {
            if libc::kill(-pid, signal) != 0 {
                libc::kill(pid, signal);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::os::unix::process::{CommandExt, ExitStatusExt};
        use std::process::{Child, Command};
        use std::time::Duration;

        use super::super::CrashTable;
        use super::*;

        fn table() -> &'static CrashTable {
            Box::leak(Box::new(CrashTable::new()))
        }

        fn start(admission: &Admission, line: &str) -> io::Result<Child> {
            let mut command = Command::new("sh");
            command.arg("-c").arg(line).process_group(0);
            admission.watch_start(&mut command);
            command.spawn()
        }

        #[test]
        fn a_command_started_but_not_yet_held_when_the_crash_stops_waiting_is_killed() {
            let table = table();
            let admission = table.admit().unwrap();
            let mut child = start(&admission, "sleep 30").unwrap();

            table.kill_all(Duration::ZERO, ProcessTree::kill_remembered);

            let since = std::time::Instant::now();
            let mut status = None;
            while status.is_none() && since.elapsed() < Duration::from_secs(5) {
                status = child.try_wait().unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
            assert_eq!(
                status.and_then(|status| status.signal()),
                Some(libc::SIGKILL)
            );
        }

        #[test]
        fn a_starting_entry_is_signalled_only_as_a_process_group() {
            let table = table();
            let admission = table.admit().unwrap();
            let mut outsider = Command::new("sleep").arg("30").spawn().unwrap();
            admission.starts.pids[admission.index].store(outsider.id() as i32, Ordering::SeqCst);

            table.kill_all(Duration::ZERO, |_| {});

            std::thread::sleep(Duration::from_millis(200));
            let survived = outsider.try_wait().unwrap().is_none();
            let _ = outsider.kill();
            let _ = outsider.wait();
            assert!(survived);
        }

        #[test]
        fn a_command_reaching_exec_after_the_crash_began_never_runs() {
            let table = table();
            let admission = table.admit().unwrap();
            table.kill_all(Duration::ZERO, ProcessTree::kill_remembered);
            let marker =
                std::env::temp_dir().join(format!("deptide-crash-start-{}", std::process::id()));

            let started = start(&admission, &format!("touch '{}'", marker.display()));

            assert!(started.is_err());
            assert!(!marker.exists());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> &'static CrashTable {
        Box::leak(Box::new(CrashTable::new()))
    }

    fn slot_of(owner: &CrashSlot) -> &'static AtomicIsize {
        &owner.table.slots[owner.index.load(Ordering::SeqCst)]
    }

    #[test]
    fn an_owner_may_not_release_a_tree_the_crash_has_claimed() {
        let table = table();
        let owner = table.admit().unwrap().hold(540).unwrap();

        assert_eq!(claim(slot_of(&owner)), Some(540));
        assert_eq!(owner.forget(), Err(Crashing));
    }

    #[test]
    fn the_crash_claims_nothing_its_owner_released_first() {
        let table = table();
        let owner = table.admit().unwrap().hold(540).unwrap();
        let slot = slot_of(&owner);

        assert_eq!(owner.forget(), Ok(()));
        assert_eq!(claim(slot), None);
    }

    #[test]
    fn a_tree_retiring_while_the_crash_waits_is_left_for_the_crash_to_kill() {
        let table = table();
        let owner = table.admit().unwrap().hold(540).unwrap();
        let starting = table.admit().unwrap();
        let killed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = killed.clone();
        let crash = std::thread::spawn(move || {
            table.kill_all(Duration::from_secs(10), |tree| {
                recorded.lock().unwrap().push(tree)
            })
        });
        std::thread::sleep(Duration::from_millis(100));

        let retired = owner.forget();
        drop(starting);
        crash.join().unwrap();

        assert_eq!(retired, Err(Crashing));
        assert_eq!(*killed.lock().unwrap(), vec![540]);
    }

    #[test]
    fn a_second_panic_does_not_return_while_the_first_is_still_killing() {
        let table = table();
        let _owner = table.admit().unwrap().hold(540).unwrap();
        let killed = std::sync::Arc::new(AtomicBool::new(false));
        let finished = killed.clone();
        let (claimed, killing) = std::sync::mpsc::channel();
        let first = std::thread::spawn(move || {
            table.kill_all(Duration::ZERO, |_| {
                claimed.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(300));
                finished.store(true, Ordering::SeqCst);
            })
        });
        killing.recv().unwrap();

        table.kill_all(Duration::ZERO, |_| {});
        let killed_before_second_returned = killed.load(Ordering::SeqCst);

        first.join().unwrap();
        assert!(killed_before_second_returned);
    }

    #[test]
    fn no_command_is_admitted_once_the_crash_has_started() {
        let table = table();
        table.kill_all(Duration::ZERO, |_| {});

        assert!(table.admit().is_err());
    }

    #[test]
    fn a_command_asking_to_start_while_the_crash_waits_is_refused() {
        let table = table();
        let starting = table.admit().unwrap();
        let crash = std::thread::spawn(move || table.kill_all(Duration::from_secs(10), |_| {}));
        std::thread::sleep(Duration::from_millis(100));

        let refused = table.admit().map(|_| ());

        drop(starting);
        crash.join().unwrap();
        assert_eq!(
            refused.unwrap_err().to_string(),
            "not started because Deptide is closing after an internal error"
        );
    }

    #[test]
    fn the_crash_waits_for_a_command_being_started_and_kills_it() {
        let table = table();
        let admission = table.admit().unwrap();
        let killed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = killed.clone();
        let crash = std::thread::spawn(move || {
            table.kill_all(Duration::from_secs(10), |tree| {
                recorded.lock().unwrap().push(tree)
            })
        });

        std::thread::sleep(Duration::from_millis(100));
        let owner = admission.hold(540);
        crash.join().unwrap();

        assert_eq!(*killed.lock().unwrap(), vec![540]);
        assert_eq!(owner.unwrap().forget(), Err(Crashing));
    }

    #[test]
    fn a_command_still_starting_when_the_crash_stops_waiting_is_refused() {
        let table = table();
        let admission = table.admit().unwrap();
        table.kill_all(Duration::ZERO, |_| {});

        assert!(admission.hold(540).is_err());
    }

    #[test]
    fn a_command_beyond_the_full_table_is_refused() {
        let table = table();
        let _owners: Vec<CrashSlot> = (1..=MOST_RUNNING_TREES as isize)
            .map(|tree| table.admit().unwrap().hold(tree).unwrap())
            .collect();

        let refused = table.admit().unwrap_err();

        assert_eq!(
            refused.to_string(),
            "not started because 1024 commands are already running"
        );
    }

    #[test]
    fn a_forgotten_tree_frees_its_slot_for_the_next_command() {
        let table = table();
        let owners: Vec<CrashSlot> = (1..=MOST_RUNNING_TREES as isize)
            .map(|tree| table.admit().unwrap().hold(tree).unwrap())
            .collect();

        assert_eq!(owners[7].forget(), Ok(()));

        assert!(table.admit().is_ok());
    }

    #[test]
    fn a_command_that_failed_to_start_frees_its_slot() {
        let table = table();
        let mut admissions: Vec<Admission> = (0..MOST_RUNNING_TREES)
            .map(|_| table.admit().unwrap())
            .collect();

        drop(admissions.pop());

        assert!(table.admit().is_ok());
    }
}
