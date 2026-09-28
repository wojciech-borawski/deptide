use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::mem::size_of;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CLIPBRD_E_CANT_OPEN, DV_E_FORMATETC, HGLOBAL};
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows::Win32::System::Com::{
    IDataObject, IStream, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED, TYMED_HGLOBAL,
    TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::{
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    OleGetClipboard, OleInitialize, OleUninitialize, ReleaseStgMedium,
};
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FD_ATTRIBUTES, FD_FILESIZE, FILEDESCRIPTORW,
};

use super::{larger_than, sanitize_relative_path, ExtractProgress, VirtualEntry, VirtualListing};
use crate::error::{AppError, AppResult};
use crate::transfer::clipboard::ClipboardFiles;

const CHUNK_BYTES: usize = 64 * 1024;
const CLIPBOARD_CHANGED: &str = "The clipboard changed, analyze again";
const CANCELLED: &str = "The download was cancelled";
const STALLED: &str = "The remote desktop stopped sending files";
const BUSY_PAUSE: Duration = Duration::from_millis(20);
const POLL_ATTEMPTS: u32 = 5;
const BUSY_ATTEMPTS: u32 = 50;
const EXTRACT_TIMING: Timing = Timing {
    tick: Duration::from_millis(200),
    stall: Duration::from_secs(60),
    grace: Duration::from_secs(2),
};

pub fn read_listing() -> ClipboardFiles {
    let format = registered_format(CFSTR_FILEDESCRIPTORW);
    if unsafe { IsClipboardFormatAvailable(u32::from(format)) }.is_err() {
        return ClipboardFiles::Empty;
    }

    let sequence = sequence_number();
    let read = on_clipboard_thread(POLL_ATTEMPTS, |clipboard| {
        Ok(read_descriptors(clipboard, format))
    });
    match read {
        Ok(Ok(described)) => ClipboardFiles::Virtual(listing_from(sequence, described)),
        Ok(Err(unread)) => unread,
        Err(error) => ClipboardFiles::Unreadable(error.to_string()),
    }
}

pub fn read_file(sequence: u32, entry: &VirtualEntry, limit: u64) -> AppResult<Vec<u8>> {
    on_clipboard_thread(POLL_ATTEMPTS, |clipboard| {
        ensure_unchanged(sequence)?;
        let mut output = Capped::new(&entry.relative, limit);
        let running = AtomicBool::new(false);
        copy_unchanged(
            clipboard,
            sequence,
            entry,
            &mut output,
            &running,
            &mut |_| {},
        )?;
        Ok(output.bytes)
    })
}

pub fn extract(
    listing: &VirtualListing,
    destination: &Path,
    progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
) -> AppResult<()> {
    let (sender, updates) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = {
        let listing = listing.clone();
        let destination = destination.to_path_buf();
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            with_clipboard(BUSY_ATTEMPTS, |clipboard| {
                write_entries(clipboard, &listing, &destination, &sender, &stop)
            })
        })
    };

    let worker = Worker {
        updates,
        handle,
        stop,
    };
    supervise(worker, starting_progress(listing), progress, EXTRACT_TIMING)
}

fn starting_progress(listing: &VirtualListing) -> ExtractProgress {
    let mut files = listing.entries.iter().filter(|entry| !entry.is_directory);
    ExtractProgress {
        files_done: 0,
        files_total: files.clone().count(),
        bytes_done: 0,
        bytes_total: files.try_fold(0u64, |total, file| total.checked_add(file.size?)),
    }
}

