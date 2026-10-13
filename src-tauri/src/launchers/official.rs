//! The official Minecraft launcher (Microsoft Store / Xbox app or the classic
//! installer). The pack gets a game folder of its own and a profile running a
//! Fabric version that joins the server as soon as the game starts. That
//! launcher has no pre-launch hook: the pack updates when played from here.

use super::{env_dir, process_running, spawn};
use crate::config::{Target, SERVER_HOST};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DOWNLOAD_PAGE: &str = "https://www.minecraft.net/fr-fr/download";
const STORE_PACKAGE: &str = "Microsoft.4297127D64EC6_8wekyb3d8bbwe";
const ICON: &[u8] = include_bytes!("../../../src/assets/icon.png");
/// Mojang's default flags, with the heap the PC can afford.
const JAVA_FLAGS: &str = "-XX:+UnlockExperimentalVMOptions -XX:+UseG1GC -XX:G1NewSizePercent=20 -XX:G1ReservePercent=20 -XX:MaxGCPauseMillis=50 -XX:G1HeapRegionSize=32M";

pub fn minecraft_dir() -> Option<PathBuf> {
    Some(env_dir("APPDATA")?.join(".minecraft"))
}

fn classic_executable() -> Option<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(|var| env_dir(var))
        .flat_map(|root| {
            ["Minecraft Launcher", "Minecraft"]
                .map(|folder| root.join(folder).join("MinecraftLauncher.exe"))
        })
        .find(|path| path.is_file())
}

fn store_installed() -> bool {
    env_dir("LOCALAPPDATA").is_some_and(|dir| dir.join("Packages").join(STORE_PACKAGE).is_dir())
}

pub fn installed() -> bool {
    classic_executable().is_some()
        || store_installed()
        || minecraft_dir().is_some_and(|dir| dir.join("launcher_profiles.json").is_file())
}

pub fn launcher_running() -> bool {
    process_running(&["MinecraftLauncher.exe", "Minecraft.exe"])
}

pub fn game_dir(target: &Target) -> Option<PathBuf> {
    Some(minecraft_dir()?.join("harpy").join(&target.profile_path))
}

fn version_id(target: &Target, loader: &str) -> String {
    format!("{}-fabric-{loader}", target.profile_path)
}

fn version_file(target: &Target, loader: &str) -> Option<PathBuf> {
    let id = version_id(target, loader);
    Some(minecraft_dir()?.join("versions").join(&id).join(format!("{id}.json")))
}

fn read_profiles() -> Result<Value, String> {
    let path = minecraft_dir().ok_or("Dossier .minecraft introuvable.")?.join("launcher_profiles.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| "launcher_profiles.json est illisible.".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({ "profiles": {} })),
        Err(e) => Err(format!("launcher_profiles.json est illisible : {e}")),
    }
}

fn profile_matches(profile: Option<&Value>, id: &str, game_dir: &Path) -> bool {
    profile.is_some_and(|profile| {
        profile.get("lastVersionId").and_then(Value::as_str) == Some(id)
            && profile.get("gameDir").and_then(Value::as_str) == Some(game_dir.to_string_lossy().as_ref())
    })
}

/// The Fabric version is installed and the profile points at it and at the pack's folder.
pub fn registered(target: &Target, loader: &str) -> bool {
    let (Some(file), Some(dir)) = (version_file(target, loader), game_dir(target)) else {
        return false;
    };
    file.is_file()
        && read_profiles().is_ok_and(|profiles| {
            profile_matches(
                profiles.get("profiles").and_then(|p| p.get(&target.profile_path)),
                &version_id(target, loader),
                &dir,
            )
        })
}

