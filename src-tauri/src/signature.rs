//! Checks that a file was published from the publisher's PC. Launcher
//! installers and pack manifests are signed there with a key that is in no
//! repository, so taking over the GitHub account is not enough to push
//! anything to players.

use crate::config::SIGNING_PUBLIC_KEY;
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};

/// `signature` is the content of a `.sig` file made by `tauri signer sign`.
pub fn verify(data: &[u8], signature: &str) -> bool {
    let decode = |text: &str| {
        STANDARD
            .decode(text.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    };
    let Some(key) = decode(SIGNING_PUBLIC_KEY).and_then(|text| PublicKey::decode(&text).ok()) else {
        return false;
    };
    let Some(signature) = decode(signature).and_then(|text| Signature::decode(&text).ok()) else {
        return false;
    };
    key.verify(data, &signature, true).is_ok()
}
