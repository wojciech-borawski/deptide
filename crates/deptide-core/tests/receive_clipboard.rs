mod common;

use std::collections::HashMap;
use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::{manifest, TempDir};
use deptide_core::error::{AppError, AppResult};
use deptide_core::transfer::{
    ClipboardAccess, ClipboardFiles, ClipboardSource, ExtractProgress, KnownProject,
    ReceiveClipboard, VirtualEntry, VirtualListing,
};

fn entry(index: u32, relative: &str, is_directory: bool, size: Option<u64>) -> VirtualEntry {
    VirtualEntry {
        index,
        relative: relative.split('/').collect(),
        is_directory,
        size,
    }
}

fn two_folders(sequence: u32) -> VirtualListing {
    VirtualListing {
        sequence,
        rejected: 2,
        entries: vec![
            entry(0, "dash", true, None),
            entry(1, "dash/package.json", false, Some(60)),
            entry(2, "dash/src/index.ts", false, Some(40)),
            entry(4, "docs", true, None),
            entry(5, "docs/readme.md", false, Some(7)),
        ],
    }
}

fn known() -> Vec<KnownProject> {
    vec![
        KnownProject {
            name: "storefront".to_string(),
            directory: PathBuf::from("C:\\repos\\storefront"),
            package_name: Some("@acme/storefront".to_string()),
        },
        KnownProject {
            name: "docs".to_string(),
            directory: PathBuf::from("C:\\repos\\docs"),
            package_name: None,
        },
    ]
}

enum ExtractPlan {
    Write,
    Fail,
    WaitFor(Mutex<Receiver<()>>),
    Report(usize),
    ReportUntilStopped,
}

/// A clipboard that serves `files` and counts every read; the answer to
/// `files` and the manifests can be changed between polls.
struct FakeClipboard {
    sequence: Mutex<Option<u32>>,
    files: Mutex<Vec<ClipboardFiles>>,
    manifests: Mutex<HashMap<PathBuf, Vec<AppResult<Vec<u8>>>>>,
    receive_root: PathBuf,
    extract_plan: Mutex<ExtractPlan>,
    extract_started: Mutex<Option<Sender<()>>>,
    file_reads: AtomicUsize,
    manifest_reads: AtomicUsize,
    directories: AtomicUsize,
    extractions: AtomicUsize,
    stopped_after: Mutex<Option<usize>>,
}

impl FakeClipboard {
    fn new(root: &TempDir, sequence: Option<u32>, files: ClipboardFiles) -> Self {
        Self {
            sequence: Mutex::new(sequence),
            files: Mutex::new(vec![files]),
            manifests: Mutex::new(HashMap::new()),
            receive_root: root.mkdir("received"),
            extract_plan: Mutex::new(ExtractPlan::Write),
            extract_started: Mutex::new(None),
            file_reads: AtomicUsize::new(0),
            manifest_reads: AtomicUsize::new(0),
            directories: AtomicUsize::new(0),
            extractions: AtomicUsize::new(0),
            stopped_after: Mutex::new(None),
        }
    }

    fn virtual_files(root: &TempDir, listing: VirtualListing) -> Self {
        let fake = Self::new(
            root,
            Some(listing.sequence),
            ClipboardFiles::Virtual(listing),
        );
        fake.serve_manifest(
            "dash/package.json",
            Ok(manifest("@acme/storefront", "1.0.0", &[]).into_bytes()),
        );
        fake
    }

    fn serve_manifest(&self, relative: &str, answer: AppResult<Vec<u8>>) {
        self.manifests
            .lock()
            .unwrap()
            .entry(relative.split('/').collect())
            .or_default()
            .push(answer);
    }

    /// The next answers to `files`, the last one repeating.
    fn then_serve(&self, files: ClipboardFiles, sequence: Option<u32>) {
        self.files.lock().unwrap().push(files);
        *self.sequence.lock().unwrap() = sequence;
    }

    fn plan_extraction(&self, plan: ExtractPlan) {
        *self.extract_plan.lock().unwrap() = plan;
    }

