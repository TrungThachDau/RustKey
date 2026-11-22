//! Clipboard typing: characters are pasted instead of sent as Unicode keys.
//!
//! Remote windows such as Linux applications shown by WSLg (`msrdc.exe`)
//! forward keys as scan codes, so the Unicode key events used for Vietnamese
//! letters never arrive. The clipboard is synchronised with the remote side,
//! so pasting works there.
//!
//! A worker thread owns the clipboard and runs [`Job`]s in order. Pasting is
//! asynchronous (the application, possibly in a Linux VM, fetches the text
//! after it handles the paste key), so the worker waits for the text to be
//! fetched before going on. While jobs are pending, the keyboard hook queues
//! every key as a job too, keeping keys and pasted text in typing order.
//!
//! The text is offered with delayed rendering: `WM_RENDERFORMAT` tells when
//! the application fetched it. The previous clipboard content is restored
//! once typing pauses.

use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use rustkey_engine::OutputOp;
use windows_sys::Win32::Foundation::{GlobalFree, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
    GetOpenClipboardWindow, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, VK_INSERT, VK_LCONTROL, VK_LSHIFT, VK_RCONTROL, VK_RSHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowThreadProcessId,
    HWND_MESSAGE, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
    PostMessageW, QS_ALLINPUT, RegisterClassExW, WM_APP, WM_RENDERALLFORMATS, WM_RENDERFORMAT,
    WNDCLASSEXW,
};

use crate::input;
use crate::settings::PasteKey;
use crate::win::{self, wide};

const WINDOW_CLASS: &str = "RustKeyClipboardWindow";
/// Wakes the worker up when a command was queued.
const WM_WAKE: u32 = WM_APP + 1;

const CF_UNICODETEXT: u32 = 13;
const INFINITE: u32 = u32::MAX;
/// Text formats Windows synthesises from `CF_UNICODETEXT`.
const SYNTHESIZED_TEXT: [u32; 3] = [1, 7, 16]; // CF_TEXT, CF_OEMTEXT, CF_LOCALE
/// Formats whose data is a GDI handle rather than global memory.
const GDI_FORMATS: [u32; 8] = [2, 3, 9, 14, 128, 130, 131, 142]; // BITMAP .. DSPENHMETAFILE
/// `CF_PRIVATEFIRST..=CF_GDIOBJLAST`: handles that cannot be copied.
const HANDLE_FORMATS: std::ops::RangeInclusive<u32> = 0x0200..=0x03FF;
const V_KEY: u16 = 0x56;

/// The previous clipboard content is restored after this much idle time.
const RESTORE_DELAY: Duration = Duration::from_millis(600);
/// The hook stops queueing keys when the worker made no progress for so long.
const STALL_TIMEOUT: Duration = Duration::from_millis(1500);
const OPEN_ATTEMPTS: u32 = 20;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(5);

/// Waits around one paste.
struct Timing {
    /// Lets the remote client announce the new clipboard before the paste key.
    before_paste: Duration,
    /// Longest wait for the application to fetch the text.
    fetch_timeout: Duration,
    /// Used instead when something fetched the text before the paste key.
    fetched_early: Duration,
    /// Lets the application insert the text before the next keys.
    settle: Duration,
}

const LOCAL_TIMING: Timing = Timing {
    before_paste: Duration::ZERO,
    fetch_timeout: Duration::from_millis(150),
    fetched_early: Duration::from_millis(30),
    settle: Duration::from_millis(10),
};

const REMOTE_TIMING: Timing = Timing {
    before_paste: Duration::from_millis(30),
    fetch_timeout: Duration::from_millis(600),
    fetched_early: Duration::from_millis(120),
    settle: Duration::from_millis(40),
};

/// How text is pasted into the foreground application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paste {
    pub key: PasteKey,
    /// A remote window (WSLg): the clipboard is synchronised asynchronously.
    pub remote: bool,
    /// Process id of the foreground window; 0 lets any process fetch the text.
    pub process: u32,
}

/// Output of one key event, run in order by the worker.
pub struct Job {
    pub ops: Vec<OutputOp>,
    /// Paste the characters of `ops` instead of typing them.
    pub paste: Option<Paste>,
    /// The original key event, re-sent after `ops` when it was not consumed.
    pub replay: Option<INPUT>,
}

