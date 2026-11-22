//! Self-contained distribution.
//!
//! The Windows App Runtime used by the settings window is embedded in the
//! executable by `build.rs`. Reg-free WinRT activation needs the runtime next
//! to the executable, so on first use it is extracted to
//! `%LOCALAPPDATA%\RustKey\runtime\<id>` together with a copy of the
//! executable, and the settings window runs from there.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::win;

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runtime.bin"));
const RUNTIME_ID: &str = env!("RUSTKEY_RUNTIME_ID");
const RUNTIME_MARKER: &str = "Microsoft.WindowsAppRuntime.dll";

/// Path of the executable the user started, inherited by the extracted copy.
const LAUNCHER_ENV: &str = "RUSTKEY_LAUNCHER";

/// Whether the settings window must be started from the extracted runtime.
pub fn needs_relaunch() -> bool {
    !PAYLOAD.is_empty() && std::env::current_exe().is_ok_and(|exe| !runtime_beside(&exe))
}

/// Command that opens the settings window, extracting the runtime if needed.
pub fn settings_command() -> Result<Command> {
    let exe = std::env::current_exe()?;
    let program = if needs_relaunch() { deploy(&exe)? } else { exe };
    let mut command = Command::new(program);
    command
        .arg(crate::SETTINGS_ARG)
        .env(LAUNCHER_ENV, launcher_exe()?);
    Ok(command)
}

/// The executable the user started, even when running the extracted copy.
pub fn launcher_exe() -> std::io::Result<PathBuf> {
    match std::env::var_os(LAUNCHER_ENV) {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => std::env::current_exe(),
    }
}

fn runtime_beside(exe: &Path) -> bool {
    exe.parent()
        .is_some_and(|dir| dir.join(RUNTIME_MARKER).is_file())
}

/// Makes sure the runtime and a copy of `exe` are extracted; returns the copy.
fn deploy(exe: &Path) -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join("RustKey").join("runtime"))
        .context("LOCALAPPDATA is not set")?;
    let dir = root.join(RUNTIME_ID);
    if !dir.join(RUNTIME_MARKER).is_file() {
        extract(&root, &dir)?;
        remove_stale(&root);
    }

    let name = exe.file_name().context("executable has no file name")?;
    let copy = dir.join(name);
    if !same_file(exe, &copy) {
        // The copy may be in use by an open settings window; keep using it then.
        if let Err(error) = fs::copy(exe, &copy) {
            if !copy.is_file() {
                return Err(error).context("cannot copy the executable");
            }
            win::log(&format!("cannot update {}: {error}", copy.display()));
        }
    }
    Ok(copy)
}

/// Extracts the payload to a staging directory, then moves it into place so
/// that a complete runtime is never observed half written.
fn extract(root: &Path, dir: &Path) -> Result<()> {
    let archive = miniz_oxide::inflate::decompress_to_vec(PAYLOAD)
        .map_err(|error| anyhow!("corrupted runtime payload: {error:?}"))?;
    let staging = root.join(format!("{RUNTIME_ID}.{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let result = write_entries(&archive, &staging).and_then(|()| {
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        fs::rename(&staging, dir)?;
        Ok(())
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
        // Another process may have finished the same extraction first.
        if dir.join(RUNTIME_MARKER).is_file() {
            return Ok(());
        }
    }
    result
}

fn write_entries(archive: &[u8], dir: &Path) -> Result<()> {
    for entry in entries(archive)? {
        let path = dir.join(entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, entry.data).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}

/// Removes runtimes extracted by other RustKey versions; ignores those in use.
fn remove_stale(root: &Path) {
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        if entry.file_name() != OsStr::new(RUNTIME_ID) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => {
            a.len() == b.len()
                && a.modified()
                    .ok()
                    .is_some_and(|time| b.modified().ok() == Some(time))
        }
        _ => false,
    }
}

#[derive(Debug, PartialEq)]
struct Entry<'a> {
    path: &'a str,
    data: &'a [u8],
}

/// Parses `(u16 path length, path, u32 data length, data)` records written by `build.rs`.
fn entries(mut archive: &[u8]) -> Result<Vec<Entry<'_>>> {
    let mut entries = Vec::new();
    while !archive.is_empty() {
        let path_len = u16::from_le_bytes(take(&mut archive)?);
        let path = std::str::from_utf8(take_slice(&mut archive, usize::from(path_len))?)?;
        if !is_relative_path(path) {
            bail!("invalid path in runtime payload: {path}");
        }
        let data_len = usize::try_from(u32::from_le_bytes(take(&mut archive)?))?;
        let data = take_slice(&mut archive, data_len)?;
        entries.push(Entry { path, data });
    }
    Ok(entries)
}

fn take<const N: usize>(archive: &mut &[u8]) -> Result<[u8; N]> {
    Ok(take_slice(archive, N)?.try_into()?)
}

fn take_slice<'a>(archive: &mut &'a [u8], len: usize) -> Result<&'a [u8]> {
    let (head, tail) = archive
        .split_at_checked(len)
        .context("truncated runtime payload")?;
    *archive = tail;
    Ok(head)
}

/// Rejects paths that could escape the extraction directory.
fn is_relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.contains([':', '\\'])
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pack(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        for (path, data) in files {
            archive.extend_from_slice(&u16::try_from(path.len()).unwrap().to_le_bytes());
            archive.extend_from_slice(path.as_bytes());
            archive.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
            archive.extend_from_slice(data);
        }
        archive
    }

    #[test]
    fn entries_should_round_trip_packed_files() {
        let archive = pack(&[("a.dll", b"abc"), ("vi-VN/b.mui", b""), ("c.pri", b"xyz")]);
        let expected = vec![
            Entry {
                path: "a.dll",
                data: b"abc",
            },
            Entry {
                path: "vi-VN/b.mui",
                data: b"",
            },
            Entry {
                path: "c.pri",
                data: b"xyz",
            },
        ];
        assert_eq!(entries(&archive).unwrap(), expected);
    }

    #[test]
    fn entries_should_reject_truncated_archive() {
        let archive = pack(&[("a.dll", b"abc")]);
        assert!(entries(&archive[..archive.len() - 1]).is_err());
    }

    #[test]
    fn entries_should_reject_parent_directory_paths() {
        let archive = pack(&[("../evil.dll", b"x")]);
        assert!(entries(&archive).is_err());
    }

    #[test]
    fn is_relative_path_should_reject_absolute_paths() {
        assert!(!is_relative_path("C:/Windows/evil.dll"));
    }

    #[test]
    fn embedded_payload_should_contain_the_runtime() {
        let archive = miniz_oxide::inflate::decompress_to_vec(PAYLOAD).unwrap();
        let entries = entries(&archive).unwrap();
        assert!(entries.iter().any(|entry| entry.path == RUNTIME_MARKER));
    }
}
