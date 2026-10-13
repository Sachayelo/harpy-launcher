//! Modrinth App and CurseForge keep their profiles in their own databases, so
//! the launcher doesn't create them: it hands the player a small modpack to
//! import (Minecraft and Fabric, no mods), finds the profile it becomes and
//! syncs the pack into it. Neither has a pre-launch hook: the pack updates when
//! played from here.

use super::{env_dir, protocol_handler, spawn, LauncherKind};
use crate::config::Target;
use crate::manifest::Manifest;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum App {
    Modrinth,
    CurseForge,
}

impl App {
    pub fn of(kind: LauncherKind) -> App {
        if kind == LauncherKind::Curseforge {
            App::CurseForge
        } else {
            App::Modrinth
        }
    }

    fn name(self) -> &'static str {
        match self {
            App::Modrinth => "Modrinth App",
            App::CurseForge => "CurseForge",
        }
    }
}

fn data_dir(app: App) -> Option<PathBuf> {
    match app {
        App::Modrinth => {
            let roaming = env_dir("APPDATA")?;
            ["ModrinthApp", "com.modrinth.theseus"]
                .map(|name| roaming.join(name))
                .into_iter()
                .find(|dir| dir.is_dir())
        }
        App::CurseForge => Some(env_dir("USERPROFILE")?.join("curseforge").join("minecraft")).filter(|dir| dir.is_dir()),
    }
}

fn profiles_dir(app: App) -> Option<PathBuf> {
    let data = data_dir(app)?;
    Some(match app {
        App::Modrinth => data.join("profiles"),
        App::CurseForge => data.join("Instances"),
    })
}

fn executable(app: App) -> Option<PathBuf> {
    let local = env_dir("LOCALAPPDATA");
    let candidates: Vec<PathBuf> = match (app, local) {
        (App::Modrinth, Some(local)) => vec![
            local.join("Modrinth App").join("Modrinth App.exe"),
            local.join("Programs").join("Modrinth App").join("Modrinth App.exe"),
        ],
        (App::CurseForge, Some(local)) => vec![local.join("Programs").join("CurseForge Windows").join("CurseForge.exe")],
        _ => Vec::new(),
    };
    let scheme = match app {
        App::Modrinth => "modrinth",
        App::CurseForge => "curseforge",
    };
    protocol_handler(scheme).or_else(|| candidates.into_iter().find(|path| path.is_file()))
}

pub fn installed(app: App) -> bool {
    data_dir(app).is_some() || executable(app).is_some()
}

/// The profile made from the import: named after the pack ("Harpy Express",
/// or "Harpy Express (1)" when that name was taken). One the launcher already
/// synced comes first.
pub fn find_profile(app: App, target: &Target) -> Option<PathBuf> {
    let wanted = target.profile_name.to_lowercase();
    let mut found: Vec<PathBuf> = fs::read_dir(profiles_dir(app)?)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let name = path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
            name == wanted || name.strip_prefix(&wanted).is_some_and(|rest| rest.starts_with(' ') || rest.starts_with('('))
        })
        .collect();
    found.sort_by_key(|path| std::cmp::Reverse(path.join(".harpy").join("state.json").is_file()));
    found.into_iter().next()
}

pub fn open(app: App) -> Result<(), String> {
    let executable = executable(app).ok_or_else(|| format!("{} est introuvable sur ce PC.", app.name()))?;
    spawn(&executable, &[])
}

/// What the player has to do once the import file is written.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// Opened in Modrinth App, or shown in the Explorer for CurseForge.
    opened: bool,
    path: String,
}

/// Writes the modpack to import and hands it over: Modrinth App opens .mrpack
/// files; CurseForge imports zips from its own menu.
pub fn start_import(app: App, target: &Target, manifest: &Manifest) -> Result<Import, String> {
    let folder = match app {
        App::Modrinth => std::env::temp_dir().join("harpy-launcher"),
        App::CurseForge => env_dir("USERPROFILE").map(|home| home.join("Downloads")).filter(|dir| dir.is_dir())
            .unwrap_or_else(|| std::env::temp_dir().join("harpy-launcher")),
    };
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = match app {
        App::Modrinth => folder.join(format!("{}.mrpack", target.profile_name)),
        App::CurseForge => folder.join(format!("{} (CurseForge).zip", target.profile_name)),
    };
    write_pack(app, &path, target, manifest)?;

    let opened = match app {
        App::Modrinth => open::that(&path).is_ok(),
        App::CurseForge => {
            let _ = open(app);
            false
        }
    };
    if !opened {
        let _ = Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
    }
    Ok(Import { opened, path: path.display().to_string() })
}

fn write_pack(app: App, path: &Path, target: &Target, manifest: &Manifest) -> Result<(), String> {
    let (minecraft, loader) = (manifest.minecraft(), manifest.loader_version());
    let (entry, index) = match app {
        App::Modrinth => (
            "modrinth.index.json",
            serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": manifest.version,
                "name": target.profile_name,
                "summary": "Les mods arrivent avec Harpy Launcher.",
                "files": [],
                "dependencies": { "minecraft": minecraft, "fabric-loader": loader }
            }),
        ),
        App::CurseForge => (
            "manifest.json",
            serde_json::json!({
                "minecraft": {
                    "version": minecraft,
                    "modLoaders": [{ "id": format!("fabric-{loader}"), "primary": true }]
                },
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": target.profile_name,
                "version": manifest.version,
                "author": "Sachayelo",
                "files": [],
                "overrides": "overrides"
            }),
        ),
    };
    let file = fs::File::create(path).map_err(|e| format!("Impossible d'écrire {} : {e}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file(entry, options).map_err(|e| e.to_string())?;
    let text = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
    zip.write_all(&text).map_err(|e| e.to_string())?;
    zip.add_directory("overrides/", options).map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}