enum Command {
    Run(Job),
    Exit,
}

struct Shared {
    pending: AtomicUsize,
    /// Milliseconds since `epoch` when the worker last made progress.
    progress: AtomicU64,
    epoch: Instant,
}

impl Shared {
    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn touch(&self) {
        self.progress.store(self.now(), Ordering::Release);
    }
}

/// Handle to the clipboard worker thread.
pub struct Typist {
    hwnd: usize,
    commands: Sender<Command>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Typist {
    pub fn spawn() -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let shared = Arc::new(Shared {
            pending: AtomicUsize::new(0),
            progress: AtomicU64::new(0),
            epoch: Instant::now(),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("rustkey-clipboard".into())
            .spawn(move || match create_window() {
                Ok(hwnd) => {
                    // A send error only means `spawn` gave up waiting.
                    let _ = ready_sender.send(Ok(hwnd as usize));
                    Worker::new(hwnd, receiver, worker_shared).run();
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(error));
                }
            })?;
        let hwnd = ready.recv()??;
        Ok(Self {
            hwnd,
            commands,
            shared,
            thread: Some(thread),
        })
    }

    /// Jobs are queued or running.
    pub fn is_busy(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire) > 0
    }

    /// The worker is stuck: keys must not wait for it any longer.
    pub fn is_stalled(&self) -> bool {
        let idle = self
            .shared
            .now()
            .saturating_sub(self.shared.progress.load(Ordering::Acquire));
        self.is_busy() && Duration::from_millis(idle) > STALL_TIMEOUT
    }

    /// Queues `job`, or gives it back if the worker is gone.
    pub fn submit(&self, job: Job) -> Result<(), Job> {
        if self.shared.pending.fetch_add(1, Ordering::AcqRel) == 0 {
            self.shared.touch();
        }
        if let Err(mpsc::SendError(command)) = self.commands.send(Command::Run(job)) {
            self.shared.pending.fetch_sub(1, Ordering::AcqRel);
            return match command {
                Command::Run(job) => Err(job),
                Command::Exit => Ok(()),
            };
        }
        self.wake();
        Ok(())
    }

    fn wake(&self) {
        // SAFETY: posting to the worker window; harmless if it is gone.
        unsafe { PostMessageW(self.hwnd as HWND, WM_WAKE, 0, 0) };
    }
}

