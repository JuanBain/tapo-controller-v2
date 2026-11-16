//! Single owner of the Tapo account secret.
//!
//! - The password is persisted only in the OS keyring (Windows Credential
//!   Manager), and only when "Remember me" is enabled.
//! - Email and the remember-me flag are non-secret and live in
//!   `data/account.json`.
//! - The active session is kept in memory for device connections.

use std::sync::{Arc, RwLock};

use keyring::Entry;
use log::{info, warn};
use serde::{Deserialize, Serialize};

use crate::storage::{AccountSettings, Storage};

const SERVICE_NAME: &str = "tapo-controller";
const PASSWORD_KEY: &str = "tapo-password";

// ============ Secret store ============

pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, secret: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

/// Windows Credential Manager / macOS Keychain / Secret Service entry.
/// Same service and key as upstream so existing entries keep working.
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry() -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, PASSWORD_KEY)
            .map_err(|e| format!("Could not access the OS credential store: {}", e))
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self) -> Result<Option<String>, String> {
        match Self::entry()?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Could not read from the OS credential store: {}", e)),
        }
    }

    fn set(&self, secret: &str) -> Result<(), String> {
        Self::entry()?
            .set_password(secret)
            .map_err(|e| format!("Could not save to the OS credential store: {}", e))
    }

    fn delete(&self) -> Result<(), String> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Could not remove from the OS credential store: {}", e)),
        }
    }
}

// ============ Session ============

