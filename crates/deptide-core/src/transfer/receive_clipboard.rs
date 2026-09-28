use std::collections::HashMap;
use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::clipboard::{clipboard_sequence, directories, read_clipboard_files, ClipboardFiles};
use super::receive::{is_project_folder, suggest_target, suggest_target_for, KnownProject};
use super::staging::create_receive_directory;
use super::virtual_files::{
    extract_virtual, read_virtual_file, top_level_folders, ExtractProgress, VirtualFolder,
    VirtualListing,
};
use super::{ClipboardContents, ClipboardEntry, ClipboardSource};
use crate::domain::PackageManifest;
use crate::error::{AppError, AppResult};
use crate::scan::{read_project_at, MANIFEST_FILE_NAME};

const MANIFEST_LIMIT: u64 = 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const SESSION_WAIT: Duration = Duration::from_secs(30);
const SESSION_PAUSE: Duration = Duration::from_millis(50);
const CLIPBOARD_CHANGED: &str = "The clipboard changed, analyze again";
const CLIPBOARD_BUSY: &str = "The clipboard is busy, try again";

/// The clipboard as Receive uses it, so tests can stand in for the system clipboard.
pub trait ClipboardAccess {
    /// `None` when the system offers no sequence number, so nothing can be cached.
    fn sequence(&self) -> Option<u32>;
    fn files(&self) -> ClipboardFiles;
    fn read_file(
        &self,
        listing: &VirtualListing,
        relative: &Path,
        limit: u64,
    ) -> AppResult<Vec<u8>>;
    fn receive_directory(&self) -> AppResult<PathBuf>;
    fn extract(
        &self,
        listing: &VirtualListing,
        destination: &Path,
        progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
    ) -> AppResult<Vec<PathBuf>>;
}

pub struct SystemClipboard;

impl ClipboardAccess for SystemClipboard {
    fn sequence(&self) -> Option<u32> {
        clipboard_sequence()
    }

    fn files(&self) -> ClipboardFiles {
        read_clipboard_files()
    }

    fn read_file(
        &self,
        listing: &VirtualListing,
        relative: &Path,
        limit: u64,
    ) -> AppResult<Vec<u8>> {
        read_virtual_file(listing, relative, limit)
    }

    fn receive_directory(&self) -> AppResult<PathBuf> {
        create_receive_directory()
    }

    fn extract(
        &self,
        listing: &VirtualListing,
        destination: &Path,
        progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
    ) -> AppResult<Vec<PathBuf>> {
        extract_virtual(listing, destination, progress)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardDownload {
    pub sequence: u32,
    pub directory: String,
    pub folders: Vec<DownloadedFolder>,
}

/// `id` is the `path` of the clipboard entry the folder was downloaded for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedFolder {
    pub id: String,
    pub path: String,
}

/// Receive's clipboard reads, one at a time and cached per sequence number; a poll during a read gets the last answer.
#[derive(Default)]
pub struct ReceiveClipboard {
    session: Mutex<Session>,
    last: Mutex<Option<ClipboardContents>>,
    cancelled: AtomicBool,
}

impl ReceiveClipboard {
    pub fn inspect(
        &self,
        access: &dyn ClipboardAccess,
        known: &[KnownProject],
    ) -> ClipboardContents {
        let mut session = match self.session.try_lock() {
            Ok(session) => session,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => {
                return lock(&self.last)
                    .clone()
                    .unwrap_or_else(|| without_entries(ClipboardSource::Busy, None));
            }
        };
        let contents = session.inspect(access, known);
        *lock(&self.last) = Some(contents.clone());
        contents
    }

