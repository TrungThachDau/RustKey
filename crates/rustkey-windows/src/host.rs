//! The background process: low level hooks, tray icon and smart switch.
//!
//! All state lives on the UI thread. Low level hooks and out-of-context win
//! event hooks are delivered to the installing thread through its message
//! loop, so a `thread_local` is enough.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use anyhow::{Result, bail};
use rustkey_engine::{Config, KeyInput, Output, Processor};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EVENT_SYSTEM_FOREGROUND, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MB_OK, MSG,
    MSLLHOOKSTRUCT, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP,
    WM_CONTEXTMENU, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WNDCLASSEXW,
};

use crate::input::{self, INJECTED_MARKER};
use crate::settings::{AppMemory, Settings, load_macros};
use crate::tray::{MenuCommand, MenuState, Tray, WM_TRAY_CALLBACK, show_menu};
use crate::win::{
    self, InstanceGuard, TRAY_WINDOW_CLASS, WM_RUSTKEY_RELOAD, WM_RUSTKEY_SHOW_SETTINGS, wide,
};

/// Posted by the keyboard hook when the switch hotkey changed the language.
const WM_LANGUAGE_CHANGED: u32 = WM_APP + 2;
const INSTANCE_MUTEX: &str = "Local\\RustKey.Tray";

/// Windows that belong to the shell; activating them (e.g. by clicking the
/// tray icon) must not be treated as switching application.
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
];

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Runs `f` with the host state, unless it is missing or already borrowed.
fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    HOST.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

struct Host {
    hwnd: HWND,
    processor: Processor,
    settings: Settings,
    tray: Tray,
    own_exe: Option<String>,
    current_app: Option<String>,
}

impl Host {
    fn config(&self) -> &Config {
        self.processor.config()
    }

    fn tooltip(&self) -> String {
        let config = self.config();
        let language = if config.vietnamese { "Tiếng Việt" } else { "English" };
        format!(
            "RustKey – {language}\n{} · {}",
            config.input_type.name(),
            config.code_table.name()
        )
    }

    fn refresh_tray(&self) {
        self.tray.show(self.config().vietnamese, &self.tooltip());
    }

    fn menu_state(&self) -> MenuState {
        let config = self.config();
        MenuState {
            vietnamese: config.vietnamese,
            hotkey: config.switch_hotkey.to_string(),
            input_type: config.input_type,
            code_table: config.code_table,
            check_spelling: config.check_spelling,
            use_macro: config.use_macro,
            smart_switch: config.smart_switch,
        }
    }

    fn save(&self) {
        if let Err(error) = self.settings.save() {
            win::log(&format!("cannot save settings: {error:#}"));
        }
    }

    /// Stores the current language/code table for the active application.
    /// Returns `true` if the memory changed.
    fn remember_app(&mut self) -> bool {
        let Some(app) = self.current_app.clone() else {
            return false;
        };
        let config = self.processor.config();
        if !config.smart_switch && !config.remember_code_table {
            return false;
        }
        let memory = AppMemory {
            vietnamese: config.vietnamese,
            code_table: config.remember_code_table.then_some(config.code_table),
        };
        self.settings.apps.insert(app, memory) != Some(memory)
    }

    /// Applies a new configuration, persists it and updates the icon.
    fn apply_config(&mut self, config: Config) {
        if config.code_table != self.processor.macros().code_table() {
            let mut macros = self.processor.macros().clone();
            macros.set_code_table(config.code_table);
            self.processor.set_macros(macros);
        }
        self.processor.set_config(config.clone());
        self.settings.config = config;
        self.remember_app();
        self.save();
        self.refresh_tray();
    }

    fn update_config(&mut self, change: impl FnOnce(&mut Config)) {
        let mut config = self.config().clone();
        change(&mut config);
        self.apply_config(config);
    }

    /// The processor switched language through the hotkey.
    fn language_changed(&mut self) {
        self.settings.config.vietnamese = self.config().vietnamese;
        self.remember_app();
        self.save();
        self.refresh_tray();
    }

    fn reload(&mut self) {
        let was_vietnamese = self.config().vietnamese;
        self.settings = Settings::load();
        let config = self.settings.config.clone();
        self.processor.set_macros(load_macros(config.code_table));
        self.processor.set_config(config);
        // The language was switched from the settings window: remember it for
        // the application the user came from, or smart switch would undo it.
        if self.config().vietnamese != was_vietnamese && self.remember_app() {
            self.save();
        }
        self.refresh_tray();
    }

