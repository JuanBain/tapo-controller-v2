use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;
use chrono::Local;
use log::warn;

// ============ Models ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub hue: Option<i32>,
    pub saturation: Option<i32>,
    pub brightness: i32,
    pub color_temperature: Option<i32>,
    pub effect: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub time: String, // HH:MM format
    pub days: Vec<i32>, // 0=Monday, 6=Sunday
    pub action: String, // "on", "off", "preset"
    pub preset_id: Option<String>,
    pub brightness: Option<i32>,
    pub enabled: bool,
    pub created_at: String,
    /// Target device. `None` is only valid while a single device is connected.
    #[serde(default)]
    pub device_ip: Option<String>,
}

/// Format written by upstream <= 2.0.0 to `data/credentials.json`, with the
/// password in plain text. Only ever read, for migration.
#[derive(Clone, Deserialize)]
pub struct LegacyCredentials {
    pub email: String,
    pub password: String,
}

impl std::fmt::Debug for LegacyCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LegacyCredentials")
            .field("email", &self.email)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Format written by upstream <= 2.0.0 through tauri-plugin-store to
/// `<app_data>/credentials.json` (no secrets).
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyStorePrefs {
    pub email: Option<String>,
    #[serde(rename = "rememberMe")]
    pub remember_me: Option<bool>,
}

/// Non-secret account preferences. The password lives in the OS keyring.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSettings {
    pub email: Option<String>,
    pub remember_me: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub last_device_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDevice {
    pub ip: String,
    pub nickname: Option<String>,
    pub model: Option<String>,
    pub added_at: Option<String>,
}

// ============ Storage Manager ============

pub struct Storage {
    app_data_dir: PathBuf,
    data_dir: PathBuf,
    write_lock: Mutex<()>,
}

impl Storage {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let data_dir = app_data_dir.join("data");
        if let Err(e) = fs::create_dir_all(&data_dir) {
            warn!("Could not create data directory: {}", e);
        }
        Self { app_data_dir, data_dir, write_lock: Mutex::new(()) }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn legacy_credentials_path(&self) -> PathBuf {
        self.data_dir.join("credentials.json")
    }

    fn legacy_store_prefs_path(&self) -> PathBuf {
        self.app_data_dir.join("credentials.json")
    }

    fn account_path(&self) -> PathBuf {
        self.data_dir.join("account.json")
    }

    fn presets_path(&self) -> PathBuf {
        self.data_dir.join("presets.json")
    }

    fn schedules_path(&self) -> PathBuf {
        self.data_dir.join("schedules.json")
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    fn devices_path(&self) -> PathBuf {
        self.data_dir.join("devices.json")
    }

    /// Returns `None` when the file is missing. A file that exists but cannot
    /// be parsed is moved aside so a later save cannot silently destroy it.
    fn load_json<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Option<T> {
        let content = fs::read_to_string(path).ok()?;
        match serde_json::from_str(&content) {
            Ok(value) => Some(value),
            Err(e) => {
                let backup = path.with_extension(format!(
                    "json.corrupt-{}",
                    Local::now().format("%Y%m%d%H%M%S")
                ));
                warn!(
                    "Could not parse {}: {}. Moving it to {}",
                    file_name(path), e, file_name(&backup)
                );
                if let Err(e) = fs::rename(path, &backup) {
                    warn!("Could not back up {}: {}", file_name(path), e);
                }
                None
            }
        }
    }

    /// Reads a file without ever modifying it (used for legacy formats).
    fn read_json_untouched<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
        let content = fs::read_to_string(path).ok()?;
        match serde_json::from_str(&content) {
            Ok(value) => Some(value),
            Err(_) => {
                warn!("Ignoring unreadable legacy file {}", file_name(path));
                None
            }
        }
    }

