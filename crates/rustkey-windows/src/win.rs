//! Small Win32 helpers shared by the tray process and the settings window.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};

use anyhow::{Result, bail};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GetLastError, HANDLE,
    HWND,
};
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClassNameW, GetWindowThreadProcessId, IsIconic, PostMessageW, SW_RESTORE,
    SetForegroundWindow, ShowWindow, WM_APP,
};

/// Window class of the hidden tray window (used for inter-process messages).
pub const TRAY_WINDOW_CLASS: &str = "RustKeyTrayWindow";
/// Asks the tray process to reload `settings.json` and `macros.txt`.
pub const WM_RUSTKEY_RELOAD: u32 = WM_APP + 10;
/// Asks the tray process to open the settings window.
pub const WM_RUSTKEY_SHOW_SETTINGS: u32 = WM_APP + 11;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "RustKey";

/// Null terminated UTF-16 string.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `text` into a fixed size, null terminated UTF-16 buffer (truncating).
pub fn copy_wide<const N: usize>(target: &mut [u16; N], text: &str) {
    let mut len = 0;
    for unit in text.encode_utf16().take(N.saturating_sub(1)) {
        target[len] = unit;
        len += 1;
    }
    target[len..].fill(0);
}

/// Writes a message to the debugger output (the app has no console).
pub fn log(message: &str) {
    let text = wide(&format!("RustKey: {message}\n"));
    // SAFETY: `text` is a valid null terminated UTF-16 string.
    unsafe { OutputDebugStringW(text.as_ptr()) };
}

/// Finds the tray window of the running RustKey process.
pub fn find_tray_window() -> Option<HWND> {
    let class = wide(TRAY_WINDOW_CLASS);
    // SAFETY: `class` is a valid null terminated string.
    let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
    (!hwnd.is_null()).then_some(hwnd)
}

/// Posts `message` to the tray process. Returns `false` if it is not running.
pub fn post_to_tray(message: u32) -> bool {
    match find_tray_window() {
        // SAFETY: posting a message to a window handle is always safe.
        Some(hwnd) => unsafe { PostMessageW(hwnd, message, 0, 0) != 0 },
        None => false,
    }
}

/// Brings the top level window with the given title to the front.
pub fn activate_window_titled(title: &str) -> bool {
    let title = wide(title);
    // SAFETY: `title` is a valid null terminated string; the handle is checked.
    unsafe {
        let hwnd = FindWindowW(null(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd) != 0
    }
}

/// A named mutex used to keep a single process instance.
pub struct InstanceGuard(HANDLE);

impl InstanceGuard {
    /// Returns `None` if another process already owns `name`.
    pub fn acquire(name: &str) -> Option<Self> {
        let name = wide(name);
        // SAFETY: `name` is a valid null terminated string.
        let (handle, error) = unsafe {
            let handle = CreateMutexW(null(), 0, name.as_ptr());
            (handle, GetLastError())
        };
        if error == ERROR_ALREADY_EXISTS {
            if !handle.is_null() {
                // SAFETY: `handle` was returned by CreateMutexW.
                unsafe { CloseHandle(handle) };
            }
            return None;
        }
        Some(Self(handle))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle was returned by CreateMutexW and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Lower case executable file name of the process owning `hwnd`.
pub fn window_process_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    // SAFETY: the process handle is checked and closed; the buffer length is passed.
    let path = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        String::from_utf16_lossy(&buffer[..len as usize])
    };
    file_name_lower(Path::new(&path))
}

/// Lower case file name of `path`.
pub fn file_name_lower(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
}

/// Window class name of `hwnd`.
pub fn window_class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: the buffer length is passed to the API.
    let len = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0)])
}

/// Whether RustKey is registered to start with Windows.
pub fn autostart_enabled() -> bool {
    let key = wide(RUN_KEY);
    let value = wide(RUN_VALUE);
    // SAFETY: valid strings; no data buffer is requested.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    status == ERROR_SUCCESS
}

/// Registers or unregisters RustKey to start with Windows.
pub fn set_autostart(enable: bool) -> Result<()> {
    let key = wide(RUN_KEY);
    let value = wide(RUN_VALUE);
    let status = if enable {
        let exe = std::env::current_exe()?;
        let command = wide(&format!("\"{}\"", exe.display()));
        let bytes = u32::try_from(command.len() * 2)?;
        // SAFETY: `command` lives during the call and `bytes` is its size in bytes.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                command.as_ptr().cast::<c_void>(),
                bytes,
            )
        }
    } else {
        // SAFETY: valid null terminated strings.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
            ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
            status => status,
        }
    };
    if status != ERROR_SUCCESS {
        bail!("registry error {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_wide_truncates_and_terminates() {
        let mut buffer = [1u16; 4];
        copy_wide(&mut buffer, "abcdef");
        assert_eq!(buffer, [97, 98, 99, 0]);
        copy_wide(&mut buffer, "a");
        assert_eq!(buffer, [97, 0, 0, 0]);
    }

    #[test]
    fn wide_is_null_terminated() {
        assert_eq!(wide("hi"), vec![104, 105, 0]);
    }
}