    /// Extracts clipboard `sequence` into a new receive folder, or returns its earlier download while that still exists.
    pub fn download(
        &self,
        access: &dyn ClipboardAccess,
        sequence: u32,
        progress: &mut dyn FnMut(ExtractProgress),
    ) -> AppResult<ClipboardDownload> {
        self.cancelled.store(false, Ordering::SeqCst);
        let mut session = self.wait_for_session()?;
        if access.sequence() != Some(sequence) {
            return Err(AppError::new(CLIPBOARD_CHANGED));
        }
        if let Some(download) = session.reusable_download(sequence) {
            return Ok(download);
        }

        let listing = session.listing(access, sequence)?;
        let directory = access.receive_directory()?;
        let folders = match self.extract(access, &listing, &directory, progress) {
            Ok(folders) => folders,
            Err(error) => {
                let _ = fs::remove_dir_all(&directory);
                return Err(error);
            }
        };

        let download = ClipboardDownload {
            sequence,
            directory: directory.to_string_lossy().to_string(),
            folders: folders
                .iter()
                .map(|folder| DownloadedFolder {
                    id: virtual_id(
                        sequence,
                        &folder.file_name().unwrap_or_default().to_string_lossy(),
                    ),
                    path: folder.to_string_lossy().to_string(),
                })
                .collect(),
        };
        session.download = Some(download.clone());
        Ok(download)
    }

    pub fn cancel_download(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn wait_for_session(&self) -> AppResult<MutexGuard<'_, Session>> {
        let deadline = Instant::now() + SESSION_WAIT;
        loop {
            match self.session.try_lock() {
                Ok(session) => return Ok(session),
                Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(SESSION_PAUSE);
                }
                Err(TryLockError::WouldBlock) => return Err(AppError::new(CLIPBOARD_BUSY)),
            }
        }
    }

    /// Passes at most one update per `PROGRESS_INTERVAL`, then the last one held back.
    fn extract(
        &self,
        access: &dyn ClipboardAccess,
        listing: &VirtualListing,
        directory: &Path,
        progress: &mut dyn FnMut(ExtractProgress),
    ) -> AppResult<Vec<PathBuf>> {
        let mut passed_at: Option<Instant> = None;
        let mut held = None;
        let folders = access.extract(listing, directory, &mut |update| {
            if passed_at.map_or(true, |at| at.elapsed() >= PROGRESS_INTERVAL) {
                progress(update);
                passed_at = Some(Instant::now());
                held = None;
            } else {
                held = Some(update);
            }
            if self.cancelled.load(Ordering::SeqCst) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })?;
        if let Some(update) = held {
            progress(update);
        }
        Ok(folders)
    }
}

#[derive(Default)]
struct Session {
    read: Option<CachedRead>,
    download: Option<ClipboardDownload>,
}

impl Session {
    fn inspect(
        &mut self,
        access: &dyn ClipboardAccess,
        known: &[KnownProject],
    ) -> ClipboardContents {
        let sequence = access.sequence();
        let cached = self
            .read
            .take()
            .filter(|read| read.sequence.is_some() && read.sequence == sequence);
        let mut read = match cached {
            Some(read) => read,
            None => match access.files() {
                ClipboardFiles::Busy => return without_entries(ClipboardSource::Busy, None),
                ClipboardFiles::Unreadable(reason) => {
                    return without_entries(ClipboardSource::Unreadable, Some(reason));
                }
                files => CachedRead {
                    sequence,
                    files,
                    package_names: HashMap::new(),
                },
            },
        };

        let contents = read.contents(access, known);
        self.read = Some(read);
        contents
    }

    fn reusable_download(&self, sequence: u32) -> Option<ClipboardDownload> {
        self.download
            .as_ref()
            .filter(|download| download.sequence == sequence)
            .filter(|download| {
                download
                    .folders
                    .iter()
                    .all(|folder| Path::new(&folder.path).is_dir())
            })
            .cloned()
    }