    fn on_foreground(&mut self, hwnd: HWND) {
        self.processor.reset();
        if hwnd.is_null() || SHELL_CLASSES.contains(&win::window_class(hwnd).as_str()) {
            return;
        }
        let Some(app) = win::window_process_name(hwnd) else {
            return;
        };
        if Some(&app) == self.own_exe.as_ref() {
            // The settings window: keep the current state.
            return;
        }
        self.current_app = Some(app.clone());

        let config = self.config();
        if !config.smart_switch && !config.remember_code_table {
            return;
        }
        match self.settings.apps.get(&app).copied() {
            Some(memory) => {
                let mut new_config = config.clone();
                if config.smart_switch {
                    new_config.vietnamese = memory.vietnamese;
                }
                if config.remember_code_table
                    && let Some(table) = memory.code_table
                {
                    new_config.code_table = table;
                }
                if new_config != *config {
                    self.apply_config(new_config);
                }
            }
            None => {
                if self.remember_app() {
                    self.save();
                }
            }
        }
    }

    fn run_command(&mut self, command: MenuCommand) {
        match command {
            MenuCommand::ToggleLanguage => self.update_config(|c| c.vietnamese = !c.vietnamese),
            MenuCommand::InputType(input_type) => self.update_config(|c| c.input_type = input_type),
            MenuCommand::CodeTable(table) => self.update_config(|c| c.code_table = table),
            MenuCommand::ToggleSpelling => self.update_config(|c| c.check_spelling = !c.check_spelling),
            MenuCommand::ToggleMacro => self.update_config(|c| c.use_macro = !c.use_macro),
            MenuCommand::ToggleSmartSwitch => self.update_config(|c| c.smart_switch = !c.smart_switch),
            MenuCommand::Settings => open_settings(),
            MenuCommand::Exit => {
                // SAFETY: destroying our own window ends the message loop.
                unsafe { DestroyWindow(self.hwnd) };
            }
        }
    }
}

/// Starts `rustkey.exe --settings`. Runs on a worker thread because the first
/// start extracts the embedded runtime, which must not stall the keyboard hook.
fn open_settings() {
    std::thread::spawn(|| {
        let result = crate::bundle::settings_command().and_then(|mut command| Ok(command.spawn()?));
        if let Err(error) = result {
            win::log(&format!("cannot open settings: {error:#}"));
        }
    });
}

fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| {
        let name = wide("TaskbarCreated");
        // SAFETY: `name` is a valid null terminated string.
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    })
}

