//! The launchers players can play from. The pack is synced into a game folder
//! that belongs to one of them: each knows how to find or create that folder,
//! whether it updates the pack by itself before starting the game (Lunar and
//! Prism run the launcher's sync first), and how to start the game.

pub mod apps;
pub mod official;
pub mod prism;

use crate::config::{Settings, Target};
use crate::lunar;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LauncherKind {
    #[default]
    Lunar,
    Official,
    Prism,
    Modrinth,
    Curseforge,
    Custom,
}

impl LauncherKind {
    pub const ALL: [LauncherKind; 6] = [
        LauncherKind::Lunar,
        LauncherKind::Official,
        LauncherKind::Prism,
        LauncherKind::Modrinth,
        LauncherKind::Curseforge,
        LauncherKind::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            LauncherKind::Lunar => "lunar",
            LauncherKind::Official => "official",
            LauncherKind::Prism => "prism",
            LauncherKind::Modrinth => "modrinth",
            LauncherKind::Curseforge => "curseforge",
            LauncherKind::Custom => "custom",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LauncherKind::Lunar => "Lunar Client",
            LauncherKind::Official => "Launcher Minecraft",
            LauncherKind::Prism => "Prism Launcher",
            LauncherKind::Modrinth => "Modrinth App",
            LauncherKind::Curseforge => "CurseForge",
            LauncherKind::Custom => "Autre launcher",
        }
    }

    /// Whether starting the game from the launcher itself keeps the pack up to date.
    pub fn syncs_itself(self) -> bool {
        matches!(self, LauncherKind::Lunar | LauncherKind::Prism)
    }

    fn download_page(self) -> Option<&'static str> {
        match self {
            LauncherKind::Lunar => Some("https://www.lunarclient.com/download"),
            LauncherKind::Official => Some(official::DOWNLOAD_PAGE),
            LauncherKind::Prism => Some("https://prismlauncher.org/download/windows/"),
            LauncherKind::Modrinth => Some("https://modrinth.com/app"),
            LauncherKind::Curseforge => Some("https://www.curseforge.com/download/app"),
            LauncherKind::Custom => None,
        }
    }
}

