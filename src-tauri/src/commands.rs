use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::device::{connected_devices, get_target, DeviceManager, TapoDevice, DeviceInfo};
use crate::storage::{Storage, Preset, Schedule, SavedDevice};
use crate::scheduler::SchedulerManager;
use crate::discovery;
use crate::credentials::{CredentialManager, SavedCredentials};
use crate::validation;

// ============ State wrappers ============

pub struct StorageState(pub Arc<Storage>);
pub struct SchedulerState(pub SchedulerManager);
pub struct CredentialsState(pub Arc<CredentialManager>);

// ============ Request/Response types ============

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
    device_ip: Option<String>,
    /// `None` keeps the stored preference.
    #[serde(default)]
    remember_me: Option<bool>,
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
    #[serde(default)]
    device_ip: Option<String>,
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

// ============ Validation helpers ============

fn validate_preset(req: &CreatePresetRequest) -> Result<(), String> {
    validation::brightness(req.brightness)?;
    if let Some(h) = req.hue { validation::hue(h)?; }
    if let Some(s) = req.saturation { validation::saturation(s)?; }
    if let Some(t) = req.color_temperature { validation::color_temperature(t)?; }
    Ok(())
}

fn schedule_from_request(id: String, req: CreateScheduleRequest) -> Result<Schedule, String> {
    validation::schedule_time(&req.time)?;
    validation::schedule_days(&req.days)?;
    validation::schedule_action(&req.action)?;
    if let Some(b) = req.brightness { validation::brightness(b)?; }
    if req.action == "preset" && req.preset_id.is_none() {
        return Err("A preset schedule requires preset_id".to_string());
    }
    let device_ip = req.device_ip.as_deref().map(validation::device_ip).transpose()?;
    Ok(Schedule {
        id,
        name: req.name,
        time: req.time,
        days: req.days,
        action: req.action,
        preset_id: req.preset_id,
        brightness: req.brightness,
        enabled: req.enabled.unwrap_or(true),
        created_at: String::new(),
        device_ip,
    })
}

/// Connects with the given credentials and registers the device. Nothing is
/// stored unless the connection succeeds.
async fn connect_and_register(
    ip: &str,
    email: &str,
    password: &str,
    devices: &DeviceManager,
    storage: &Storage,
) -> Result<DeviceInfo, String> {
    let mut device = TapoDevice::new(email, password);
    let info = device.connect(ip).await?;

    devices.write().await.insert(ip.to_string(), Arc::new(device));

    storage.save_device_info(SavedDevice {
        ip: ip.to_string(),
        nickname: Some(info.nickname.clone()),
        model: Some(info.model.clone()),
        added_at: None,
    })?;
    storage.save_last_ip(ip)?;
    Ok(info)
}

// ============ Auth Commands ============