/// Handles the output of the processor for the current key event.
/// Returns `true` if the original key must be swallowed.
fn deliver(hwnd: HWND, output: &Output) -> bool {
    if !output.ops.is_empty() {
        input::send(&output.ops);
    }
    if output.beep {
        // SAFETY: MessageBeep has no memory safety requirements.
        unsafe { MessageBeep(MB_OK) };
    }
    if output.language_changed.is_some() {
        // SAFETY: posting to our own window.
        unsafe { PostMessageW(hwnd, WM_LANGUAGE_CHANGED, 0, 0) };
    }
    output.consumed
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if info.dwExtraInfo != INJECTED_MARKER
            && let Ok(key) = u16::try_from(info.vkCode)
        {
            let down = matches!(u32::try_from(wparam), Ok(WM_KEYDOWN | WM_SYSKEYDOWN));
            // SAFETY: GetKeyState has no memory safety requirements.
            let caps_lock = unsafe { GetKeyState(i32::from(VK_CAPITAL)) } & 1 != 0;
            let event = KeyInput { key, down, caps_lock };
            if let Some((hwnd, output)) = with_host(|host| (host.hwnd, host.processor.handle_key(event)))
                && deliver(hwnd, &output)
            {
                return 1;
            }
        }
    }
    // SAFETY: forwarding the unmodified hook arguments.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, `lparam` points to an MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let click = matches!(
            u32::try_from(wparam),
            Ok(WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
        );
        if click && info.dwExtraInfo != INJECTED_MARKER {
            with_host(|host| host.processor.handle_mouse_click());
        }
    }
    // SAFETY: forwarding the unmodified hook arguments.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    with_host(|host| host.on_foreground(hwnd));
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TRAY_CALLBACK => {
            match u32::try_from(lparam & 0xFFFF).unwrap_or(0) {
                WM_LBUTTONUP => {
                    with_host(|host| host.run_command(MenuCommand::ToggleLanguage));
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    // The menu runs a modal loop: do not hold the host borrow.
                    if let Some(state) = with_host(|host| host.menu_state())
                        && let Some(command) = show_menu(hwnd, &state)
                    {
                        with_host(|host| host.run_command(command));
                    }
                }
                _ => {}
            }
            0
        }
        WM_LANGUAGE_CHANGED => {
            with_host(Host::language_changed);
            0
        }
        WM_RUSTKEY_RELOAD => {
            with_host(Host::reload);
            0
        }
        WM_RUSTKEY_SHOW_SETTINGS => {
            open_settings();
            0
        }
        WM_DESTROY => {
            // SAFETY: ends the message loop of this thread.
            unsafe { PostQuitMessage(0) };
            0
        }
        _ if message == taskbar_created_message() => {
            with_host(|host| host.refresh_tray());
            0
        }
        // SAFETY: default processing with the original arguments.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn create_window() -> Result<HWND> {
    let class_name = wide(TRAY_WINDOW_CLASS);
    // SAFETY: the class structure and strings outlive the calls.
    unsafe {
        let instance = GetModuleHandleW(null());
        // SAFETY: WNDCLASSEXW is plain data; all-zero is a valid value.
        let mut class: WNDCLASSEXW = std::mem::zeroed();
        class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.lpszClassName = class_name.as_ptr();
        if RegisterClassExW(&class) == 0 {
            bail!("RegisterClassExW failed");
        }
        // A hidden top level window (not message-only) so that it receives
        // the "TaskbarCreated" broadcast.
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            bail!("CreateWindowExW failed");
        }
        Ok(hwnd)
    }
}

/// Removes the hooks when dropped.
struct Hooks {
    keyboard: HHOOK,
    mouse: HHOOK,
    foreground: HWINEVENTHOOK,
}

impl Hooks {
    fn install() -> Result<Self> {
        // SAFETY: the hook procedures have the required signatures and live
        // for the whole program.
        let hooks = unsafe {
            let instance = GetModuleHandleW(null());
            Self {
                keyboard: SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0),
                mouse: SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0),
                foreground: SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    null_mut(),
                    Some(foreground_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                ),
            }
        };
        if hooks.keyboard.is_null() {
            bail!("cannot install the keyboard hook");
        }
        Ok(hooks)
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        // SAFETY: each handle was returned by the matching install call.
        unsafe {
            if !self.keyboard.is_null() {
                UnhookWindowsHookEx(self.keyboard);
            }
            if !self.mouse.is_null() {
                UnhookWindowsHookEx(self.mouse);
            }
            if !self.foreground.is_null() {
                UnhookWinEvent(self.foreground);
            }
        }
    }
}

/// Runs the input method until "Exit" is chosen.
pub fn run() -> Result<()> {
    let Some(_instance) = InstanceGuard::acquire(INSTANCE_MUTEX) else {
        // Already running: show its settings window instead.
        win::post_to_tray(WM_RUSTKEY_SHOW_SETTINGS);
        return Ok(());
    };
    // SAFETY: called before any window is created.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let hwnd = create_window()?;
    let settings = Settings::load();
    let mut processor = Processor::new(settings.config.clone());
    processor.set_macros(load_macros(settings.config.code_table));
    let own_exe = std::env::current_exe()
        .ok()
        .and_then(|path| win::file_name_lower(&path));
    let host = Host {
        hwnd,
        processor,
        settings,
        tray: Tray::new(hwnd),
        own_exe,
        current_app: None,
    };
    host.refresh_tray();
    host.save();
    HOST.with(|cell| *cell.borrow_mut() = Some(host));

    let hooks = Hooks::install()?;
    let mut message = MSG::default();
    // SAFETY: standard message loop with a valid MSG buffer.
    unsafe {
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    drop(hooks);
    HOST.with(|cell| cell.borrow_mut().take());
    Ok(())
}
