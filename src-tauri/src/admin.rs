//! The workshop: publishing the pack and saving code from the launcher. Only
//! available on the machine that holds the harpy-pack repository.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

const OWNER: &str = "Sachayelo";
const REPOSITORIES: [&str; 9] = [
    "harpy-launcher",
    "harpy-pack",
    "gexpress-main",
    "Wathe-Extended",
    "wathe",
    "NoellesRoles",
    "KinsWathe",
    "StarryExpress",
    "StupidExpress-main",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    name: String,
    tracked: bool,
    branch: Option<String>,
    owned: bool,
    changes: Vec<String>,
    change_count: usize,
    unpushed: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    version: String,
    nothing: bool,
    #[serde(default)]
    added: Vec<String>,
    #[serde(default)]
    updated: Vec<String>,
    #[serde(default)]
    removed: Vec<String>,
    #[serde(default)]
    blocked: Vec<String>,
    #[serde(default)]
    blocked_changed: bool,
    #[serde(default)]
    upload_files: u64,
    #[serde(default)]
    upload_bytes: u64,
    #[serde(default)]
    dev_version: Option<String>,
    #[serde(default)]
    prod_version: Option<String>,
}

fn desktop() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("USERPROFILE")?).join("Desktop"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherRelease {
    /// Version in the launcher's Cargo.toml.
    source: Option<String>,
    /// Version players currently get.
    online: Option<String>,
    /// Version the next release will carry.
    next: Option<String>,
    key_found: bool,
}

fn launcher_repo() -> Option<PathBuf> {
    let repo = desktop()?.join("harpy-launcher");
    repo.join("scripts").join("release.ps1").is_file().then_some(repo)
}

fn signing_key() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("USERPROFILE")?)
            .join(".harpy")
            .join("launcher-update.key"),
    )
}

fn pack_repo() -> Option<PathBuf> {
    let repo = desktop()?.join("harpy-pack");
    repo.join("scripts").join("publish.ps1").is_file().then_some(repo)
}

pub fn available() -> bool {
    pack_repo().is_some()
}

