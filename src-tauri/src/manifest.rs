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

/// `Ok(None)` when nothing has been published on this channel yet.
pub async fn fetch(client: &reqwest::Client, channel: &str) -> Result<Option<Manifest>, String> {
    // raw.githubusercontent.com caches for a few minutes; a unique query string bypasses it.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let url = format!("https://raw.githubusercontent.com/{PACK_REPO}/main/channels/{channel}.json?t={stamp}");

    let response = client
        .get(&url)
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

    let manifest: Manifest =
        serde_json::from_slice(&body).map_err(|e| format!("Manifeste illisible : {e}"))?;
    if manifest.schema != 1 {
        return Err("Cette version du pack demande la dernière version du launcher : ferme-le et rouvre-le pour la recevoir.".into());
    }
    Ok(Some(manifest))
}
