mod admin;
mod config;
mod lunar;
mod manifest;
mod server_status;
mod servers;
mod signature;
mod sync;
mod updater;

use config::{Settings, Target, SERVER_HOST};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackStatus {
    target: Target,
    version: Option<String>,
    published: Option<String>,
    notes: Vec<String>,
    state: &'static str,
    download_bytes: u64,
    download_files: usize,
    /// Lunar already has this profile, from before the launcher or an older install.
    profile_exists: bool,
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
    Launched,
    OpenedLunar,
    AlreadyRunning,
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

#[tauri::command]
async fn pack_status() -> Result<PackStatus, String> {
    let target = Target::current();
    let client = manifest::http_client();
    let Some(manifest) = manifest::fetch(&client, &target.channel).await? else {
        return Ok(PackStatus {
            target,
            version: None,
            published: None,
            notes: Vec::new(),
            state: "unpublished",
            download_bytes: 0,
            download_files: 0,
            profile_exists: false,
        });
    };

    // Until Lunar is set up, the whole pack is to be downloaded.
    let dir = lunar::profile_dir(&target.profile_path)
        .filter(|_| lunar::state() == lunar::LunarState::Ready);
    let Some(dir) = dir else {
        return Ok(PackStatus {
            target,
            version: Some(manifest.version),
            published: Some(manifest.published),
            notes: manifest.notes,
            state: "install",
            download_bytes: manifest.files.iter().map(|file| file.size).sum(),
            download_files: manifest.files.len(),
            profile_exists: false,
        });
    };

    let expected_command = pre_launch_command(&target);
    let (task_target, task_manifest) = (target.clone(), manifest.clone());
    let (installed, profile_exists, plan) = blocking(move || {
        let registration = lunar::registration(&task_target)?;
        let registered = registration
            .as_ref()
            .is_some_and(|profile| profile.pre_launch_command.as_deref() == Some(expected_command.as_str()));
        let mut state = sync::load_state(&dir);
        let plan = sync::plan(&dir, &task_manifest, &mut state)?;
        if dir.is_dir() {
            sync::save_state(&dir, &state)?;
        }
        Ok((registered && state.version.is_some(), registration.is_some(), plan))
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
        target,
        version: Some(manifest.version),
        published: Some(manifest.published),
        notes: manifest.notes,
        state,
        download_bytes: plan.download_bytes,
        download_files: plan.downloads.len(),
        profile_exists,
    })
}

async fn sync_files(app: &AppHandle, target: &Target) -> Result<(), String> {
    let client = manifest::http_client();
    let manifest = manifest::fetch(&client, &target.channel)
        .await?
        .ok_or("Aucune version n'est publiée pour l'instant.")?;
    let dir = lunar::profile_dir(&target.profile_path).ok_or("Lunar Client n'est pas installé.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let (task_dir, task_manifest) = (dir.clone(), manifest.clone());
    let (mut state, plan) = blocking(move || {
        let mut state = sync::load_state(&task_dir);
        let plan = sync::plan(&task_dir, &task_manifest, &mut state)?;
        Ok((state, plan))
    })
    .await?;

    let mut last_report = Instant::now() - Duration::from_secs(1);
    sync::apply(&client, &dir, &manifest, &plan, &mut state, |progress| {
        if last_report.elapsed() >= Duration::from_millis(80) || progress.bytes_done == progress.bytes_total {
            last_report = Instant::now();
            let _ = app.emit("sync-progress", progress);
        }
    })
    .await?;
    sync::save_state(&dir, &state)
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

#[tauri::command]
async fn play(app: AppHandle) -> Result<PlayOutcome, String> {
    let target = Target::current();
    if blocking(|| Ok(lunar::game_running())).await? {
        return Ok(PlayOutcome::AlreadyRunning);
    }
    sync_files(&app, &target).await?;
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
            PlayOutcome::OpenedLunar
        })
    })
    .await
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
    let (Some(profile), Some(channel)) = (value("--profile"), value("--channel")) else {
        return 0;
    };
    let Some(dir) = lunar::profile_dir(&profile) else {
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
            get_settings,
            set_developer,
            set_dev_channel,
            pack_status,
            play,
            admin::repositories,
            admin::commit_and_push,
            admin::pack_preview,
            admin::publish_pack,
            admin::promote_pack,
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