/// Installs the Fabric version (from Fabric's own servers, as their installer
/// does) and the profile. The launcher must be closed to take a new profile.
pub async fn install(client: &reqwest::Client, target: &Target, minecraft: &str, loader: &str) -> Result<(), String> {
    let file = version_file(target, loader).ok_or("Dossier .minecraft introuvable.")?;
    if !file.is_file() {
        let url = format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft}/{loader}/profile/json");
        let body = client
            .get(&url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|e| format!("Fabric est injoignable : {e}"))?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;
        let mut version: Value = serde_json::from_slice(&body).map_err(|_| "Réponse de Fabric illisible.".to_string())?;
        let id = version_id(target, loader);
        version["id"] = json!(id);
        // Quick Play: the game goes straight to the server, like Lunar does.
        let address = format!("{SERVER_HOST}:{}", target.server_port);
        let arguments = version
            .as_object_mut()
            .ok_or("Réponse de Fabric illisible.")?
            .entry("arguments")
            .or_insert_with(|| json!({}));
        let game = arguments
            .as_object_mut()
            .ok_or("Réponse de Fabric illisible.")?
            .entry("game")
            .or_insert_with(|| json!([]));
        if let Some(game) = game.as_array_mut() {
            game.push(json!("--quickPlayMultiplayer"));
            game.push(json!(address));
        }
        let folder = file.parent().ok_or("Dossier de version invalide.")?;
        fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&version).map_err(|e| e.to_string())?;
        fs::write(&file, text).map_err(|e| format!("Impossible d'installer Fabric : {e}"))?;
        // The launcher wants a jar next to every version; Fabric's installer leaves an empty one too.
        fs::write(folder.join(format!("{id}.jar")), b"").map_err(|e| e.to_string())?;
    }

    let (target, loader) = (target.clone(), loader.to_owned());
    crate::blocking(move || ensure_profile(&target, &loader)).await
}

fn ensure_profile(target: &Target, loader: &str) -> Result<(), String> {
    let id = version_id(target, loader);
    let dir = game_dir(target).ok_or("Dossier .minecraft introuvable.")?;
    fs::create_dir_all(dir.join("mods")).map_err(|e| e.to_string())?;
    let mut profiles = read_profiles()?;
    let existing = profiles.get("profiles").and_then(|p| p.get(&target.profile_path)).cloned();
    let running = launcher_running();
    if running && profile_matches(existing.as_ref(), &id, &dir) {
        return Ok(());
    }
    // Rewritten even when up to date, to mark it used last: the launcher opens on it.
    if running {
        return Err(format!(
            "Ferme le launcher Minecraft puis clique à nouveau sur Jouer : il doit être fermé pour recevoir le profil {}.",
            target.profile_name
        ));
    }

    let now = crate::clock::iso(crate::clock::now_secs());
    let mut profile = existing.unwrap_or_else(|| json!({ "created": now }));
    let fields = profile.as_object_mut().ok_or("launcher_profiles.json est illisible.")?;
    fields.insert("name".into(), json!(target.profile_name));
    fields.insert("type".into(), json!("custom"));
    fields.insert("lastUsed".into(), json!(now));
    fields.insert("lastVersionId".into(), json!(id));
    fields.insert("gameDir".into(), json!(dir.to_string_lossy()));
    fields.insert("icon".into(), json!(format!("data:image/png;base64,{}", STANDARD.encode(ICON))));
    fields
        .entry("javaArgs")
        .or_insert_with(|| json!(format!("-Xmx{}M {JAVA_FLAGS}", super::memory_mb())));

    let root = profiles.as_object_mut().ok_or("launcher_profiles.json est illisible.")?;
    let list = root.entry("profiles").or_insert_with(|| json!({}));
    list.as_object_mut()
        .ok_or("launcher_profiles.json est illisible.")?
        .insert(target.profile_path.clone(), profile);

    let path = minecraft_dir().ok_or("Dossier .minecraft introuvable.")?.join("launcher_profiles.json");
    let text = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.harpy-tmp");
    fs::write(&temporary, text).map_err(|e| format!("launcher_profiles.json non modifiable : {e}"))?;
    fs::rename(&temporary, &path).map_err(|e| format!("launcher_profiles.json non modifiable : {e}"))
}

/// Opens the launcher; the profile was used last, so it is the one selected.
pub fn open() -> Result<(), String> {
    if let Some(executable) = classic_executable() {
        return spawn(&executable, &[]);
    }
    if store_installed() {
        return Command::new("explorer.exe")
            .arg(format!(r"shell:AppsFolder\{STORE_PACKAGE}!Minecraft"))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Impossible d'ouvrir le launcher Minecraft : {e}"));
    }
    Err("Le launcher Minecraft est introuvable sur ce PC.".into())
}
