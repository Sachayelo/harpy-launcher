//! Brings a Lunar profile folder in line with a pack manifest.
//!
//! Rules:
//! - a mod is replaced whenever its content differs from the manifest;
//! - a config is replaced only when the pack changed it since the last sync,
//!   so a player's own tweaks survive updates that don't touch that file;
//! - any jar whose mod id belongs to the pack (or is blocked, on prod only) but
//!   whose file is not the pack's own copy is removed, which clears leftovers
//!   and duplicates;
//! - a resource pack that gets installed or updated is switched on in
//!   options.txt, touching only the `resourcePacks` line (keybinds stay intact);
//! - everything else in the profile belongs to the player and is never touched.

use crate::manifest::{Manifest, PackFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ROOTS: [&str; 4] = ["mods", "config", "resourcepacks", "shaderpacks"];
const STATE_DIR: &str = ".harpy";

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub version: Option<String>,
    pub channel: Option<String>,
    /// Pack path -> hash of the pack's copy as of the last successful sync.
    #[serde(default)]
    pub installed: HashMap<String, String>,
    /// Pack path -> hash of the file on disk, reused while size and date are unchanged.
    #[serde(default)]
    pub cache: HashMap<String, CachedHash>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedHash {
    size: u64,
    modified: u64,
    sha256: String,
}

pub struct Plan {
    pub downloads: Vec<PackFile>,
    pub removals: Vec<String>,
    pub download_bytes: u64,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.downloads.is_empty() && self.removals.is_empty()
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub file: String,
    pub index: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub fn load_state(profile: &Path) -> State {
    fs::read(profile.join(STATE_DIR).join("state.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_state(profile: &Path, state: &State) -> Result<(), String> {
    let dir = profile.join(STATE_DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    let temporary = dir.join("state.json.tmp");
    fs::write(&temporary, json).map_err(|e| e.to_string())?;
    fs::rename(&temporary, dir.join("state.json")).map_err(|e| e.to_string())
}

pub fn append_log(profile: &Path, line: &str) {
    let dir = profile.join(STATE_DIR);
    let _ = fs::create_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(dir.join("sync.log")) {
        let _ = writeln!(file, "[{stamp}] {line}");
    }
}

pub fn plan(profile: &Path, manifest: &Manifest, state: &mut State) -> Result<Plan, String> {
    let mut downloads = Vec::new();
    let mut pack_paths = HashSet::new();

    for file in &manifest.files {
        let relative = safe_relative(&file.path)
            .ok_or_else(|| format!("Chemin refusé dans le manifeste : {}", file.path))?;
        pack_paths.insert(file.path.as_str());

        let local = cached_sha256(&profile.join(relative), &file.path, &mut state.cache)
            .map_err(|e| format!("Lecture de {} impossible : {e}", file.path))?;
        let keep = match local.as_deref() {
            Some(sha) if sha == file.sha256 => true,
            Some(_) if !is_mod(&file.path) => state.installed.get(&file.path) == Some(&file.sha256),
            _ => false,
        };
        if !keep {
            downloads.push(file.clone());
        }
    }

    let mut removals: Vec<String> = state
        .installed
        .keys()
        .filter(|path| is_mod(path) && !pack_paths.contains(path.as_str()))
        .filter(|path| profile.join(path).exists())
        .cloned()
        .collect();

    // Blocked mods are only enforced on prod, so tools like freecam stay usable
    // in development profiles.
    let blocked = manifest
        .blocked_mods
        .iter()
        .filter(|_| manifest.channel == "prod")
        .map(String::as_str);
    let owned_ids: HashSet<&str> = manifest
        .files
        .iter()
        .filter_map(|file| file.mod_id.as_deref())
        .chain(blocked)
        .collect();
    if let Ok(entries) = fs::read_dir(profile.join("mods")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("mods/{name}");
            if !name.to_ascii_lowercase().ends_with(".jar")
                || pack_paths.contains(path.as_str())
                || removals.contains(&path)
            {
                continue;
            }
            if mod_id(&entry.path()).is_some_and(|id| owned_ids.contains(id.as_str())) {
                removals.push(path);
            }
        }
    }

    let download_bytes = downloads.iter().map(|file| file.size).sum();
    Ok(Plan { downloads, removals, download_bytes })
}

pub async fn apply<F: FnMut(Progress)>(
    client: &reqwest::Client,
    profile: &Path,
    manifest: &Manifest,
    plan: &Plan,
    state: &mut State,
    mut report: F,
) -> Result<(), String> {
    for path in &plan.removals {
        match fs::remove_file(profile.join(path)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Impossible de supprimer {path} : {e}")),
        }
        state.installed.remove(path);
        state.cache.remove(path);
    }

    let total = plan.downloads.len();
    let mut bytes_done = 0;
    for (index, file) in plan.downloads.iter().enumerate() {
        let relative = safe_relative(&file.path)
            .ok_or_else(|| format!("Chemin refusé dans le manifeste : {}", file.path))?;
        let destination = profile.join(relative);
        download(client, file, &destination, |chunk| {
            bytes_done += chunk;
            report(Progress {
                file: file.path.clone(),
                index: index + 1,
                total,
                bytes_done,
                bytes_total: plan.download_bytes,
            });
        })
        .await?;
        if let Ok(metadata) = fs::metadata(&destination) {
            state.cache.insert(
                file.path.clone(),
                CachedHash {
                    size: metadata.len(),
                    modified: modified_millis(&metadata),
                    sha256: file.sha256.clone(),
                },
            );
        }
    }

    let updated_packs: HashSet<&str> = plan
        .downloads
        .iter()
        .filter_map(|file| file.path.strip_prefix("resourcepacks/")?.split('/').next())
        .collect();
    enable_resource_packs(profile, &updated_packs);

    let pack_paths: HashSet<&str> = manifest.files.iter().map(|file| file.path.as_str()).collect();
    state.installed.retain(|path, _| pack_paths.contains(path.as_str()));
    for file in &manifest.files {
        state.installed.insert(file.path.clone(), file.sha256.clone());
    }
    state.version = Some(manifest.version.clone());
    state.channel = Some(manifest.channel.clone());
    Ok(())
}

async fn download<F: FnMut(u64)>(
    client: &reqwest::Client,
    file: &PackFile,
    destination: &Path,
    mut on_chunk: F,
) -> Result<(), String> {
    let name = file.path.rsplit('/').next().unwrap_or(&file.path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let partial = PathBuf::from(format!("{}.harpy-part", destination.display()));

    let mut response = client
        .get(&file.url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Téléchargement de {name} impossible : {e}"))?;

    let mut output = fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Téléchargement de {name} interrompu : {e}"))?
    {
        hasher.update(&chunk);
        output.write_all(&chunk).map_err(|e| e.to_string())?;
        written += chunk.len() as u64;
        on_chunk(chunk.len() as u64);
    }
    output.flush().map_err(|e| e.to_string())?;
    drop(output);

    if written != file.size || format!("{:x}", hasher.finalize()) != file.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!("{name} est arrivé corrompu. Réessaie."));
    }
    fs::rename(&partial, destination).map_err(|e| format!("Impossible d'installer {name} : {e}"))
}

/// Adds the packs to the game's enabled resource packs. Best effort: an
/// unreadable options.txt is left alone rather than risking the player's settings.
fn enable_resource_packs(profile: &Path, packs: &HashSet<&str>) {
    if packs.is_empty() {
        return;
    }
    let path = profile.join("options.txt");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(_) => return,
    };
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();

    const KEY: &str = "resourcePacks:";
    let index = lines.iter().position(|line| line.starts_with(KEY));
    let mut enabled: Vec<String> = match index {
        Some(i) => match serde_json::from_str(&lines[i][KEY.len()..]) {
            Ok(enabled) => enabled,
            Err(_) => return,
        },
        None => vec!["vanilla".into(), "fabric".into()],
    };

    let before = enabled.len();
    for pack in packs {
        let id = format!("file/{pack}");
        if !enabled.contains(&id) {
            enabled.push(id);
        }
    }
    if enabled.len() == before {
        return;
    }

    let Ok(list) = serde_json::to_string(&enabled) else {
        return;
    };
    let line = format!("{KEY}{list}");
    match index {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    let _ = fs::write(&path, lines.join(newline) + newline);
}

/// Accepts only plain relative paths under a folder the pack is allowed to manage.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    let mut components = relative.components();
    let root = match components.next()? {
        Component::Normal(root) => root.to_str()?,
        _ => return None,
    };
    let rest: Vec<Component> = components.collect();
    let valid = ROOTS.contains(&root)
        && !rest.is_empty()
        && rest.iter().all(|component| matches!(component, Component::Normal(_)));
    valid.then(|| relative.to_path_buf())
}

fn is_mod(path: &str) -> bool {
    path.starts_with("mods/")
}

fn modified_millis(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn cached_sha256(
    path: &Path,
    key: &str,
    cache: &mut HashMap<String, CachedHash>,
) -> io::Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let size = metadata.len();
    let modified = modified_millis(&metadata);
    if let Some(cached) = cache.get(key) {
        if cached.size == size && cached.modified == modified {
            return Ok(Some(cached.sha256.clone()));
        }
    }

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let sha256 = format!("{:x}", hasher.finalize());
    cache.insert(
        key.to_string(),
        CachedHash { size, modified, sha256: sha256.clone() },
    );
    Ok(Some(sha256))
}

fn mod_id(jar: &Path) -> Option<String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(jar).ok()?).ok()?;
    let mut text = String::new();
    archive
        .by_name("fabric.mod.json")
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => json.get("id")?.as_str().map(str::to_owned),
        // Some mods ship fabric.mod.json with raw control characters that strict
        // JSON rejects; the id is always a simple string, so read it directly.
        Err(_) => {
            let after_key = &text[text.find("\"id\"")? + 4..];
            let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
            let value = after_colon.strip_prefix('"')?;
            Some(value[..value.find('"')?].to_string())
        }
    }
}
