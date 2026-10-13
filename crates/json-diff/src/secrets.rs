//! API keys, remembered in the operating system's keychain: Windows
//! Credential Manager, or the Secret Service (GNOME Keyring, KWallet) on
//! Linux. Never in settings.json.
//!
//! The keychain is optional. Without one (a Linux session with no Secret
//! Service), keys simply are not remembered and the user is told why.
//!
//! These calls block; run them off the UI thread.

use keyring::Entry;

/// The keychain's name for json-diff's entries; one entry per provider.
const SERVICE: &str = "io.github.ultra-infinity.json-diff";

/// The provider a `provider:model` reference names, which keys are filed
/// under: `openai:gpt-5.5` → `openai`.
pub fn provider_of(reference: &str) -> &str {
    reference
        .split_once(':')
        .map_or(reference, |(provider, _)| provider)
        .trim()
}

fn entry(provider: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, provider).map_err(describe)
}

pub fn save(provider: &str, key: &str) -> Result<(), String> {
    entry(provider)?.set_password(key).map_err(describe)
}

/// The saved key for `provider`, if there is one and the keychain is
/// reachable.
pub fn load(provider: &str) -> Option<String> {
    entry(provider).ok()?.get_password().ok()
}

pub fn forget(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(describe(error)),
    }
}

fn describe(error: keyring::Error) -> String {
    match error {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            format!("no system keychain is available ({error})")
        }
        error => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_filed_by_provider() {
        assert_eq!(provider_of("openai:gpt-5.5"), "openai");
        assert_eq!(provider_of("ollama:qwen3:4b"), "ollama");
        assert_eq!(provider_of("minimax/openai:MiniMax-M3"), "minimax/openai");
        assert_eq!(provider_of("anthropic"), "anthropic");
    }
}
