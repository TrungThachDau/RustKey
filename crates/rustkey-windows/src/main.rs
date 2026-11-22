//! RustKey - Vietnamese input method for Windows.
//!
//! * `rustkey.exe` runs the input method in the notification area.
//! * `rustkey.exe --settings` opens the WinUI 3 settings window, using the
//!   Windows App Runtime embedded in the executable (see [`bundle`]).

#![windows_subsystem = "windows"]

mod bundle;
mod clipboard;
mod host;
mod input;
mod settings;
mod tray;
mod ui;
mod win;

/// Command line switch that opens the settings window.
pub const SETTINGS_ARG: &str = "--settings";
/// Keeps startup in the notification area without opening settings.
pub const BACKGROUND_ARG: &str = "--background";

const SETTINGS_MUTEX: &str = "Local\\RustKey.Settings";

fn main() -> anyhow::Result<()> {
    let settings_mode = std::env::args().skip(1).any(|arg| arg == SETTINGS_ARG);
    if settings_mode {
        if bundle::needs_relaunch() {
            bundle::settings_command()?.spawn()?;
            return Ok(());
        }
        let Some(_guard) = win::InstanceGuard::acquire(SETTINGS_MUTEX) else {
            win::activate_window_titled(ui::WINDOW_TITLE);
            return Ok(());
        };
        ui::run()
    } else {
        let background = std::env::args().skip(1).any(|arg| arg == BACKGROUND_ARG);
        host::run(!background)
    }
}