impl Drop for Typist {
    fn drop(&mut self) {
        // The worker restores the clipboard before it exits.
        if self.commands.send(Command::Exit).is_ok() {
            self.wake();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

thread_local! {
    /// Text offered on the clipboard, rendered on request.
    static OFFERED: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    /// The offered text was fetched by the target application.
    static FETCHED: Cell<bool> = const { Cell::new(false) };
    /// Process of the application the offered text is pasted into.
    static TARGET_PROCESS: Cell<u32> = const { Cell::new(0) };
}

struct Worker {
    hwnd: HWND,
    commands: Receiver<Command>,
    shared: Arc<Shared>,
    /// Clipboard content to restore, taken before the first paste.
    saved: Option<Vec<(u32, Vec<u8>)>>,
    restore_at: Option<Instant>,
    history_format: u32,
    cloud_format: u32,
}

impl Worker {
    fn new(hwnd: HWND, commands: Receiver<Command>, shared: Arc<Shared>) -> Self {
        Self {
            hwnd,
            commands,
            shared,
            saved: None,
            restore_at: None,
            history_format: register_format("CanIncludeInClipboardHistory"),
            cloud_format: register_format("CanUploadToCloudClipboard"),
        }
    }

    fn run(mut self) {
        loop {
            match self.commands.try_recv() {
                Ok(Command::Run(job)) => {
                    self.shared.touch();
                    self.run_job(job);
                    self.shared.touch();
                    self.shared.pending.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                Ok(Command::Exit) | Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {}
            }
            let timeout = match self.restore_at {
                Some(at) => match at.checked_duration_since(Instant::now()) {
                    Some(left) if !left.is_zero() => left,
                    _ => {
                        self.restore();
                        continue;
                    }
                },
                None => Duration::MAX,
            };
            wait_for_messages(timeout);
            pump_messages();
        }
        self.restore();
        // SAFETY: destroying the window created by this thread.
        unsafe { DestroyWindow(self.hwnd) };
    }

    fn run_job(&mut self, job: Job) {
        let is_char = |op: &OutputOp| matches!(op, OutputOp::Char(_));
        for run in job.ops.chunk_by(|a, b| is_char(a) == is_char(b)) {
            let pasted = match job.paste {
                Some(paste) if run.first().is_some_and(is_char) => self.paste(run, paste),
                _ => false,
            };
            if !pasted {
                input::send(run);
            }
        }
        if let Some(replay) = job.replay {
            input::send_inputs(&[replay]);
        }
    }

    /// Pastes the characters of `chars`. Returns `false` if the clipboard
    /// could not be used.
    fn paste(&mut self, chars: &[OutputOp], paste: Paste) -> bool {
        let mut text: Vec<u16> = Vec::with_capacity(chars.len() + 1);
        let mut buffer = [0u16; 2];
        for op in chars {
            if let OutputOp::Char(ch) = *op {
                text.extend_from_slice(ch.encode_utf16(&mut buffer));
            }
        }
        if !self.offer(text, paste.process) {
            return false;
        }
        let timing = if paste.remote {
            &REMOTE_TIMING
        } else {
            &LOCAL_TIMING
        };
        pump_for(timing.before_paste);
        let fetched_early = FETCHED.get();
        send_paste_key(paste.key);
        let mut fetched = fetched_early || pump_until(|| FETCHED.get(), timing.fetch_timeout);
        if !fetched {
            // The paste failed without reading our text, typically because a
            // clipboard monitor held the clipboard open: try once more.
            send_paste_key(paste.key);
            fetched = pump_until(|| FETCHED.get(), timing.fetch_timeout);
        }
        if fetched_early || !fetched {
            pump_for(timing.fetched_early);
        }
        pump_for(timing.settle);
        self.restore_at = Some(Instant::now() + RESTORE_DELAY);
        true
    }

    /// Puts `text` on the clipboard (delay rendered) for `process`, saving
    /// the previous content first.
    fn offer(&mut self, mut text: Vec<u16>, process: u32) -> bool {
        let Some(_clipboard) = self.open() else {
            return false;
        };
        if self.saved.is_none() {
            self.saved = Some(read_clipboard());
        }
        text.push(0);
        OFFERED.set(text);
        FETCHED.set(false);
        TARGET_PROCESS.set(process);
        // SAFETY: the clipboard is open; a null handle requests delayed rendering.
        unsafe {
            EmptyClipboard();
            SetClipboardData(CF_UNICODETEXT, null_mut());
        }
        self.keep_out_of_history();
        true
    }

    /// Puts back the clipboard content saved before the first paste, unless
    /// something else was copied since.
    fn restore(&mut self) {
        self.restore_at = None;
        let Some(saved) = self.saved.take() else {
            return;
        };
        // SAFETY: GetClipboardOwner has no memory safety requirements.
        if unsafe { GetClipboardOwner() } != self.hwnd {
            return;
        }
        let Some(_clipboard) = self.open() else {
            return;
        };
        // SAFETY: the clipboard is open.
        unsafe { EmptyClipboard() };
        for (format, data) in &saved {
            set_clipboard_bytes(*format, data);
        }
        // The content is already in the history; do not add it again.
        self.keep_out_of_history();
        OFFERED.with_borrow_mut(Vec::clear);
    }

    /// Marks the content being set so that Windows keeps it out of the
    /// clipboard history and cloud clipboard.
    fn keep_out_of_history(&self) {
        let zero = 0u32.to_ne_bytes();
        for format in [self.history_format, self.cloud_format] {
            if format != 0 {
                set_clipboard_bytes(format, &zero);
            }
        }
    }

    fn open(&self) -> Option<ClipboardGuard> {
        for attempt in 0..OPEN_ATTEMPTS {
            if attempt > 0 {
                pump_for(OPEN_RETRY_DELAY);
            }
            // SAFETY: `self.hwnd` is a window of this thread.
            if unsafe { OpenClipboard(self.hwnd) } != 0 {
                return Some(ClipboardGuard);
            }
        }
        win::log("cannot open the clipboard");
        None
    }
}

/// Closes the clipboard when dropped.
struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is only created after OpenClipboard succeeded.
        unsafe { CloseClipboard() };
    }
}

fn register_format(name: &str) -> u32 {
    let name = wide(name);
    // SAFETY: `name` is a valid null terminated string.
    unsafe { RegisterClipboardFormatW(name.as_ptr()) }
}

/// Whether the data of `format` can be saved and restored as plain bytes.
fn can_save_format(format: u32, has_unicode_text: bool) -> bool {
    !GDI_FORMATS.contains(&format)
        && !HANDLE_FORMATS.contains(&format)
        && !(has_unicode_text && SYNTHESIZED_TEXT.contains(&format))
}

/// Copies every format of the open clipboard that is stored in global memory.
fn read_clipboard() -> Vec<(u32, Vec<u8>)> {
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        // SAFETY: the clipboard is open.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        formats.push(format);
    }
    let has_unicode_text = formats.contains(&CF_UNICODETEXT);
    formats
        .into_iter()
        .filter(|&format| can_save_format(format, has_unicode_text))
        .filter_map(|format| Some((format, read_clipboard_bytes(format)?)))
        .collect()
}

fn read_clipboard_bytes(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open; the memory is only read while locked and
    // GlobalSize bounds the copy.
    unsafe {
        let handle = GetClipboardData(format);
        if handle.is_null() {
            return None;
        }
        let size = GlobalSize(handle);
        let data = GlobalLock(handle);
        if data.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec();
        GlobalUnlock(handle);
        Some(bytes)
    }
}

/// Sets `format` of the open clipboard to a copy of `data`.
fn set_clipboard_bytes(format: u32, data: &[u8]) {
    // SAFETY: the allocation holds at least `data.len()` bytes; ownership of
    // the memory passes to the system only when SetClipboardData succeeds.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1));
        if memory.is_null() {
            return;
        }
        let target = GlobalLock(memory);
        if target.is_null() {
            GlobalFree(memory);
            return;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), target.cast::<u8>(), data.len());
        GlobalUnlock(memory);
        if SetClipboardData(format, memory).is_null() {
            GlobalFree(memory);
        }
    }
}