#[derive(Clone)]
pub struct Session {
    pub email: String,
    pub password: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("email", &self.email)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Shape returned to the frontend for the login form. The password is never
/// sent back to the webview; the field is kept for IPC compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCredentials {
    pub email: String,
    pub password: String,
    #[serde(rename = "rememberMe")]
    pub remember_me: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStatus {
    /// `account.json` already exists; legacy files are not consulted.
    AlreadyMigrated,
    NothingToMigrate,
    /// Only email / remember-me were imported from the plugin-store file.
    PreferencesOnly,
    /// Password copied to the keyring and read back successfully (or the
    /// keyring already held the identical value).
    PasswordMigrated,
    /// Remember-me was off: preferences imported, password not persisted.
    NotRemembered,
    /// The keyring holds a different password. Nothing was overwritten.
    Conflict,
    /// Keyring write or verification failed. Nothing was committed.
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct RestoreReport {
    pub migration: MigrationStatus,
    pub session_restored: bool,
    /// The password was read twice from the OS store on this launch and both
    /// reads matched. On the migration launch this can already be true, but
    /// deletion still waits for a later launch (`AlreadyMigrated`).
    pub keyring_verified: bool,
    /// The legacy file was parsed and its password matches the value recovered
    /// from the OS store. An unreadable file does not match.
    pub legacy_matches_keyring: bool,
    pub legacy_plaintext_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyCleanup {
    NotPresent,
    /// Caller may delete `data/credentials.json` and nothing else.
    Removed,
    Kept(&'static str),
}

/// Decides whether the legacy plain-text file may be deleted.
///
/// All of these must hold: the migration was already committed on a previous
/// launch, this launch recovered the password from the OS store, and every
/// saved device reconnected. Any gap keeps the file.
pub fn legacy_cleanup_decision(
    report: &RestoreReport,
    saved_devices: usize,
    reconnected_devices: usize,
) -> LegacyCleanup {
    if !report.legacy_plaintext_present {
        return LegacyCleanup::NotPresent;
    }
    if report.migration != MigrationStatus::AlreadyMigrated {
        return LegacyCleanup::Kept(
            "this launch performed the migration; a restart must confirm it",
        );
    }
    if !report.session_restored || !report.keyring_verified || !report.legacy_matches_keyring {
        return LegacyCleanup::Kept(
            "the password was not recovered from Windows Credential Manager or does not match data/credentials.json",
        );
    }
    if saved_devices == 0 {
        return LegacyCleanup::Kept("there are no saved devices to verify reconnection");
    }
    if reconnected_devices != saved_devices {
        return LegacyCleanup::Kept("not every saved device reconnected");
    }
    LegacyCleanup::Removed
}

// ============ Manager ============

pub struct CredentialManager {
    secrets: Box<dyn SecretStore>,
    storage: Arc<Storage>,
    session: RwLock<Option<Session>>,
}

impl CredentialManager {
    pub fn new(secrets: Box<dyn SecretStore>, storage: Arc<Storage>) -> Self {
        Self { secrets, storage, session: RwLock::new(None) }
    }

    pub fn session(&self) -> Option<Session> {
        self.session.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    fn set_session(&self, session: Option<Session>) {
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = session;
    }

    /// Credentials to pre-fill the login form (only with remember-me).
    pub fn saved_credentials(&self) -> Option<SavedCredentials> {
        let account = self.storage.load_account()?;
        match account.email {
            Some(email) if account.remember_me && !email.is_empty() => Some(SavedCredentials {
                email,
                password: String::new(),
                remember_me: true,
            }),
            _ => None,
        }
    }

    /// Starts a session. `remember = None` keeps the stored preference.
    pub fn sign_in(&self, email: &str, password: &str, remember: Option<bool>) -> Result<(), String> {
        let remember = match remember {
            Some(r) => r,
            None => self.storage.load_account().map(|a| a.remember_me).unwrap_or(false),
        };

        if remember {
            self.secrets.set(password)?;
        } else if let Err(e) = self.secrets.delete() {
            // Not fatal: with remember-me off a leftover entry is never read.
            warn!("{}", e);
        }
        self.storage.save_account(&AccountSettings {
            email: Some(email.to_string()),
            remember_me: remember,
        })?;
        self.set_session(Some(Session { email: email.to_string(), password: password.to_string() }));
        info!("Signed in (remember me: {})", remember);
        Ok(())
    }

    /// Ends the session and removes every persisted copy of the password,
    /// including the upstream plain-text file.
    pub fn sign_out(&self) -> Result<(), String> {
        self.set_session(None);
        self.forget()?;
        self.storage.remove_legacy_credentials()?;
        info!("Signed out; stored credentials removed");
        Ok(())
    }

    /// Removes the remembered password and preferences without touching
    /// the in-memory session.
    pub fn forget(&self) -> Result<(), String> {
        self.secrets.delete()?;
        // An explicit (empty) account file marks migration as done so legacy
        // files are not re-imported on the next start.
        self.storage.save_account(&AccountSettings::default())
    }

    /// Startup: migrate legacy storage if needed, then restore a remembered
    /// session from the keyring.
    pub fn restore(&self) -> RestoreReport {
        let migration = match self.storage.load_account() {
            Some(_) => MigrationStatus::AlreadyMigrated,
            None => self.migrate_legacy(),
        };

        let mut session_restored = self.is_signed_in();
        let mut keyring_verified = false;
        let mut legacy_matches_keyring = false;
        if !session_restored {
            if let Some(AccountSettings { email: Some(email), remember_me: true }) = self.storage.load_account() {
                let legacy = self.storage.load_legacy_credentials();
                match (self.secrets.get(), self.secrets.get()) {
                    (Ok(Some(first)), Ok(Some(second))) if !first.is_empty() && first == second => {
                        keyring_verified = true;
                        match &legacy {
                            Some(legacy) if legacy.password == first && legacy.email == email => {
                                legacy_matches_keyring = true;
                                self.set_session(Some(Session { email, password: first }));
                                session_restored = true;
                            }
                            Some(legacy) => {
                                warn!("Recovered password does not match data/credentials.json; the file was kept");
                                self.set_session(Some(Session {
                                    email: legacy.email.clone(),
                                    password: legacy.password.clone(),
                                }));
                                session_restored = true;
                            }
                            None => {
                                self.set_session(Some(Session { email, password: first }));
                                session_restored = true;
                            }
                        }
                    }
                    (Ok(Some(_)), Ok(Some(_))) => {
                        warn!("Windows Credential Manager returned inconsistent or empty values; session not restored from it");
                    }
                    (Ok(None), _) | (_, Ok(None)) => {
                        warn!("Remember me is on but no password is stored in Windows Credential Manager");
                    }
                    (Err(e), _) | (_, Err(e)) => warn!("{}", e),
                }
                if !session_restored {
                    if let Some(legacy) = legacy {
                        warn!("Using the legacy credentials for this launch only; data/credentials.json was kept");
                        self.set_session(Some(Session {
                            email: legacy.email,
                            password: legacy.password,
                        }));
                        session_restored = true;
                    }
                }
            }
        }

        let report = RestoreReport {
            migration,
            session_restored,
            keyring_verified,
            legacy_matches_keyring,
            legacy_plaintext_present: self.storage.legacy_credentials_present(),
        };
        info!(
            "Credential restore: migration={:?}, session_restored={}, keyring_verified={}, legacy_matches={}",
            report.migration, report.session_restored, report.keyring_verified, report.legacy_matches_keyring
        );
        if report.legacy_plaintext_present && !report.keyring_verified {
            warn!("Keeping data/credentials.json until a later launch recovers the password and reconnects every saved device");
        }
        report
    }

    /// Never deletes or overwrites existing secrets. On conflict or failure,
    /// `account.json` is not written so the migration is retried next start,
    /// and the legacy password is used in memory only for this run.
    fn migrate_legacy(&self) -> MigrationStatus {
        let prefs = self.storage.load_legacy_store_prefs();
        let Some(legacy) = self.storage.load_legacy_credentials() else {
            return match prefs {
                Some(p) => {
                    let account = AccountSettings {
                        email: p.email.filter(|e| !e.is_empty()),
                        remember_me: p.remember_me.unwrap_or(false),
                    };
                    match self.storage.save_account(&account) {
                        Ok(()) => MigrationStatus::PreferencesOnly,
                        Err(e) => MigrationStatus::Failed(e),
                    }
                }
                None => MigrationStatus::NothingToMigrate,
            };
        };

        // Upstream always kept the session, so a missing preference means
        // the user was effectively remembered.
        let remember = prefs.and_then(|p| p.remember_me).unwrap_or(true);
        let account = AccountSettings { email: Some(legacy.email.clone()), remember_me: remember };

        if !remember {
            return match self.storage.save_account(&account) {
                Ok(()) => MigrationStatus::NotRemembered,
                Err(e) => MigrationStatus::Failed(e),
            };
        }

        let status = match self.secrets.get() {
            Ok(Some(existing)) if existing == legacy.password => MigrationStatus::PasswordMigrated,
            Ok(Some(_)) => MigrationStatus::Conflict,
            Ok(None) => match self.secrets.set(&legacy.password).and_then(|_| self.secrets.get()) {
                Ok(Some(read_back)) if read_back == legacy.password => MigrationStatus::PasswordMigrated,
                Ok(_) => MigrationStatus::Failed("keyring read-back did not match".into()),
                Err(e) => MigrationStatus::Failed(e),
            },
            Err(e) => MigrationStatus::Failed(e),
        };

        match status {
            MigrationStatus::PasswordMigrated => match self.storage.save_account(&account) {
                Ok(()) => MigrationStatus::PasswordMigrated,
                Err(e) => {
                    self.set_session(Some(Session { email: legacy.email, password: legacy.password }));
                    MigrationStatus::Failed(e)
                }
            },
            other => {
                warn!("Legacy credential migration not completed: {:?}", other);
                self.set_session(Some(Session { email: legacy.email, password: legacy.password }));
                other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::TempDir;
    use std::fs;
    use std::sync::Mutex;

    const SECRET: &str = "dummy-test-secret";

    #[derive(Default)]
    struct MemorySecretStore {
        value: Mutex<Option<String>>,
        fail_set: bool,
        corrupt_read_back: bool,
    }

    impl SecretStore for Arc<MemorySecretStore> {
        fn get(&self) -> Result<Option<String>, String> {
            let v = self.value.lock().unwrap().clone();
            Ok(if self.corrupt_read_back { v.map(|s| format!("{s}-x")) } else { v })
        }
        fn set(&self, secret: &str) -> Result<(), String> {
            if self.fail_set {
                return Err("store unavailable".into());
            }
            *self.value.lock().unwrap() = Some(secret.to_string());
            Ok(())
        }
        fn delete(&self) -> Result<(), String> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }

    fn setup(store: MemorySecretStore) -> (TempDir, Arc<Storage>, Arc<MemorySecretStore>, CredentialManager) {
        let tmp = TempDir::new();
        let storage = Arc::new(Storage::new(tmp.0.clone()));
        let store = Arc::new(store);
        let manager = CredentialManager::new(Box::new(store.clone()), storage.clone());
        (tmp, storage, store, manager)
    }

    fn write_legacy(tmp: &TempDir, remember: Option<bool>) {
        fs::write(
            tmp.0.join("data").join("credentials.json"),
            format!(r#"{{"email":"user@example.com","password":"{SECRET}","device_ip":null}}"#),
        ).unwrap();
        if let Some(r) = remember {
            fs::write(tmp.0.join("credentials.json"), format!(r#"{{"email":"user@example.com","rememberMe":{r}}}"#)).unwrap();
        }
    }

    fn data_files_contain_secret(tmp: &TempDir) -> bool {
        fs::read_dir(tmp.0.join("data")).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "credentials.json")
            .any(|e| fs::read_to_string(e.path()).unwrap_or_default().contains(SECRET))
    }

    #[test]
    fn sign_in_with_remember_uses_keyring_not_json() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(true)).unwrap();

        assert_eq!(store.value.lock().unwrap().as_deref(), Some(SECRET));
        assert!(manager.is_signed_in());
        assert!(!data_files_contain_secret(&tmp));
        assert!(!storage.legacy_credentials_present());
        assert_eq!(storage.load_account().unwrap().remember_me, true);
    }

    #[test]
    fn sign_in_without_remember_keeps_secret_in_memory_only() {
        let (tmp, _storage, store, manager) = setup(MemorySecretStore::default());
        *store.value.lock().unwrap() = Some("stale".into());
        manager.sign_in("user@example.com", SECRET, Some(false)).unwrap();

        assert_eq!(*store.value.lock().unwrap(), None);
        assert_eq!(manager.session().unwrap().password, SECRET);
        assert!(!data_files_contain_secret(&tmp));
        assert!(manager.saved_credentials().is_none());
    }

    #[test]
    fn sign_in_with_unspecified_remember_keeps_preference() {
        let (_tmp, storage, store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(true)).unwrap();
        manager.sign_in("user@example.com", "other", None).unwrap();
        assert!(storage.load_account().unwrap().remember_me);
        assert_eq!(store.value.lock().unwrap().as_deref(), Some("other"));
    }

    #[test]
    fn restore_after_remembered_sign_in() {
        let (tmp, _storage, store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(true)).unwrap();

        let storage2 = Arc::new(Storage::new(tmp.0.clone()));
        let manager2 = CredentialManager::new(Box::new(store.clone()), storage2);
        let report = manager2.restore();
        assert_eq!(report.migration, MigrationStatus::AlreadyMigrated);
        assert!(report.session_restored);
        assert_eq!(manager2.session().unwrap().email, "user@example.com");
    }

    #[test]
    fn restore_without_remember_starts_signed_out() {
        let (tmp, _storage, store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(false)).unwrap();
        let manager2 = CredentialManager::new(Box::new(store.clone()), Arc::new(Storage::new(tmp.0.clone())));
        assert!(!manager2.restore().session_restored);
        assert!(!manager2.is_signed_in());
    }

    #[test]
    fn saved_credentials_never_include_password() {
        let (_tmp, _storage, _store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(true)).unwrap();
        let saved = manager.saved_credentials().unwrap();
        assert_eq!(saved.email, "user@example.com");
        assert!(saved.password.is_empty());
        assert!(!serde_json::to_string(&saved).unwrap().contains(SECRET));
        assert!(!format!("{:?}", manager.session().unwrap()).contains(SECRET));
    }

    #[test]
    fn sign_out_removes_keyring_and_legacy_file() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, Some(true));
        manager.restore();
        assert!(storage.legacy_credentials_present());

        manager.sign_out().unwrap();
        assert!(!manager.is_signed_in());
        assert_eq!(*store.value.lock().unwrap(), None);
        assert!(!storage.legacy_credentials_present());
        // Legacy files are not re-imported afterwards.
        let manager2 = CredentialManager::new(Box::new(store.clone()), Arc::new(Storage::new(tmp.0.clone())));
        let report = manager2.restore();
        assert_eq!(report.migration, MigrationStatus::AlreadyMigrated);
        assert!(!report.session_restored);
    }

    #[test]
    fn migration_copies_password_and_verifies() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, Some(true));

        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::PasswordMigrated);
        assert!(report.session_restored);
        assert!(report.legacy_plaintext_present, "legacy file must not be deleted automatically");
        assert_eq!(store.value.lock().unwrap().as_deref(), Some(SECRET));
        assert_eq!(storage.load_account().unwrap(), AccountSettings {
            email: Some("user@example.com".into()), remember_me: true,
        });
        assert!(!data_files_contain_secret(&tmp));
    }

    #[test]
    fn migration_without_prefs_preserves_upstream_auto_login() {
        let (tmp, _storage, _store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, None);
        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::PasswordMigrated);
        assert!(report.session_restored);
    }

    #[test]
    fn migration_is_idempotent_when_keyring_already_matches() {
        let (tmp, _storage, store, manager) = setup(MemorySecretStore::default());
        *store.value.lock().unwrap() = Some(SECRET.into());
        write_legacy(&tmp, Some(true));
        assert_eq!(manager.restore().migration, MigrationStatus::PasswordMigrated);
    }

    #[test]
    fn migration_respects_remember_me_off() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, Some(false));
        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::NotRemembered);
        assert!(!report.session_restored);
        assert_eq!(*store.value.lock().unwrap(), None);
        assert!(!storage.load_account().unwrap().remember_me);
    }

    #[test]
    fn migration_conflict_never_overwrites_keyring() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        *store.value.lock().unwrap() = Some("different".into());
        write_legacy(&tmp, Some(true));

        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::Conflict);
        assert_eq!(store.value.lock().unwrap().as_deref(), Some("different"));
        assert!(storage.load_account().is_none(), "migration must be retried");
        assert!(report.session_restored, "current run keeps working with legacy credentials");
        assert_eq!(manager.session().unwrap().password, SECRET);
    }

    #[test]
    fn migration_failure_commits_nothing() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore { fail_set: true, ..Default::default() });
        write_legacy(&tmp, Some(true));
        let report = manager.restore();
        assert!(matches!(report.migration, MigrationStatus::Failed(_)));
        assert_eq!(*store.value.lock().unwrap(), None);
        assert!(storage.load_account().is_none());
        assert!(storage.legacy_credentials_present());
        assert!(report.session_restored);
    }

    #[test]
    fn migration_read_back_mismatch_is_failure() {
        let (tmp, storage, _store, manager) = setup(MemorySecretStore { corrupt_read_back: true, ..Default::default() });
        write_legacy(&tmp, Some(true));
        let report = manager.restore();
        assert!(matches!(report.migration, MigrationStatus::Failed(_)));
        assert!(storage.load_account().is_none());
    }

    #[test]
    fn migration_imports_preferences_only() {
        let (tmp, storage, _store, manager) = setup(MemorySecretStore::default());
        fs::write(tmp.0.join("credentials.json"), r#"{"email":"user@example.com","rememberMe":true}"#).unwrap();
        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::PreferencesOnly);
        assert!(!report.session_restored);
        assert!(storage.load_account().unwrap().remember_me);
    }

    #[test]
    fn fresh_install_has_nothing_to_migrate() {
        let (_tmp, storage, _store, manager) = setup(MemorySecretStore::default());
        let report = manager.restore();
        assert_eq!(report.migration, MigrationStatus::NothingToMigrate);
        assert!(!report.session_restored);
        assert!(storage.load_account().is_none());
    }

    fn saved_config(tmp: &TempDir) {
        fs::write(tmp.0.join("data").join("devices.json"), r#"[{"ip":"192.168.1.10","nickname":"Desk","model":"L930","added_at":null}]"#).unwrap();
        fs::write(tmp.0.join("data").join("settings.json"), r#"{"last_device_ip":"192.168.1.10"}"#).unwrap();
        fs::write(tmp.0.join("data").join("presets.json"), "[]").unwrap();
    }

    #[test]
    fn plaintext_is_removed_only_after_a_verified_restart_and_reconnection() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, Some(true));
        saved_config(&tmp);
        let devices_before = fs::read(tmp.0.join("data").join("devices.json")).unwrap();
        let settings_before = fs::read(tmp.0.join("data").join("settings.json")).unwrap();

        let first = manager.restore();
        assert_eq!(first.migration, MigrationStatus::PasswordMigrated);
        assert!(first.keyring_verified);
        assert!(first.legacy_matches_keyring);
        assert_eq!(
            legacy_cleanup_decision(&first, 1, 1),
            LegacyCleanup::Kept("this launch performed the migration; a restart must confirm it")
        );
        assert!(storage.legacy_credentials_present());

        let storage2 = Arc::new(Storage::new(tmp.0.clone()));
        let manager2 = CredentialManager::new(Box::new(store.clone()), storage2.clone());
        let second = manager2.restore();
        assert_eq!(second.migration, MigrationStatus::AlreadyMigrated);
        assert!(second.session_restored);
        assert!(second.keyring_verified);
        assert!(second.legacy_matches_keyring);
        assert_eq!(manager2.session().unwrap().password, SECRET);

        assert_eq!(
            legacy_cleanup_decision(&second, 1, 0),
            LegacyCleanup::Kept("not every saved device reconnected")
        );
        assert_eq!(
            legacy_cleanup_decision(&second, 0, 0),
            LegacyCleanup::Kept("there are no saved devices to verify reconnection")
        );
        assert!(storage2.legacy_credentials_present());

        assert_eq!(legacy_cleanup_decision(&second, 2, 2), LegacyCleanup::Removed);
        storage2.remove_legacy_credentials().unwrap();
        assert!(!storage2.legacy_credentials_present());
        assert_eq!(fs::read(tmp.0.join("data").join("devices.json")).unwrap(), devices_before);
        assert_eq!(fs::read(tmp.0.join("data").join("settings.json")).unwrap(), settings_before);
        assert_eq!(fs::read_to_string(tmp.0.join("data").join("presets.json")).unwrap(), "[]");
        assert_eq!(store.value.lock().unwrap().as_deref(), Some(SECRET));
    }

    #[test]
    fn mismatched_keyring_keeps_the_plaintext_file() {
        let (tmp, storage, store, manager) = setup(MemorySecretStore::default());
        write_legacy(&tmp, Some(true));
        manager.restore();

        *store.value.lock().unwrap() = Some("different".into());
        let storage2 = Arc::new(Storage::new(tmp.0.clone()));
        let manager2 = CredentialManager::new(Box::new(store.clone()), storage2.clone());
        let report = manager2.restore();
        assert!(report.keyring_verified);
        assert!(!report.legacy_matches_keyring);
        assert_eq!(manager2.session().unwrap().password, SECRET);
        assert!(matches!(legacy_cleanup_decision(&report, 1, 1), LegacyCleanup::Kept(_)));
        assert!(storage.legacy_credentials_present());
        assert_eq!(store.value.lock().unwrap().as_deref(), Some("different"));
    }

    #[test]
    fn unreadable_plaintext_file_is_not_eligible_for_deletion() {
        let (tmp, _storage, store, manager) = setup(MemorySecretStore::default());
        manager.sign_in("user@example.com", SECRET, Some(true)).unwrap();
        fs::write(tmp.0.join("data").join("credentials.json"), "garbage").unwrap();

        let storage2 = Arc::new(Storage::new(tmp.0.clone()));
        let manager2 = CredentialManager::new(Box::new(store), storage2);
        let report = manager2.restore();
        assert!(report.legacy_plaintext_present);
        assert!(!report.legacy_matches_keyring);
        assert!(matches!(legacy_cleanup_decision(&report, 1, 1), LegacyCleanup::Kept(_)));
        assert_eq!(fs::read_to_string(tmp.0.join("data").join("credentials.json")).unwrap(), "garbage");
    }
}
