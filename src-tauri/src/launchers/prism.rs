//! Prism Launcher and MultiMC: the pack gets an instance of its own whose
//! pre-launch command runs the launcher's sync, so starting it from Prism keeps
//! it up to date too, and which joins the server once the game is up.

use super::{env_dir, process_running, protocol_handler, slashes, spawn, version_at_least};
use crate::config::{Target, SERVER_HOST};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const ICON: &[u8] = include_bytes!("../../../src/assets/icon.png");
const ICON_KEY: &str = "harpy-express";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Prism,
    /// MultiMC keeps instance.cfg in its own format: no sections, no quoting.
    MultiMc,
}

pub struct Install {
    data: PathBuf,
    executable: Option<PathBuf>,
    flavor: Flavor,
}

/// The folder the player picked (MultiMC, a portable Prism), else Prism's usual one.
pub fn find(settings: &crate::config::Settings) -> Option<Install> {
    if let Some(install) = settings.folders.get("prism").and_then(|dir| at(Path::new(dir))) {
        return Some(install);
    }
    let data = env_dir("APPDATA")?.join("PrismLauncher");
    if !data.join("prismlauncher.cfg").is_file() {
        return None;
    }
    let executable = protocol_handler("prismlauncher").or_else(|| {
        Some(env_dir("LOCALAPPDATA")?.join("Programs").join("PrismLauncher").join("prismlauncher.exe"))
            .filter(|path| path.is_file())
    });
    Some(Install { data, executable, flavor: Flavor::Prism })
}

pub fn at(dir: &Path) -> Option<Install> {
    let file = |name: &str| Some(dir.join(name)).filter(|path| path.is_file());
    if file("multimc.cfg").is_some() || file("MultiMC.exe").is_some() {
        return Some(Install { data: dir.to_path_buf(), executable: file("MultiMC.exe"), flavor: Flavor::MultiMc });
    }
    if file("prismlauncher.cfg").is_some() || file("prismlauncher.exe").is_some() {
        return Some(Install { data: dir.to_path_buf(), executable: file("prismlauncher.exe"), flavor: Flavor::Prism });
    }
    None
}

fn config_value(install: &Install, key: &str) -> Option<String> {
    let name = match install.flavor {
        Flavor::Prism => "prismlauncher.cfg",
        Flavor::MultiMc => "multimc.cfg",
    };
    let text = fs::read_to_string(install.data.join(name)).ok()?;
    let value = text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))?;
    let value = value.trim();
    let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
    Some(value.replace("\\\\", "\\")).filter(|value| !value.is_empty())
}

fn instances_dir(install: &Install) -> PathBuf {
    let configured = PathBuf::from(config_value(install, "InstanceDir").unwrap_or_else(|| "instances".into()));
    if configured.is_absolute() {
        configured
    } else {
        install.data.join(configured)
    }
}

fn instance_dir(install: &Install, target: &Target) -> PathBuf {
    instances_dir(install).join(&target.profile_path)
}

/// Prism's rule: "minecraft", unless the instance only has a ".minecraft".
pub fn game_dir(install: &Install, target: &Target) -> PathBuf {
    let instance = instance_dir(install, target);
    let (plain, dotted) = (instance.join("minecraft"), instance.join(".minecraft"));
    if dotted.is_dir() && !plain.is_dir() {
        dotted
    } else {
        plain
    }
}

/// Run without a shell, so no fallback: the launcher's sync always exits 0.
pub fn pre_launch_command(game_dir: &Path, channel: &str) -> String {
    let executable = std::env::current_exe().unwrap_or_default();
    format!(
        "\"{}\" --sync --dir \"{}\" --channel {channel}",
        slashes(&executable),
        slashes(game_dir)
    )
}

/// A value as each flavor writes it in instance.cfg: Qt's INI escaping for
/// Prism, MultiMC's own for MultiMC.
fn cfg_value(install: &Install, value: &str) -> String {
    match install.flavor {
        Flavor::Prism => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            let quoted = value.contains([';', ',', '=']) || value.starts_with(' ') || value.ends_with(' ');
            if quoted {
                format!("\"{escaped}\"")
            } else {
                escaped
            }
        }
        Flavor::MultiMc => value.replace('\\', "\\\\").replace('#', "\\#"),
    }
}