/// Renders the offered text into the (already open) clipboard.
fn render_offered_text() {
    FETCHED.set(true);
    OFFERED.with_borrow(|text| {
        let bytes: Vec<u8> = text.iter().flat_map(|unit| unit.to_ne_bytes()).collect();
        set_clipboard_bytes(CF_UNICODETEXT, &bytes);
    });
}

/// Whether the clipboard was opened by the application the text is typed
/// into. Clipboard monitors (history, sync tools) read every change right
/// away; they are refused so that the format stays delay rendered and the
/// render request of the actual paste can be recognised.
fn opened_by_target() -> bool {
    // SAFETY: GetOpenClipboardWindow has no memory safety requirements.
    let opener = unsafe { GetOpenClipboardWindow() };
    let target = TARGET_PROCESS.get();
    if opener.is_null() || target == 0 {
        // The reader cannot be told apart.
        return true;
    }
    let mut process = 0u32;
    // SAFETY: `process` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(opener, &mut process) };
    process == target
}

/// Modifier keys currently down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Modifiers {
    left_shift: bool,
    right_shift: bool,
    left_control: bool,
    right_control: bool,
}

impl Modifiers {
    fn current() -> Self {
        // SAFETY: GetAsyncKeyState has no memory safety requirements.
        let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
        Self {
            left_shift: down(VK_LSHIFT),
            right_shift: down(VK_RSHIFT),
            left_control: down(VK_LCONTROL),
            right_control: down(VK_RCONTROL),
        }
    }
}

