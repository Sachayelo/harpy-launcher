mod admin;
mod clock;
mod config;
mod crash;
mod dialogs;
mod launchers;
mod lunar;
mod manifest;
mod news;
mod server_status;
mod servers;
mod signature;
mod sync;
mod updater;

use config::{Settings, Target, SERVER_HOST};
use launchers::{apps, official, prism, LauncherKind, Setup};
use manifest::{Manifest, Release};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackStatus {
    target: Target,
    version: Option<String>,
    published: Option<String>,
    /// This version's patch note.
    release: Option<Release>,
    /// The channel's earlier versions, newest first.
    history: Vec<Release>,
    state: &'static str,
    download_bytes: u64,
    download_files: usize,
    /// The launcher already has this profile, from before Harpy Launcher or an older install.
    profile_exists: bool,
}

impl PackStatus {
    fn new(target: Target, manifest: Option<&Manifest>, state: &'static str) -> Self {
        Self {
            target,
            version: manifest.map(|m| m.version.clone()),
            published: manifest.map(|m| m.published.clone()),
            release: manifest.map(Manifest::release),
            history: manifest.map(|m| m.history.clone()).unwrap_or_default(),
            state,
            download_bytes: manifest.map_or(0, |m| m.files.iter().map(|file| file.size).sum()),
            download_files: manifest.map_or(0, |m| m.files.len()),
            profile_exists: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherSettings {
    version: &'static str,
    developer: bool,
    /// Developer mode on the machine that holds the pack sources: unlocks the workshop.
    admin: bool,
    target: Target,
}

impl LauncherSettings {
    fn current() -> Self {
        let developer = Settings::load().developer;
        Self {
            version: updater::current_version(),
            developer,
            admin: developer && admin::available(),
            target: Target::current(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum PlayOutcome {
    /// The game is starting.
    Launched,
    /// The player's launcher is open on the pack's profile: one more click there.
    Opened,
    AlreadyRunning,
    /// Another launcher: the files are up to date, the player starts the game there.
    Synced,
}

pub(crate) async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| e.to_string())?
}

/// Command Lunar runs through cmd.exe before every launch of the profile, so
/// players stay up to date even when they start the game from Lunar directly.
/// Lunar refuses to start the game when this command fails: the fallback keeps
/// a missing or broken launcher from ever locking anyone out.
fn pre_launch_command(target: &Target) -> String {
    let executable = std::env::current_exe().unwrap_or_default();
    format!(
        "\"{}\" --sync --profile {} --channel {} || exit 0",
        executable.display(),
        target.profile_path,
        target.channel
    )
}

#[tauri::command]
fn get_settings() -> LauncherSettings {
    LauncherSettings::current()
}

#[tauri::command]
fn set_developer(enabled: bool) -> Result<LauncherSettings, String> {
    let mut settings = Settings::load();
    settings.developer = enabled;
    if !enabled {
        settings.dev_channel = false;
    }
    settings.save()?;
    Ok(LauncherSettings::current())
}

#[tauri::command]
fn set_dev_channel(enabled: bool) -> Result<LauncherSettings, String> {
    let mut settings = Settings::load();
    settings.dev_channel = enabled && settings.developer;
    settings.save()?;
    Ok(LauncherSettings::current())
}

/// The game folder of the launcher the player uses, once it is set up.
fn game_dir(target: &Target, settings: &Settings) -> Result<PathBuf, String> {
    match launchers::locate(settings.launcher, target, settings) {
        (Setup::Ready, Some(dir)) => Ok(dir),
        _ => Err(format!(
            "{} n'est pas encore prêt : suis les indications au-dessus du bouton.",
            settings.launcher.name()
        )),
    }
}

/// Whether the launcher's profile is set up to run this pack (and to keep it
/// updated, for the launchers that can), and whether it existed at all.
fn registration(settings: &Settings, target: &Target, manifest: &Manifest) -> Result<(bool, bool), String> {
    Ok(match settings.launcher {
        LauncherKind::Lunar => {
            let expected_command = pre_launch_command(target);
            let registration = lunar::registration(target)?;
            let registered = registration
                .as_ref()
                .is_some_and(|profile| profile.pre_launch_command.as_deref() == Some(expected_command.as_str()));
            (registered, registration.is_some())
        }
        LauncherKind::Official => {
            let registered = official::registered(target, manifest.loader_version());
            (registered, registered)
        }
        LauncherKind::Prism => {
            let registered = prism::find(settings)
                .is_some_and(|install| prism::registered(&install, target, manifest.loader_version()));
            (registered, registered)
        }
        LauncherKind::Modrinth | LauncherKind::Curseforge | LauncherKind::Custom => (true, true),
    })
}

fn game_running(kind: LauncherKind, dir: &Path) -> bool {
    match kind {
        LauncherKind::Lunar => lunar::game_running(),
        _ => launchers::game_running_in(dir),
    }
}

#[tauri::command]
async fn pack_status() -> Result<PackStatus, String> {
    let target = Target::current();
    let settings = Settings::load();
    let client = manifest::http_client();
    let Some(manifest) = manifest::fetch(&client, &target.channel).await? else {
        return Ok(PackStatus::new(target, None, "unpublished"));
    };

    // Until the launcher is set up, the whole pack is to be downloaded.
    let Ok(dir) = game_dir(&target, &settings) else {
        return Ok(PackStatus::new(target, Some(&manifest), "install"));
    };

    let (task_target, task_manifest) = (target.clone(), manifest.clone());
    let (installed, profile_exists, plan) = blocking(move || {
        let (registered, exists) = registration(&settings, &task_target, &task_manifest)?;
        let mut state = sync::load_state(&dir);
        let plan = sync::plan(&dir, &task_manifest, &mut state)?;
        if dir.is_dir() {
            sync::save_state(&dir, &state)?;
        }
        Ok((registered && state.version.is_some(), exists, plan))
    })
    .await?;

    let state = if !installed {
        "install"
    } else if plan.is_empty() {
        "ready"
    } else {
        "update"
    };
    Ok(PackStatus {
        download_bytes: plan.download_bytes,
        download_files: plan.downloads.len(),
        profile_exists,
        ..PackStatus::new(target, Some(&manifest), state)
    })
}

/// Brings the game folder in line with the manifest. `repair` hashes every file
/// again and puts back the pack's own configs.
async fn sync_files(
    app: &AppHandle,
    client: &reqwest::Client,
    manifest: &Manifest,
    dir: &Path,
    repair: bool,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let (task_dir, task_manifest) = (dir.to_path_buf(), manifest.clone());
    let (mut state, plan) = blocking(move || {
        let mut state = sync::load_state(&task_dir);
        if repair {
            state.cache.clear();
            state.installed.clear();
        }
        let plan = sync::plan(&task_dir, &task_manifest, &mut state)?;
        Ok((state, plan))
    })
    .await?;

    let mut last_report = Instant::now() - Duration::from_secs(1);
    sync::apply(client, dir, manifest, &plan, &mut state, |progress| {
        if last_report.elapsed() >= Duration::from_millis(80) || progress.bytes_done == progress.bytes_total {
            last_report = Instant::now();
            let _ = app.emit("sync-progress", progress);
        }
    })
    .await?;
    sync::save_state(dir, &state)
}

/// Makes sure Lunar knows the profile and runs the launcher before each launch.
/// Only closes Lunar when its database actually needs a change.
async fn register(app: &AppHandle, target: &Target) -> Result<String, String> {
    let expected_command = pre_launch_command(target);
    let (target, app) = (target.clone(), app.clone());
    blocking(move || {
        if let Some(existing) = lunar::registration(&target)? {
            if existing.pre_launch_command.as_deref() == Some(expected_command.as_str()) {
                return Ok(existing.id);
            }
        }
        if lunar::game_running() {
            return Err("Ferme le jeu avant d'installer le pack.".into());
        }
        if lunar::launcher_running() {
            let _ = app.emit("launch-step", "Fermeture de Lunar…");
            lunar::close_launcher()?;
        }
        lunar::ensure_profile(&target, &expected_command)
    })
    .await
}

async fn play_lunar(app: AppHandle, target: Target) -> Result<PlayOutcome, String> {
    let profile_id = register(&app, &target).await?;
    let port = target.server_port;
    blocking(move || {
        if lunar::launcher_running() {
            let _ = app.emit("launch-step", "Fermeture de Lunar…");
            lunar::close_launcher()?;
        }
        lunar::select_profile(&profile_id)?;
        let launched = lunar::start_and_play(SERVER_HOST, port, |step| {
            let _ = app.emit("launch-step", step);
        })?;
        Ok(if launched {
            PlayOutcome::Launched
        } else {
            PlayOutcome::Opened
        })
    })
    .await
}

#[tauri::command]
async fn play(app: AppHandle) -> Result<PlayOutcome, String> {
    let target = Target::current();
    let settings = Settings::load();
    let kind = settings.launcher;
    let dir = game_dir(&target, &settings)?;
    let task_dir = dir.clone();
    if blocking(move || Ok(game_running(kind, &task_dir))).await? {
        return Ok(PlayOutcome::AlreadyRunning);
    }
    let client = manifest::http_client();
    let manifest = manifest::fetch(&client, &target.channel)
        .await?
        .ok_or("Aucune version n'est publiée pour l'instant.")?;
    sync_files(&app, &client, &manifest, &dir, false).await?;

    let (minecraft, loader) = (manifest.minecraft().to_owned(), manifest.loader_version().to_owned());
    match kind {
        LauncherKind::Lunar => play_lunar(app, target).await,
        LauncherKind::Official => {
            let _ = app.emit("launch-step", "Préparation du profil Minecraft…");
            official::install(&client, &target, &minecraft, &loader).await?;
            let _ = app.emit("launch-step", "Ouverture du launcher Minecraft…");
            blocking(official::open).await?;
            Ok(PlayOutcome::Opened)
        }
        LauncherKind::Prism => {
            let install = prism::find(&settings).ok_or("Prism Launcher est introuvable.")?;
            let _ = app.emit("launch-step", "Préparation de l'instance…");
            blocking(move || {
                prism::ensure_instance(&install, &target, &minecraft, &loader)?;
                let _ = app.emit("launch-step", "Lancement par Prism…");
                prism::launch(&install, &target)?;
                Ok(
                    if launchers::wait_until(Duration::from_secs(60), || launchers::game_running_in(&dir)) {
                        PlayOutcome::Launched
                    } else {
                        PlayOutcome::Opened
                    },
                )
            })
            .await
        }
        LauncherKind::Modrinth | LauncherKind::Curseforge => {
            blocking(move || apps::open(apps::App::of(kind))).await?;
            Ok(PlayOutcome::Opened)
        }
        LauncherKind::Custom => Ok(PlayOutcome::Synced),
    }
}

/// Modrinth App and CurseForge: hands the player the profile to import.
#[tauri::command]
async fn import_profile() -> Result<apps::Import, String> {
    let target = Target::current();
    let kind = Settings::load().launcher;
    if !matches!(kind, LauncherKind::Modrinth | LauncherKind::Curseforge) {
        return Err("Ce launcher n'a rien à importer.".into());
    }
    let manifest = manifest::fetch(&manifest::http_client(), &target.channel)
        .await?
        .ok_or("Aucune version n'est publiée pour l'instant.")?;
    blocking(move || apps::start_import(apps::App::of(kind), &target, &manifest)).await
}

/// The pack's mods ("mods/x.jar") as of the last sync of this folder.
fn pack_mods(dir: &Path) -> Vec<String> {
    sync::load_state(dir)
        .installed
        .into_keys()
        .filter(|path| path.starts_with("mods/"))
        .collect()
}

#[tauri::command]
async fn crash_status() -> Result<Option<crash::Crash>, String> {
    let (target, settings) = (Target::current(), Settings::load());
    let Ok(dir) = game_dir(&target, &settings) else {
        return Ok(None);
    };
    blocking(move || Ok(crash::latest(&dir, &pack_mods(&dir)))).await
}

#[tauri::command]
async fn crash_dismiss() -> Result<(), String> {
    let dir = game_dir(&Target::current(), &Settings::load())?;
    blocking(move || crash::dismiss(&dir)).await
}

/// Puts a crash report, or the game's log, on mclo.gs. Only files of the pack's
/// game folder can be sent.
#[tauri::command]
async fn share_log(path: Option<String>) -> Result<String, String> {
    let dir = game_dir(&Target::current(), &Settings::load())?;
    let file = match path {
        Some(path) => PathBuf::from(path),
        None => dir.join("logs").join("latest.log"),
    };
    let inside = match (std::fs::canonicalize(&file), std::fs::canonicalize(&dir)) {
        (Ok(file), Ok(dir)) => file.starts_with(dir),
        _ => false,
    };
    if !inside || !file.is_file() {
        return Err("Rapport introuvable.".into());
    }
    crash::share(&manifest::http_client(), &file).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Repair {
    /// The player's own mods, moved to mods-desactives.
    moved: usize,
}

/// Checks every file of the pack again and puts back the pack's own copies.
#[tauri::command]
async fn repair(app: AppHandle, disable_extra: bool) -> Result<Repair, String> {
    let target = Target::current();
    let settings = Settings::load();
    let kind = settings.launcher;
    let dir = game_dir(&target, &settings)?;
    let task_dir = dir.clone();
    if blocking(move || Ok(game_running(kind, &task_dir))).await? {
        return Err("Ferme le jeu avant de réparer.".into());
    }
    let client = manifest::http_client();
    let manifest = manifest::fetch(&client, &target.channel)
        .await?
        .ok_or("Aucune version n'est publiée pour l'instant.")?;
    sync_files(&app, &client, &manifest, &dir, true).await?;
    let mods: Vec<String> = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .filter(|path| path.starts_with("mods/"))
        .collect();
    blocking(move || {
        let moved = if disable_extra {
            crash::disable_extra_mods(&dir, &mods)?
        } else {
            0
        };
        crash::dismiss(&dir)?;
        Ok(Repair { moved })
    })
    .await
}

#[tauri::command]
fn open_game_folder() -> Result<(), String> {
    let dir = game_dir(&Target::current(), &Settings::load())?;
    open::that(&dir).map_err(|e| format!("Impossible d'ouvrir le dossier : {e}"))
}

/// Entry point of `--sync`, run by Lunar before each launch. Never blocks the
/// game: whatever happens, it exits 0 and Lunar starts with what is installed.
/// Players who only ever start the game from Lunar get launcher updates here.
pub fn run_headless_sync(args: &[String]) -> i32 {
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let Some(channel) = value("--channel") else {
        return 0;
    };
    // Lunar names its profile; Prism gives the instance's game folder, which
    // the launcher has synced before (it holds .harpy).
    let dir = match (value("--profile"), value("--dir")) {
        (Some(profile), _) => lunar::profile_dir(&profile),
        (None, Some(dir)) => Some(PathBuf::from(dir)).filter(|dir| dir.is_absolute() && dir.join(".harpy").is_dir()),
        _ => None,
    };
    let Some(dir) = dir else {
        return 0;
    };
    if !channel.chars().all(|c| c.is_ascii_lowercase()) {
        return 0;
    }

    let started = Instant::now();
    let result = tauri::async_runtime::block_on(async {
        let client = manifest::http_client();
        let manifest = manifest::fetch(&client, &channel)
            .await?
            .ok_or("aucune version publiée")?;
        let mut state = sync::load_state(&dir);
        let plan = sync::plan(&dir, &manifest, &mut state)?;
        let summary = format!(
            "{} : {} fichier(s) téléchargé(s), {} retiré(s)",
            manifest.version,
            plan.downloads.len(),
            plan.removals.len()
        );
        sync::apply(&client, &dir, &manifest, &plan, &mut state, |_| {}).await?;
        sync::save_state(&dir, &state)?;
        Ok::<_, String>(summary)
    });
    sync::append_log(
        &dir,
        &match result {
            Ok(summary) => format!("OK {summary} en {} ms", started.elapsed().as_millis()),
            Err(error) => format!("ERREUR {error}"),
        },
    );

    if !updater::other_instance_running() {
        let update = tauri::async_runtime::block_on(async {
            let client = manifest::http_client();
            let Some(release) = updater::check(&client).await? else {
                return Ok(None);
            };
            let installer = updater::download(&client, &release, |_, _| {}).await?;
            updater::launch_installer(&installer, false)?;
            Ok::<_, String>(Some(release.version))
        });
        match update {
            Ok(Some(version)) => sync::append_log(&dir, &format!("Launcher mis à jour vers {version}")),
            Ok(None) => {}
            Err(error) => sync::append_log(&dir, &format!("ERREUR mise à jour du launcher : {error}")),
        }
    }
    0
}

/// Entry point of `--forget`, run by the uninstaller: players' profiles must
/// keep launching once the launcher is gone.
pub fn forget_launcher() -> i32 {
    let _ = lunar::forget_launcher();
    if let Some(install) = prism::find(&Settings::load()) {
        for target in [Target::prod(), Target::dev()] {
            let _ = prism::forget(&install, &target);
        }
    }
    0
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            server_status::server_status,
            lunar::lunar_state,
            lunar::open_lunar,
            lunar::download_lunar,
            launchers::launcher_state,
            launchers::launchers,
            launchers::set_launcher,
            launchers::download_launcher,
            launchers::choose_folder,
            get_settings,
            set_developer,
            set_dev_channel,
            pack_status,
            play,
            import_profile,
            crash_status,
            crash_dismiss,
            share_log,
            repair,
            open_game_folder,
            news::news,
            news::open_link,
            admin::repositories,
            admin::commit_and_push,
            admin::pack_preview,
            admin::publish_pack,
            admin::promote_preview,
            admin::promote_pack,
            admin::news_draft,
            admin::publish_news,
            admin::stage_news_image,
            admin::launcher_release,
            admin::publish_launcher,
            servers::servers_status,
            servers::server_power,
            servers::server_deploy,
            servers::server_rollback,
            updater::check_update,
            updater::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