/// Reads a value back, whichever way it was quoted when last saved.
fn cfg_decode(install: &Install, raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.trim().chars();
    while let Some(c) = chars.next() {
        match c {
            '"' if install.flavor == Flavor::Prism => {}
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => {}
            },
            _ => out.push(c),
        }
    }
    out
}

fn read_cfg(install: &Install, target: &Target) -> Option<String> {
    fs::read_to_string(instance_dir(install, target).join("instance.cfg")).ok()
}

fn cfg_get<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
}

fn loader_installed(install: &Install, target: &Target) -> Option<String> {
    let text = fs::read_to_string(instance_dir(install, target).join("mmc-pack.json")).ok()?;
    let pack: Value = serde_json::from_str(&text).ok()?;
    pack.get("components")?
        .as_array()?
        .iter()
        .find(|component| component.get("uid").and_then(Value::as_str) == Some("net.fabricmc.fabric-loader"))?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

pub fn registered(install: &Install, target: &Target, loader: &str) -> bool {
    let command = pre_launch_command(&game_dir(install, target), &target.channel);
    read_cfg(install, target).is_some_and(|text| {
        cfg_get(&text, "OverrideCommands") == Some("true")
            && cfg_get(&text, "PreLaunchCommand").map(|raw| cfg_decode(install, raw)) == Some(command)
    }) && loader_installed(install, target).is_some_and(|version| version_at_least(&version, loader))
}

pub fn launcher_running() -> bool {
    process_running(&["prismlauncher.exe", "MultiMC.exe"])
}

/// Creates the instance, or brings an existing one up to date. Prism keeps the
/// settings of loaded instances in memory, so an existing one is only rewritten
/// while it is closed.
pub fn ensure_instance(install: &Install, target: &Target, minecraft: &str, loader: &str) -> Result<(), String> {
    if registered(install, target, loader) {
        return Ok(());
    }
    let instance = instance_dir(install, target);
    let exists = instance.join("instance.cfg").is_file();
    if exists && launcher_running() {
        return Err(format!(
            "Ferme Prism Launcher puis clique à nouveau sur Jouer : l'instance {} doit être mise à jour.",
            target.profile_name
        ));
    }
    let game = game_dir(install, target);
    fs::create_dir_all(game.join("mods")).map_err(|e| e.to_string())?;
    write_pack(&instance, minecraft, loader)?;

    let command = pre_launch_command(&game, &target.channel);
    let address = format!("{SERVER_HOST}:{}", target.server_port);
    let mut settings = vec![
        ("OverrideCommands", "true".to_string()),
        ("PreLaunchCommand", cfg_value(install, &command)),
        ("JoinServerOnLaunch", "true".to_string()),
        ("JoinServerOnLaunchAddress", address),
    ];
    if !exists {
        settings.extend([
            ("InstanceType", "OneSix".to_string()),
            ("name", target.profile_name.clone()),
            ("iconKey", ICON_KEY.to_string()),
            ("OverrideMemory", "true".to_string()),
            ("MinMemAlloc", "1024".to_string()),
            ("MaxMemAlloc", super::memory_mb().to_string()),
        ]);
        let icons = install.data.join("icons");
        if fs::create_dir_all(&icons).is_ok() && !icons.join(format!("{ICON_KEY}.png")).exists() {
            let _ = fs::write(icons.join(format!("{ICON_KEY}.png")), ICON);
        }
    }
    let text = read_cfg(install, target).unwrap_or_else(|| match install.flavor {
        Flavor::Prism => "[General]\nConfigVersion=1.3\n".into(),
        Flavor::MultiMc => String::new(),
    });
    fs::write(instance.join("instance.cfg"), set_keys(&text, &settings))
        .map_err(|e| format!("Impossible d'écrire l'instance : {e}"))
}

/// Minecraft and Fabric Loader; Prism adds LWJGL and the mappings by itself.
fn write_pack(instance: &Path, minecraft: &str, loader: &str) -> Result<(), String> {
    let path = instance.join("mmc-pack.json");
    let mut pack: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| {
            json!({
                "components": [
                    { "uid": "net.minecraft", "version": minecraft, "important": true },
                    { "uid": "net.fabricmc.intermediary", "version": minecraft, "dependencyOnly": true },
                ],
                "formatVersion": 1
            })
        });
    let components = pack
        .get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("mmc-pack.json est illisible.")?;
    match components
        .iter_mut()
        .find(|component| component.get("uid").and_then(Value::as_str) == Some("net.fabricmc.fabric-loader"))
    {
        Some(component) => {
            let current = component.get("version").and_then(Value::as_str).unwrap_or("0");
            if !version_at_least(current, loader) {
                component["version"] = json!(loader);
                if let Some(fields) = component.as_object_mut() {
                    fields.remove("cachedVersion");
                }
            }
        }
        None => components.push(json!({ "uid": "net.fabricmc.fabric-loader", "version": loader })),
    }
    let text = serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("Impossible d'écrire l'instance : {e}"))
}