    fn reads(&self) -> (usize, usize) {
        (
            self.file_reads.load(Ordering::SeqCst),
            self.manifest_reads.load(Ordering::SeqCst),
        )
    }
}

impl ClipboardAccess for FakeClipboard {
    fn sequence(&self) -> Option<u32> {
        *self.sequence.lock().unwrap()
    }

    fn files(&self) -> ClipboardFiles {
        let reads = self.file_reads.fetch_add(1, Ordering::SeqCst);
        let files = self.files.lock().unwrap();
        files[reads.min(files.len() - 1)].clone()
    }

    fn read_file(
        &self,
        _listing: &VirtualListing,
        relative: &Path,
        _limit: u64,
    ) -> AppResult<Vec<u8>> {
        self.manifest_reads.fetch_add(1, Ordering::SeqCst);
        let mut manifests = self.manifests.lock().unwrap();
        let answers = manifests
            .get_mut(relative)
            .ok_or_else(|| AppError::new(format!("{} is not served", relative.display())))?;
        if answers.len() > 1 {
            answers.remove(0)
        } else {
            answers[0].clone()
        }
    }

    fn receive_directory(&self) -> AppResult<PathBuf> {
        let count = self.directories.fetch_add(1, Ordering::SeqCst);
        let folder = self.receive_root.join(format!("download-{count}"));
        fs::create_dir_all(&folder)?;
        Ok(folder)
    }

    fn extract(
        &self,
        listing: &VirtualListing,
        destination: &Path,
        progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
    ) -> AppResult<Vec<PathBuf>> {
        self.extractions.fetch_add(1, Ordering::SeqCst);
        if let Some(started) = self.extract_started.lock().unwrap().take() {
            let _ = started.send(());
        }
        let total = listing
            .entries
            .iter()
            .filter(|entry| !entry.is_directory)
            .count();
        let report = |done: usize| ExtractProgress {
            files_done: done,
            files_total: total,
            bytes_done: done as u64,
            bytes_total: Some(total as u64),
        };

        match &*self.extract_plan.lock().unwrap() {
            ExtractPlan::Write => {}
            ExtractPlan::Fail => {
                let _ = fs::remove_dir_all(destination);
                return Err(AppError::new("The clipboard changed, analyze again"));
            }
            ExtractPlan::WaitFor(release) => {
                let _ = release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10));
            }
            ExtractPlan::Report(count) => {
                for _ in 0..*count {
                    let _ = progress(report(0));
                }
                let _ = progress(report(total));
            }
            ExtractPlan::ReportUntilStopped => {
                for call in 1..=1000 {
                    if progress(report(0)).is_break() {
                        *self.stopped_after.lock().unwrap() = Some(call);
                        let _ = fs::remove_dir_all(destination);
                        return Err(AppError::new("The download was cancelled"));
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }

        let mut folders = Vec::new();
        for entry in &listing.entries {
            let target = destination.join(&entry.relative);
            if entry.is_directory {
                fs::create_dir_all(&target)?;
            } else {
                fs::create_dir_all(target.parent().unwrap())?;
                fs::write(&target, "x")?;
            }
            let top = destination.join(entry.relative.components().next().unwrap());
            if !folders.contains(&top) {
                folders.push(top);
            }
        }
        Ok(folders)
    }
}

#[test]
fn virtual_folders_are_listed_with_their_size_package_name_and_suggestion() {
    let root = TempDir::new("receive-clipboard-list");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));

    let contents = ReceiveClipboard::default().inspect(&clipboard, &known());

    assert_eq!(contents.source, ClipboardSource::Virtual);
    assert_eq!(contents.sequence, Some(7));
    assert_eq!(contents.rejected, 2);
    assert_eq!(contents.problem, None);
    let listed: Vec<_> = contents
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.name.as_str(),
                entry.is_project,
                entry.package_name.as_deref(),
                entry.suggested_project.as_deref(),
                entry.files,
                entry.bytes,
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            (
                "virtual:7/dash",
                "dash",
                true,
                Some("@acme/storefront"),
                Some("storefront"),
                Some(2),
                Some(100)
            ),
            (
                "virtual:7/docs",
                "docs",
                false,
                None,
                Some("docs"),
                Some(1),
                Some(7)
            ),
        ]
    );
    assert_eq!(
        clipboard.reads(),
        (1, 1),
        "only the folder with a package.json is read"
    );
}

