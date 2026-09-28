#![cfg(windows)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::mem::{size_of, ManuallyDrop};
use std::ops::ControlFlow;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::TempDir;
use deptide_core::error::AppResult;
use deptide_core::transfer::{
    extract_virtual, read_clipboard_files, read_virtual_file, top_level_folders, ClipboardFiles,
    ExtractProgress, VirtualEntry, VirtualFolder, VirtualListing,
};
use windows::core::{implement, Ref, BOOL, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    CLIPBRD_E_CANT_OPEN, DATA_S_SAMEFORMATETC, DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED, E_FAIL,
    E_NOTIMPL, E_OUTOFMEMORY, LPARAM, OLE_E_ADVISENOTSUPPORTED, S_OK, WPARAM,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    OleInitialize, OleIsCurrentClipboard, OleSetClipboard, OleUninitialize, CF_HDROP,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    SHCreateMemStream, SHCreateStdEnumFmtEtc, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, DROPFILES,
    FD_ATTRIBUTES, FD_FILESIZE, FILEDESCRIPTORW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, TranslateMessage, MSG,
    PM_REMOVE, WM_QUIT,
};

const BIG_FILE_BYTES: usize = 2_621_447;
const GLOBAL_SLACK_BYTES: usize = 37;
const SLACK_BYTE: u8 = 0xAB;
const PACKAGE_JSON: &[u8] = b"{ \"name\": \"@acme/web\", \"version\": \"1.0.0\" }\n";
const INDEX_TS: &[u8] = b"export const answer = 42;\n";
const NOTES: &[u8] = b"loose file\n";

#[derive(Clone, Copy)]
enum Delivery {
    Stream,
    Global,
}

#[derive(Clone)]
enum Kind {
    Directory,
    File {
        bytes: Vec<u8>,
        delivery: Delivery,
        declared_size: Option<u64>,
    },
}

#[derive(Clone)]
struct Offered {
    name: &'static str,
    kind: Kind,
}

fn directory(name: &'static str) -> Offered {
    Offered {
        name,
        kind: Kind::Directory,
    }
}

fn file_declaring(
    name: &'static str,
    bytes: &[u8],
    delivery: Delivery,
    declared_size: Option<u64>,
) -> Offered {
    Offered {
        name,
        kind: Kind::File {
            bytes: bytes.to_vec(),
            delivery,
            declared_size,
        },
    }
}

fn file(name: &'static str, bytes: &[u8], delivery: Delivery) -> Offered {
    file_declaring(name, bytes, delivery, Some(bytes.len() as u64))
}

fn unsized_file(name: &'static str, bytes: &[u8]) -> Offered {
    file_declaring(name, bytes, Delivery::Stream, None)
}

fn noise(length: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn big_file() -> Vec<u8> {
    noise(BIG_FILE_BYTES, 0x5eed)
}

fn project_on_clipboard() -> Vec<Offered> {
    vec![
        directory("web"),
        file("..\\escape.txt", b"outside\n", Delivery::Stream),
        file("web\\package.json", PACKAGE_JSON, Delivery::Global),
        file("web\\src\\index.ts", INDEX_TS, Delivery::Stream),
        directory("web\\src\\empty"),
        file("web\\assets\\big.bin", &big_file(), Delivery::Stream),
        file("notes.txt", NOTES, Delivery::Stream),
    ]
}

fn path(segments: &[&str]) -> PathBuf {
    segments.iter().collect()
}

fn registered(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

fn format_etc(format: u16, tymed: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: tymed as u32,
    }
}

fn struct_bytes<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

fn global_medium(bytes: &[u8], slack: usize) -> windows::core::Result<STGMEDIUM> {
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len() + slack)?;
        let target = GlobalLock(global).cast::<u8>();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        std::ptr::write_bytes(target.add(bytes.len()), SLACK_BYTE, slack);
        let _ = GlobalUnlock(global);
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: global },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }
}

