//! Everything that touches Lunar Client: its profile database, its settings,
//! its processes and its `lunarclient://` links. Writes to the database and the
//! settings file only happen while the Lunar launcher is closed.

use crate::config::Target;
use serde::Serialize;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessesToUpdate, System};

const ICON: &[u8] = include_bytes!("../../src/assets/icon.png");
const FEATURED_IMAGE: &[u8] = include_bytes!("../../src/assets/featured_image.png");

/// Logged by Lunar once its metadata is loaded. A `play` link sent before that
/// is dropped ("Attempted to launch without metadata").
const READY_MARKER: &str = "[Metadata] Profile selected";

const REQUIRED_COLUMNS: [&str; 18] = [
    "id",
    "name",
    "description",
    "icon_path",
    "featured_image_path",
    "path",
    "type",
    "major_game_version",
    "game_version",
    "loaders",
    "use_lunar_features",
    "is_badlion",
    "user_modpack",
    "allocated_memory",
    "lunar_module",
    "created_at",
    "config_version",
    "pre_launch_command",
];

pub struct Registration {
    pub id: String,
    pub pre_launch_command: Option<String>,
}

pub fn lunar_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("USERPROFILE")?).join(".lunarclient");
    dir.is_dir().then_some(dir)
}

pub fn profile_dir(profile_path: &str) -> Option<PathBuf> {
    let valid = !profile_path.is_empty()
        && profile_path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| lunar_dir().map(|dir| dir.join("profiles").join(profile_path)))?
}

const DOWNLOAD_PAGE: &str = "https://www.lunarclient.com/download";

/// What the player still has to do before the launcher can use Lunar.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LunarState {
    Missing,
    /// Installed but never opened: Lunar hasn't created its profiles yet.
    NeverOpened,
    Ready,
}

pub fn state() -> LunarState {
    if lunar_executable().is_none() {
        return LunarState::Missing;
    }
    let database = lunar_dir().is_some_and(|dir| dir.join("db").join("profiles.db").is_file());
    if database {
        LunarState::Ready
    } else {
        LunarState::NeverOpened
    }
}

fn lunar_executable() -> Option<PathBuf> {
    let default = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("Programs")
        .join("Lunar Client")
        .join("Lunar Client.exe");
    if default.is_file() {
        return Some(default);
    }
    registered_executable().filter(|path| path.is_file())
}

/// Wherever Lunar was installed, its installer registers it as the handler of
/// lunarclient:// links.
#[cfg(windows)]
fn registered_executable() -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].into_iter().find_map(|root| {
        let command: String = RegKey::predef(root)
            .open_subkey(r"Software\Classes\lunarclient\shell\open\command")
            .ok()?
            .get_value("")
            .ok()?;
        let command = command.trim();
        let path = match command.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next()?,
            None => &command[..command.to_lowercase().find(".exe")? + 4],
        };
        Some(PathBuf::from(path))
    })
}

#[cfg(not(windows))]
fn registered_executable() -> Option<PathBuf> {
    None
}

fn spawn_lunar() -> Result<(), String> {
    let executable = lunar_executable().ok_or("Lunar Client est introuvable sur ce PC.")?;
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible d'ouvrir Lunar Client : {e}"))
}

#[tauri::command]
pub fn lunar_state() -> LunarState {
    state()
}

#[tauri::command]
pub fn open_lunar() -> Result<(), String> {
    spawn_lunar()
}

#[tauri::command]
pub fn download_lunar() -> Result<(), String> {
    open::that(DOWNLOAD_PAGE).map_err(|e| format!("Impossible d'ouvrir le navigateur : {e}"))
}

fn processes() -> System {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system
}

pub fn launcher_running() -> bool {
    processes()
        .processes()
        .values()
        .any(|process| process.name().eq_ignore_ascii_case("Lunar Client.exe"))
}

/// A Lunar game is a javaw.exe from Lunar's bundled JRE. When the path can't be
/// read, assume it is one: Lunar must never be closed under a running game.
pub fn game_running() -> bool {
    processes().processes().values().any(|process| {
        process.name().eq_ignore_ascii_case("javaw.exe")
            && process.exe().map_or(true, |exe| {
                exe.to_string_lossy().to_lowercase().contains(".lunarclient")
            })
    })
}

pub fn close_launcher() -> Result<(), String> {
    if !launcher_running() {
        return Ok(());
    }
    open::that("lunarclient://close-launcher")
        .map_err(|e| format!("Impossible de fermer Lunar : {e}"))?;
    if wait_until(Duration::from_secs(20), || !launcher_running()) {
        Ok(())
    } else {
        Err("Lunar ne s'est pas fermé. Ferme-le puis réessaie.".into())
    }
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        sleep(Duration::from_millis(400));
    }
    condition()
}

fn open_database() -> Result<Connection, String> {
    let path = lunar_dir()
        .ok_or("Lunar Client n'est pas installé.")?
        .join("db")
        .join("profiles.db");
    let connection =
        Connection::open(path).map_err(|e| format!("Base de profils Lunar illisible : {e}"))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;

    let columns: HashSet<String> = {
        let mut statement = connection
            .prepare("select name from pragma_table_info('profiles')")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
    };
    if let Some(missing) = REQUIRED_COLUMNS.iter().find(|column| !columns.contains(**column)) {
        return Err(format!(
            "Cette version de Lunar n'est pas encore prise en charge (colonne {missing} absente)."
        ));
    }
    Ok(connection)
}