    fn listing(&self, access: &dyn ClipboardAccess, sequence: u32) -> AppResult<VirtualListing> {
        let cached = self
            .read
            .as_ref()
            .filter(|read| read.sequence == Some(sequence))
            .map(|read| read.files.clone());
        match cached.unwrap_or_else(|| access.files()) {
            ClipboardFiles::Virtual(listing) if listing.sequence == sequence => Ok(listing),
            ClipboardFiles::Busy => Err(AppError::new(CLIPBOARD_BUSY)),
            ClipboardFiles::Unreadable(reason) => Err(AppError::new(reason)),
            _ => Err(AppError::new(CLIPBOARD_CHANGED)),
        }
    }
}

/// One read of the clipboard; package names are kept only once a manifest read succeeds.
struct CachedRead {
    sequence: Option<u32>,
    files: ClipboardFiles,
    package_names: HashMap<String, Option<String>>,
}

impl CachedRead {
    fn contents(
        &mut self,
        access: &dyn ClipboardAccess,
        known: &[KnownProject],
    ) -> ClipboardContents {
        let listing = match &self.files {
            ClipboardFiles::Virtual(listing) => listing,
            ClipboardFiles::Paths(paths) => {
                return ClipboardContents {
                    entries: directories(paths)
                        .iter()
                        .map(|folder| folder_entry(folder, known))
                        .collect(),
                    ..without_entries(ClipboardSource::Paths, None)
                };
            }
            _ => return without_entries(ClipboardSource::Empty, None),
        };

        let entries = top_level_folders(listing)
            .into_iter()
            .map(|folder| {
                let package_name = if folder.has_manifest {
                    package_name(access, listing, &folder.name, &mut self.package_names)
                } else {
                    None
                };
                ClipboardEntry {
                    path: virtual_id(listing.sequence, &folder.name),
                    is_project: folder.has_manifest,
                    suggested_project: suggest_target_for(
                        &folder.name,
                        package_name.as_deref(),
                        known,
                    ),
                    files: Some(folder.files),
                    bytes: folder_bytes(listing, &folder),
                    package_name,
                    name: folder.name,
                }
            })
            .collect();

        ClipboardContents {
            source: ClipboardSource::Virtual,
            sequence: Some(listing.sequence),
            entries,
            rejected: listing.rejected,
            problem: None,
        }
    }
}

fn package_name(
    access: &dyn ClipboardAccess,
    listing: &VirtualListing,
    folder: &str,
    read_before: &mut HashMap<String, Option<String>>,
) -> Option<String> {
    if let Some(name) = read_before.get(folder) {
        return name.clone();
    }
    let manifest = access
        .read_file(
            listing,
            &Path::new(folder).join(MANIFEST_FILE_NAME),
            MANIFEST_LIMIT,
        )
        .ok()?;
    let name = serde_json::from_slice::<PackageManifest>(&manifest)
        .ok()
        .and_then(|manifest| manifest.name);
    read_before.insert(folder.to_string(), name.clone());
    name
}

/// The size of `folder`, unknown while any file in it came without one.
fn folder_bytes(listing: &VirtualListing, folder: &VirtualFolder) -> Option<u64> {
    let top = Path::new(&folder.name);
    listing
        .entries
        .iter()
        .filter(|entry| !entry.is_directory && entry.relative.starts_with(top))
        .all(|entry| entry.size.is_some())
        .then_some(folder.bytes)
}

fn folder_entry(folder: &Path, known: &[KnownProject]) -> ClipboardEntry {
    ClipboardEntry {
        name: folder
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        is_project: is_project_folder(folder),
        package_name: read_project_at(folder, folder).and_then(|found| found.package_name),
        suggested_project: suggest_target(folder, known),
        path: folder.to_string_lossy().to_string(),
        files: None,
        bytes: None,
    }
}

fn virtual_id(sequence: u32, folder: &str) -> String {
    format!("virtual:{sequence}/{folder}")
}

fn without_entries(source: ClipboardSource, problem: Option<String>) -> ClipboardContents {
    ClipboardContents {
        source,
        sequence: None,
        entries: Vec::new(),
        rejected: 0,
        problem,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
