//! Keeps the launcher itself up to date from the GitHub releases of
//! harpy-launcher. Installers are signed with a key that only exists on the
//! publisher's PC: a download whose signature doesn't match the public key in
//! config.rs is never run.

use crate::config::{LAUNCHER_REPO, UPDATE_PUBLIC_KEY};
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter};

const PLATFORM: &str = "windows-x86_64";

/// Same format as Tauri's official updater, so either can read it.
#[derive(Deserialize)]
pub struct Release {
    pub version: String,
    platforms: HashMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    signature: String,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current: &'static str,
    available: Option<String>,
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    done: u64,
    total: u64,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn parse(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.trim().split('.').map(|part| part.parse::<u64>().ok());
    let version = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(version)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse(candidate), parse(current)), (Some(candidate), Some(current)) if candidate > current)
}

/// `HARPY_UPDATE_URL` points the launcher at a test release instead.
fn endpoint() -> String {
    std::env::var("HARPY_UPDATE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{LAUNCHER_REPO}/releases/latest/download/latest.json"))
}

/// A dev build must never replace itself with the released installer.
fn enabled() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("HARPY_UPDATE_URL").is_some()
}

/// The latest published release, or `None` before the first one.
pub async fn latest(client: &reqwest::Client) -> Result<Option<Release>, String> {
    let response = client
        .get(endpoint())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| "GitHub est injoignable.".to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| format!("latest.json illisible : {e}"))
}

/// The release to install, when it is newer than this launcher.
pub async fn check(client: &reqwest::Client) -> Result<Option<Release>, String> {
    if !enabled() {
        return Ok(None);
    }
    Ok(latest(client)
        .await?
        .filter(|release| is_newer(&release.version, current_version())))
}

/// Downloads the installer and only writes it to disk once its signature checks out.
pub async fn download(
    client: &reqwest::Client,
    release: &Release,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let platform = release
        .platforms
        .get(PLATFORM)
        .ok_or("Cette version du launcher n'existe pas pour Windows.")?;
    let mut response = client
        .get(&platform.url)
        .timeout(Duration::from_secs(600))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Téléchargement impossible : {e}"))?;

    let total = response.content_length().unwrap_or(0);
    let mut bytes = Vec::with_capacity(total as usize);
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        on_progress(bytes.len() as u64, total);
    }
    verify(&bytes, &platform.signature)?;

    let dir = std::env::temp_dir().join("harpy-launcher-update");
    // Installers of earlier updates are no longer needed.
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // `version` passed `is_newer`, so it is only digits and dots.
    let path = dir.join(format!("Harpy-Launcher-Setup-{}.exe", release.version));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn verify(data: &[u8], signature: &str) -> Result<(), String> {
    let decode = |text: &str| {
        STANDARD
            .decode(text.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    };
    let key = decode(UPDATE_PUBLIC_KEY)
        .and_then(|text| PublicKey::decode(&text).ok())
        .ok_or("Clé de mise à jour invalide.")?;
    let signature = decode(signature)
        .and_then(|text| Signature::decode(&text).ok())
        .ok_or("Signature de la mise à jour illisible.")?;
    key.verify(data, &signature, true)
        .map_err(|_| "Mise à jour refusée : elle n'est pas signée par Harpy Express.".to_string())
}

/// Starts the installer on its own. The caller must exit right away: the
/// installer replaces this executable, and closes it if it is still running.
/// `relaunch` shows a small progress window and reopens the launcher afterwards;
/// otherwise the installer is silent.
pub fn launch_installer(installer: &Path, relaunch: bool) -> Result<(), String> {
    let args: &[&str] = if relaunch {
        &["/P", "/R", "/UPDATE"]
    } else {
        &["/S", "/UPDATE"]
    };
    release_std_handles();
    let mut command = Command::new(installer);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Outlive whatever started us, even if it kills its children on exit.
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
        if command.spawn().is_ok() {
            return Ok(());
        }
        command.creation_flags(0);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible de lancer la mise à jour : {e}"))
}

/// Lunar waits until every handle on the pre-launch command's output is closed:
/// an installer inheriting ours would hold the game back until it finishes.
#[cfg(windows)]
fn release_std_handles() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(std_handle: u32) -> isize;
        fn SetHandleInformation(handle: isize, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    // STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE
    for std_handle in [-10i32, -11, -12] {
        // SAFETY: plain Win32 calls on this process's own standard handles.
        unsafe {
            let handle = GetStdHandle(std_handle as u32);
            if handle != 0 && handle != -1 {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

#[cfg(not(windows))]
fn release_std_handles() {}

/// Another copy of the launcher is open: the pre-launch sync leaves the update to it.
pub fn other_instance_running() -> bool {
    let Some(name) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|name| name.to_os_string()))
    else {
        return false;
    };
    let own = sysinfo::get_current_pid().ok();
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system.processes().values().any(|process| {
        Some(process.pid()) != own && process.name().eq_ignore_ascii_case(&name)
    })
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateCheck, String> {
    let available = check(&crate::manifest::http_client()).await?;
    Ok(UpdateCheck {
        current: current_version(),
        available: available.map(|release| release.version),
    })
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let client = crate::manifest::http_client();
    let release = check(&client).await?.ok_or("Le launcher est déjà à jour.")?;
    let mut last_report = Instant::now() - Duration::from_secs(1);
    let installer = download(&client, &release, |done, total| {
        if last_report.elapsed() >= Duration::from_millis(80) || done == total {
            last_report = Instant::now();
            let _ = app.emit("update-progress", UpdateProgress { done, total });
        }
    })
    .await?;
    launch_installer(&installer, true)?;
    app.exit(0);
    Ok(())
}