#[test]
fn polls_of_an_unchanged_clipboard_read_nothing_from_it() {
    let root = TempDir::new("receive-clipboard-cache");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();

    let first = session.inspect(&clipboard, &known());
    let second = session.inspect(&clipboard, &known());
    assert_eq!(second, first);
    assert_eq!(clipboard.reads(), (1, 1));

    let renamed = vec![KnownProject {
        name: "shop".to_string(),
        directory: PathBuf::from("C:\\repos\\shop"),
        package_name: Some("@acme/storefront".to_string()),
    }];
    let rematched = session.inspect(&clipboard, &renamed);
    assert_eq!(
        rematched.entries[0].suggested_project.as_deref(),
        Some("shop"),
        "suggestions follow the configured projects on every poll"
    );
    assert_eq!(clipboard.reads(), (1, 1));

    clipboard.then_serve(ClipboardFiles::Virtual(two_folders(8)), Some(8));
    let changed = session.inspect(&clipboard, &known());
    assert_eq!(changed.entries[0].path, "virtual:8/dash");
    assert_eq!(
        clipboard.reads(),
        (2, 2),
        "a new sequence reads the list and manifests again"
    );
}

#[test]
fn every_poll_reads_the_clipboard_when_no_sequence_number_is_available() {
    let root = TempDir::new("receive-clipboard-no-sequence");
    let clipboard = FakeClipboard::new(&root, None, ClipboardFiles::Empty);
    let session = ReceiveClipboard::default();

    session.inspect(&clipboard, &known());
    session.inspect(&clipboard, &known());

    assert_eq!(clipboard.reads().0, 2);
}

#[test]
fn a_busy_clipboard_is_reported_and_read_again_on_the_next_poll() {
    let root = TempDir::new("receive-clipboard-busy");
    let clipboard = FakeClipboard::new(&root, Some(7), ClipboardFiles::Busy);
    let session = ReceiveClipboard::default();

    let busy = session.inspect(&clipboard, &known());
    assert_eq!(busy.source, ClipboardSource::Busy);
    assert!(busy.entries.is_empty());

    clipboard.then_serve(ClipboardFiles::Virtual(two_folders(7)), Some(7));
    clipboard.serve_manifest("dash/package.json", Ok(b"{\"name\":\"x\"}".to_vec()));
    let listed = session.inspect(&clipboard, &known());

    assert_eq!(listed.source, ClipboardSource::Virtual);
    assert_eq!(clipboard.reads().0, 2);
}

#[test]
fn an_unreadable_clipboard_reports_the_reason() {
    let root = TempDir::new("receive-clipboard-unreadable");
    let reason = "The clipboard file list is malformed".to_string();
    let clipboard = FakeClipboard::new(&root, Some(7), ClipboardFiles::Unreadable(reason.clone()));

    let contents = ReceiveClipboard::default().inspect(&clipboard, &known());

    assert_eq!(contents.source, ClipboardSource::Unreadable);
    assert_eq!(contents.problem, Some(reason));
}

#[test]
fn a_failed_manifest_read_is_tried_again_and_a_good_one_is_kept() {
    let root = TempDir::new("receive-clipboard-manifest-retry");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    clipboard.manifests.lock().unwrap().clear();
    clipboard.serve_manifest("dash/package.json", Err(AppError::new("busy")));
    clipboard.serve_manifest("dash/package.json", Ok(b"not json".to_vec()));
    let session = ReceiveClipboard::default();

    let failed = session.inspect(&clipboard, &known());
    assert_eq!(failed.entries[0].package_name, None);
    let unparsed = session.inspect(&clipboard, &known());
    assert_eq!(unparsed.entries[0].package_name, None);
    session.inspect(&clipboard, &known());

    assert_eq!(
        clipboard.reads(),
        (1, 2),
        "the failed read is repeated once, the unparsable answer is kept"
    );
}

