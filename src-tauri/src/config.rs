use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const PACK_REPO: &str = "Sachayelo/harpy-pack";
pub const LAUNCHER_REPO: &str = "Sachayelo/harpy-launcher";
/// Public half of the key that signs launcher releases and pack manifests. The
/// private half lives only on the publisher's PC (%USERPROFILE%\.harpy\launcher-update.key).
pub const SIGNING_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEIwRDVDRENDMTdFNzcwNzUKUldSMWNPY1h6TTNWc0EzQnN5WTJYaW1kallsbUc2aDdqUlFkNzJBRGJMWi9Pc3VjOHhsMjBaVjYK";
pub const SERVER_HOST: &str = "195.88.87.173";
const APP_ID: &str = "com.sachayelo.harpylauncher";

/// A channel of the pack, the Lunar profile it is synced into and the game
/// server that runs it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub channel: String,
    pub profile_path: String,
    pub profile_name: String,
    pub profile_description: String,
    pub server_port: u16,
}

impl Target {
    pub fn prod() -> Self {
        Self::new(
            "prod",
            "harpy-express",
            "Harpy Express",
            "Préparez-vous à embarquer pour votre voyage final à bord du Harpy Express.",
            25565,
        )
    }

    pub fn dev() -> Self {
        Self::new("dev", "harpy-dev", "Harpy Dev", "Version de test du Harpy Express", 25566)
    }

    pub fn current() -> Self {
        if Settings::load().uses_dev() {
            Self::dev()
        } else {
            Self::prod()
        }
    }

    fn new(channel: &str, path: &str, name: &str, description: &str, server_port: u16) -> Self {
        Self {
            channel: channel.into(),
            profile_path: path.into(),
            profile_name: name.into(),
            profile_description: description.into(),
            server_port,
        }
    }

}

/// Launcher preferences. Developer mode unlocks the dev channel, which syncs
/// into its own "Harpy Dev" profile; players only ever see prod.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub developer: bool,
    #[serde(default)]
    pub dev_channel: bool,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("APPDATA")?)
                .join(APP_ID)
                .join("settings.json"),
        )
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Dossier des réglages introuvable.")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn uses_dev(&self) -> bool {
        self.developer && self.dev_channel
    }
}
