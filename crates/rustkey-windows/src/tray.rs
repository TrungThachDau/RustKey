//! Notification area icon and its context menu.

use std::mem::size_of;
use std::ptr::{null, null_mut};

use rustkey_engine::{CodeTable, InputType};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    GetSystemMetrics, HICON, HMENU, LR_DEFAULTCOLOR, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    PostMessageW, SM_CXSMICON, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_NULL,
};

use crate::win::{copy_wide, wide};

/// Callback message of the notification icon.
pub const WM_TRAY_CALLBACK: u32 = WM_APP + 1;

const TRAY_ID: u32 = 1;
const ICON_VIETNAMESE: &[u8] = include_bytes!("../assets/StatusViet.ico");
const ICON_ENGLISH: &[u8] = include_bytes!("../assets/StatusEng.ico");

/// Creates an icon of roughly `size` pixels from the bytes of an `.ico` file.
fn icon_from_ico(data: &[u8], size: i32) -> HICON {
    let read_u16 = |at: usize| data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let read_u32 =
        |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let count = read_u16(4).unwrap_or(0) as usize;
    let wanted = u32::try_from(size).unwrap_or(16);

    // (width, offset, length) of the best entry: the smallest one not smaller
    // than requested, otherwise the largest one.
    let mut best: Option<(u32, usize, usize)> = None;
    for index in 0..count {
        let entry = 6 + index * 16;
        let (Some(&w), Some(len), Some(offset)) = (data.get(entry), read_u32(entry + 8), read_u32(entry + 12))
        else {
            break;
        };
        let width = if w == 0 { 256 } else { u32::from(w) };
        let candidate = (width, offset as usize, len as usize);
        best = match best {
            None => Some(candidate),
            Some(current) => {
                let better = if width >= wanted {
                    current.0 < wanted || width < current.0
                } else {
                    current.0 < wanted && width > current.0
                };
                Some(if better { candidate } else { current })
            }
        };
    }
    let Some((_, offset, len)) = best else {
        return null_mut();
    };
    let Some(bits) = data.get(offset..offset + len) else {
        return null_mut();
    };
    // SAFETY: `bits` is a complete icon image (BMP or PNG) of `len` bytes.
    unsafe {
        CreateIconFromResourceEx(
            bits.as_ptr(),
            len as u32,
            1,
            0x0003_0000,
            size,
            size,
            LR_DEFAULTCOLOR,
        )
    }
}

/// The notification area icon.
pub struct Tray {
    hwnd: HWND,
    vietnamese_icon: HICON,
    english_icon: HICON,
}

impl Tray {
    /// Loads the icons. The icon is shown by [`Tray::show`].
    pub fn new(hwnd: HWND) -> Self {
        // SAFETY: GetSystemMetrics has no memory safety requirements.
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
        Self {
            hwnd,
            vietnamese_icon: icon_from_ico(ICON_VIETNAMESE, size),
            english_icon: icon_from_ico(ICON_ENGLISH, size),
        }
    }

    fn data(&self, vietnamese: bool, tip: &str) -> NOTIFYICONDATAW {
        // SAFETY: NOTIFYICONDATAW is plain data; all-zero is a valid value.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_CALLBACK;
        data.hIcon = if vietnamese {
            self.vietnamese_icon
        } else {
            self.english_icon
        };
        copy_wide(&mut data.szTip, tip);
        data
    }

    /// Adds the icon (also used after Explorer restarts).
    pub fn show(&self, vietnamese: bool, tip: &str) {
        let data = self.data(vietnamese, tip);
        // SAFETY: `data` is fully initialized.
        unsafe {
            if Shell_NotifyIconW(NIM_MODIFY, &data) == 0 {
                Shell_NotifyIconW(NIM_ADD, &data);
            }
        }
    }

    /// Removes the icon.
    pub fn hide(&self) {
        let data = self.data(false, "");
        // SAFETY: `data` is fully initialized.
        unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.hide();
        for icon in [self.vietnamese_icon, self.english_icon] {
            if !icon.is_null() {
                // SAFETY: the icons were created by CreateIconFromResourceEx.
                unsafe { DestroyIcon(icon) };
            }
        }
    }
}

/// State shown in the context menu.
#[derive(Debug, Clone)]
pub struct MenuState {
    pub vietnamese: bool,
    pub hotkey: String,
    pub input_type: InputType,
    pub code_table: CodeTable,
    pub check_spelling: bool,
    pub use_macro: bool,
    pub smart_switch: bool,
}

/// A command chosen from the context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    ToggleLanguage,
    InputType(InputType),
    CodeTable(CodeTable),
    ToggleSpelling,
    ToggleMacro,
    ToggleSmartSwitch,
    Settings,
    Exit,
}