#[test]
fn folders_on_disk_are_listed_from_the_file_list() {
    let root = TempDir::new("receive-clipboard-paths");
    let web = root.mkdir("staged/web");
    root.write(
        "staged/web/package.json",
        &manifest("@acme/storefront", "1.0.0", &[]),
    );
    let loose = root.write("staged/notes.txt", "notes\n");
    let clipboard = FakeClipboard::new(
        &root,
        Some(3),
        ClipboardFiles::Paths(vec![web.clone(), loose]),
    );

    let contents = ReceiveClipboard::default().inspect(&clipboard, &known());

    assert_eq!(contents.source, ClipboardSource::Paths);
    assert_eq!(contents.sequence, None);
    assert_eq!(contents.entries.len(), 1);
    let entry = &contents.entries[0];
    assert_eq!(entry.path, web.to_string_lossy());
    assert!(entry.is_project);
    assert_eq!(entry.package_name.as_deref(), Some("@acme/storefront"));
    assert_eq!(entry.suggested_project.as_deref(), Some("storefront"));
    assert_eq!((entry.files, entry.bytes), (None, None));
}

#[test]
fn a_download_is_extracted_once_per_sequence_and_reused() {
    let root = TempDir::new("receive-clipboard-download");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());

    let first = session.download(&clipboard, 7, &mut |_| {}).unwrap();
    let second = session.download(&clipboard, 7, &mut |_| {}).unwrap();

    assert_eq!(second, first);
    assert_eq!(clipboard.extractions.load(Ordering::SeqCst), 1);
    assert_eq!(
        clipboard.reads().0,
        1,
        "the listing of the last poll is extracted"
    );
    let directory = PathBuf::from(&first.directory);
    assert_eq!(first.sequence, 7);
    let folders: Vec<(&str, PathBuf)> = first
        .folders
        .iter()
        .map(|folder| (folder.id.as_str(), PathBuf::from(&folder.path)))
        .collect();
    assert_eq!(
        folders,
        vec![
            ("virtual:7/dash", directory.join("dash")),
            ("virtual:7/docs", directory.join("docs")),
        ]
    );
}

#[test]
fn a_download_whose_folder_is_gone_is_extracted_again() {
    let root = TempDir::new("receive-clipboard-download-gone");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());

    let first = session.download(&clipboard, 7, &mut |_| {}).unwrap();
    fs::remove_dir_all(&first.folders[1].path).unwrap();
    let second = session.download(&clipboard, 7, &mut |_| {}).unwrap();

    assert_eq!(clipboard.extractions.load(Ordering::SeqCst), 2);
    assert_ne!(second.directory, first.directory);
}

#[test]
fn a_download_for_a_clipboard_that_changed_since_the_poll_is_refused() {
    let root = TempDir::new("receive-clipboard-download-changed");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());
    clipboard.then_serve(ClipboardFiles::Virtual(two_folders(8)), Some(8));

    let error = session.download(&clipboard, 7, &mut |_| {}).unwrap_err();

    assert!(error.message().contains("clipboard changed"), "{error}");
    assert_eq!(clipboard.directories.load(Ordering::SeqCst), 0);
    assert_eq!(clipboard.extractions.load(Ordering::SeqCst), 0);
}

#[test]
fn a_download_reads_the_list_itself_when_no_poll_saw_this_sequence() {
    let root = TempDir::new("receive-clipboard-download-unpolled");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();

    let download = session.download(&clipboard, 7, &mut |_| {}).unwrap();

    assert_eq!(download.folders.len(), 2);
    assert_eq!(clipboard.reads().0, 1);
}