fn stream_medium(bytes: &[u8]) -> windows::core::Result<STGMEDIUM> {
    let stream = unsafe { SHCreateMemStream(Some(bytes)) }.ok_or(E_OUTOFMEMORY)?;
    Ok(STGMEDIUM {
        tymed: TYMED_ISTREAM.0 as u32,
        u: STGMEDIUM_0 {
            pstm: ManuallyDrop::new(Some(stream)),
        },
        pUnkForRelease: ManuallyDrop::new(None),
    })
}

fn drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        fWide: BOOL(1),
        ..Default::default()
    };
    let mut bytes = struct_bytes(&header).to_vec();
    for path in paths {
        for unit in path.as_os_str().encode_wide().chain([0]) {
            bytes.extend_from_slice(&unit.to_ne_bytes());
        }
    }
    bytes.extend_from_slice(&0u16.to_ne_bytes());
    bytes
}

fn pump_messages() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AfterChange {
    Serve,
    Refuse,
}

/// The gate index that holds back the file list instead of a file.
const FILE_LIST: i32 = -1;

/// Holds back the request for file `index`, or the file list, until opened,
/// then serves or refuses it.
struct Gate {
    index: i32,
    after_change: AfterChange,
    arrived: Mutex<Sender<()>>,
    opened: Mutex<Receiver<()>>,
}

fn change_clipboard_while_serving(
    index: i32,
    after_change: AfterChange,
) -> (Gate, thread::JoinHandle<Option<()>>) {
    change_while_serving(index, after_change, change_clipboard)
}

/// A gate on `index` and a thread that runs `change` while that request is
/// held back, then lets it go on. The thread returns what `change` returned.
fn change_while_serving<T: Send + 'static>(
    index: i32,
    after_change: AfterChange,
    change: impl FnOnce() -> T + Send + 'static,
) -> (Gate, thread::JoinHandle<Option<T>>) {
    let (arrive, arrived) = mpsc::channel();
    let (open, opened) = mpsc::channel();
    let changer = thread::spawn(move || {
        let changed = arrived
            .recv_timeout(Duration::from_secs(10))
            .ok()
            .map(|()| change());
        let _ = open.send(());
        changed
    });
    let gate = Gate {
        index,
        after_change,
        arrived: Mutex::new(arrive),
        opened: Mutex::new(opened),
    };
    (gate, changer)
}

#[implement(IDataObject)]
struct FakeClipboard {
    entries: Vec<Offered>,
    drop_list: Option<Vec<PathBuf>>,
    gate: Option<Gate>,
    /// Errors given, last first, to file contents requests before serving them.
    busy_answers: Arc<Mutex<Vec<HRESULT>>>,
    contents_requests: Arc<AtomicUsize>,
}

impl FakeClipboard {
    fn new(entries: Vec<Offered>) -> Self {
        Self {
            entries,
            drop_list: None,
            gate: None,
            busy_answers: Arc::default(),
            contents_requests: Arc::default(),
        }
    }

    fn formats(&self) -> Vec<FORMATETC> {
        let mut formats = vec![
            format_etc(registered(CFSTR_FILEDESCRIPTORW), TYMED_HGLOBAL.0),
            format_etc(
                registered(CFSTR_FILECONTENTS),
                TYMED_ISTREAM.0 | TYMED_HGLOBAL.0,
            ),
        ];
        if self.drop_list.is_some() {
            formats.insert(0, format_etc(CF_HDROP.0, TYMED_HGLOBAL.0));
        }
        formats
    }

    fn descriptor(&self) -> Vec<u8> {
        let mut bytes = (self.entries.len() as u32).to_ne_bytes().to_vec();
        for offered in &self.entries {
            let mut descriptor = FILEDESCRIPTORW::default();
            let mut name = [0u16; 260];
            for (slot, unit) in name.iter_mut().zip(offered.name.encode_utf16()) {
                *slot = unit;
            }
            descriptor.cFileName = name;
            match &offered.kind {
                Kind::Directory => {
                    descriptor.dwFlags = FD_ATTRIBUTES.0 as u32;
                    descriptor.dwFileAttributes = FILE_ATTRIBUTE_DIRECTORY.0;
                }
                Kind::File { declared_size, .. } => {
                    descriptor.dwFileAttributes = FILE_ATTRIBUTE_NORMAL.0;
                    if let Some(size) = *declared_size {
                        descriptor.dwFlags = (FD_ATTRIBUTES.0 | FD_FILESIZE.0) as u32;
                        descriptor.nFileSizeHigh = (size >> 32) as u32;
                        descriptor.nFileSizeLow = size as u32;
                    } else {
                        descriptor.dwFlags = FD_ATTRIBUTES.0 as u32;
                    }
                }
            }
            bytes.extend_from_slice(struct_bytes(&descriptor));
        }
        bytes
    }

