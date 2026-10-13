//! The workshop: publishing the pack and saving code from the launcher. Only
//! available on the machine that holds the harpy-pack repository.

use crate::config::PACK_REPO;
use crate::manifest::{Change, Release};
use crate::news::NewsFile;
use base64::{engine::general_purpose::STANDARD, Engine};
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
    /// Commits since the online version, `None` when it can't be told.
    unreleased: Option<usize>,
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

/// A patch note as written in the workshop.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotes {
    #[serde(default)]
    title: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    changes: Vec<Change>,
}

/// Runs a pack script on a JSON file holding its input.
fn run_with_file<T: Serialize>(app: &AppHandle, script: &str, flag: &str, data: &T) -> Result<(), String> {
    let file = std::env::temp_dir().join(format!("harpy-{}.json", uuid::Uuid::new_v4()));
    fs::write(&file, serde_json::to_vec(data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().into_owned();
    let result = run_pack_script(Some(app), script, &["-Yes", flag, &path]);
    let _ = fs::remove_file(&file);
    result.map(|_| ())
}

/// Publishes pack/ on dev. The note goes into dev's journal: it reaches players
/// through the prod patch note only.
#[tauri::command]
pub async fn publish_pack(app: AppHandle, notes: ReleaseNotes) -> Result<(), String> {
    crate::blocking(move || run_with_file(&app, "publish.ps1", "-ChangesFile", &notes)).await
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotePreview {
    dev_version: String,
    prod_version: Option<String>,
    nothing: bool,
    /// What was published on dev since the version in prod, newest first.
    #[serde(default)]
    pending: Vec<Release>,
}

#[tauri::command]
pub async fn promote_preview() -> Result<PromotePreview, String> {
    crate::blocking(|| {
        let lines = run_pack_script(None, "promote.ps1", &["-DryRun", "-Json"])?;
        let json = lines
            .iter()
            .rev()
            .find_map(|line| line.strip_prefix("HARPY_JSON:"))
            .ok_or("Le script de passage en prod a répondu de façon inattendue.")?;
        serde_json::from_str(json).map_err(|e| e.to_string())
    })
    .await
}

/// Sends dev's version to every player, with the patch note they will read.
#[tauri::command]
pub async fn promote_pack(app: AppHandle, notes: ReleaseNotes) -> Result<(), String> {
    crate::blocking(move || run_with_file(&app, "promote.ps1", "-ChangesFile", &notes)).await
}

/// The announcements as last published from this PC.
#[tauri::command]
pub async fn news_draft() -> Result<NewsFile, String> {
    crate::blocking(|| {
        let path = pack_repo().ok_or("Dépôt harpy-pack introuvable.")?.join("channels").join("news.json");
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("news.json illisible : {e}")),
            Err(_) => Ok(NewsFile::default()),
        }
    })
    .await
}

#[tauri::command]
pub async fn publish_news(app: AppHandle, news: NewsFile) -> Result<(), String> {
    crate::blocking(move || run_with_file(&app, "announce.ps1", "-File", &news)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedImage {
    /// Where players load it from once pushed.
    url: String,
    /// The image itself, for the workshop's preview until then.
    preview: String,
}

/// Copies an image into harpy-pack/news; it goes online with the next prod or announcement.
#[tauri::command]
pub async fn stage_news_image(app: AppHandle) -> Result<Option<StagedImage>, String> {
    let owner = crate::dialogs::owner(&app);
    crate::blocking(move || {
        let filter = ("Images", "*.png;*.jpg;*.jpeg;*.webp;*.gif");
        let Some(source) = crate::dialogs::pick_file(owner, "Image de la note de version", filter)? else {
            return Ok(None);
        };
        let bytes = fs::read(&source).map_err(|e| e.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Image trop lourde : 4 Mo au plus (chaque joueur la télécharge).".into());
        }
        let extension = source
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .filter(|ext| ["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()))
            .ok_or("Format d'image non pris en charge.")?;
        let stem: String = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let name = format!("{}-{}.{extension}", crate::clock::now_secs(), stem.trim_matches('-'));
        let folder = pack_repo().ok_or("Dépôt harpy-pack introuvable.")?.join("news");
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        fs::write(folder.join(&name), &bytes).map_err(|e| e.to_string())?;
        let mime = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/png",
        };
        Ok(Some(StagedImage {
            url: format!("https://raw.githubusercontent.com/{PACK_REPO}/main/news/{name}"),
            preview: format!("data:{mime};base64,{}", STANDARD.encode(&bytes)),
        }))
    })
    .await
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

/// Commits made to the launcher since `version` was released.
fn commits_since(repo: &Path, version: &str) -> Option<usize> {
    // Release tags are created on GitHub, not locally.
    let _ = git(repo, &["fetch", "--tags", "--quiet"]);
    git(repo, &["rev-list", "--count", &format!("v{version}..HEAD")])
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[tauri::command]
pub async fn launcher_release() -> Result<LauncherRelease, String> {
    let repo = launcher_repo().ok_or("Dépôt harpy-launcher introuvable.")?;
    let online = crate::updater::latest(&crate::manifest::http_client())
        .await?
        .map(|release| release.version);
    let source = source_version(&repo);
    let unreleased = match online.clone() {
        Some(online) => {
            let repo = repo.clone();
            crate::blocking(move || Ok(commits_since(&repo, &online))).await?
        }
        None => None,
    };
    let next = source
        .as_deref()
        .and_then(|source| next_version(source, online.as_deref()));
    Ok(LauncherRelease {
        source,
        online,
        next,
        unreleased,
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
