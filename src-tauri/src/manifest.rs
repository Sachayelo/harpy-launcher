use crate::config::PACK_REPO;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub path: String,
    pub mod_id: Option<String>,
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema: u32,
    pub channel: String,
    pub version: String,
    pub published: String,
    #[serde(default)]
    pub notes: Vec<String>,
    /// Mod ids to remove from players' mods folder even though the pack never installed them.
    #[serde(default)]
    pub blocked_mods: Vec<String>,
    pub files: Vec<PackFile>,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("HarpyLauncher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client configuration is static")
}

/// `Ok(None)` on 404.
async fn download(client: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "GitHub est injoignable. Vérifie ta connexion.".to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(body.to_vec()))
}

/// `Ok(None)` when nothing has been published on this channel yet. The manifest
/// decides what runs on every player's PC, so only a signed one is trusted.
pub async fn fetch(client: &reqwest::Client, channel: &str) -> Result<Option<Manifest>, String> {
    let base = format!("https://raw.githubusercontent.com/{PACK_REPO}/main/channels/{channel}.json");
    // A publication pushes the manifest and its signature together: reading them
    // in the middle of it can pair a new manifest with the old signature.
    for attempt in 0..2 {
        if attempt > 0 {
            let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_secs(2))).await;
        }
        // raw.githubusercontent.com caches for a few minutes; a unique query string bypasses it.
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let Some(body) = download(client, &format!("{base}?t={stamp}")).await? else {
            return Ok(None);
        };
        let signature = download(client, &format!("{base}.sig?t={stamp}")).await?;
        let signed = signature
            .as_deref()
            .and_then(|signature| std::str::from_utf8(signature).ok())
            .is_some_and(|signature| crate::signature::verify(&body, signature));
        if signed {
            return parse(&body, channel).map(Some);
        }
    }
    Err("Mise à jour du pack refusée : elle n'est pas signée par Harpy Express.".into())
}

fn parse(body: &[u8], channel: &str) -> Result<Manifest, String> {
    let manifest: Manifest =
        serde_json::from_slice(body).map_err(|e| format!("Manifeste illisible : {e}"))?;
    if manifest.schema != 1 {
        return Err("Cette version du pack demande la dernière version du launcher : ferme-le et rouvre-le pour la recevoir.".into());
    }
    // Signed dev manifests must not pass for prod ones.
    if manifest.channel != channel {
        return Err("Mise à jour du pack refusée : le manifeste ne correspond pas à ce canal.".into());
    }
    Ok(manifest)
}
