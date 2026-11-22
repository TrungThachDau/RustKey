//! Persistent settings shared by the tray process and the settings window.
//!
//! Everything lives in `%APPDATA%\RustKey`:
//! * `settings.json` - [`Settings`] (engine options and per-application memory),
//! * `macros.txt` - the macro table in OpenKey's text format.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rustkey_engine::{CodeTable, Config, MacroTable};
use serde::{Deserialize, Serialize};

const APP_DIR: &str = "RustKey";
const SETTINGS_FILE: &str = "settings.json";
const MACROS_FILE: &str = "macros.txt";

/// What RustKey remembers about an application when smart switch is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMemory {
    /// Language last used in the application.
    pub vietnamese: bool,
    /// Code table last used in the application (only with "remember code table").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_table: Option<CodeTable>,
}

/// All persisted settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Engine options.
    pub config: Config,
    /// Per application memory keyed by the lower case executable name.
    pub apps: BTreeMap<String, AppMemory>,
}

/// `%APPDATA%\RustKey`.
pub fn data_dir() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(base).join(APP_DIR))
}

fn settings_path() -> Result<PathBuf> {
    Ok(data_dir()?.join(SETTINGS_FILE))
}

/// Last modification time of `settings.json`, used to notice changes made by
/// the other process.
pub fn settings_modified() -> Option<SystemTime> {
    fs::metadata(settings_path().ok()?).ok()?.modified().ok()
}

fn macros_path() -> Result<PathBuf> {
    Ok(data_dir()?.join(MACROS_FILE))
}

/// Writes `contents` atomically (temporary file + rename).
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

impl Settings {
    /// Loads the settings, falling back to defaults when the file is missing
    /// or unreadable.
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    fn try_load() -> Result<Self> {
        let path = settings_path()?;
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Saves the settings.
    pub fn save(&self) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        write_atomic(&settings_path()?, &text)
    }
}

/// Loads the macro table encoded for `code_table`.
pub fn load_macros(code_table: CodeTable) -> MacroTable {
    let mut table = MacroTable::new(code_table);
    if let Ok(path) = macros_path()
        && let Ok(text) = fs::read_to_string(path)
    {
        table.load_from_str(&text);
    }
    table
}

/// Saves the macro table.
pub fn save_macros(table: &MacroTable) -> Result<()> {
    write_atomic(&macros_path()?, &table.to_file_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_json() -> Result<()> {
        let mut settings = Settings::default();
        settings.config.quick_telex = true;
        settings.apps.insert(
            "code.exe".into(),
            AppMemory {
                vietnamese: false,
                code_table: Some(CodeTable::VniWindows),
            },
        );
        let text = serde_json::to_string(&settings)?;
        let parsed: Settings = serde_json::from_str(&text)?;
        assert_eq!(parsed, settings);
        Ok(())
    }

    #[test]
    fn missing_fields_use_defaults() -> Result<()> {
        let parsed: Settings = serde_json::from_str(r#"{"config":{"quick_telex":true}}"#)?;
        assert!(parsed.config.quick_telex);
        assert!(parsed.config.check_spelling);
        assert!(parsed.apps.is_empty());
        Ok(())
    }
}