/// What the player still has to do before the pack can be synced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Setup {
    /// The launcher isn't installed.
    Missing,
    /// Lunar is installed but was never opened, so it has no profiles yet.
    NeverOpened,
    /// Modrinth and CurseForge: the profile is to be imported. Any other launcher: a folder is to be picked.
    NeedsProfile,
    Ready,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherState {
    pub kind: LauncherKind,
    pub name: &'static str,
    pub setup: Setup,
    /// Where the pack goes, once known.
    pub game_dir: Option<String>,
    pub syncs_itself: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherInfo {
    kind: LauncherKind,
    name: &'static str,
    installed: bool,
}

pub fn installed(kind: LauncherKind, settings: &Settings) -> bool {
    match kind {
        LauncherKind::Lunar => lunar::state() != lunar::LunarState::Missing,
        LauncherKind::Official => official::installed(),
        LauncherKind::Prism => prism::find(settings).is_some(),
        LauncherKind::Modrinth => apps::installed(apps::App::Modrinth),
        LauncherKind::Curseforge => apps::installed(apps::App::CurseForge),
        LauncherKind::Custom => true,
    }
}

/// A folder the player picked for this launcher and channel.
fn picked(settings: &Settings, kind: LauncherKind, target: &Target) -> Option<PathBuf> {
    settings
        .folders
        .get(&folder_key(kind, target))
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
}

pub fn folder_key(kind: LauncherKind, target: &Target) -> String {
    format!("{}/{}", kind.key(), target.channel)
}

/// Where the pack is synced, and what is missing when it can't be yet.
pub fn locate(kind: LauncherKind, target: &Target, settings: &Settings) -> (Setup, Option<PathBuf>) {
    match kind {
        LauncherKind::Lunar => match lunar::state() {
            lunar::LunarState::Missing => (Setup::Missing, None),
            lunar::LunarState::NeverOpened => (Setup::NeverOpened, None),
            lunar::LunarState::Ready => match lunar::profile_dir(&target.profile_path) {
                Some(dir) => (Setup::Ready, Some(dir)),
                None => (Setup::Missing, None),
            },
        },
        LauncherKind::Official => match official::game_dir(target).filter(|_| official::installed()) {
            Some(dir) => (Setup::Ready, Some(dir)),
            None => (Setup::Missing, None),
        },
        LauncherKind::Prism => match prism::find(settings) {
            Some(install) => (Setup::Ready, Some(prism::game_dir(&install, target))),
            None => (Setup::Missing, None),
        },
        LauncherKind::Modrinth | LauncherKind::Curseforge => {
            let app = apps::App::of(kind);
            if let Some(dir) = picked(settings, kind, target).or_else(|| apps::find_profile(app, target)) {
                (Setup::Ready, Some(dir))
            } else if apps::installed(app) {
                (Setup::NeedsProfile, None)
            } else {
                (Setup::Missing, None)
            }
        }
        LauncherKind::Custom => match picked(settings, kind, target) {
            Some(dir) => (Setup::Ready, Some(dir)),
            None => (Setup::NeedsProfile, None),
        },
    }
}

pub fn state(target: &Target) -> LauncherState {
    let settings = Settings::load();
    let kind = settings.launcher;
    let (setup, dir) = locate(kind, target, &settings);
    LauncherState {
        kind,
        name: kind.name(),
        setup,
        game_dir: dir.map(|dir| dir.display().to_string()),
        syncs_itself: kind.syncs_itself(),
    }
}

#[tauri::command]
pub fn launcher_state() -> LauncherState {
    state(&Target::current())
}

#[tauri::command]
pub async fn launchers() -> Result<Vec<LauncherInfo>, String> {
    crate::blocking(|| {
        let settings = Settings::load();
        Ok(LauncherKind::ALL
            .iter()
            .map(|&kind| LauncherInfo {
                kind,
                name: kind.name(),
                installed: installed(kind, &settings),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub fn set_launcher(kind: LauncherKind) -> Result<LauncherState, String> {
    let mut settings = Settings::load();
    settings.launcher = kind;
    settings.save()?;
    Ok(launcher_state())
}

#[tauri::command]
pub fn download_launcher(kind: LauncherKind) -> Result<(), String> {
    let page = kind.download_page().ok_or("Pas de page de téléchargement pour ce choix.")?;
    open::that(page).map_err(|e| format!("Impossible d'ouvrir le navigateur : {e}"))
}

/// The player points the launcher at a folder: the game folder of any other
/// launcher, a Modrinth or CurseForge profile it didn't find, or a MultiMC folder.
#[tauri::command]
pub async fn choose_folder(app: tauri::AppHandle, kind: LauncherKind) -> Result<Option<LauncherState>, String> {
    let title = match kind {
        LauncherKind::Prism => "Choisis le dossier de MultiMC ou de Prism Launcher",
        LauncherKind::Modrinth | LauncherKind::Curseforge => "Choisis le dossier du profil Harpy Express",
        _ => "Choisis le dossier du jeu (celui qui contient le dossier mods)",
    };
    let owner = crate::dialogs::owner(&app);
    let Some(dir) = crate::blocking(move || crate::dialogs::pick_folder(owner, title)).await? else {
        return Ok(None);
    };
    let target = Target::current();
    let mut settings = Settings::load();
    let key = match kind {
        LauncherKind::Prism => {
            if prism::at(&dir).is_none() {
                return Err("Ce dossier n'est ni MultiMC ni Prism Launcher (on y cherche MultiMC.exe ou prismlauncher.exe).".into());
            }
            "prism".to_string()
        }
        LauncherKind::Lunar | LauncherKind::Official => return Err("Ce launcher n'a pas besoin de dossier.".into()),
        _ => folder_key(kind, &target),
    };
    if !dir.is_absolute() {
        return Err("Dossier invalide.".into());
    }
    settings.folders.insert(key, dir.display().to_string());
    settings.launcher = kind;
    settings.save()?;
    Ok(Some(state(&target)))
}

/// Recommended Java heap for the pack, from the PC's memory.
pub fn memory_mb() -> u64 {
    let mut system = System::new();
    system.refresh_memory();
    let total_gb = system.total_memory() as f64 / 1_073_741_824.0;
    if total_gb >= 15.0 {
        6144
    } else if total_gb >= 11.0 {
        5120
    } else if total_gb >= 7.5 {
        4096
    } else {
        3072
    }
}

fn processes(with_cmd: bool) -> System {
    let mut system = System::new();
    let kind = if with_cmd {
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::Always)
    } else {
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always)
    };
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    system
}

pub fn process_running(names: &[&str]) -> bool {
    processes(false)
        .processes()
        .values()
        .any(|process| names.iter().any(|name| process.name().eq_ignore_ascii_case(OsStr::new(name))))
}

/// A Minecraft started on this game folder, whatever launched it.
pub fn game_running_in(dir: &Path) -> bool {
    let wanted = normalize(&dir.display().to_string());
    processes(true).processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        (name == "javaw.exe" || name == "java.exe")
            && process
                .cmd()
                .iter()
                .any(|arg| normalize(&arg.to_string_lossy()).contains(&wanted))
    })
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// Wherever a launcher was installed, its installer registers it as the handler of its links.
#[cfg(windows)]
pub fn protocol_handler(scheme: &str) -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].into_iter().find_map(|root| {
        let command: String = RegKey::predef(root)
            .open_subkey(format!(r"Software\Classes\{scheme}\shell\open\command"))
            .ok()?
            .get_value("")
            .ok()?;
        let command = command.trim();
        let path = match command.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next()?,
            None => &command[..command.to_lowercase().find(".exe")? + 4],
        };
        Some(PathBuf::from(path)).filter(|path| path.is_file())
    })
}

#[cfg(not(windows))]
pub fn protocol_handler(_scheme: &str) -> Option<PathBuf> {
    None
}

pub fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

pub fn spawn(program: &Path, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible d'ouvrir {} : {e}", program.display()))
}

/// Forward slashes: other launchers read them everywhere, and they need no escaping.
pub fn slashes(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Compares dotted version numbers ("0.18.4").
pub fn version_at_least(version: &str, minimum: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> { text.split('.').map(|part| part.parse().unwrap_or(0)).collect() };
    parse(version) >= parse(minimum)
}

pub fn wait_until(timeout: std::time::Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    condition()
}