fn write_entries(
    clipboard: &Clipboard,
    listing: &VirtualListing,
    destination: &Path,
    report: &Sender<ExtractProgress>,
    stop: &AtomicBool,
) -> AppResult<()> {
    ensure_unchanged(listing.sequence)?;
    fs::create_dir_all(destination)?;
    for folder in listing.entries.iter().filter(|entry| entry.is_directory) {
        fs::create_dir_all(destination.join(&folder.relative))?;
    }

    let mut progress = starting_progress(listing);
    let _ = report.send(progress);

    for file in listing.entries.iter().filter(|entry| !entry.is_directory) {
        ensure_running(stop)?;
        let target = destination.join(&file.relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create_new(&target).map_err(|error| {
            AppError::with_context(
                &format!("{} could not be created", file.relative.display()),
                error,
            )
        })?;
        copy_unchanged(
            clipboard,
            listing.sequence,
            file,
            &mut output,
            stop,
            &mut |bytes| {
                progress.bytes_done += bytes;
                let _ = report.send(progress);
            },
        )?;
        progress.files_done += 1;
        let _ = report.send(progress);
    }

    Ok(())
}

fn ensure_running(stop: &AtomicBool) -> AppResult<()> {
    if stop.load(Ordering::SeqCst) {
        Err(AppError::new(CANCELLED))
    } else {
        Ok(())
    }
}

struct Timing {
    tick: Duration,
    stall: Duration,
    grace: Duration,
}

struct Worker {
    updates: Receiver<ExtractProgress>,
    handle: JoinHandle<AppResult<()>>,
    stop: Arc<AtomicBool>,
}

impl Worker {
    fn stop_with(self, reason: &str, grace: Duration) -> AppResult<()> {
        self.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + grace;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Err(RecvTimeoutError::Disconnected) = self.updates.recv_timeout(left) {
                break;
            }
        }
        Err(AppError::new(reason))
    }
}