    fn pass_gate(&self, lindex: i32) -> windows::core::Result<()> {
        let Some(gate) = self.gate.as_ref().filter(|gate| gate.index == lindex) else {
            return Ok(());
        };
        let _ = gate.arrived.lock().unwrap().send(());
        let opened = gate.opened.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match opened.recv_timeout(Duration::from_millis(5)) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => pump_messages(),
            }
        }
        match gate.after_change {
            AfterChange::Serve => Ok(()),
            AfterChange::Refuse => Err(E_FAIL.into()),
        }
    }
}

impl IDataObject_Impl for FakeClipboard_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let format = unsafe { &*format };
        let accepts = |tymed: TYMED| format.tymed & tymed.0 as u32 != 0;

        if format.cfFormat == registered(CFSTR_FILEDESCRIPTORW) && accepts(TYMED_HGLOBAL) {
            self.pass_gate(FILE_LIST)?;
            return global_medium(&self.descriptor(), 0);
        }
        if format.cfFormat == registered(CFSTR_FILECONTENTS) {
            if format.lindex != FILE_LIST {
                self.pass_gate(format.lindex)?;
            }
            let offered = usize::try_from(format.lindex)
                .ok()
                .and_then(|index| self.entries.get(index));
            let Some(Kind::File {
                bytes, delivery, ..
            }) = offered.map(|offered| &offered.kind)
            else {
                return Err(DV_E_LINDEX.into());
            };
            self.contents_requests.fetch_add(1, Ordering::SeqCst);
            if let Some(busy) = self.busy_answers.lock().unwrap().pop() {
                return Err(busy.into());
            }
            return match delivery {
                Delivery::Stream if accepts(TYMED_ISTREAM) => stream_medium(bytes),
                Delivery::Global if accepts(TYMED_HGLOBAL) => {
                    global_medium(bytes, GLOBAL_SLACK_BYTES)
                }
                _ => Err(DV_E_TYMED.into()),
            };
        }
        if let Some(paths) = self
            .drop_list
            .as_ref()
            .filter(|_| format.cfFormat == CF_HDROP.0)
        {
            if accepts(TYMED_HGLOBAL) {
                return global_medium(&drop_files(paths), 0);
            }
        }
        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(
        &self,
        _format: *const FORMATETC,
        _medium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        let format = unsafe { &*format };
        if self
            .formats()
            .iter()
            .any(|offered| offered.cfFormat == format.cfFormat)
        {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(&self, _format: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        if let Some(out) = unsafe { out.as_mut() } {
            out.ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _format: *const FORMATETC,
        _medium: *const STGMEDIUM,
        _release: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        unsafe { SHCreateStdEnumFmtEtc(&self.formats()) }
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _flags: u32,
        _sink: Ref<'_, IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _connection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

/// Puts a data object on the clipboard from its own STA thread and keeps it
/// live, without flushing, until dropped.
struct ClipboardOwner {
    thread_id: u32,
    thread: Option<thread::JoinHandle<()>>,
}

impl ClipboardOwner {
    fn offer(fake: FakeClipboard) -> Self {
        let (started, ready) = mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            OleInitialize(None).expect("OLE starts on the owner thread");
            let object: IDataObject = fake.into();
            with_retries("the fake data object goes on the clipboard", || {
                OleSetClipboard(&object)
            });
            started.send(GetCurrentThreadId()).unwrap();

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            if OleIsCurrentClipboard(&object).is_ok() {
                let _ = OleSetClipboard(None::<&IDataObject>);
            }
            drop(object);
            OleUninitialize();
        });
        let thread_id = ready.recv().expect("the clipboard owner thread starts");
        Self {
            thread_id,
            thread: Some(thread),
        }
    }
}

impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn virtual_listing() -> VirtualListing {
    match read_clipboard_files() {
        ClipboardFiles::Virtual(listing) => listing,
        other => panic!("expected virtual files on the clipboard, got {other:?}"),
    }
}

fn read_whole(listing: &VirtualListing, relative: &Path) -> AppResult<Vec<u8>> {
    read_virtual_file(listing, relative, u64::MAX)
}

fn files_under(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                found.insert(relative, fs::read(&path).unwrap());
            }
        }
    }
    found
}

