use keyring::Entry;
use serde::{Deserialize, Serialize};
use log::info;

const SERVICE_NAME: &str = "tapo-controller";
const PASSWORD_KEY: &str = "tapo-password";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredentials {
    pub email: String,
    pub password: String,
    #[serde(rename = "rememberMe")]
    pub remember_me: bool,
}

/// Save password to OS keyring (Windows Credential Manager / Linux Secret Service / macOS Keychain)
pub fn save_password_to_keyring(password: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, PASSWORD_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry.set_password(password)
        .map_err(|e| format!("Failed to save password to keyring: {}", e))?;
    info!("Password saved to OS keyring");
    Ok(())
}

/// Get password from OS keyring
pub fn get_password_from_keyring() -> Option<String> {
    let entry = Entry::new(SERVICE_NAME, PASSWORD_KEY).ok()?;
    entry.get_password().ok()
}

/// Clear password from OS keyring
pub fn clear_password_from_keyring() -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, PASSWORD_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!("Password cleared from OS keyring");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()), // Already cleared
        Err(e) => Err(format!("Failed to clear password from keyring: {}", e)),
    }
}