    /// Writes to a temporary file and renames it over the target so readers
    /// never observe a partially written file.
    fn save_json<T: Serialize>(&self, path: &Path, data: &T) -> Result<(), String> {
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| format!("Serialize error: {}", e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, content)
            .map_err(|e| format!("Write error ({}): {}", file_name(path), e))?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Write error ({}): {}", file_name(path), e)
        })
    }

    // ============ Account (non-secret) ============

    pub fn load_account(&self) -> Option<AccountSettings> {
        self.load_json(&self.account_path())
    }

    pub fn save_account(&self, account: &AccountSettings) -> Result<(), String> {
        let _guard = self.lock();
        self.save_json(&self.account_path(), account)
    }

    // ============ Legacy credentials (migration only) ============

    pub fn load_legacy_credentials(&self) -> Option<LegacyCredentials> {
        Self::read_json_untouched(&self.legacy_credentials_path())
    }

    pub fn legacy_credentials_present(&self) -> bool {
        self.legacy_credentials_path().exists()
    }

    pub fn remove_legacy_credentials(&self) -> Result<(), String> {
        let path = self.legacy_credentials_path();
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Delete error: {}", e))?;
        }
        Ok(())
    }

    pub fn load_legacy_store_prefs(&self) -> Option<LegacyStorePrefs> {
        Self::read_json_untouched(&self.legacy_store_prefs_path())
    }

    // ============ Presets ============

    pub fn get_presets(&self) -> Vec<Preset> {
        self.load_json::<Vec<Preset>>(&self.presets_path())
            .unwrap_or_default()
    }

    pub fn save_preset(&self, mut preset: Preset) -> Result<Preset, String> {
        let _guard = self.lock();
        let mut presets = self.get_presets();
        preset.id = Uuid::new_v4().to_string()[..8].to_string();
        preset.created_at = Local::now().to_rfc3339();
        presets.push(preset.clone());
        self.save_json(&self.presets_path(), &presets)?;
        Ok(preset)
    }

    pub fn delete_preset(&self, preset_id: &str) -> bool {
        let _guard = self.lock();
        let mut presets = self.get_presets();
        let original_len = presets.len();
        presets.retain(|p| p.id != preset_id);
        if presets.len() == original_len {
            return false;
        }
        self.save_json(&self.presets_path(), &presets).is_ok()
    }

    pub fn get_preset(&self, preset_id: &str) -> Option<Preset> {
        self.get_presets().into_iter().find(|p| p.id == preset_id)
    }

    // ============ Schedules ============

    pub fn get_schedules(&self) -> Vec<Schedule> {
        self.load_json::<Vec<Schedule>>(&self.schedules_path())
            .unwrap_or_default()
    }

    pub fn save_schedule(&self, mut schedule: Schedule) -> Result<Schedule, String> {
        let _guard = self.lock();
        let mut schedules = self.get_schedules();
        schedule.id = Uuid::new_v4().to_string()[..8].to_string();
        schedule.created_at = Local::now().to_rfc3339();
        schedules.push(schedule.clone());
        self.save_json(&self.schedules_path(), &schedules)?;
        Ok(schedule)
    }

    pub fn update_schedule(&self, schedule_id: &str, mut schedule: Schedule) -> Option<Schedule> {
        let _guard = self.lock();
        let mut schedules = self.get_schedules();
        if let Some(idx) = schedules.iter().position(|s| s.id == schedule_id) {
            schedule.id = schedule_id.to_string();
            schedule.created_at = schedules[idx].created_at.clone();
            schedules[idx] = schedule.clone();
            self.save_json(&self.schedules_path(), &schedules).ok()?;
            Some(schedule)
        } else {
            None
        }
    }

    pub fn delete_schedule(&self, schedule_id: &str) -> bool {
        let _guard = self.lock();
        let mut schedules = self.get_schedules();
        let original_len = schedules.len();
        schedules.retain(|s| s.id != schedule_id);
        if schedules.len() == original_len {
            return false;
        }
        self.save_json(&self.schedules_path(), &schedules).is_ok()
    }

    // ============ Settings ============

    pub fn load_settings(&self) -> Settings {
        self.load_json(&self.settings_path()).unwrap_or_default()
    }

    pub fn save_last_ip(&self, ip: &str) -> Result<(), String> {
        let _guard = self.lock();
        let mut settings = self.load_settings();
        settings.last_device_ip = Some(ip.to_string());
        self.save_json(&self.settings_path(), &settings)
    }

    pub fn get_last_ip(&self) -> Option<String> {
        self.load_settings().last_device_ip
    }

    // ============ Saved Devices ============

    pub fn get_saved_devices(&self) -> Vec<SavedDevice> {
        self.load_json::<Vec<SavedDevice>>(&self.devices_path())
            .unwrap_or_default()
    }

    pub fn save_device_info(&self, mut device: SavedDevice) -> Result<SavedDevice, String> {
        let _guard = self.lock();
        let mut devices = self.get_saved_devices();
        if let Some(idx) = devices.iter().position(|d| d.ip == device.ip) {
            if device.added_at.is_none() {
                device.added_at = devices[idx].added_at.clone();
            }
            devices[idx] = device.clone();
        } else {
            device.added_at = Some(Local::now().to_rfc3339());
            devices.push(device.clone());
        }
        self.save_json(&self.devices_path(), &devices)?;
        Ok(device)
    }

    pub fn remove_saved_device(&self, ip: &str) -> bool {
        let _guard = self.lock();
        let mut devices = self.get_saved_devices();
        let original_len = devices.len();
        devices.retain(|d| d.ip != ip);
        if devices.len() == original_len {
            return false;
        }
        self.save_json(&self.devices_path(), &devices).is_ok()
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Temporary app data directory, removed on drop.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("tapo-controller-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn preset(name: &str) -> Preset {
        Preset {
            id: String::new(),
            name: name.into(),
            hue: Some(120),
            saturation: Some(80),
            brightness: 60,
            color_temperature: None,
            effect: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn presets_roundtrip_and_delete() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let saved = storage.save_preset(preset("Reading")).unwrap();
        assert_eq!(saved.id.len(), 8);
        assert_eq!(storage.get_presets().len(), 1);
        assert_eq!(storage.get_preset(&saved.id).unwrap().name, "Reading");
        assert!(storage.delete_preset(&saved.id));
        assert!(!storage.delete_preset(&saved.id));
        assert!(storage.get_presets().is_empty());
    }

    #[test]
    fn schedules_without_device_ip_still_load() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let legacy = r#"[{"id":"abc12345","name":"Wake","time":"07:00","days":[0],
            "action":"on","preset_id":null,"brightness":50,"enabled":true,"created_at":"x"}]"#;
        fs::write(tmp.0.join("data").join("schedules.json"), legacy).unwrap();
        let schedules = storage.get_schedules();
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].device_ip, None);
    }

    #[test]
    fn corrupt_file_is_backed_up_not_overwritten() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let path = tmp.0.join("data").join("presets.json");
        fs::write(&path, "{ not json").unwrap();

        assert!(storage.get_presets().is_empty());
        storage.save_preset(preset("New")).unwrap();

        let backups: Vec<_> = fs::read_dir(tmp.0.join("data")).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("presets.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "{ not json");
        assert_eq!(storage.get_presets().len(), 1);
    }

    #[test]
    fn save_leaves_no_temp_file() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        storage.save_last_ip("192.168.1.10").unwrap();
        assert_eq!(storage.get_last_ip().as_deref(), Some("192.168.1.10"));
        assert!(!tmp.0.join("data").join("settings.json.tmp").exists());
    }

    #[test]
    fn saved_devices_update_preserves_added_at() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let first = storage.save_device_info(SavedDevice {
            ip: "192.168.1.10".into(), nickname: Some("Desk".into()), model: Some("L930".into()), added_at: None,
        }).unwrap();
        let updated = storage.save_device_info(SavedDevice {
            ip: "192.168.1.10".into(), nickname: Some("Desk back".into()), model: Some("L930".into()), added_at: None,
        }).unwrap();
        assert_eq!(first.added_at, updated.added_at);
        let all = storage.get_saved_devices();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].nickname.as_deref(), Some("Desk back"));
        assert!(storage.remove_saved_device("192.168.1.10"));
        assert!(storage.get_saved_devices().is_empty());
    }

    #[test]
    fn concurrent_preset_saves_are_not_lost() {
        let tmp = TempDir::new();
        let storage = std::sync::Arc::new(Storage::new(tmp.0.clone()));
        let handles: Vec<_> = (0..8).map(|i| {
            let s = storage.clone();
            std::thread::spawn(move || s.save_preset(preset(&format!("p{i}"))).unwrap())
        }).collect();
        for h in handles { h.join().unwrap(); }
        assert_eq!(storage.get_presets().len(), 8);
    }

    #[test]
    fn legacy_files_are_read_without_modification() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let legacy_path = tmp.0.join("data").join("credentials.json");
        let content = r#"{"email":"user@example.com","password":"dummy-test-secret","device_ip":null}"#;
        fs::write(&legacy_path, content).unwrap();
        fs::write(tmp.0.join("credentials.json"), r#"{"email":"user@example.com","rememberMe":true}"#).unwrap();

        let legacy = storage.load_legacy_credentials().unwrap();
        assert_eq!(legacy.email, "user@example.com");
        assert!(!format!("{:?}", legacy).contains("dummy-test-secret"));
        assert_eq!(fs::read_to_string(&legacy_path).unwrap(), content);

        let prefs = storage.load_legacy_store_prefs().unwrap();
        assert_eq!(prefs.remember_me, Some(true));
    }

    #[test]
    fn corrupt_legacy_file_is_left_in_place() {
        let tmp = TempDir::new();
        let storage = Storage::new(tmp.0.clone());
        let legacy_path = tmp.0.join("data").join("credentials.json");
        fs::write(&legacy_path, "garbage").unwrap();
        assert!(storage.load_legacy_credentials().is_none());
        assert_eq!(fs::read_to_string(&legacy_path).unwrap(), "garbage");
    }
}