/// Passes the worker's updates to `progress`, and the latest one again on every quiet `tick`.
fn supervise(
    worker: Worker,
    starting: ExtractProgress,
    progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
    timing: Timing,
) -> AppResult<()> {
    let mut latest = starting;
    let mut heard_at = Instant::now();

    loop {
        match worker.updates.recv_timeout(timing.tick) {
            Ok(update) => {
                latest = update;
                heard_at = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) if heard_at.elapsed() >= timing.stall => {
                return worker.stop_with(STALLED, timing.grace);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return finished(worker.handle.join()),
        }
        if progress(latest).is_break() {
            return worker.stop_with(CANCELLED, timing.grace);
        }
    }
}

fn on_clipboard_thread<T: Send>(
    attempts: u32,
    work: impl FnOnce(&Clipboard) -> AppResult<T> + Send,
) -> AppResult<T> {
    thread::scope(|scope| finished(scope.spawn(|| with_clipboard(attempts, work)).join()))
}

fn finished<T>(outcome: thread::Result<AppResult<T>>) -> AppResult<T> {
    outcome.unwrap_or_else(|_| Err(AppError::new("The clipboard thread stopped unexpectedly")))
}

fn with_clipboard<T>(attempts: u32, work: impl FnOnce(&Clipboard) -> AppResult<T>) -> AppResult<T> {
    let _ole = OleThread::start()?;
    let clipboard = Clipboard::new(|| unsafe { OleGetClipboard() }, attempts);
    work(&clipboard)
}

/// The clipboard of one OLE thread; each busy answer drops the data object and fetches a new one.
struct Clipboard {
    fetch: Box<dyn Fn() -> windows::core::Result<IDataObject>>,
    attempts: u32,
    data: RefCell<Option<IDataObject>>,
}

impl Clipboard {
    fn new(
        fetch: impl Fn() -> windows::core::Result<IDataObject> + 'static,
        attempts: u32,
    ) -> Self {
        Self {
            fetch: Box::new(fetch),
            attempts,
            data: RefCell::new(None),
        }
    }

    fn try_get(&self, format: &FORMATETC) -> windows::core::Result<Medium> {
        while_busy(self.attempts, || {
            let source = match self.data.take() {
                Some(source) => source,
                None => (self.fetch)()?,
            };
            let medium = unsafe { source.GetData(format) }?;
            self.data.replace(Some(source));
            Ok(Medium(medium))
        })
    }

    fn get(&self, format: &FORMATETC) -> AppResult<Medium> {
        self.try_get(format)
            .map_err(|error| AppError::with_context("Reading the clipboard failed", error))
    }
}

fn while_busy<T>(
    attempts: u32,
    mut attempt: impl FnMut() -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    let mut made = 1;
    loop {
        match attempt() {
            Err(error) if is_busy(&error) && made < attempts => {
                made += 1;
                thread::sleep(BUSY_PAUSE);
            }
            outcome => return outcome,
        }
    }
}

fn is_busy(error: &windows::core::Error) -> bool {
    let code = error.code();
    code == CLIPBRD_E_CANT_OPEN || code == DV_E_FORMATETC
}

fn unread(error: &windows::core::Error) -> ClipboardFiles {
    if is_busy(error) {
        ClipboardFiles::Busy
    } else {
        ClipboardFiles::Unreadable(format!("Reading the clipboard failed: {error}"))
    }
}

struct OleThread;

impl OleThread {
    fn start() -> AppResult<Self> {
        unsafe { OleInitialize(None) }
            .map_err(|error| AppError::with_context("OLE could not start", error))?;
        Ok(Self)
    }
}

impl Drop for OleThread {
    fn drop(&mut self) {
        unsafe { OleUninitialize() }
    }
}

pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

fn ensure_unchanged(sequence: u32) -> AppResult<()> {
    if sequence_number() == sequence {
        Ok(())
    } else {
        Err(AppError::new(CLIPBOARD_CHANGED))
    }
}

fn registered_format(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

fn format_etc(format: u16, index: i32, tymed: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed: tymed as u32,
    }
}

struct Described {
    name: Option<String>,
    is_directory: Option<bool>,
    size: Option<u64>,
}

fn read_descriptors(clipboard: &Clipboard, format: u16) -> Result<Vec<Described>, ClipboardFiles> {
    let unreadable = |error: AppError| ClipboardFiles::Unreadable(error.to_string());
    let medium = clipboard
        .try_get(&format_etc(format, -1, TYMED_HGLOBAL.0))
        .map_err(|error| unread(&error))?;
    let global = medium
        .global()
        .ok_or_else(|| unreadable(AppError::new("The clipboard file list is not in memory")))?;
    with_locked(global, parse_descriptors)
        .and_then(|parsed| parsed)
        .map_err(unreadable)
}

fn parse_descriptors(bytes: &[u8]) -> AppResult<Vec<Described>> {
    let malformed = || AppError::new("The clipboard file list is malformed");
    let (count, rest) = bytes.split_first_chunk::<4>().ok_or_else(malformed)?;
    let descriptor_bytes = size_of::<FILEDESCRIPTORW>();
    let listed = (u32::from_ne_bytes(*count) as usize)
        .checked_mul(descriptor_bytes)
        .and_then(|length| rest.get(..length))
        .ok_or_else(malformed)?;

    Ok(listed
        .chunks_exact(descriptor_bytes)
        .map(|chunk| describe(unsafe { chunk.as_ptr().cast::<FILEDESCRIPTORW>().read_unaligned() }))
        .collect())
}

fn describe(descriptor: FILEDESCRIPTORW) -> Described {
    let units = descriptor.cFileName;
    let length = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let name = String::from_utf16(&units[..length]).ok();
    let size = (descriptor.dwFlags & FD_FILESIZE.0 as u32 != 0)
        .then(|| (u64::from(descriptor.nFileSizeHigh) << 32) | u64::from(descriptor.nFileSizeLow));
    let is_directory = if descriptor.dwFlags & FD_ATTRIBUTES.0 as u32 != 0 {
        Some(descriptor.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0)
    } else if name
        .as_deref()
        .is_some_and(|name| name.ends_with(['\\', '/']))
    {
        Some(true)
    } else {
        None
    };

    Described {
        name,
        is_directory,
        size,
    }
}

fn listing_from(sequence: u32, described: Vec<Described>) -> VirtualListing {
    let mut listing = VirtualListing {
        sequence,
        entries: Vec::new(),
        rejected: 0,
    };
    let mut seen = HashSet::new();
    let mut kinds = Vec::new();

    for (index, file) in (0u32..).zip(described) {
        let relative = file.name.as_deref().and_then(sanitize_relative_path);
        match relative {
            Some(relative) if seen.insert(folded(&relative)) => {
                kinds.push(file.is_directory);
                listing.entries.push(VirtualEntry {
                    index,
                    relative,
                    is_directory: false,
                    size: file.size,
                });
            }
            _ => listing.rejected += 1,
        }
    }

    let parents: HashSet<String> = listing
        .entries
        .iter()
        .flat_map(|entry| {
            let path = folded(&entry.relative);
            path.match_indices('/')
                .map(|(end, _)| path[..end].to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    for (entry, kind) in listing.entries.iter_mut().zip(kinds) {
        entry.is_directory = kind.unwrap_or_else(|| parents.contains(&folded(&entry.relative)));
    }

    listing
}

fn folded(relative: &Path) -> String {
    relative.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// Copies one file; a clipboard change is reported ahead of any copy error.
fn copy_unchanged(
    clipboard: &Clipboard,
    sequence: u32,
    entry: &VirtualEntry,
    output: &mut dyn Write,
    stop: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> AppResult<()> {
    let copied = copy_contents(clipboard, entry, output, stop, on_bytes);
    ensure_unchanged(sequence)?;
    copied
}

fn copy_contents(
    clipboard: &Clipboard,
    entry: &VirtualEntry,
    output: &mut dyn Write,
    stop: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> AppResult<()> {
    let name = entry.relative.display();
    let index = i32::try_from(entry.index)
        .map_err(|_| AppError::new(format!("{name} has no valid clipboard index")))?;
    let format = format_etc(
        registered_format(CFSTR_FILECONTENTS),
        index,
        TYMED_ISTREAM.0 | TYMED_HGLOBAL.0,
    );
    let medium = clipboard.get(&format)?;

    let written = if let Some(stream) = medium.stream() {
        copy_stream(stream, entry.size, output, stop, on_bytes)?
    } else if let Some(global) = medium.global() {
        let size = entry.size.ok_or_else(|| {
            AppError::new(format!(
                "{name} arrived as a memory block without a size, so its end is unknown"
            ))
        })?;
        with_locked(global, |bytes| {
            copy_global(bytes, size, output, stop, on_bytes)
        })??
    } else {
        return Err(AppError::new(format!(
            "{name} arrived in a form Deptide cannot read"
        )));
    };

    match entry.size {
        Some(expected) if written != expected => Err(AppError::new(format!(
            "{name} arrived with {written} of {expected} bytes"
        ))),
        _ => Ok(()),
    }
}

fn copy_stream(
    stream: &IStream,
    size: Option<u64>,
    output: &mut dyn Write,
    stop: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> AppResult<u64> {
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut total = 0u64;

    while size.map_or(true, |size| total <= size) {
        let mut read = 0u32;
        unsafe {
            stream.Read(
                buffer.as_mut_ptr().cast(),
                CHUNK_BYTES as u32,
                Some(&mut read),
            )
        }
        .ok()
        .map_err(|error| AppError::with_context("Reading from the clipboard failed", error))?;
        let chunk = buffer
            .get(..read as usize)
            .ok_or_else(|| AppError::new("The clipboard returned more bytes than asked for"))?;
        if chunk.is_empty() {
            break;
        }
        ensure_running(stop)?;
        output.write_all(chunk)?;
        total += chunk.len() as u64;
        on_bytes(chunk.len() as u64);
    }

    Ok(total)
}

fn copy_global(
    bytes: &[u8],
    size: u64,
    output: &mut dyn Write,
    stop: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> AppResult<u64> {
    let length = usize::try_from(size).map_or(bytes.len(), |size| size.min(bytes.len()));

    for chunk in bytes[..length].chunks(CHUNK_BYTES) {
        ensure_running(stop)?;
        output.write_all(chunk)?;
        on_bytes(chunk.len() as u64);
    }

    Ok(length as u64)
}

/// A buffer that refuses to grow past `limit` bytes.
struct Capped<'a> {
    relative: &'a Path,
    limit: u64,
    bytes: Vec<u8>,
}

impl<'a> Capped<'a> {
    fn new(relative: &'a Path, limit: u64) -> Self {
        Self {
            relative,
            limit,
            bytes: Vec::new(),
        }
    }
}

impl Write for Capped<'_> {
    fn write(&mut self, chunk: &[u8]) -> io::Result<usize> {
        if (self.bytes.len() + chunk.len()) as u64 > self.limit {
            return Err(io::Error::other(larger_than(self.relative, self.limit)));
        }
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn with_locked<R>(global: HGLOBAL, read: impl FnOnce(&[u8]) -> R) -> AppResult<R> {
    let start = unsafe { GlobalLock(global) };
    if start.is_null() {
        return Err(AppError::new("The clipboard data could not be read"));
    }
    let _unlock = Unlock(global);
    let length = unsafe { GlobalSize(global) };
    let bytes = unsafe { std::slice::from_raw_parts(start.cast::<u8>(), length) };
    Ok(read(bytes))
}

struct Unlock(HGLOBAL);

impl Drop for Unlock {
    fn drop(&mut self) {
        let _ = unsafe { GlobalUnlock(self.0) };
    }
}

struct Medium(STGMEDIUM);

impl Medium {
    fn kind(&self) -> TYMED {
        TYMED(self.0.tymed as i32)
    }

    fn global(&self) -> Option<HGLOBAL> {
        if self.kind() != TYMED_HGLOBAL {
            return None;
        }
        Some(unsafe { self.0.u.hGlobal })
    }

    fn stream(&self) -> Option<&IStream> {
        if self.kind() != TYMED_ISTREAM {
            return None;
        }
        unsafe { self.0.u.pstm.as_ref() }
    }
}

impl Drop for Medium {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::mem::ManuallyDrop;
    use std::path::PathBuf;
    use std::rc::Rc;

    use windows::core::{implement, Ref, BOOL, HRESULT};
    use windows::Win32::Foundation::{DV_E_LINDEX, E_NOTIMPL, S_OK};
    use windows::Win32::System::Com::{
        IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM_0,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GMEM_MOVEABLE};
    use windows::Win32::UI::Shell::SHCreateMemStream;

    use super::*;

    /// A clipboard source that always gives the same answer.
    #[implement(IDataObject)]
    struct Source(HRESULT);

    impl IDataObject_Impl for Source_Impl {
        fn GetData(&self, _format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            self.0.ok()?;
            let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, 1) }?;
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: global },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        }

        fn GetDataHere(
            &self,
            _format: *const FORMATETC,
            _medium: *mut STGMEDIUM,
        ) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        fn QueryGetData(&self, _format: *const FORMATETC) -> HRESULT {
            E_NOTIMPL
        }

        fn GetCanonicalFormatEtc(
            &self,
            _format: *const FORMATETC,
            _out: *mut FORMATETC,
        ) -> HRESULT {
            E_NOTIMPL
        }

        fn SetData(
            &self,
            _format: *const FORMATETC,
            _medium: *const STGMEDIUM,
            _release: BOOL,
        ) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        fn EnumFormatEtc(&self, _direction: u32) -> windows::core::Result<IEnumFORMATETC> {
            Err(E_NOTIMPL.into())
        }

        fn DAdvise(
            &self,
            _format: *const FORMATETC,
            _flags: u32,
            _sink: Ref<'_, IAdviseSink>,
        ) -> windows::core::Result<u32> {
            Err(E_NOTIMPL.into())
        }

        fn DUnadvise(&self, _connection: u32) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
            Err(E_NOTIMPL.into())
        }
    }

    /// A clipboard whose fetches hand out `answers` in order, the last one
    /// repeating, and the number of fetches made.
    fn clipboard_answering(answers: &[HRESULT]) -> (Clipboard, Rc<Cell<usize>>) {
        let answers = answers.to_vec();
        let fetches = Rc::new(Cell::new(0));
        let counted = Rc::clone(&fetches);
        let clipboard = Clipboard::new(
            move || {
                let answer = answers[counted.get().min(answers.len() - 1)];
                counted.set(counted.get() + 1);
                Ok(Source(answer).into())
            },
            BUSY_ATTEMPTS,
        );
        (clipboard, fetches)
    }

    fn descriptor(name: &str, flags: u32, attributes: u32) -> FILEDESCRIPTORW {
        let mut units = [0u16; 260];
        for (slot, unit) in units.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }
        FILEDESCRIPTORW {
            cFileName: units,
            dwFlags: flags,
            dwFileAttributes: attributes,
            ..Default::default()
        }
    }

    /// A file list block: the count, then one descriptor per name.
    fn file_list(count: u32, names: &[&str]) -> Vec<u8> {
        let mut bytes = count.to_ne_bytes().to_vec();
        for name in names {
            let descriptor = descriptor(name, 0, 0);
            let raw = std::ptr::from_ref(&descriptor).cast::<u8>();
            bytes.extend_from_slice(unsafe {
                std::slice::from_raw_parts(raw, size_of::<FILEDESCRIPTORW>())
            });
        }
        bytes
    }

    fn described(name: &str, is_directory: Option<bool>) -> Described {
        Described {
            name: Some(name.to_string()),
            is_directory,
            size: Some(1),
        }
    }

    fn kinds(listing: &VirtualListing) -> Vec<(String, bool)> {
        listing
            .entries
            .iter()
            .map(|entry| (folded(&entry.relative), entry.is_directory))
            .collect()
    }

    fn unsized_entry() -> VirtualEntry {
        VirtualEntry {
            index: 0,
            relative: PathBuf::from("web").join("data.bin"),
            is_directory: false,
            size: None,
        }
    }

    #[test]
    fn a_file_list_is_read_up_to_its_count() {
        let described = parse_descriptors(&file_list(1, &["a.txt", "slack.txt"])).unwrap();

        let names: Vec<Option<&str>> = described.iter().map(|file| file.name.as_deref()).collect();
        assert_eq!(names, vec![Some("a.txt")]);
    }

    #[test]
    fn a_file_list_shorter_than_its_count_is_malformed() {
        for bytes in [file_list(2, &["a.txt"]), vec![1, 0]] {
            let error = parse_descriptors(&bytes)
                .err()
                .expect("the list is refused");
            assert!(error.message().contains("malformed"), "{error}");
        }
    }

    #[test]
    fn folder_attributes_count_only_when_the_descriptor_says_they_are_set() {
        let directory = FILE_ATTRIBUTE_DIRECTORY.0;
        let attributes = FD_ATTRIBUTES.0 as u32;

        assert_eq!(
            describe(descriptor("web", attributes, directory)).is_directory,
            Some(true)
        );
        assert_eq!(
            describe(descriptor("web", attributes, 0)).is_directory,
            Some(false)
        );
        assert_eq!(describe(descriptor("web", 0, directory)).is_directory, None);
        assert_eq!(describe(descriptor("web\\", 0, 0)).is_directory, Some(true));
    }

    #[test]
    fn an_entry_of_unknown_kind_is_a_folder_only_when_something_lies_inside_it() {
        let listing = listing_from(
            3,
            vec![
                described("web", None),
                described("web\\src", None),
                described("web\\src\\index.ts", None),
                described("notes", None),
                described("lib", Some(true)),
                described("docs\\readme.md", None),
                described("WEB\\SRC\\app.ts", None),
            ],
        );

        assert_eq!(
            kinds(&listing),
            vec![
                ("web".to_string(), true),
                ("web/src".to_string(), true),
                ("web/src/index.ts".to_string(), false),
                ("notes".to_string(), false),
                ("lib".to_string(), true),
                ("docs/readme.md".to_string(), false),
                ("web/src/app.ts".to_string(), false),
            ]
        );
    }

    #[test]
    fn a_name_listed_twice_in_any_case_keeps_its_first_entry() {
        let listing = listing_from(
            3,
            vec![
                described("web\\a.txt", Some(false)),
                described("WEB\\A.TXT", Some(false)),
                described("web/a.txt", Some(false)),
                described("web", Some(true)),
                described("Web", Some(true)),
                described("web\\b.txt", Some(false)),
            ],
        );

        let kept: Vec<(u32, String)> = listing
            .entries
            .iter()
            .map(|entry| (entry.index, folded(&entry.relative)))
            .collect();
        assert_eq!(
            kept,
            vec![
                (0, "web/a.txt".to_string()),
                (3, "web".to_string()),
                (5, "web/b.txt".to_string()),
            ]
        );
        assert_eq!(listing.rejected, 3);
    }

    #[test]
    fn a_busy_clipboard_is_told_apart_from_one_that_cannot_be_read() {
        assert_eq!(unread(&CLIPBRD_E_CANT_OPEN.into()), ClipboardFiles::Busy);
        assert_eq!(unread(&DV_E_FORMATETC.into()), ClipboardFiles::Busy);
        match unread(&DV_E_LINDEX.into()) {
            ClipboardFiles::Unreadable(reason) => {
                assert!(reason.contains("0x80040068"), "{reason}")
            }
            other => panic!("expected an unreadable clipboard, got {other:?}"),
        }
    }

    #[test]
    fn a_memory_block_without_a_size_is_refused() {
        let (clipboard, _) = clipboard_answering(&[S_OK]);
        let mut output = Vec::new();

        let error = copy_contents(
            &clipboard,
            &unsized_entry(),
            &mut output,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .expect_err("a memory block without a size has no known end");

        assert!(error.message().contains("without a size"), "{error}");
        assert!(output.is_empty());
    }

    #[test]
    fn a_stopped_copy_writes_nothing_more() {
        let bytes = vec![7u8; CHUNK_BYTES * 3];
        let stream = unsafe { SHCreateMemStream(Some(&bytes)) }.expect("a memory stream");
        let stop = AtomicBool::new(false);
        let mut output = Vec::new();

        let error = copy_stream(&stream, None, &mut output, &stop, &mut |_| {
            stop.store(true, Ordering::SeqCst)
        })
        .expect_err("the copy stops");

        assert_eq!(error.message(), CANCELLED);
        assert_eq!(output.len(), CHUNK_BYTES);

        let mut output = Vec::new();
        let error = copy_global(&bytes, bytes.len() as u64, &mut output, &stop, &mut |_| {})
            .expect_err("a stopped copy writes nothing");
        assert_eq!(error.message(), CANCELLED);
        assert!(output.is_empty());
    }

    #[test]
    fn a_capped_buffer_refuses_to_grow_past_its_limit() {
        let relative = PathBuf::from("web").join("package.json");
        let mut output = Capped::new(&relative, 10);

        output.write_all(b"0123456789").unwrap();
        let error = output
            .write_all(b"x")
            .expect_err("the eleventh byte is refused");

        assert!(
            error.to_string().contains("larger than 10 bytes"),
            "{error}"
        );
        assert_eq!(output.bytes, b"0123456789");
    }

    fn fast_timing(stall: Duration) -> Timing {
        Timing {
            tick: Duration::from_millis(10),
            stall,
            grace: Duration::from_millis(500),
        }
    }

    fn progress_of(files_done: usize) -> ExtractProgress {
        ExtractProgress {
            files_done,
            files_total: 3,
            bytes_done: 0,
            bytes_total: None,
        }
    }

    /// A worker running `work` with the update sender and the stop flag.
    fn worker(
        work: impl FnOnce(Sender<ExtractProgress>, Arc<AtomicBool>) -> AppResult<()> + Send + 'static,
    ) -> (Worker, Arc<AtomicBool>) {
        let (sender, updates) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || work(sender, flag));
        (
            Worker {
                updates,
                handle,
                stop: Arc::clone(&stop),
            },
            stop,
        )
    }

    #[test]
    fn a_finished_worker_hands_back_its_result_after_its_updates() {
        let (done, _) = worker(|report, _| {
            let _ = report.send(progress_of(1));
            let _ = report.send(progress_of(3));
            Ok(())
        });
        let mut seen = Vec::new();

        supervise(
            done,
            progress_of(0),
            &mut |update| {
                seen.push(update.files_done);
                ControlFlow::Continue(())
            },
            fast_timing(Duration::from_secs(10)),
        )
        .unwrap();
        assert_eq!(seen.last(), Some(&3));

        let (failed, _) = worker(|_, _| Err(AppError::new("disk full")));
        let error = supervise(
            failed,
            progress_of(0),
            &mut |_| ControlFlow::Continue(()),
            fast_timing(Duration::from_secs(10)),
        )
        .unwrap_err();
        assert_eq!(error.message(), "disk full");
    }

    #[test]
    fn a_worker_that_goes_quiet_is_given_up_and_told_to_stop() {
        let (release, released) = mpsc::channel::<()>();
        let (quiet, stop) = worker(move |report, _| {
            let _ = report.send(progress_of(1));
            let _ = released.recv_timeout(Duration::from_secs(10));
            Ok(())
        });
        let started = Instant::now();

        let error = supervise(
            quiet,
            progress_of(0),
            &mut |_| ControlFlow::Continue(()),
            fast_timing(Duration::from_millis(150)),
        )
        .unwrap_err();
        let _ = release.send(());

        assert_eq!(error.message(), STALLED);
        assert!(
            stop.load(Ordering::SeqCst),
            "the worker was not told to stop"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn breaking_the_progress_callback_stops_even_a_quiet_worker() {
        let (release, released) = mpsc::channel::<()>();
        let (quiet, stop) = worker(move |_, _| {
            let _ = released.recv_timeout(Duration::from_secs(10));
            Ok(())
        });
        let mut calls = 0;

        let error = supervise(
            quiet,
            progress_of(0),
            &mut |update| {
                calls += 1;
                assert_eq!(update, progress_of(0));
                if calls == 2 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
            fast_timing(Duration::from_secs(10)),
        )
        .unwrap_err();
        let _ = release.send(());

        assert_eq!(error.message(), CANCELLED);
        assert!(stop.load(Ordering::SeqCst));
    }

    #[test]
    fn a_stopped_worker_is_waited_for_before_the_error_returns() {
        let (busy, _) = worker(|report, stop| {
            while !stop.load(Ordering::SeqCst) {
                let _ = report.send(progress_of(1));
                thread::sleep(Duration::from_millis(1));
            }
            thread::sleep(Duration::from_millis(50));
            Ok(())
        });
        let (ended, end) = mpsc::channel::<()>();
        let watcher = thread::spawn(move || end.recv_timeout(Duration::from_secs(5)));

        let error = supervise(
            busy,
            progress_of(0),
            &mut |_| ControlFlow::Break(()),
            fast_timing(Duration::from_secs(10)),
        )
        .unwrap_err();
        drop(ended);

        assert_eq!(error.message(), CANCELLED);
        assert!(watcher.join().unwrap().is_err());
    }

    fn any_format() -> FORMATETC {
        format_etc(1, -1, TYMED_HGLOBAL.0)
    }

    #[test]
    fn a_busy_answer_fetches_the_clipboard_again() {
        let (clipboard, fetches) = clipboard_answering(&[DV_E_FORMATETC, S_OK]);

        let medium = clipboard
            .get(&any_format())
            .expect("the second fetch answers");

        assert!(medium.global().is_some());
        assert_eq!(fetches.get(), 2);
    }

    #[test]
    fn a_clipboard_that_answers_is_fetched_once() {
        let (clipboard, fetches) = clipboard_answering(&[S_OK]);

        clipboard.get(&any_format()).unwrap();
        clipboard.get(&any_format()).unwrap();

        assert_eq!(fetches.get(), 1);
    }

    #[test]
    fn a_refusal_is_not_asked_again() {
        let (clipboard, fetches) = clipboard_answering(&[DV_E_LINDEX, S_OK]);

        let error = clipboard.get(&any_format()).err().expect("the read fails");

        assert!(error.message().contains("0x80040068"), "{error}");
        assert_eq!(fetches.get(), 1);
    }

    #[test]
    fn a_clipboard_that_stays_busy_is_given_up_on_after_its_attempts() {
        for attempts in [POLL_ATTEMPTS, BUSY_ATTEMPTS] {
            let fetches = Rc::new(Cell::new(0));
            let counted = Rc::clone(&fetches);
            let clipboard = Clipboard::new(
                move || {
                    counted.set(counted.get() + 1);
                    Err(CLIPBRD_E_CANT_OPEN.into())
                },
                attempts,
            );

            let error = clipboard.get(&any_format()).err().expect("the read fails");

            assert!(error.message().contains("0x800401D0"), "{error}");
            assert_eq!(fetches.get(), attempts as usize);
        }
    }
}