fn with_retries<T, E: std::fmt::Display>(
    what: &str,
    mut attempt: impl FnMut() -> Result<T, E>,
) -> T {
    let mut failures = 0;
    loop {
        match attempt() {
            Ok(value) => return value,
            Err(error) => {
                failures += 1;
                assert!(failures < 50, "{what}: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn change_clipboard() {
    with_retries("clipboard text is set", || {
        clipboard_win::set_clipboard_string("something else")
    });
}

#[test]
#[ignore = "touches the real system clipboard"]
fn virtual_files_are_listed_read_and_extracted_byte_for_byte() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(project_on_clipboard()));
    let big = big_file();

    let listing = virtual_listing();

    assert_eq!(listing.sequence, unsafe { GetClipboardSequenceNumber() });
    assert_eq!(listing.rejected, 1);
    assert_eq!(
        listing.entries,
        vec![
            VirtualEntry {
                index: 0,
                relative: path(&["web"]),
                is_directory: true,
                size: None,
            },
            VirtualEntry {
                index: 2,
                relative: path(&["web", "package.json"]),
                is_directory: false,
                size: Some(PACKAGE_JSON.len() as u64),
            },
            VirtualEntry {
                index: 3,
                relative: path(&["web", "src", "index.ts"]),
                is_directory: false,
                size: Some(INDEX_TS.len() as u64),
            },
            VirtualEntry {
                index: 4,
                relative: path(&["web", "src", "empty"]),
                is_directory: true,
                size: None,
            },
            VirtualEntry {
                index: 5,
                relative: path(&["web", "assets", "big.bin"]),
                is_directory: false,
                size: Some(BIG_FILE_BYTES as u64),
            },
            VirtualEntry {
                index: 6,
                relative: path(&["notes.txt"]),
                is_directory: false,
                size: Some(NOTES.len() as u64),
            },
        ]
    );
    let web_bytes = (PACKAGE_JSON.len() + INDEX_TS.len() + BIG_FILE_BYTES) as u64;
    assert_eq!(
        top_level_folders(&listing),
        vec![VirtualFolder {
            name: "web".to_string(),
            files: 3,
            bytes: web_bytes,
            has_manifest: true,
        }]
    );

    assert_eq!(
        read_whole(&listing, Path::new("web/package.json")).unwrap(),
        PACKAGE_JSON
    );
    assert_eq!(
        read_whole(&listing, &path(&["web", "src", "index.ts"])).unwrap(),
        INDEX_TS
    );
    assert_eq!(
        read_whole(&listing, &path(&["web", "assets", "big.bin"])).unwrap(),
        big
    );
    let error = read_whole(&listing, &path(&["web", "src", "empty"])).unwrap_err();
    assert!(error.message().contains("is not a file"), "{error}");

    let before_render = unsafe { GetClipboardSequenceNumber() };
    let descriptor_format = u32::from(registered(CFSTR_FILEDESCRIPTORW));
    let rendered: Vec<u8> = with_retries("the descriptor renders through Win32", || {
        clipboard_win::get_clipboard(clipboard_win::formats::RawData(descriptor_format))
    });
    assert!(!rendered.is_empty());
    assert_eq!(
        unsafe { GetClipboardSequenceNumber() },
        before_render,
        "rendering a delayed format must not look like a clipboard change"
    );

    let root = TempDir::new("virtual-extract");
    let destination = root.join("received");
    let mut reports: Vec<ExtractProgress> = Vec::new();

    let folders = extract_virtual(&listing, &destination, &mut |report| {
        reports.push(report);
        ControlFlow::Continue(())
    })
    .expect("extraction succeeds");

    assert_eq!(folders, vec![destination.join("web")]);
    let expected: BTreeMap<PathBuf, Vec<u8>> = [
        (path(&["web", "package.json"]), PACKAGE_JSON.to_vec()),
        (path(&["web", "src", "index.ts"]), INDEX_TS.to_vec()),
        (path(&["web", "assets", "big.bin"]), big.clone()),
        (path(&["notes.txt"]), NOTES.to_vec()),
    ]
    .into_iter()
    .collect();
    assert!(
        files_under(&destination) == expected,
        "extracted tree differs"
    );
    assert!(destination.join("web").join("src").join("empty").is_dir());
    assert!(!root.join("escape.txt").exists());

    let total = web_bytes + NOTES.len() as u64;
    assert_eq!(
        reports.last(),
        Some(&ExtractProgress {
            files_done: 4,
            files_total: 4,
            bytes_done: total,
            bytes_total: Some(total),
        })
    );
    assert!(reports.windows(2).all(|pair| {
        pair[0].files_done <= pair[1].files_done && pair[0].bytes_done <= pair[1].bytes_done
    }));
    let before_big = (PACKAGE_JSON.len() + INDEX_TS.len()) as u64;
    assert!(
        reports.iter().any(|report| report.files_done == 2
            && report.bytes_done > before_big
            && report.bytes_done < before_big + BIG_FILE_BYTES as u64),
        "no progress was reported while the big file streamed: {reports:?}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_file_without_a_declared_size_is_read_to_the_end_of_its_stream() {
    let contents = noise(200_003, 0xfeed);
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![unsized_file(
        "lib\\data.bin",
        &contents,
    )]));

    let listing = virtual_listing();
    assert_eq!(listing.entries[0].size, None);
    assert_eq!(
        read_whole(&listing, &path(&["lib", "data.bin"])).unwrap(),
        contents
    );

    let root = TempDir::new("virtual-unsized");
    let destination = root.join("received");
    let mut last = None;
    extract_virtual(&listing, &destination, &mut |report| {
        last = Some(report);
        ControlFlow::Continue(())
    })
    .unwrap();

    assert_eq!(
        fs::read(destination.join("lib").join("data.bin")).unwrap(),
        contents
    );
    assert_eq!(
        last,
        Some(ExtractProgress {
            files_done: 1,
            files_total: 1,
            bytes_done: contents.len() as u64,
            bytes_total: None,
        })
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_clipboard_change_after_listing_stops_reading_and_extraction() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(project_on_clipboard()));
    let listing = virtual_listing();

    let replacement = FakeClipboard::new(project_on_clipboard());
    let replacement_asked = Arc::clone(&replacement.contents_requests);
    let _replacement = ClipboardOwner::offer(replacement);

    let error = read_whole(&listing, &path(&["web", "package.json"])).unwrap_err();
    assert!(error.message().contains("clipboard changed"), "{error}");

    let root = TempDir::new("virtual-changed");
    let destination = root.join("received");
    let mut reports = Vec::new();
    let error = extract_virtual(&listing, &destination, &mut |report| {
        reports.push(report);
        ControlFlow::Continue(())
    })
    .unwrap_err();

    assert!(error.message().contains("clipboard changed"), "{error}");
    assert!(!destination.exists());
    assert!(
        reports
            .iter()
            .all(|report| report.files_done == 0 && report.bytes_done == 0),
        "{reports:?}"
    );
    assert_eq!(
        replacement_asked.load(Ordering::SeqCst),
        0,
        "the new clipboard was asked for files"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_clipboard_change_while_one_file_is_read_is_refused() {
    let (gate, changer) = change_clipboard_while_serving(2, AfterChange::Serve);
    let _owner = ClipboardOwner::offer(FakeClipboard {
        gate: Some(gate),
        ..FakeClipboard::new(project_on_clipboard())
    });
    let listing = virtual_listing();

    let error = read_whole(&listing, &path(&["web", "package.json"])).unwrap_err();
    changer.join().unwrap();

    assert!(error.message().contains("clipboard changed"), "{error}");
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_file_refused_after_a_clipboard_change_reports_the_change_and_removes_the_partial_folder() {
    let (gate, changer) = change_clipboard_while_serving(5, AfterChange::Refuse);
    let _owner = ClipboardOwner::offer(FakeClipboard {
        gate: Some(gate),
        ..FakeClipboard::new(project_on_clipboard())
    });
    let listing = virtual_listing();

    let root = TempDir::new("virtual-changed-midway");
    let destination = root.join("received");
    let mut reports = Vec::new();
    let error = extract_virtual(&listing, &destination, &mut |report| {
        reports.push(report);
        ControlFlow::Continue(())
    })
    .unwrap_err();
    changer.join().unwrap();

    assert!(error.message().contains("clipboard changed"), "{error}");
    assert!(!destination.exists());
    assert!(
        reports.iter().any(|report| report.files_done == 2),
        "extraction stopped before the gated file: {reports:?}"
    );
    assert!(
        reports.iter().all(|report| report.files_done <= 2),
        "{reports:?}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_file_that_arrives_shorter_than_declared_fails_extraction() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![
        file("web\\package.json", PACKAGE_JSON, Delivery::Global),
        file_declaring("web\\cut.txt", b"short", Delivery::Stream, Some(50)),
    ]));
    let listing = virtual_listing();

    let error = read_whole(&listing, &path(&["web", "cut.txt"])).unwrap_err();
    assert!(error.message().contains("5 of 50 bytes"), "{error}");

    let root = TempDir::new("virtual-short");
    let destination = root.join("received");
    let error =
        extract_virtual(&listing, &destination, &mut |_| ControlFlow::Continue(())).unwrap_err();

    assert!(error.message().contains("5 of 50 bytes"), "{error}");
    assert!(!destination.exists());
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_memory_block_shorter_than_declared_fails_the_read() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![file_declaring(
        "web\\cut.txt",
        b"short",
        Delivery::Global,
        Some(5000),
    )]));
    let listing = virtual_listing();

    let error = read_whole(&listing, &path(&["web", "cut.txt"])).unwrap_err();

    assert!(error.message().contains("of 5000 bytes"), "{error}");
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_file_drop_list_wins_over_virtual_files() {
    let root = TempDir::new("virtual-with-drop");
    let folder = root.mkdir("web");
    let fake = FakeClipboard {
        drop_list: Some(vec![folder.clone()]),
        ..FakeClipboard::new(project_on_clipboard())
    };
    let _owner = ClipboardOwner::offer(fake);

    assert_eq!(read_clipboard_files(), ClipboardFiles::Paths(vec![folder]));
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_clipboard_another_program_holds_open_is_busy_until_it_is_released() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(project_on_clipboard()));
    let (holding, held) = mpsc::channel();
    let holder = thread::spawn(move || {
        with_retries("another program opens the clipboard", || unsafe {
            OpenClipboard(None)
        });
        holding.send(()).unwrap();
        thread::sleep(Duration::from_millis(1000));
        unsafe { CloseClipboard() }.unwrap();
    });
    held.recv().unwrap();

    let while_held = read_clipboard_files();
    holder.join().unwrap();

    assert_eq!(while_held, ClipboardFiles::Busy);
    assert_eq!(virtual_listing().entries.len(), 6);
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_file_is_asked_for_again_while_the_clipboard_is_busy() {
    let busy_answers = Arc::new(Mutex::new(vec![DV_E_FORMATETC, CLIPBRD_E_CANT_OPEN]));
    let _owner = ClipboardOwner::offer(FakeClipboard {
        busy_answers: Arc::clone(&busy_answers),
        ..FakeClipboard::new(project_on_clipboard())
    });
    let listing = virtual_listing();

    let bytes = read_whole(&listing, &path(&["web", "package.json"])).unwrap();

    assert_eq!(bytes, PACKAGE_JSON);
    assert!(busy_answers.lock().unwrap().is_empty());
}

#[test]
#[ignore = "touches the real system clipboard"]
fn an_empty_file_drop_list_reads_as_no_files() {
    let _owner = ClipboardOwner::offer(FakeClipboard {
        drop_list: Some(Vec::new()),
        ..FakeClipboard::new(project_on_clipboard())
    });

    assert_eq!(read_clipboard_files(), ClipboardFiles::Empty);
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_stream_longer_than_declared_fails_the_read() {
    let declared = 64 * 1024;
    let contents = noise(declared + 10, 0x10ad);
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![file_declaring(
        "web/long.bin",
        &contents,
        Delivery::Stream,
        Some(declared as u64),
    )]));
    let listing = virtual_listing();

    let error = read_whole(&listing, &path(&["web", "long.bin"])).unwrap_err();

    assert!(
        error
            .message()
            .contains(&format!("{} of {declared} bytes", contents.len())),
        "{error}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_clipboard_change_while_the_file_list_is_read_leaves_the_listing_stale() {
    let mut other_files = project_on_clipboard();
    other_files[2] = file(
        "web\\package.json",
        b"{ \"name\": \"other\" }\n",
        Delivery::Global,
    );
    let (gate, changer) = change_while_serving(FILE_LIST, AfterChange::Serve, move || {
        ClipboardOwner::offer(FakeClipboard::new(other_files))
    });
    let _owner = ClipboardOwner::offer(FakeClipboard {
        gate: Some(gate),
        ..FakeClipboard::new(project_on_clipboard())
    });

    let listing = virtual_listing();
    let _other = changer
        .join()
        .unwrap()
        .expect("the clipboard changed while its file list was read");

    let read = read_whole(&listing, &path(&["web", "package.json"]));
    assert!(
        read.as_ref()
            .is_err_and(|error| error.message().contains("clipboard changed")),
        "{read:?}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_memory_block_without_a_declared_size_is_refused() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![
        file("web\\package.json", PACKAGE_JSON, Delivery::Stream),
        file_declaring("web\\data.bin", b"no end", Delivery::Global, None),
    ]));
    let listing = virtual_listing();

    let error = read_whole(&listing, &path(&["web", "data.bin"])).unwrap_err();
    assert!(error.message().contains("without a size"), "{error}");

    let root = TempDir::new("virtual-unsized-global");
    let destination = root.join("received");
    let error =
        extract_virtual(&listing, &destination, &mut |_| ControlFlow::Continue(())).unwrap_err();

    assert!(error.message().contains("without a size"), "{error}");
    assert!(!destination.exists());
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_stream_without_a_size_is_cut_off_at_the_read_limit() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![unsized_file(
        "lib\\data.bin",
        &noise(200_003, 0xfeed),
    )]));
    let listing = virtual_listing();

    let error = read_virtual_file(&listing, &path(&["lib", "data.bin"]), 1000).unwrap_err();

    assert!(
        error.message().contains("larger than 1000 bytes"),
        "{error}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn breaking_the_progress_stops_the_extraction_and_leaves_no_folder() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(project_on_clipboard()));
    let listing = virtual_listing();
    let root = TempDir::new("virtual-cancelled");
    let destination = root.join("received");
    let mut reports = Vec::new();

    let error = extract_virtual(&listing, &destination, &mut |report| {
        reports.push(report);
        if report.files_done == 2 && report.bytes_done > 0 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .unwrap_err();
    thread::sleep(Duration::from_millis(500));

    assert!(error.message().contains("cancelled"), "{error}");
    assert!(!destination.exists());
    assert!(
        reports.iter().all(|report| report.files_done <= 2),
        "{reports:?}"
    );
}

#[test]
#[ignore = "touches the real system clipboard"]
fn a_name_offered_twice_in_another_case_keeps_the_first_file() {
    let _owner = ClipboardOwner::offer(FakeClipboard::new(vec![
        file("web\\a.txt", b"first\n", Delivery::Stream),
        file("WEB\\A.TXT", b"second\n", Delivery::Stream),
    ]));
    let listing = virtual_listing();
    let root = TempDir::new("virtual-duplicate");
    let destination = root.join("received");

    extract_virtual(&listing, &destination, &mut |_| ControlFlow::Continue(())).unwrap();

    assert_eq!(listing.rejected, 1);
    assert_eq!(
        files_under(&destination),
        [(path(&["web", "a.txt"]), b"first\n".to_vec())]
            .into_iter()
            .collect()
    );
}
