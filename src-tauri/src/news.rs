//! Announcements from the workshop (maintenance, events, a bundle leaving the
//! shop...) and the Twitch channels whose live shows up as a banner. Published
//! as channels/news.json, signed like the manifests.

use crate::config::Target;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsFile {
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub twitch: Vec<String>,
    #[serde(default)]
    pub announcements: Vec<Announcement>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    #[serde(default)]
    pub id: String,
    /// "info", "event" or "maintenance".
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub text: String,
    /// Last day it shows (YYYY-MM-DD).
    #[serde(default)]
    pub until: Option<String>,
    /// "all", "prod" or "dev".
    #[serde(default = "everyone")]
    pub audience: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
}

fn everyone() -> String {
    "all".into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    channel: String,
    url: String,
    /// Twitch's own preview of the stream, refreshed every few minutes.
    preview: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct News {
    announcements: Vec<Announcement>,
    live: Vec<Live>,
}

/// Twitch serves a stream's preview while it is live, and redirects to a
/// placeholder otherwise: no account or key needed to tell.
async fn is_live(channel: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
    else {
        return false;
    };
    client
        .head(format!("https://static-cdn.jtvnw.net/previews-ttv/live_user_{channel}-440x248.jpg"))
        .send()
        .await
        .is_ok_and(|response| response.status() == reqwest::StatusCode::OK)
}

fn valid_channel(channel: &str) -> bool {
    (3..=25).contains(&channel.len()) && channel.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[tauri::command]
pub async fn news() -> Result<News, String> {
    let target = Target::current();
    let client = crate::manifest::http_client();
    let file: NewsFile = match crate::manifest::fetch_signed(&client, "news.json").await? {
        Some(body) => serde_json::from_slice(&body).map_err(|e| format!("Annonces illisibles : {e}"))?,
        None => NewsFile::default(),
    };
    let today = crate::clock::today();
    let announcements = file
        .announcements
        .into_iter()
        .filter(|item| item.audience == "all" || item.audience == target.channel)
        .filter(|item| item.until.as_deref().map_or(true, |until| until >= today.as_str()))
        .filter(|item| item.url.as_deref().map_or(true, |url| url.starts_with("https://")))
        .collect();

    let stamp = crate::clock::now_secs() / 300;
    let mut live = Vec::new();
    for channel in file.twitch.iter().filter(|channel| valid_channel(channel)).take(6) {
        if is_live(channel).await {
            live.push(Live {
                url: format!("https://www.twitch.tv/{channel}"),
                preview: format!("https://static-cdn.jtvnw.net/previews-ttv/live_user_{channel}-440x248.jpg?t={stamp}"),
                channel: channel.clone(),
            });
        }
    }
    Ok(News { announcements, live })
}

/// Links from announcements and live banners, opened in the player's browser.
#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(char::is_whitespace) {
        return Err("Lien refusé.".into());
    }
    open::that(&url).map_err(|e| format!("Impossible d'ouvrir le navigateur : {e}"))
}