#[test]
fn a_failed_download_is_not_reused() {
    let root = TempDir::new("receive-clipboard-download-failed");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());
    clipboard.plan_extraction(ExtractPlan::Fail);

    let error = session.download(&clipboard, 7, &mut |_| {}).unwrap_err();
    assert!(error.message().contains("clipboard changed"), "{error}");

    clipboard.plan_extraction(ExtractPlan::Write);
    let download = session.download(&clipboard, 7, &mut |_| {}).unwrap();

    assert_eq!(clipboard.extractions.load(Ordering::SeqCst), 2);
    assert!(Path::new(&download.folders[0].path).is_dir());
}

#[test]
fn a_poll_during_a_download_answers_from_the_last_poll_without_reading() {
    let root = TempDir::new("receive-clipboard-poll-during-download");
    let clipboard = Arc::new(FakeClipboard::virtual_files(&root, two_folders(7)));
    let session = Arc::new(ReceiveClipboard::default());
    let before = session.inspect(&*clipboard, &known());
    let (release, released) = mpsc::channel();
    let (started, extracting) = mpsc::channel();
    clipboard.plan_extraction(ExtractPlan::WaitFor(Mutex::new(released)));
    *clipboard.extract_started.lock().unwrap() = Some(started);

    let downloader = {
        let clipboard = Arc::clone(&clipboard);
        let session = Arc::clone(&session);
        thread::spawn(move || session.download(&*clipboard, 7, &mut |_| {}))
    };
    extracting
        .recv_timeout(Duration::from_secs(10))
        .expect("the extraction starts");
    clipboard.then_serve(ClipboardFiles::Virtual(two_folders(9)), Some(9));

    let during = session.inspect(&*clipboard, &known());
    release.send(()).unwrap();
    let download = downloader.join().unwrap();

    assert_eq!(during, before);
    assert_eq!(
        clipboard.reads(),
        (1, 1),
        "the poll did not touch the clipboard"
    );
    assert!(download.is_ok(), "{download:?}");
}

#[test]
fn cancelling_stops_the_running_download() {
    let root = TempDir::new("receive-clipboard-cancel");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());
    clipboard.plan_extraction(ExtractPlan::ReportUntilStopped);
    let mut delivered = 0;

    let error = session
        .download(&clipboard, 7, &mut |_| {
            delivered += 1;
            session.cancel_download();
        })
        .unwrap_err();

    assert!(error.message().contains("cancelled"), "{error}");
    assert_eq!(delivered, 1);
    assert_eq!(*clipboard.stopped_after.lock().unwrap(), Some(1));

    clipboard.plan_extraction(ExtractPlan::Write);
    assert!(
        session.download(&clipboard, 7, &mut |_| {}).is_ok(),
        "a cancel does not stop the next download"
    );
}

#[test]
fn progress_is_passed_on_at_most_ten_times_a_second_ending_with_the_last_update() {
    let root = TempDir::new("receive-clipboard-throttle");
    let clipboard = FakeClipboard::virtual_files(&root, two_folders(7));
    let session = ReceiveClipboard::default();
    session.inspect(&clipboard, &known());
    clipboard.plan_extraction(ExtractPlan::Report(500));
    let mut delivered = Vec::new();

    session
        .download(&clipboard, 7, &mut |progress| delivered.push(progress))
        .unwrap();

    assert!(
        (2..=4).contains(&delivered.len()),
        "{} updates passed: {delivered:?}",
        delivered.len()
    );
    assert_eq!(delivered[0].files_done, 0);
    let last = delivered.last().unwrap();
    assert_eq!((last.files_done, last.files_total), (3, 3));
}

#[test]
fn a_folder_holding_a_file_of_unknown_size_has_no_size() {
    let root = TempDir::new("receive-clipboard-unsized");
    let mut listing = two_folders(7);
    listing.entries[2].size = None;
    let clipboard = FakeClipboard::virtual_files(&root, listing);

    let contents = ReceiveClipboard::default().inspect(&clipboard, &known());

    let sizes: Vec<_> = contents
        .entries
        .iter()
        .map(|entry| (entry.files, entry.bytes))
        .collect();
    assert_eq!(sizes, vec![(Some(2), None), (Some(1), Some(7))]);
}