/// Replaces or adds keys of the main section, leaving every other line as it was.
fn set_keys(text: &str, settings: &[(&str, String)]) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    // Keys live before the first section other than [General] (MultiMC has no sections).
    let end = lines
        .iter()
        .position(|line| line.starts_with('[') && line.trim() != "[General]")
        .unwrap_or(lines.len());
    let mut missing = Vec::new();
    for (key, value) in settings {
        let prefix = format!("{key}=");
        match lines[..end].iter().position(|line| line.starts_with(&prefix)) {
            Some(index) => lines[index] = format!("{prefix}{value}"),
            None => missing.push(format!("{prefix}{value}")),
        }
    }
    let insert_at = lines
        .iter()
        .position(|line| line.trim() == "[General]")
        .map_or(0, |index| index + 1);
    for (offset, line) in missing.into_iter().enumerate() {
        lines.insert(insert_at + offset, line);
    }
    lines.join(newline) + newline
}

/// Takes the launcher's sync out of the instance, which keeps working without it.
pub fn forget(install: &Install, target: &Target) -> Result<(), String> {
    let Some(text) = read_cfg(install, target) else {
        return Ok(());
    };
    if !cfg_get(&text, "PreLaunchCommand").is_some_and(|raw| cfg_decode(install, raw).contains(" --sync --dir ")) {
        return Ok(());
    }
    let cleared = set_keys(&text, &[("OverrideCommands", "false".into()), ("PreLaunchCommand", String::new())]);
    fs::write(instance_dir(install, target).join("instance.cfg"), cleared).map_err(|e| e.to_string())
}

/// Starts the instance; Prism also joins the server, MultiMC stops at the menu.
pub fn launch(install: &Install, target: &Target) -> Result<(), String> {
    let executable = install
        .executable
        .as_ref()
        .ok_or("Prism Launcher est introuvable : ouvre-le et lance l'instance toi-même.")?;
    let address = format!("{SERVER_HOST}:{}", target.server_port);
    match install.flavor {
        Flavor::Prism => spawn(executable, &["--launch", &target.profile_path, "--server", &address]),
        Flavor::MultiMc => spawn(executable, &["--launch", &target.profile_path]),
    }
}

#[cfg(test)]
mod tests {
    use super::{cfg_decode, cfg_value, set_keys, Flavor, Install};

    #[test]
    fn values_survive_both_formats() {
        let command = r#""C:/Users/a b/harpy-launcher.exe" --sync --dir "C:/x/minecraft" --channel prod"#;
        for flavor in [Flavor::Prism, Flavor::MultiMc] {
            let install = Install { data: Default::default(), executable: None, flavor };
            assert_eq!(cfg_decode(&install, &cfg_value(&install, command)), command);
        }
        let prism = Install { data: Default::default(), executable: None, flavor: Flavor::Prism };
        // As Qt writes it back, and wrapped in quotes as Qt also accepts.
        let saved = r#"\"C:/Users/a b/harpy-launcher.exe\" --sync --dir \"C:/x/minecraft\" --channel prod"#;
        assert_eq!(cfg_decode(&prism, saved), command);
        assert_eq!(cfg_decode(&prism, &format!("\"{saved}\"")), command);
    }

    #[test]
    fn keys_go_in_the_general_section() {
        let text = "[General]\nname=x\nOverrideCommands=false\n\n[UI]\nmods_Page\\Columns=\"a\"\n";
        let out = set_keys(text, &[("OverrideCommands", "true".into()), ("PreLaunchCommand", "\"y\"".into())]);
        assert_eq!(
            out,
            "[General]\nPreLaunchCommand=\"y\"\nname=x\nOverrideCommands=true\n\n[UI]\nmods_Page\\Columns=\"a\"\n"
        );
        assert_eq!(set_keys("", &[("a", "1".into())]), "a=1\n");
    }
}
