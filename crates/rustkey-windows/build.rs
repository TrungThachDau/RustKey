use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories that cargo creates in the profile directory.
const CARGO_DIRS: &[&str] = &["build", "deps", "examples", "incremental", ".fingerprint"];

/// Deployed by `windows-reactor-setup` for the `WebView2` control, which RustKey doesn't use.
const UNUSED_FILES: &[&str] = &["microsoft.web.webview2.core.dll"];

fn main() {
    // Deploys the Windows App Runtime next to the executable and embeds a
    // manifest that activates it without the framework package.
    windows_reactor_setup::as_self_contained();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=rustkey.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("rustkey.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap_or_else(|error| println!("cargo:warning=icon resource not embedded: {error}"));
    embed_runtime();
    delay_load_dynamic_dependencies();
}

/// `windows-reactor` imports the Windows 11 Dynamic Dependencies API to bootstrap
/// the framework package. A self-contained build never calls it, so delay-load
/// it to keep the executable starting on Windows 10.
fn delay_load_dynamic_dependencies() {
    if std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc") {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:api-ms-win-appmodel-runtime-l1-1-5.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
}

/// Packs the deployed runtime into `OUT_DIR/runtime.bin` so that a single
/// `rustkey.exe` can be copied to another machine.
fn embed_runtime() {
    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        println!("cargo:warning=OUT_DIR is not set");
        return;
    };
    let archive = profile_dir(&out_dir)
        .ok_or_else(|| "profile directory not found".into())
        .and_then(|dir| pack(&dir))
        .unwrap_or_else(|error| {
            println!("cargo:warning=Windows App Runtime not embedded: {error}");
            Vec::new()
        });
    let compressed = if archive.is_empty() {
        Vec::new()
    } else {
        miniz_oxide::deflate::compress_to_vec(&archive, 9)
    };
    println!(
        "cargo:rustc-env=RUSTKEY_RUNTIME_ID={:016x}",
        fnv1a(&archive)
    );
    if let Err(error) = fs::write(out_dir.join("runtime.bin"), compressed) {
        println!("cargo:warning=cannot write runtime.bin: {error}");
    }
}

/// `target/<profile>`, where `windows-reactor-setup` deploys the runtime.
fn profile_dir(out_dir: &Path) -> Option<PathBuf> {
    let profile = std::env::var_os("PROFILE")?;
    out_dir
        .ancestors()
        .find(|dir| dir.file_name() == Some(profile.as_os_str()))
        .map(Path::to_path_buf)
}

/// Serializes the runtime files as `(u16 path length, path, u32 data length, data)` records.
fn pack(profile_dir: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(profile_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let path = entry.path();
        if path.is_dir() {
            if !CARGO_DIRS.contains(&lower.as_str()) {
                collect(&path, &name, &mut files)?;
            }
        } else if (lower.ends_with(".dll") || lower.ends_with(".pri"))
            && !UNUSED_FILES.contains(&lower.as_str())
        {
            files.push((name, path));
        }
    }
    if !files
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("Microsoft.WindowsAppRuntime.dll"))
    {
        return Err("Microsoft.WindowsAppRuntime.dll is missing".into());
    }
    files.sort();

    let mut archive = Vec::new();
    for (name, path) in files {
        let data = fs::read(&path)?;
        archive.extend_from_slice(&u16::try_from(name.len())?.to_le_bytes());
        archive.extend_from_slice(name.as_bytes());
        archive.extend_from_slice(&u32::try_from(data.len())?.to_le_bytes());
        archive.extend_from_slice(&data);
    }
    Ok(archive)
}

fn collect(
    dir: &Path,
    prefix: &str,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        let path = entry.path();
        if path.is_dir() {
            collect(&path, &name, files)?;
        } else {
            files.push((name, path));
        }
    }
    Ok(())
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
