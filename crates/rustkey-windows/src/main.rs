//! RustKey - Vietnamese input method for Windows.
//!
//! * `rustkey.exe` runs the input method in the notification area.
//! * `rustkey.exe --settings` opens the WinUI 3 settings window.

#![windows_subsystem = "windows"]

mod host;
mod input;
mod settings;
mod tray;
mod ui;
mod win;

/// Command line switch that opens the settings window.
pub const SETTINGS_ARG: &str = "--settings";

const SETTINGS_MUTEX: &str = "Local\\RustKey.Settings";

fn main() -> anyhow::Result<()> {
    let settings_mode = std::env::args().skip(1).any(|arg| arg == SETTINGS_ARG);
    if settings_mode {
        let Some(_guard) = win::InstanceGuard::acquire(SETTINGS_MUTEX) else {
            win::activate_window_titled(ui::WINDOW_TITLE);
            return Ok(());
        };
        ui::run()
    } else {
        host::run()
    }
}