/// Key presses (`false`) and releases (`true`) that trigger `key`, releasing
/// modifiers held by the user that would change the shortcut and pressing
/// them again afterwards.
fn paste_sequence(key: PasteKey, held: Modifiers) -> Vec<(u16, bool)> {
    let (control, shift, main) = match key {
        PasteKey::CtrlV => (true, false, V_KEY),
        PasteKey::CtrlShiftV => (true, true, V_KEY),
        PasteKey::ShiftInsert => (false, true, VK_INSERT),
    };
    let mut released = Vec::new();
    if !shift {
        released.extend([(VK_LSHIFT, held.left_shift), (VK_RSHIFT, held.right_shift)]);
    }
    if !control {
        released.extend([
            (VK_LCONTROL, held.left_control),
            (VK_RCONTROL, held.right_control),
        ]);
    }
    let released: Vec<u16> = released
        .into_iter()
        .filter_map(|(vk, down)| down.then_some(vk))
        .collect();
    let mut pressed = Vec::new();
    if control && !held.left_control && !held.right_control {
        pressed.push(VK_LCONTROL);
    }
    if shift && !held.left_shift && !held.right_shift {
        pressed.push(VK_LSHIFT);
    }

    let mut sequence: Vec<(u16, bool)> = released.iter().map(|&vk| (vk, true)).collect();
    sequence.extend(pressed.iter().map(|&vk| (vk, false)));
    sequence.extend([(main, false), (main, true)]);
    sequence.extend(pressed.iter().rev().map(|&vk| (vk, true)));
    sequence.extend(released.iter().map(|&vk| (vk, false)));
    sequence
}

fn send_paste_key(key: PasteKey) {
    let inputs: Vec<INPUT> = paste_sequence(key, Modifiers::current())
        .into_iter()
        .map(|(vk, up)| input::key_event(vk, up))
        .collect();
    input::send_inputs(&inputs);
}

/// Waits until a message arrives or `timeout` passes.
fn wait_for_messages(timeout: Duration) {
    let millis = if timeout == Duration::MAX {
        INFINITE
    } else {
        u32::try_from(timeout.as_millis()).unwrap_or(INFINITE - 1)
    };
    // SAFETY: no handles are passed.
    unsafe { MsgWaitForMultipleObjectsEx(0, null(), millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
}

/// Dispatches the queued messages, including the clipboard render requests.
fn pump_messages() {
    let mut message = MSG::default();
    // SAFETY: standard message pumping with a valid MSG buffer.
    unsafe {
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            DispatchMessageW(&message);
        }
    }
}

/// Pumps messages until `done` returns `true` or `timeout` passes. Returns
/// whether `done` returned `true`.
fn pump_until(done: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        pump_messages();
        if done() {
            return true;
        }
        match deadline.checked_duration_since(Instant::now()) {
            Some(left) if !left.is_zero() => wait_for_messages(left),
            _ => return false,
        }
    }
}

fn pump_for(duration: Duration) {
    if !duration.is_zero() {
        pump_until(|| false, duration);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_RENDERFORMAT => {
            if u32::try_from(wparam) == Ok(CF_UNICODETEXT) && opened_by_target() {
                render_offered_text();
            }
            0
        }
        WM_RENDERALLFORMATS => {
            // SAFETY: the clipboard is opened by its owner, as required.
            unsafe {
                if OpenClipboard(hwnd) != 0 {
                    if GetClipboardOwner() == hwnd {
                        render_offered_text();
                    }
                    CloseClipboard();
                }
            }
            0
        }
        // SAFETY: default processing with the original arguments.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Creates the message-only window that owns the clipboard.
fn create_window() -> Result<HWND> {
    let class_name = wide(WINDOW_CLASS);
    // SAFETY: the class structure and strings outlive the calls.
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut class: WNDCLASSEXW = std::mem::zeroed();
        class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.lpszClassName = class_name.as_ptr();
        if RegisterClassExW(&class) == 0 {
            bail!("RegisterClassExW failed for the clipboard window");
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            bail!("cannot create the clipboard window");
        }
        Ok(hwnd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_v_releases_held_shift_around_the_shortcut() {
        let held = Modifiers {
            left_shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(PasteKey::CtrlV, held),
            vec![
                (VK_LSHIFT, true),
                (VK_LCONTROL, false),
                (V_KEY, false),
                (V_KEY, true),
                (VK_LCONTROL, true),
                (VK_LSHIFT, false),
            ]
        );
    }

    #[test]
    fn shift_insert_reuses_held_shift() {
        let held = Modifiers {
            right_shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            paste_sequence(PasteKey::ShiftInsert, held),
            vec![(VK_INSERT, false), (VK_INSERT, true)]
        );
    }

    #[test]
    fn synthesized_and_handle_formats_are_not_saved() {
        assert!(can_save_format(CF_UNICODETEXT, true));
        assert!(!can_save_format(1, true));
        assert!(can_save_format(1, false));
        assert!(!can_save_format(2, false));
        assert!(!can_save_format(0x0300, false));
    }
}
