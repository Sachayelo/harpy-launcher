mod admin;
mod config;
mod lunar;
mod manifest;
mod server_status;
mod sync;

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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherSettings {
    developer: bool,
    /// Developer mode on the machine that holds the pack sources: unlocks the workshop.
    admin: bool,
    target: Target,
}

impl LauncherSettings {
    fn current() -> Self {
        let developer = Settings::load().developer;
        Self {
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

/// Command Lunar runs before every launch of the profile, so players stay up
/// to date even when they start the game from Lunar directly.
fn pre_launch_command(target: &Target) -> String {
    let executable = std::env::current_exe().unwrap_or_default();
    let executable = executable.to_string_lossy();
    let executable = if executable.contains(' ') {
        format!("\"{executable}\"")
    } else {
        executable.into_owned()
    };
    format!(
        "{executable} --sync --profile {} --channel {}",
        target.profile_path, target.channel
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
        });
    };

    let dir = lunar::profile_dir(&target.profile_path).ok_or("Lunar Client n'est pas installé.")?;
    let expected_command = pre_launch_command(&target);
    let (task_target, task_manifest) = (target.clone(), manifest.clone());
    let (installed, plan) = blocking(move || {
        let registered = lunar::registration(&task_target)?
            .is_some_and(|profile| profile.pre_launch_command.as_deref() == Some(expected_command.as_str()));
        let mut state = sync::load_state(&dir);
        let plan = sync::plan(&dir, &task_manifest, &mut state)?;
        if dir.is_dir() {
            sync::save_state(&dir, &state)?;
        }
        Ok((registered && state.version.is_some(), plan))
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
async fn sync_pack(app: AppHandle) -> Result<(), String> {
    let target = Target::current();
    sync_files(&app, &target).await?;
    register(&app, &target).await.map(|_| ())
}

#[tauri::command]
async fn play(app: AppHandle) -> Result<PlayOutcome, String> {
    let target = Target::current();
    if blocking(|| Ok(lunar::game_running())).await? {
        return Ok(PlayOutcome::AlreadyRunning);
    }
    sync_files(&app, &target).await?;
    let profile_id = register(&app, &target).await?;

    blocking(move || {
        if lunar::launcher_running() {
            let _ = app.emit("launch-step", "Fermeture de Lunar…");
            lunar::close_launcher()?;
        }
        lunar::select_profile(&profile_id)?;
        let launched = lunar::start_and_play(SERVER_HOST, |step| {
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
            lunar::find_lunar,
            get_settings,
            set_developer,
            set_dev_channel,
            pack_status,
            sync_pack,
            play,
            admin::repositories,
            admin::commit_and_push,
            admin::pack_preview,
            admin::publish_pack,
            admin::promote_pack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