#[tauri::command]
pub async fn save_credentials_api(
    req: SaveCredentialsRequest,
    creds: State<'_, CredentialsState>,
) -> Result<SuccessResponse, String> {
    let email = validation::email(&req.email)?;
    validation::password(&req.password)?;
    // Upstream persisted these unconditionally, i.e. "remember me".
    creds.0.sign_in(&email, &req.password, Some(true))?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn has_credentials(
    creds: State<'_, CredentialsState>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "has_credentials": creds.0.is_signed_in() }))
}

#[tauri::command]
pub async fn login(
    req: LoginRequest,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
    creds: State<'_, CredentialsState>,
) -> Result<serde_json::Value, String> {
    let email = validation::email(&req.email)?;
    validation::password(&req.password)?;

    match req.device_ip.as_deref() {
        Some(raw_ip) => {
            let ip = validation::device_ip(raw_ip)?;
            let info = connect_and_register(&ip, &email, &req.password, devices.inner(), &storage.0).await?;
            creds.0.sign_in(&email, &req.password, req.remember_me)?;
            Ok(serde_json::json!({ "success": true, "device": info }))
        }
        None => {
            creds.0.sign_in(&email, &req.password, req.remember_me)?;
            Ok(serde_json::json!({ "success": true, "device": null }))
        }
    }
}

#[tauri::command]
pub async fn logout(
    devices: State<'_, DeviceManager>,
    creds: State<'_, CredentialsState>,
) -> Result<SuccessResponse, String> {
    devices.write().await.clear();
    creds.0.sign_out()?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn auth_status(
    devices: State<'_, DeviceManager>,
    creds: State<'_, CredentialsState>,
) -> Result<serde_json::Value, String> {
    if !creds.0.is_signed_in() {
        return Ok(serde_json::json!({ "authenticated": false, "connected": false }));
    }

    let connected = connected_devices(devices.inner()).await;
    if connected.len() == 1 {
        let device = connected.values().next().expect("len checked");
        return match device.get_info().await {
            Ok(info) => Ok(serde_json::json!({
                "authenticated": true,
                "connected": true,
                "device": info
            })),
            Err(_) => Ok(serde_json::json!({ "authenticated": true, "connected": false })),
        };
    }

    Ok(serde_json::json!({ "authenticated": true, "connected": !connected.is_empty() }))
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
    let connected = connected_devices(devices.inner()).await;
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

/// Adds a device using the signed-in session, so the webview never needs to
/// hold the password after login.
#[tauri::command]
pub async fn add_device(
    ip: String,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
    creds: State<'_, CredentialsState>,
) -> Result<serde_json::Value, String> {
    let ip = validation::device_ip(&ip)?;
    let session = creds.0.session().ok_or("Not signed in")?;
    let info = connect_and_register(&ip, &session.email, &session.password, devices.inner(), &storage.0).await?;
    Ok(serde_json::json!({ "success": true, "device": info }))
}

#[tauri::command]
pub async fn delete_device(
    ip: String,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    devices.write().await.remove(&ip);
    storage.0.remove_saved_device(&ip);
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn reconnect_device(
    ip: String,
    devices: State<'_, DeviceManager>,
    creds: State<'_, CredentialsState>,
) -> Result<serde_json::Value, String> {
    let ip = validation::device_ip(&ip)?;
    let session = creds.0.session().ok_or("Not signed in")?;

    let mut device = TapoDevice::new(&session.email, &session.password);
    let info = device.connect(&ip).await?;

    devices.write().await.insert(ip, Arc::new(device));

    Ok(serde_json::json!({ "success": true, "device": info }))
}

// ============ Device Discovery ============

#[tauri::command]
pub async fn discover_devices_cmd() -> Result<serde_json::Value, String> {
    let result = discovery::discover_devices().await?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

// ============ Device Control Commands ============

#[tauri::command]
pub async fn device_info(
    ip: Option<String>,
    devices: State<'_, DeviceManager>,
) -> Result<DeviceInfo, String> {
    get_target(devices.inner(), ip.as_deref()).await?.get_info().await
}

#[tauri::command]
pub async fn device_power(
    req: PowerRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    if req.on {
        device.turn_on().await?;
    } else {
        device.turn_off().await?;
    }
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn device_brightness(
    req: BrightnessRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let value = validation::brightness(req.value)?;
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    device.set_brightness(value).await?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn device_color(
    req: ColorRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let hue = validation::hue(req.hue)?;
    let saturation = validation::saturation(req.saturation)?;
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    device.set_color(hue, saturation).await?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn device_temperature(
    req: TemperatureRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let kelvin = validation::color_temperature(req.kelvin)?;
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    device.set_color_temperature(kelvin).await?;
    Ok(SuccessResponse { success: true })
}

#[tauri::command]
pub async fn device_effect(
    req: EffectRequest,
    devices: State<'_, DeviceManager>,
) -> Result<SuccessResponse, String> {
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    device.set_effect(&req.name).await?;
    Ok(SuccessResponse { success: true })
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
    validate_preset(&req)?;
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
    ip: Option<String>,
    devices: State<'_, DeviceManager>,
    storage: State<'_, StorageState>,
) -> Result<SuccessResponse, String> {
    let preset = storage.0.get_preset(&preset_id)
        .ok_or("Preset not found")?;
    let device = get_target(devices.inner(), ip.as_deref()).await?;
    device.apply_preset(&preset).await?;
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
    let schedule = schedule_from_request(String::new(), req)?;
    let saved = storage.0.save_schedule(schedule)?;

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
    let schedule = schedule_from_request(schedule_id.clone(), req)?;

    let updated = storage.0.update_schedule(&schedule_id, schedule)
        .ok_or("Schedule not found")?;

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
    creds: State<'_, CredentialsState>,
) -> Result<Option<SavedCredentials>, String> {
    Ok(creds.0.saved_credentials())
}

#[tauri::command]
pub async fn save_credentials_local(
    req: SaveCredentialsLocalRequest,
    creds: State<'_, CredentialsState>,
) -> Result<bool, String> {
    let email = validation::email(&req.email)?;
    validation::password(&req.password)?;
    creds.0.sign_in(&email, &req.password, Some(req.remember_me))?;
    Ok(true)
}

#[tauri::command]
pub async fn clear_credentials_cmd(
    creds: State<'_, CredentialsState>,
) -> Result<bool, String> {
    creds.0.forget()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule_req(time: &str, action: &str) -> CreateScheduleRequest {
        CreateScheduleRequest {
            name: "s".into(), time: time.into(), days: vec![0], action: action.into(),
            preset_id: None, brightness: Some(50), enabled: None, device_ip: None,
        }
    }

    #[test]
    fn login_request_remember_me_is_optional() {
        let req: LoginRequest = serde_json::from_str(
            r#"{"email":"a@b.c","password":"x","device_ip":null}"#).unwrap();
        assert_eq!(req.remember_me, None);
        let req: LoginRequest = serde_json::from_str(
            r#"{"email":"a@b.c","password":"x","device_ip":null,"remember_me":true}"#).unwrap();
        assert_eq!(req.remember_me, Some(true));
    }

    #[test]
    fn schedule_request_validation() {
        assert!(schedule_from_request(String::new(), schedule_req("07:00", "on")).is_ok());
        assert!(schedule_from_request(String::new(), schedule_req("7am", "on")).is_err());
        assert!(schedule_from_request(String::new(), schedule_req("07:00", "explode")).is_err());
        assert!(schedule_from_request(String::new(), schedule_req("07:00", "preset")).is_err());
        let mut bad_brightness = schedule_req("07:00", "on");
        bad_brightness.brightness = Some(0);
        assert!(schedule_from_request(String::new(), bad_brightness).is_err());
        let mut bad_ip = schedule_req("07:00", "on");
        bad_ip.device_ip = Some("not-an-ip".into());
        assert!(schedule_from_request(String::new(), bad_ip).is_err());
    }

    #[test]
    fn preset_request_validation() {
        let ok = CreatePresetRequest {
            name: "p".into(), hue: Some(360), saturation: Some(100), brightness: 1,
            color_temperature: None, effect: None,
        };
        assert!(validate_preset(&ok).is_ok());
        let bad = CreatePresetRequest { brightness: 300, ..ok };
        assert!(validate_preset(&bad).is_err());
    }
}
