//! Help when the game crashes: the launcher spots the newest crash in the pack's
//! game folder (Minecraft's crash report, a Java crash, or Fabric refusing to
//! start), sums it up, and can put the whole file on mclo.gs so the player only
//! has to paste a link on Discord.

use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Older crashes are not news anymore.
const RECENT_MS: u64 = 3 * 24 * 3600 * 1000;
const SEEN_FILE: &str = "crash-seen";
/// mclo.gs takes up to 10 MiB and 25 000 lines.
const MAX_LINES: usize = 24_000;
const MAX_BYTES: usize = 9 * 1024 * 1024;

/// Lines of latest.log that mean Fabric stopped before the game started.
const LOADER_MARKERS: [&str; 5] = [
    "Incompatible mods found!",
    "Mod resolution failed",
    "net.fabricmc.loader.impl.FormattedException",
    "Mixin apply failed",
    "Could not execute entrypoint stage",
];

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashKind {
    /// Minecraft's own crash report.
    Game,
    /// The Java runtime itself (often the graphics driver).
    Java,
    /// Fabric refused to start (a mod missing or incompatible).
    Loader,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crash {
    kind: CrashKind,
    path: String,
    file: String,
    at: u64,
    /// What the report says went wrong.
    summary: String,
    /// Mods the report blames.
    suspects: Vec<String>,
    /// What the player can try, in plain words.
    hints: Vec<String>,
    /// Jars in the mods folder that don't come from the pack.
    extra_mods: Vec<String>,
}

fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as u64)
}

fn seen_path(game_dir: &Path) -> PathBuf {
    game_dir.join(".harpy").join(SEEN_FILE)
}

/// The player dismissed the crashes up to now.
pub fn dismiss(game_dir: &Path) -> Result<(), String> {
    let path = seen_path(game_dir);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(path, crate::clock::now_millis().to_string()).map_err(|e| e.to_string())
}

fn files_in(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file() && path.file_name().is_some_and(|name| keep(&name.to_string_lossy())))
                .collect()
        })
        .unwrap_or_default()
}

fn read_text(path: &Path, limit: usize) -> String {
    let mut bytes = Vec::new();
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(limit as u64).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The newest crash the player hasn't dismissed, if it is recent.
pub fn latest(game_dir: &Path, pack_mods: &[String]) -> Option<Crash> {
    let seen: u64 = fs::read_to_string(seen_path(game_dir))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0);
    let since = seen.max(crate::clock::now_millis().saturating_sub(RECENT_MS));

    let mut candidates: Vec<(CrashKind, PathBuf)> = Vec::new();
    candidates.extend(
        files_in(&game_dir.join("crash-reports"), |name| name.starts_with("crash-") && name.ends_with(".txt"))
            .into_iter()
            .map(|path| (CrashKind::Game, path)),
    );
    candidates.extend(
        files_in(game_dir, |name| name.starts_with("hs_err_pid") && name.ends_with(".log"))
            .into_iter()
            .map(|path| (CrashKind::Java, path)),
    );
    let log = game_dir.join("logs").join("latest.log");
    if modified_ms(&log) > since {
        let text = read_text(&log, MAX_BYTES);
        if LOADER_MARKERS.iter().any(|marker| text.contains(marker)) {
            candidates.push((CrashKind::Loader, log));
        }
    }

    let (kind, path) = candidates
        .into_iter()
        .map(|(kind, path)| (modified_ms(&path), kind, path))
        .filter(|(at, _, _)| *at > since)
        .max_by_key(|(at, _, _)| *at)
        .map(|(_, kind, path)| (kind, path))?;
    let text = read_text(&path, MAX_BYTES);
    let (summary, suspects) = match kind {
        CrashKind::Game => game_summary(&text),
        CrashKind::Java => java_summary(&text),
        CrashKind::Loader => loader_summary(&text),
    };
    let extra_mods = extra_mods(game_dir, pack_mods);
    let hints = hints(kind, &text, &extra_mods);
    Some(Crash {
        kind,
        file: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.display().to_string(),
        at: modified_ms(&path),
        summary,
        suspects,
        hints,
        extra_mods,
    })
}

fn game_summary(text: &str) -> (String, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let description = lines
        .iter()
        .find_map(|line| line.strip_prefix("Description: "))
        .unwrap_or("Crash du jeu");
    // The exception comes right after the description and a blank line.
    let exception = lines
        .iter()
        .position(|line| line.starts_with("Description: "))
        .and_then(|index| lines[index + 1..].iter().find(|line| !line.trim().is_empty()))
        .map(|line| line.trim())
        .unwrap_or("");
    let suspects = lines
        .iter()
        .filter_map(|line| line.trim().strip_prefix("Suspected Mods:"))
        .flat_map(|list| list.split(','))
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty() && name != "NONE")
        .take(5)
        .collect();
    (join_summary(description, exception), suspects)
}

fn java_summary(text: &str) -> (String, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let frame = lines
        .iter()
        .position(|line| line.starts_with("# Problematic frame:"))
        .and_then(|index| lines.get(index + 1))
        .map(|line| line.trim_start_matches('#').trim())
        .unwrap_or("");
    (join_summary("Java s'est arrêté brutalement", frame), Vec::new())
}

