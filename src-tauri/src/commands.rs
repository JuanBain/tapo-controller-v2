use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use tauri_plugin_store::StoreExt;

use crate::device::{DeviceManager, TapoDevice, DeviceInfo};
use crate::storage::{Storage, Credentials, Preset, Schedule, SavedDevice};
use crate::scheduler::SchedulerManager;
use crate::discovery;
use crate::credentials::{
    SavedCredentials, save_password_to_keyring, get_password_from_keyring, clear_password_from_keyring,
};

// ============ State wrappers ============

pub struct StorageState(pub Arc<Storage>);
pub struct SchedulerState(pub SchedulerManager);

// ============ Request/Response types ============

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
    device_ip: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveCredentialsRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct PowerRequest {
    on: bool,
    ip: Option<String>,
}

#[derive(Deserialize)]
pub struct BrightnessRequest {
    value: i32,
    ip: Option<String>,
}

#[derive(Deserialize)]
pub struct ColorRequest {
    hue: i32,
    saturation: i32,
    ip: Option<String>,
}

#[derive(Deserialize)]
pub struct TemperatureRequest {
    kelvin: i32,
    ip: Option<String>,
}

#[derive(Deserialize)]
pub struct EffectRequest {
    name: String,
    ip: Option<String>,
}

#[derive(Deserialize)]
pub struct CreatePresetRequest {
    name: String,
    hue: Option<i32>,
    saturation: Option<i32>,
    brightness: i32,
    color_temperature: Option<i32>,
    effect: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateScheduleRequest {
    name: String,
    time: String,
    days: Vec<i32>,
    action: String,
    preset_id: Option<String>,
    brightness: Option<i32>,
    enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct SaveCredentialsLocalRequest {
    email: String,
    password: String,
    #[serde(rename = "rememberMe")]
    remember_me: bool,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    success: bool,
}

#[derive(Serialize)]
pub struct DevicesListResponse {
    devices: Vec<DeviceListItem>,
}

#[derive(Serialize)]
pub struct DeviceListItem {
    ip: String,
    nickname: Option<String>,
    model: Option<String>,
    connected: bool,
    device_on: Option<bool>,
    brightness: Option<u8>,
}

// ============ Auth Commands ============

#[tauri::command]
pub async fn save_credentials_api(
    req: SaveCredentialsRequest,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    storage.0.save_credentials(&Credentials {
        email: req.email,
        password: req.password,
        device_ip: None,
    })?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn has_credentials(
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let has = storage.0.load_credentials().is_some();
    Ok(serde_json::json!({ "has_credentials": has }))
}

#[tauri::command]
pub async fn login(
    req: LoginRequest,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    // Save credentials
    storage.0.save_credentials(&Credentials {
        email: req.email.clone(),
        password: req.password.clone(),
        device_ip: req.device_ip.clone(),
    })?;

    if let Some(ref ip) = req.device_ip {
        let mut device = TapoDevice::new(&req.email, &req.password);
        let info = device.connect(ip).await?;

        // Add to device manager
        {
            let mut dm = devices.write().await;
            dm.insert(ip.clone(), device);
        }

        // Save device info for persistence
        storage.0.save_device_info(SavedDevice {
            ip: ip.clone(),
            nickname: Some(info.nickname.clone()),
            model: Some(info.model.clone()),
            added_at: None,
        })?;

        // Save last IP
        storage.0.save_last_ip(ip)?;

        Ok(serde_json::json!({ "success": true, "device": info }))
    } else {
        Ok(serde_json::json!({ "success": true, "device": null }))
    }
}

#[tauri::command]
pub async fn logout(
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    storage.0.clear_credentials()?;
    {
        let mut dm = devices.write().await;
        dm.clear();
    }
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn auth_status(
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let creds = storage.0.load_credentials();
    if creds.is_none() {
        return Ok(serde_json::json!({ "authenticated": false, "connected": false }));
    }

    let dm = devices.read().await;
    if let Some(device) = dm.values().next() {
        match device.get_info().await {
            Ok(info) => Ok(serde_json::json!({
                "authenticated": true,
                "connected": true,
                "device": info
            })),
            Err(_) => Ok(serde_json::json!({ "authenticated": true, "connected": false })),
        }
    } else {
        Ok(serde_json::json!({ "authenticated": true, "connected": false }))
    }
}

#[tauri::command]
pub async fn last_ip(
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let ip = storage.0.get_last_ip();
    Ok(serde_json::json!({ "ip": ip }))
}

// ============ Device Management Commands ============

#[tauri::command]
pub async fn list_devices(
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<DevicesListResponse, String> {
    let connected = devices.read().await;
    let saved = storage.0.get_saved_devices();

    let mut result = Vec::new();
    for s in saved {
        let mut item = DeviceListItem {
            ip: s.ip.clone(),
            nickname: s.nickname.clone(),
            model: s.model.clone(),
            connected: connected.contains_key(&s.ip),
            device_on: None,
            brightness: None,
        };

        if let Some(device) = connected.get(&s.ip) {
            match device.get_info().await {
                Ok(info) => {
                    item.nickname = Some(info.nickname);
                    item.device_on = Some(info.device_on);
                    item.brightness = Some(info.brightness);
                }
                Err(_) => {
                    item.connected = false;
                }
            }
        }

        result.push(item);
    }

    Ok(DevicesListResponse { devices: result })
}

#[tauri::command]
pub async fn delete_device(
    ip: String,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    {
        let mut dm = devices.write().await;
        dm.remove(&ip);
    }
    storage.0.remove_saved_device(&ip);
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn reconnect_device(
    ip: String,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let creds = storage.0.load_credentials()
        .ok_or("No credentials saved")?;

    let mut device = TapoDevice::new(&creds.email, &creds.password);
    let info = device.connect(&ip).await?;

    {
        let mut dm = devices.write().await;
        dm.insert(ip, device);
    }

    Ok(serde_json::json!({ "success": true, "device": info }))
}

// ============ Device Discovery ============

#[tauri::command]
pub async fn discover_devices_cmd() -> Result<serde_json::Value, String> {
    let result = discovery::discover_devices().await?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

// ============ Device Control Commands ============

#[tauri::command]
pub async fn device_info(
    ip: Option<String>,
    devices: State<'_, DeviceManager>,
) -> Result<DeviceInfo, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => d.get_info().await,
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn device_power(
    req: PowerRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = req.ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => {
            if req.on {
                d.turn_on().await?;
            } else {
                d.turn_off().await?;
            }
            Ok(SuccessResponse { success: true })
        }
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn device_brightness(
    req: BrightnessRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = req.ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => {
            d.set_brightness(req.value as u8).await?;
            Ok(SuccessResponse { success: true })
        }
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn device_color(
    req: ColorRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = req.ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => {
            d.set_color(req.hue as u16, req.saturation as u8).await?;
            Ok(SuccessResponse { success: true })
        }
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn device_temperature(
    req: TemperatureRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = req.ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => {
            d.set_color_temperature(req.kelvin as u16).await?;
            Ok(SuccessResponse { success: true })
        }
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn device_effect(
    req: EffectRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let dm = devices.read().await;
    let device = if let Some(ref ip) = req.ip {
        dm.get(ip)
    } else {
        dm.values().next()
    };

    match device {
        Some(d) => {
            d.set_effect(&req.name).await?;
            Ok(SuccessResponse { success: true })
        }
        None => Err("Not connected to device".to_string()),
    }
}

#[tauri::command]
pub async fn available_effects() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "effects": TapoDevice::get_available_effects() }))
}

// ============ Preset Commands ============

#[tauri::command]
pub async fn list_presets(
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let presets = storage.0.get_presets();
    Ok(serde_json::json!({ "presets": presets }))
}

#[tauri::command]
pub async fn create_preset(
    req: CreatePresetRequest,
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let preset = Preset {
        id: String::new(),
        name: req.name,
        hue: req.hue,
        saturation: req.saturation,
        brightness: req.brightness,
        color_temperature: req.color_temperature,
        effect: req.effect,
        created_at: String::new(),
    };
    let saved = storage.0.save_preset(preset)?;
    Ok(serde_json::json!({ "preset": saved }))
}

#[tauri::command]
pub async fn delete_preset_cmd(
    preset_id: String,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    if storage.0.delete_preset(&preset_id) {
        Ok(SuccessResponse { success: true })
    } else {
        Err("Preset not found".to_string())
    }
}

#[tauri::command]
pub async fn apply_preset(
    preset_id: String,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    let preset = storage.0.get_preset(&preset_id)
        .ok_or("Preset not found")?;

    let dm = devices.read().await;
    let device = dm.values().next()
        .ok_or("Not connected to device")?;

    device.turn_on().await?;
    device.set_brightness(preset.brightness as u8).await?;

    if let (Some(hue), Some(sat)) = (preset.hue, preset.saturation) {
        device.set_color(hue as u16, sat as u8).await?;
    } else if let Some(temp) = preset.color_temperature {
        device.set_color_temperature(temp as u16).await?;
    }

    if let Some(ref effect) = preset.effect {
        device.set_effect(effect).await?;
    }

    Ok(SuccessResponse { success: true })
}

// ============ Schedule Commands ============

#[tauri::command]
pub async fn list_schedules(
    storage: State<'_, StorageState>,
) -> Result<serde_json::Value, String> {
    let schedules = storage.0.get_schedules();
    Ok(serde_json::json!({ "schedules": schedules }))
}

#[tauri::command]
pub async fn create_schedule(
    req: CreateScheduleRequest,
    storage: State<'_, StorageState>,
    scheduler: State<'_, SchedulerState>,
) -> Result<serde_json::Value, String> {
    let schedule = Schedule {
        id: String::new(),
        name: req.name,
        time: req.time,
        days: req.days,
        action: req.action,
        preset_id: req.preset_id,
        brightness: req.brightness,
        enabled: req.enabled.unwrap_or(true),
        created_at: String::new(),
    };
    let saved = storage.0.save_schedule(schedule)?;

    // Add to scheduler
    {
        let mut sched = scheduler.0.write().await;
        sched.add_schedule_job(&saved).await?;
    }

    Ok(serde_json::json!({ "schedule": saved }))
}

#[tauri::command]
pub async fn update_schedule(
    schedule_id: String,
    req: CreateScheduleRequest,
    storage: State<'_, StorageState>,
    scheduler: State<'_, SchedulerState>,
) -> Result<serde_json::Value, String> {
    let schedule = Schedule {
        id: schedule_id.clone(),
        name: req.name,
        time: req.time,
        days: req.days,
        action: req.action,
        preset_id: req.preset_id,
        brightness: req.brightness,
        enabled: req.enabled.unwrap_or(true),
        created_at: String::new(),
    };

    let updated = storage.0.update_schedule(&schedule_id, schedule)
        .ok_or("Schedule not found")?;

    // Update scheduler
    {
        let mut sched = scheduler.0.write().await;
        sched.remove_schedule_job(&schedule_id).await.ok();
        if updated.enabled {
            sched.add_schedule_job(&updated).await?;
        }
    }

    Ok(serde_json::json!({ "schedule": updated }))
}

#[tauri::command]
pub async fn delete_schedule_cmd(
    schedule_id: String,
    storage: State<'_, StorageState>,
    scheduler: State<'_, SchedulerState>,
) -> Result<SuccessResponse, String> {
    {
        let mut sched = scheduler.0.write().await;
        sched.remove_schedule_job(&schedule_id).await.ok();
    }

    if storage.0.delete_schedule(&schedule_id) {
        Ok(SuccessResponse { success: true })
    } else {
        Err("Schedule not found".to_string())
    }
}

#[tauri::command]
pub async fn toggle_schedule(
    schedule_id: String,
    storage: State<'_, StorageState>,
    scheduler: State<'_, SchedulerState>,
) -> Result<serde_json::Value, String> {
    let schedules = storage.0.get_schedules();
    let schedule = schedules.iter().find(|s| s.id == schedule_id)
        .ok_or("Schedule not found")?;

    let mut updated = schedule.clone();
    updated.enabled = !updated.enabled;
    storage.0.update_schedule(&schedule_id, updated.clone())
        .ok_or("Failed to update schedule")?;

    {
        let mut sched = scheduler.0.write().await;
        if updated.enabled {
            sched.add_schedule_job(&updated).await?;
        } else {
            sched.remove_schedule_job(&schedule_id).await.ok();
        }
    }

    Ok(serde_json::json!({ "enabled": updated.enabled }))
}

// ============ Credential Storage Commands ============

#[tauri::command]
pub async fn get_saved_credentials(
    app: tauri::AppHandle,
) -> Result<Option<SavedCredentials>, String> {
    // Try to load from tauri-plugin-store
    let store = app.store("credentials.json")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    let email: Option<serde_json::Value> = store.get("email");
    let remember_me: Option<serde_json::Value> = store.get("rememberMe");

    if let (Some(email_val), Some(remember_val)) = (email, remember_me) {
        let email: String = email_val.as_str().unwrap_or("").to_string();
        let remember: bool = remember_val.as_bool().unwrap_or(false);

        if remember && !email.is_empty() {
            // Get password from OS keyring
            let password = get_password_from_keyring().unwrap_or_default();
            return Ok(Some(SavedCredentials {
                email,
                password,
                remember_me: remember,
            }));
        }
    }

    Ok(None)
}

#[tauri::command]
pub async fn save_credentials_local(
    req: SaveCredentialsLocalRequest,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let store = app.store("credentials.json")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    store.set("email", serde_json::json!(req.email));
    store.set("rememberMe", serde_json::json!(req.remember_me));

    if req.remember_me {
        save_password_to_keyring(&req.password)?;
    } else {
        clear_password_from_keyring().ok();
    }

    store.save().map_err(|e| format!("Failed to save store: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn clear_credentials_cmd(
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let store = app.store("credentials.json")
        .map_err(|e| format!("Failed to open store: {}", e))?;

    store.delete("email");
    store.delete("rememberMe");
    clear_password_from_keyring().ok();

    store.save().map_err(|e| format!("Failed to save store: {}", e))?;
    Ok(true)
}
