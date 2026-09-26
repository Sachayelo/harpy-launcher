//! The workshop's Servers tab: drives the Pelican game servers over SSH with
//! the publisher's key. The work happens in `server/harpy_server.py`, sent to
//! the server on every call, so nothing has to be installed there.

use crate::config::SERVER_HOST;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

const SCRIPT: &str = include_str!("../server/harpy_server.py");
const SERVERS: [&str; 2] = ["prod", "dev"];

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    state: String,
    online: Option<u32>,
    max: Option<u32>,
    memory_mb: Option<u64>,
    memory_limit_mb: Option<u64>,
    /// Pack version the launcher last installed on this server.
    version: Option<String>,
    deployed_at: Option<String>,
    backups: u32,
    /// Pack version currently published on the server's channel.
    #[serde(default)]
    pack_version: Option<String>,
}

fn ssh() -> Command {
    let mut command = Command::new("ssh");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        &format!("root@{SERVER_HOST}"),
    ]);
    command
}

/// Runs the server script and returns what it reported on its `HARPY_JSON:` line.
fn run(app: Option<&AppHandle>, args: &[&str], preamble: &str) -> Result<String, String> {
    let mut child = ssh()
        .arg("python3")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("SSH est introuvable sur ce PC : {e}"))?;

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = format!("{preamble}{SCRIPT}");
    std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let (mut result, mut failure) = (None, None);
    for line in BufReader::new(child.stdout.take().expect("stdout is piped"))
        .lines()
        .map_while(Result::ok)
    {
        if let Some(json) = line.strip_prefix("HARPY_JSON:") {
            result = Some(json.to_owned());
        } else if let Some(message) = line.strip_prefix("HARPY_ERROR:") {
            failure = Some(message.to_owned());
        } else if let Some(app) = app {
            let _ = app.emit("workshop-log", &line);
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    let errors = errors.join().unwrap_or_default();
    match (status.success(), result, failure) {
        (true, Some(result), _) => Ok(result),
        (_, _, Some(message)) => Err(message),
        _ => {
            let detail = errors.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
            Err(if detail.contains("Permission denied") || detail.contains("Connection") {
                format!("Connexion SSH au serveur impossible : {detail}")
            } else {
                format!("Le serveur n'a pas répondu comme prévu. {detail}")
            })
        }
    }
}

fn check_server(server: &str) -> Result<(), String> {
    SERVERS
        .contains(&server)
        .then_some(())
        .ok_or_else(|| "Serveur inconnu.".to_string())
}

fn force_flag(force: bool) -> &'static str {
    if force {
        "--force"
    } else {
        ""
    }
}

#[tauri::command]
pub async fn servers_status() -> Result<HashMap<String, ServerInfo>, String> {
    let output = crate::blocking(|| run(None, &["status"], "")).await?;
    let mut servers: HashMap<String, ServerInfo> =
        serde_json::from_str(&output).map_err(|e| e.to_string())?;
    let client = crate::manifest::http_client();
    for (server, info) in servers.iter_mut() {
        info.pack_version = crate::manifest::fetch(&client, server)
            .await
            .ok()
            .flatten()
            .map(|manifest| manifest.version);
    }
    Ok(servers)
}

#[tauri::command]
pub async fn server_power(app: AppHandle, server: String, action: String, force: bool) -> Result<(), String> {
    check_server(&server)?;
    if !["start", "stop", "restart"].contains(&action.as_str()) {
        return Err("Action inconnue.".into());
    }
    crate::blocking(move || {
        run(Some(&app), &[action.as_str(), server.as_str(), force_flag(force)], "").map(|_| ())
    })
    .await
}

/// Installs the mods of the server's channel. The manifest is fetched and its
/// signature checked here, then handed to the server.
#[tauri::command]
pub async fn server_deploy(app: AppHandle, server: String, force: bool) -> Result<(), String> {
    check_server(&server)?;
    let manifest = crate::manifest::fetch(&crate::manifest::http_client(), &server)
        .await?
        .ok_or("Aucune version du pack n'est publiée pour ce serveur.")?;
    let json = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    let preamble = format!("MANIFEST_B64 = \"{}\"\n", STANDARD.encode(json));
    crate::blocking(move || {
        run(Some(&app), &["deploy", server.as_str(), force_flag(force)], &preamble).map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn server_rollback(app: AppHandle, server: String, force: bool) -> Result<(), String> {
    check_server(&server)?;
    crate::blocking(move || run(Some(&app), &["rollback", server.as_str(), force_flag(force)], "").map(|_| ()))
        .await
}