fn loader_summary(text: &str) -> (String, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| LOADER_MARKERS.iter().any(|marker| line.contains(marker)))
        .unwrap_or(0);
    let details: Vec<&str> = lines[start + 1..]
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("- "))
        .take(4)
        .collect();
    let suspects = details
        .iter()
        .filter_map(|line| line.split('\'').nth(1))
        .map(str::to_owned)
        .take(5)
        .collect();
    (join_summary("Fabric n'a pas pu démarrer le jeu", &details.join(" ")), suspects)
}

fn join_summary(title: &str, detail: &str) -> String {
    let detail: String = detail.chars().take(260).collect();
    if detail.is_empty() {
        title.to_owned()
    } else {
        format!("{title} : {detail}")
    }
}

fn hints(kind: CrashKind, text: &str, extra_mods: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut hints = Vec::new();
    if lower.contains("outofmemoryerror") || lower.contains("out of memory") {
        hints.push("Le jeu a manqué de mémoire : donne-lui au moins 4 Go de RAM dans ton launcher.".into());
    }
    if ["atio6axx", "atioglxx", "nvoglv", "ig9icd", "ig7icd", "ig75icd", "opengl32"]
        .iter()
        .any(|driver| lower.contains(driver))
    {
        hints.push("Le pilote de ta carte graphique a planté : mets-le à jour (site d'AMD, NVIDIA ou Intel).".into());
    }
    if !extra_mods.is_empty() {
        hints.push(format!(
            "{} mod{} ajouté{} par toi peu{} être en cause : essaie sans.",
            extra_mods.len(),
            if extra_mods.len() > 1 { "s" } else { "" },
            if extra_mods.len() > 1 { "s" } else { "" },
            if extra_mods.len() > 1 { "vent" } else { "t" },
        ));
    }
    if matches!(kind, CrashKind::Loader) || lower.contains("mixin") {
        hints.push("Clique sur Réparer : le launcher remet chaque fichier du pack d'origine.".into());
    }
    if hints.is_empty() {
        hints.push("Essaie Réparer, puis envoie le rapport sur le Discord si ça recommence.".into());
    }
    hints
}

/// Jars the pack didn't install. `pack_mods` are the pack's paths ("mods/x.jar").
pub fn extra_mods(game_dir: &Path, pack_mods: &[String]) -> Vec<String> {
    if pack_mods.is_empty() {
        return Vec::new();
    }
    let mut extra: Vec<String> = files_in(&game_dir.join("mods"), |name| name.to_lowercase().ends_with(".jar"))
        .into_iter()
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .filter(|name| !pack_mods.iter().any(|pack| pack == &format!("mods/{name}")))
        .collect();
    extra.sort();
    extra
}

/// Moves the player's own mods aside (never deletes them).
pub fn disable_extra_mods(game_dir: &Path, pack_mods: &[String]) -> Result<usize, String> {
    let extra = extra_mods(game_dir, pack_mods);
    if extra.is_empty() {
        return Ok(0);
    }
    let aside = game_dir.join("mods-desactives");
    fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
    for name in &extra {
        let mut destination = aside.join(name);
        if destination.exists() {
            destination = aside.join(format!("{}-{name}", crate::clock::now_secs()));
        }
        fs::rename(game_dir.join("mods").join(name), destination)
            .map_err(|e| format!("Impossible de déplacer {name} : {e}"))?;
    }
    Ok(extra.len())
}

/// Puts a report on mclo.gs (which hides IP addresses) and returns its link.
pub async fn share(client: &reqwest::Client, path: &Path) -> Result<String, String> {
    let text = read_text(path, 64 * 1024 * 1024);
    let lines: Vec<&str> = text.lines().collect();
    // Crash reports matter from the top, logs from the bottom.
    let kept = if lines.len() <= MAX_LINES {
        lines
    } else if path.extension().is_some_and(|ext| ext == "txt") {
        lines[..MAX_LINES].to_vec()
    } else {
        lines[lines.len() - MAX_LINES..].to_vec()
    };
    let mut content = kept.join("\n");
    if content.len() > MAX_BYTES {
        let mut cut = content.len() - MAX_BYTES;
        while !content.is_char_boundary(cut) {
            cut += 1;
        }
        content = content[cut..].to_owned();
    }
    let response = client
        .post("https://api.mclo.gs/1/log")
        .form(&[("content", content)])
        .send()
        .await
        .map_err(|_| "mclo.gs est injoignable. Vérifie ta connexion.".to_string())?;
    let body: serde_json::Value = serde_json::from_slice(&response.bytes().await.map_err(|e| e.to_string())?)
        .map_err(|_| "Réponse de mclo.gs illisible.".to_string())?;
    match body.get("url").and_then(|url| url.as_str()) {
        Some(url) if body.get("success").and_then(|ok| ok.as_bool()) == Some(true) => Ok(url.to_owned()),
        _ => Err(format!(
            "mclo.gs a refusé le rapport : {}",
            body.get("error").and_then(|error| error.as_str()).unwrap_or("raison inconnue")
        )),
    }
}