const ID_TOGGLE_LANGUAGE: usize = 1;
const ID_SPELLING: usize = 2;
const ID_MACRO: usize = 3;
const ID_SMART_SWITCH: usize = 4;
const ID_SETTINGS: usize = 5;
const ID_EXIT: usize = 6;
const ID_INPUT_TYPE: usize = 100;
const ID_CODE_TABLE: usize = 200;

impl MenuCommand {
    fn from_id(id: usize) -> Option<Self> {
        Some(match id {
            ID_TOGGLE_LANGUAGE => Self::ToggleLanguage,
            ID_SPELLING => Self::ToggleSpelling,
            ID_MACRO => Self::ToggleMacro,
            ID_SMART_SWITCH => Self::ToggleSmartSwitch,
            ID_SETTINGS => Self::Settings,
            ID_EXIT => Self::Exit,
            _ if id >= ID_CODE_TABLE => Self::CodeTable(CodeTable::from_index(id - ID_CODE_TABLE)?),
            _ if id >= ID_INPUT_TYPE => {
                Self::InputType(*InputType::ALL.get(id - ID_INPUT_TYPE)?)
            }
            _ => return None,
        })
    }
}

struct Menu(HMENU);

impl Menu {
    fn new() -> Self {
        // SAFETY: CreatePopupMenu has no preconditions.
        Self(unsafe { CreatePopupMenu() })
    }

    fn item(&self, id: usize, text: &str, checked: bool) {
        let text = wide(text);
        let flags = MF_STRING | if checked { MF_CHECKED } else { 0 };
        // SAFETY: `text` is a valid null terminated string.
        unsafe { AppendMenuW(self.0, flags, id, text.as_ptr()) };
    }

    fn separator(&self) {
        // SAFETY: separators take no text.
        unsafe { AppendMenuW(self.0, MF_SEPARATOR, 0, null()) };
    }

    /// Appends `sub` as a submenu; ownership moves to this menu.
    fn submenu(&self, sub: Menu, text: &str) {
        let text = wide(text);
        let handle = sub.0;
        std::mem::forget(sub);
        // SAFETY: `handle` is a valid menu that is destroyed with its parent.
        unsafe { AppendMenuW(self.0, MF_POPUP, handle as usize, text.as_ptr()) };
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        // SAFETY: the menu was created by CreatePopupMenu and is destroyed once.
        unsafe { DestroyMenu(self.0) };
    }
}

/// Shows the context menu at the cursor and returns the chosen command.
///
/// This runs a modal loop, so the caller must not hold any borrowed state.
pub fn show_menu(hwnd: HWND, state: &MenuState) -> Option<MenuCommand> {
    let menu = Menu::new();
    menu.item(ID_TOGGLE_LANGUAGE, &format!("Tiếng Việt\t{}", state.hotkey), state.vietnamese);
    menu.separator();

    let input_types = Menu::new();
    for (index, input_type) in InputType::ALL.iter().enumerate() {
        input_types.item(ID_INPUT_TYPE + index, input_type.name(), *input_type == state.input_type);
    }
    menu.submenu(input_types, &format!("Kiểu gõ: {}", state.input_type.name()));

    let code_tables = Menu::new();
    for table in CodeTable::ALL {
        code_tables.item(ID_CODE_TABLE + table.index(), table.name(), table == state.code_table);
    }
    menu.submenu(code_tables, &format!("Bảng mã: {}", state.code_table.name()));

    menu.separator();
    menu.item(ID_SPELLING, "Kiểm tra chính tả", state.check_spelling);
    menu.item(ID_MACRO, "Gõ tắt", state.use_macro);
    menu.item(ID_SMART_SWITCH, "Nhớ ngôn ngữ theo ứng dụng", state.smart_switch);
    menu.separator();
    menu.item(ID_SETTINGS, "Cài đặt…", false);
    menu.item(ID_EXIT, "Thoát", false);

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: valid handles and out pointer; see the Win32 notes on TrackPopupMenu
    // (the window must be in the foreground and receive WM_NULL afterwards).
    let id = unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let id = TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        id
    };
    MenuCommand::from_id(usize::try_from(id).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_round_trip() {
        assert_eq!(MenuCommand::from_id(ID_EXIT), Some(MenuCommand::Exit));
        assert_eq!(
            MenuCommand::from_id(ID_INPUT_TYPE + 1),
            Some(MenuCommand::InputType(InputType::Vni))
        );
        assert_eq!(
            MenuCommand::from_id(ID_CODE_TABLE + 2),
            Some(MenuCommand::CodeTable(CodeTable::VniWindows))
        );
        assert_eq!(MenuCommand::from_id(ID_CODE_TABLE + 99), None);
        assert_eq!(MenuCommand::from_id(0), None);
    }
}