fn find_registration(connection: &Connection, target: &Target) -> Result<Option<Registration>, String> {
    connection
        .query_row(
            "select id, pre_launch_command from profiles where path = ?1",
            [&target.profile_path],
            |row| {
                Ok(Registration {
                    id: row.get(0)?,
                    pre_launch_command: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
}

pub fn registration(target: &Target) -> Result<Option<Registration>, String> {
    find_registration(&open_database()?, target)
}

/// Removes the launcher's pre-launch sync from every profile. The profiles and
/// their mods stay: they simply stop updating.
pub fn forget_launcher() -> Result<usize, String> {
    open_database()?
        .execute(
            "update profiles set pre_launch_command = null
             where pre_launch_command like '%--sync --profile % --channel %'",
            [],
        )
        .map_err(|e| e.to_string())
}

/// Creates the profile folder and database entry, or adopts an existing one,
/// and points its pre-launch command at the launcher. Lunar must be closed.
pub fn ensure_profile(target: &Target, pre_launch_command: &str) -> Result<String, String> {
    let dir = profile_dir(&target.profile_path).ok_or("Nom de profil invalide.")?;
    fs::create_dir_all(dir.join("mods")).map_err(|e| e.to_string())?;
    write_if_missing(&dir.join("icon.png"), ICON)?;
    write_if_missing(&dir.join("featured_image.png"), FEATURED_IMAGE)?;

    let connection = open_database()?;
    if let Some(existing) = find_registration(&connection, target)? {
        connection
            .execute(
                "update profiles set pre_launch_command = ?1 where id = ?2",
                params![pre_launch_command, existing.id],
            )
            .map_err(|e| e.to_string())?;
        return Ok(existing.id);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    connection
        .execute(
            "insert into profiles (id, name, description, icon_path, featured_image_path, path, type,
                 major_game_version, game_version, loaders, use_lunar_features, is_badlion, user_modpack,
                 allocated_memory, lunar_module, created_at, config_version, pre_launch_command)
             values (?1, ?2, ?3, 'icon.png', 'featured_image.png', ?4, 'user-modpack', '1.21', '1.21.1',
                 '[\"fabric\"]', 1, 0, '{\"recommendedGameVersion\":\"1.21.1\"}', 6144, 'fabric', ?5, 3, ?6)",
            params![
                id,
                target.profile_name,
                target.profile_description,
                target.profile_path,
                created_at,
                pre_launch_command
            ],
        )
        .map_err(|e| format!("Impossible d'ajouter le profil à Lunar : {e}"))?;
    Ok(id)
}

fn write_if_missing(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, bytes).map_err(|e| e.to_string())
}

/// Makes Lunar open on this profile next time it starts. Lunar must be closed.
pub fn select_profile(id: &str) -> Result<(), String> {
    let path = lunar_dir()
        .ok_or("Lunar Client n'est pas installé.")?
        .join("settings")
        .join("launcher.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("Réglages Lunar illisibles : {e}"))?;
    let updated = match replace_game_profile(&text, id) {
        Some(updated) => updated,
        None => set_game_profile(&text, id)?,
    };
    fs::write(&path, updated).map_err(|e| format!("Réglages Lunar non modifiables : {e}"))
}

/// Swaps the value in place so the rest of Lunar's file stays byte-for-byte identical.
fn replace_game_profile(text: &str, id: &str) -> Option<String> {
    let key = text.find("\"gameProfile\"")?;
    let colon = key + text[key..].find(':')?;
    let open = colon + text[colon..].find('"')?;
    if !text[colon + 1..open].trim().is_empty() {
        return None;
    }
    let close = open + 1 + text[open + 1..].find('"')?;
    Some(format!("{}{}{}", &text[..=open], id, &text[close..]))
}

fn set_game_profile(text: &str, id: &str) -> Result<String, String> {
    let mut json: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    json.get_mut("settings")
        .and_then(|settings| settings.as_object_mut())
        .ok_or("Réglages Lunar illisibles.")?
        .insert("gameProfile".into(), serde_json::Value::String(id.into()));
    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
}

/// Starts Lunar on the selected profile, waits until it is ready, then asks it
/// to launch the game and join the server. Returns whether the game started.
pub fn start_and_play(server: &str, on_step: impl Fn(&str)) -> Result<bool, String> {
    let log = lunar_dir()
        .ok_or("Lunar Client n'est pas installé.")?
        .join("logs")
        .join("launcher")
        .join("main.log");
    let offset = fs::metadata(&log).map(|m| m.len()).unwrap_or(0);

    on_step("Ouverture de Lunar…");
    spawn_lunar()?;
    wait_until(Duration::from_secs(30), || log_contains_since(&log, offset, READY_MARKER));

    on_step("Lancement du jeu…");
    open::that(format!("lunarclient://play?serverAddress={server}"))
        .map_err(|e| format!("Impossible de lancer le jeu : {e}"))?;
    Ok(wait_until(Duration::from_secs(60), game_running))
}

fn log_contains_since(log: &Path, offset: u64, marker: &str) -> bool {
    let Ok(mut file) = fs::File::open(log) else {
        return false;
    };
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if length < offset { 0 } else { offset };
    let mut appended = Vec::new();
    file.seek(SeekFrom::Start(start)).is_ok()
        && file.read_to_end(&mut appended).is_ok()
        && String::from_utf8_lossy(&appended).contains(marker)
}