/// Runs a console tool without flashing a console window over the launcher.
fn hidden(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = hidden("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("Git est introuvable : {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn inspect(name: &str) -> Option<Repository> {
    let path = desktop()?.join(name);
    if !path.is_dir() {
        return None;
    }
    if !path.join(".git").exists() {
        return Some(Repository {
            name: name.into(),
            tracked: false,
            branch: None,
            owned: false,
            changes: Vec::new(),
            change_count: 0,
            unpushed: 0,
        });
    }

    let branch = git(&path, &["branch", "--show-current"])
        .ok()
        .map(|branch| branch.trim().to_owned())
        .filter(|branch| !branch.is_empty());
    let owned = git(&path, &["remote", "get-url", "origin"]).is_ok_and(|url| {
        let url = url.trim().to_lowercase();
        let owner = OWNER.to_lowercase();
        url.contains(&format!("github.com/{owner}/")) || url.contains(&format!("github.com:{owner}/"))
    });
    let status = git(&path, &["status", "--porcelain"]).unwrap_or_default();
    let changes: Vec<String> = status.lines().map(str::to_owned).collect();
    let unpushed = git(&path, &["rev-list", "--count", "@{upstream}..HEAD"])
        .ok()
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0);

    Some(Repository {
        name: name.into(),
        tracked: true,
        branch,
        owned,
        change_count: changes.len(),
        changes: changes.into_iter().take(40).collect(),
        unpushed,
    })
}

#[tauri::command]
pub async fn repositories() -> Result<Vec<Repository>, String> {
    crate::blocking(|| Ok(REPOSITORIES.iter().filter_map(|name| inspect(name)).collect())).await
}

#[tauri::command]
pub async fn commit_and_push(name: String, message: String) -> Result<String, String> {
    crate::blocking(move || {
        if !REPOSITORIES.contains(&name.as_str()) {
            return Err("Dépôt inconnu.".into());
        }
        let repo = inspect(&name).ok_or("Dossier introuvable.")?;
        if !repo.tracked {
            return Err("Ce dossier n'est pas encore suivi par Git.".into());
        }
        let path = desktop().ok_or("Bureau introuvable.")?.join(&name);

        if repo.change_count > 0 {
            let message = message.trim();
            if message.is_empty() {
                return Err("Écris un message qui décrit tes modifications.".into());
            }
            git(&path, &["add", "-A"])?;
            git(&path, &["commit", "-m", message])?;
        }
        if !repo.owned {
            return Ok(format!("{name} : enregistré sur ton PC (ce dépôt n'est pas à toi, rien n'est envoyé)."));
        }
        git(&path, &["push"]).map_err(|e| format!("Envoi refusé : {e}"))?;
        Ok(format!("{name} : enregistré et envoyé sur GitHub."))
    })
    .await
}

fn run_pack_script(app: Option<&AppHandle>, script: &str, args: &[&str]) -> Result<Vec<String>, String> {
    run_script(app, &pack_repo().ok_or("Dépôt harpy-pack introuvable.")?, script, args)
}

/// Runs one of a repository's scripts, forwarding each output line to the workshop.
fn run_script(app: Option<&AppHandle>, repo: &Path, script: &str, args: &[&str]) -> Result<Vec<String>, String> {
    let mut child = hidden("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(repo.join("scripts").join(script))
        .args(args)
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("PowerShell est introuvable : {e}"))?;

    let mut stderr = child.stderr.take().expect("stderr is piped");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let mut lines = Vec::new();
    let stdout = child.stdout.take().expect("stdout is piped");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(app) = app {
            if !line.starts_with("HARPY_JSON:") {
                let _ = app.emit("workshop-log", &line);
            }
        }
        lines.push(line);
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    let errors = errors.join().unwrap_or_default();
    if status.success() {
        return Ok(lines);
    }
    // PowerShell prefixes a thrown message with the script path and appends
    // position details; keep only the message itself.
    let message = errors
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.rsplit(" : ").next().unwrap_or(line).to_owned())
        .unwrap_or_else(|| format!("{script} a échoué."));
    Err(message)
}

fn channel_version(channel: &str) -> Option<String> {
    let path = pack_repo()?.join("channels").join(format!("{channel}.json"));
    let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    json.get("version")?.as_str().map(str::to_owned)
}

#[tauri::command]
pub async fn pack_preview() -> Result<PackPreview, String> {
    crate::blocking(|| {
        let lines = run_pack_script(None, "publish.ps1", &["-DryRun", "-Json"])?;
        let json = lines
            .iter()
            .rev()
            .find_map(|line| line.strip_prefix("HARPY_JSON:"))
            .ok_or("Le script de publication a répondu de façon inattendue.")?;
        let mut preview: PackPreview = serde_json::from_str(json).map_err(|e| e.to_string())?;
        preview.dev_version = channel_version("dev");
        preview.prod_version = channel_version("prod");
        Ok(preview)
    })
    .await
}

#[tauri::command]
pub async fn publish_pack(app: AppHandle, notes: Vec<String>) -> Result<(), String> {
    crate::blocking(move || {
        let notes_file = std::env::temp_dir().join("harpy-publish-notes.txt");
        fs::write(&notes_file, notes.join("\n")).map_err(|e| e.to_string())?;
        let notes_path = notes_file.to_string_lossy().into_owned();
        let result = run_pack_script(Some(&app), "publish.ps1", &["-Yes", "-NotesFile", &notes_path]);
        let _ = fs::remove_file(&notes_file);
        result.map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn promote_pack(app: AppHandle) -> Result<(), String> {
    crate::blocking(move || run_pack_script(Some(&app), "promote.ps1", &["-Yes"]).map(|_| ())).await
}

fn source_version(repo: &Path) -> Option<String> {
    let manifest = fs::read_to_string(repo.join("src-tauri").join("Cargo.toml")).ok()?;
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|version| version.trim().trim_matches('"').to_owned())
}

/// The version the next release will carry: the one in Cargo.toml, unless it
/// is already online, in which case the patch number goes up.
fn next_version(source: &str, online: Option<&str>) -> Option<String> {
    match online {
        Some(online) if !crate::updater::is_newer(source, online) => {
            let [major, minor, patch] = crate::updater::parse(online)?;
            Some(format!("{major}.{minor}.{}", patch + 1))
        }
        _ => crate::updater::parse(source).map(|_| source.to_owned()),
    }
}

#[tauri::command]
pub async fn launcher_release() -> Result<LauncherRelease, String> {
    let repo = launcher_repo().ok_or("Dépôt harpy-launcher introuvable.")?;
    let online = crate::updater::latest(&crate::manifest::http_client())
        .await?
        .map(|release| release.version);
    let source = source_version(&repo);
    let next = source
        .as_deref()
        .and_then(|source| next_version(source, online.as_deref()));
    Ok(LauncherRelease {
        source,
        online,
        next,
        key_found: signing_key().is_some_and(|key| key.is_file()),
    })
}

#[tauri::command]
pub async fn publish_launcher(app: AppHandle, version: String) -> Result<(), String> {
    crate::blocking(move || {
        if crate::updater::parse(&version).is_none() {
            return Err("Numéro de version invalide.".into());
        }
        let repo = launcher_repo().ok_or("Dépôt harpy-launcher introuvable.")?;
        run_script(Some(&app), &repo, "release.ps1", &["-Version", &version, "-Yes"]).map(|_| ())
    })
    .await
}
