use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use chrono::Local;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
    pub device_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub last_device_ip: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { last_device_ip: None }
    }
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
    data_dir: PathBuf,
}

impl Storage {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let data_dir = app_data_dir.join("data");
        fs::create_dir_all(&data_dir).ok();
        Self { data_dir }
    }

    fn credentials_path(&self) -> PathBuf {
        self.data_dir.join("credentials.json")
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

    fn load_json<T: serde::de::DeserializeOwned>(&self, path: &PathBuf) -> Option<T> {
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_json<T: Serialize>(&self, path: &PathBuf, data: &T) -> Result<(), String> {
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| format!("Serialize error: {}", e))?;
        fs::write(path, content)
            .map_err(|e| format!("Write error: {}", e))
    }

    // ============ Credentials ============

    pub fn save_credentials(&self, creds: &Credentials) -> Result<(), String> {
        self.save_json(&self.credentials_path(), creds)
    }

    pub fn load_credentials(&self) -> Option<Credentials> {
        self.load_json(&self.credentials_path())
    }

    pub fn clear_credentials(&self) -> Result<(), String> {
        let path = self.credentials_path();
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Delete error: {}", e))?;
        }
        Ok(())
    }

    // ============ Presets ============

    pub fn get_presets(&self) -> Vec<Preset> {
        self.load_json::<Vec<Preset>>(&self.presets_path())
            .unwrap_or_default()
    }

    pub fn save_preset(&self, mut preset: Preset) -> Result<Preset, String> {
        let mut presets = self.get_presets();
        preset.id = Uuid::new_v4().to_string()[..8].to_string();
        preset.created_at = Local::now().to_rfc3339();
        presets.push(preset.clone());
        self.save_json(&self.presets_path(), &presets)?;
        Ok(preset)
    }

    pub fn delete_preset(&self, preset_id: &str) -> bool {
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
        let mut schedules = self.get_schedules();
        schedule.id = Uuid::new_v4().to_string()[..8].to_string();
        schedule.created_at = Local::now().to_rfc3339();
        schedules.push(schedule.clone());
        self.save_json(&self.schedules_path(), &schedules)?;
        Ok(schedule)
    }

    pub fn update_schedule(&self, schedule_id: &str, mut schedule: Schedule) -> Option<Schedule> {
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
        let mut schedules = self.get_schedules();
        let original_len = schedules.len();
        schedules.retain(|s| s.id != schedule_id);
        if schedules.len() == original_len {
            return false;
        }
        self.save_json(&self.schedules_path(), &schedules).is_ok()
    }

    // ============ Settings ============

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        self.save_json(&self.settings_path(), settings)
    }

    pub fn load_settings(&self) -> Settings {
        self.load_json(&self.settings_path()).unwrap_or_default()
    }

    pub fn save_last_ip(&self, ip: &str) -> Result<(), String> {
        let mut settings = self.load_settings();
        settings.last_device_ip = Some(ip.to_string());
        self.save_settings(&settings)
    }

    pub fn get_last_ip(&self) -> Option<String> {
        self.load_settings().last_device_ip
    }

    // ============ Saved Devices ============

    pub fn get_saved_devices(&self) -> Vec<SavedDevice> {
        self.load_json::<Vec<SavedDevice>>(&self.devices_path())
            .unwrap_or_default()
    }

    pub fn save_device_info(&self, device: SavedDevice) -> Result<SavedDevice, String> {
        let mut devices = self.get_saved_devices();
        // Update if exists, otherwise add
        if let Some(idx) = devices.iter().position(|d| d.ip == device.ip) {
            devices[idx] = device.clone();
        } else {
            let mut device = device;
            device.added_at = Some(Local::now().to_rfc3339());
            let dev_clone = device.clone();
            devices.push(device);
            self.save_json(&self.devices_path(), &devices)?;
            return Ok(dev_clone);
        }
        self.save_json(&self.devices_path(), &devices)?;
        Ok(device)
    }

    pub fn remove_saved_device(&self, ip: &str) -> bool {
        let mut devices = self.get_saved_devices();
        let original_len = devices.len();
        devices.retain(|d| d.ip != ip);
        if devices.len() == original_len {
            return false;
        }
        self.save_json(&self.devices_path(), &devices).is_ok()
    }
}
