use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::device::{self, connected_devices, get_target, DeviceManager, TapoDevice, DeviceInfo};
use crate::storage::{Storage, Preset, Schedule, SavedDevice};
use crate::scheduler::SchedulerManager;
use crate::discovery;
use crate::credentials::{CredentialManager, SavedCredentials};
use crate::validation;
use crate::custom_effects::{self, CustomMode, EffectColor, Hsv, RainbowSpeed};

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
    #[serde(default)]
    brightness: Option<i32>,
    /// Rainbow only; rejected for every other effect.
    #[serde(default)]
    rainbow_speed: Option<RainbowSpeed>,
}

#[derive(Serialize)]
pub struct EffectResponse {
    success: bool,
    /// `"classic"` (`set_lighting_effect`) or `"segment"` (`set_segment_effect`).
    kind: &'static str,
    effect: String,
    brightness: Option<u8>,
    /// Round-trip time of the device request, for latency measurements.
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    rainbow_speed: Option<RainbowSpeed>,
}

/// Only the mode, three colors and brightness come from the UI.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomEffectRequest {
    ip: String,
    mode: CustomMode,
    colors: Vec<EffectColor>,
    brightness: i32,
}

#[derive(Serialize)]
pub struct CustomEffectResponse {
    success: bool,
    mode: CustomMode,
    /// The validated `[hue, saturation, value]` colors that were sent.
    colors: [Hsv; custom_effects::COLOR_COUNT],
    brightness: u8,
    elapsed_ms: u64,
}

/// Experiments never fall back to "the only connected device" and accept no
/// parameters besides the fixed experiment id.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentRequest {
    ip: String,
    experiment: device::Experiment,
}

#[derive(Serialize)]
pub struct ExperimentResponse {
    success: bool,
    experiment: device::Experiment,
    elapsed_ms: u64,
}

#[derive(Serialize)]
pub struct EffectCatalogItem {
    id: &'static str,
    label: &'static str,
}

#[derive(Deserialize, Clone)]
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
    supports_effects: Option<bool>,
}

// ============ Validation helpers ============

fn validate_preset(req: &CreatePresetRequest) -> Result<(), String> {
    validation::brightness(req.brightness)?;
    if let Some(h) = req.hue { validation::hue(h)?; }
    if let Some(s) = req.saturation { validation::saturation(s)?; }
    if let Some(t) = req.color_temperature { validation::color_temperature(t)?; }
    if let Some(e) = &req.effect {
        if device::effect_preset(e).is_none() {
            return Err(format!("Unknown effect: {}", e));
        }
    }
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
            supports_effects: None,
        };

        if let Some(device) = connected.get(&s.ip) {
            match device.get_info().await {
                Ok(info) => {
                    item.nickname = Some(info.nickname);
                    item.device_on = Some(info.device_on);
                    item.brightness = Some(info.brightness);
                    item.supports_effects = Some(info.supports_effects);
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
) -> Result<EffectResponse, String> {
    let (effect, brightness) = validate_effect_request(&req)?;
    let rainbow_speed = validate_rainbow_speed(&effect, req.rainbow_speed)?;
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    let started = std::time::Instant::now();
    device.set_effect_with_speed(&effect, brightness, rainbow_speed).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    log::info!("Effect {} applied in {} ms", effect, elapsed_ms);
    Ok(EffectResponse { success: true, kind: "classic", effect, brightness, elapsed_ms, rainbow_speed })
}

#[tauri::command]
pub async fn device_custom_effect(
    req: CustomEffectRequest,
    devices: State<'_, DeviceManager>,
) -> Result<CustomEffectResponse, String> {
    run_custom_effect_request(req, devices.inner()).await
}

/// Available in every build. Unlike diagnostics it is not gated on the build
/// profile; it is limited by the closed request instead.
async fn run_custom_effect_request(req: CustomEffectRequest, devices: &DeviceManager) -> Result<CustomEffectResponse, String> {
    let (mode, colors, brightness) = validate_custom_effect_request(&req)?;
    let device = get_target(devices, Some(explicit_ip(&req.ip)?)).await?;
    let started = std::time::Instant::now();
    device.set_custom_effect(mode, colors, brightness).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    log::info!("Custom {:?} effect applied in {} ms", mode, elapsed_ms);
    Ok(CustomEffectResponse { success: true, mode, colors, brightness, elapsed_ms })
}

#[tauri::command]
pub async fn device_segment_effect(
    req: EffectRequest,
    devices: State<'_, DeviceManager>,
) -> Result<EffectResponse, String> {
    let (effect, brightness) = validate_segment_effect_request(&req)?;
    let device = get_target(devices.inner(), req.ip.as_deref()).await?;
    let started = std::time::Instant::now();
    device.set_segment_effect(&effect, brightness).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    log::info!("Segment effect {} applied in {} ms", effect, elapsed_ms);
    Ok(EffectResponse { success: true, kind: "segment", effect, brightness, elapsed_ms, rainbow_speed: None })
}

fn explicit_ip(ip: &str) -> Result<&str, String> {
    let ip = ip.trim();
    if ip.is_empty() {
        return Err("An explicit device IP is required".to_string());
    }
    Ok(ip)
}

/// Read-only. The response holds allowlisted lighting fields only; it is not logged.
#[tauri::command]
pub async fn device_lighting_capabilities(
    ip: String,
    devices: State<'_, DeviceManager>,
) -> Result<crate::capabilities::LightingCapabilities, String> {
    let device = get_target(devices.inner(), Some(explicit_ip(&ip)?)).await?;
    device.lighting_capabilities().await
}

#[tauri::command]
pub async fn device_experiment(
    req: ExperimentRequest,
    devices: State<'_, DeviceManager>,
) -> Result<ExperimentResponse, String> {
    run_experiment_request(req, devices.inner()).await
}

/// Diagnostics X1–X5: development builds only. Checked before anything else,
/// so a release build never looks up or contacts a device for them.
async fn run_experiment_request(req: ExperimentRequest, devices: &DeviceManager) -> Result<ExperimentResponse, String> {
    if !device::experiments_enabled() {
        return Err("Lighting experiments are only available in development builds".to_string());
    }
    let device = get_target(devices, Some(explicit_ip(&req.ip)?)).await?;
    let started = std::time::Instant::now();
    device.run_experiment(req.experiment).await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    log::info!("Experiment {:?} applied in {} ms", req.experiment, elapsed_ms);
    Ok(ExperimentResponse { success: true, experiment: req.experiment, elapsed_ms })
}

fn validate_effect_request(req: &EffectRequest) -> Result<(String, Option<u8>), String> {
    let effect = req.name.trim().to_lowercase();
    if device::effect_preset(&effect).is_none() {
        return Err(format!("Unknown effect: {}", req.name));
    }
    let brightness = req.brightness.map(validation::brightness).transpose()?;
    Ok((effect, brightness))
}

fn validate_rainbow_speed(effect: &str, speed: Option<RainbowSpeed>) -> Result<Option<RainbowSpeed>, String> {
    match speed {
        Some(_) if effect != "rainbow" => Err("Speed can only be set for Rainbow".to_string()),
        speed => Ok(speed),
    }
}

fn validate_custom_effect_request(
    req: &CustomEffectRequest,
) -> Result<(CustomMode, [Hsv; custom_effects::COLOR_COUNT], u8), String> {
    let colors = custom_effects::validate_colors(&req.colors)?;
    let brightness = validation::brightness(req.brightness)?;
    Ok((req.mode, colors, brightness))
}

fn validate_segment_effect_request(req: &EffectRequest) -> Result<(String, Option<u8>), String> {
    let effect = req.name.trim().to_lowercase();
    if device::segment_effect_preset(&effect).is_none() {
        return Err(format!("Unknown segment effect: {}", req.name));
    }
    let brightness = req.brightness.map(validation::brightness).transpose()?;
    Ok((effect, brightness))
}

fn catalog_items(catalog: &[(&'static str, &'static str)]) -> Vec<EffectCatalogItem> {
    catalog.iter().map(|&(id, label)| EffectCatalogItem { id, label }).collect()
}

/// `effects` keeps the original id list; `catalog` adds display names;
/// `segment_catalog` lists the segment presets.
#[tauri::command]
pub async fn available_effects() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "effects": TapoDevice::get_available_effects(),
        "catalog": catalog_items(&device::EFFECT_CATALOG),
        "segment_catalog": catalog_items(&device::SEGMENT_EFFECT_CATALOG),
    }))
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
        let bad_effect = CreatePresetRequest { effect: Some("disco".into()), ..ok.clone() };
        assert!(validate_preset(&bad_effect).is_err());
        let good_effect = CreatePresetRequest { effect: Some("aurora".into()), ..ok.clone() };
        assert!(validate_preset(&good_effect).is_ok());
        let bad = CreatePresetRequest { brightness: 300, ..ok };
        assert!(validate_preset(&bad).is_err());
    }

    fn effect_req(json: &str) -> EffectRequest {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn effect_request_contract() {
        let req = effect_req(r#"{"name":"Aurora","ip":"192.168.0.10"}"#);
        assert_eq!(validate_effect_request(&req), Ok(("aurora".to_string(), None)));
        let req = effect_req(r#"{"name":"ocean","ip":null,"brightness":40}"#);
        assert_eq!(validate_effect_request(&req), Ok(("ocean".to_string(), Some(40))));
    }

    #[test]
    fn effect_request_rejects_invalid_input() {
        assert!(validate_effect_request(&effect_req(r#"{"name":"disco","ip":null}"#)).is_err());
        assert!(validate_effect_request(&effect_req(r#"{"name":"aurora","ip":null,"brightness":0}"#)).is_err());
        assert!(validate_effect_request(&effect_req(r#"{"name":"aurora","ip":null,"brightness":101}"#)).is_err());
    }

    #[test]
    fn effect_catalog_contract() {
        let v = tauri::async_runtime::block_on(available_effects()).unwrap();
        assert_eq!(v["effects"].as_array().unwrap().len(), 17);
        let catalog = v["catalog"].as_array().unwrap();
        assert_eq!(catalog.len(), 17);
        assert_eq!(catalog[0]["id"], "aurora");
        assert_eq!(catalog[0]["label"], "Aurora");
        let segments = v["segment_catalog"].as_array().unwrap();
        assert_eq!(segments.len(), 55);
        assert_eq!(segments[10]["id"], "electro_dance");
        assert_eq!(segments[10]["label"], "Electro Dance");
    }

    #[test]
    fn segment_effect_request_contract() {
        let req = effect_req(r#"{"name":"Disco","ip":"192.168.0.10"}"#);
        assert_eq!(validate_segment_effect_request(&req), Ok(("disco".to_string(), None)));
        let req = effect_req(r#"{"name":"moonlight","ip":null,"brightness":25}"#);
        assert_eq!(validate_segment_effect_request(&req), Ok(("moonlight".to_string(), Some(25))));
    }

    #[test]
    fn segment_and_classic_requests_are_not_interchangeable() {
        assert!(validate_segment_effect_request(&effect_req(r#"{"name":"aurora","ip":null}"#)).is_err());
        assert!(validate_effect_request(&effect_req(r#"{"name":"disco","ip":null}"#)).is_err());
        assert!(validate_segment_effect_request(&effect_req(r#"{"name":"disco","ip":null,"brightness":0}"#)).is_err());
        assert!(validate_segment_effect_request(&effect_req(r#"{"name":"disco","ip":null,"brightness":101}"#)).is_err());
    }

    #[test]
    fn experiment_request_contract() {
        let req: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x1_static_red_white_blue"}"#).unwrap();
        assert_eq!(req.experiment, device::Experiment::X1StaticRedWhiteBlue);
        let x2: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x2_sequence_red_white_blue"}"#).unwrap();
        assert_eq!(x2.experiment, device::Experiment::X2SequenceRedWhiteBlue);
        let x3: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x3_segment_circulating"}"#).unwrap();
        assert_eq!(x3.experiment, device::Experiment::X3SegmentCirculating);
        let x4a: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x4a_rainbow_transition_3000"}"#).unwrap();
        let x4b: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x4b_rainbow_transition_750"}"#).unwrap();
        assert_eq!(x4a.experiment, device::Experiment::X4aRainbowTransition3000);
        assert_eq!(x4b.experiment, device::Experiment::X4bRainbowTransition750);
        let x5a: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x5a_rainbow_direction_3"}"#).unwrap();
        let x5b: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"192.168.0.10","experiment":"x5b_rainbow_direction_4"}"#).unwrap();
        assert_eq!(x5a.experiment, device::Experiment::X5aRainbowDirection3);
        assert_eq!(x5b.experiment, device::Experiment::X5bRainbowDirection4);
        for invalid in [
            r#"{"ip":"192.168.0.10"}"#,
            r#"{"experiment":"x1_static_red_white_blue"}"#,
            r#"{"ip":null,"experiment":"x1_static_red_white_blue"}"#,
            r#"{"ip":"192.168.0.10","experiment":"x4_transition"}"#,
            r#"{"ip":"192.168.0.10","experiment":"x3_segment"}"#,
            r#"{"ip":"192.168.0.10","experiment":"x1_static_red_white_blue","sequence":[[0,0,0]]}"#,
            r#"{"ip":"192.168.0.10","experiment":"x1_static_red_white_blue","brightness":50}"#,
            r#"{"ip":"192.168.0.10","experiment":"x2_sequence_red_white_blue","direction":4}"#,
            r#"{"ip":"192.168.0.10","experiment":"x2_sequence_red_white_blue","duration":50}"#,
            r#"{"ip":"192.168.0.10","experiment":"x3_segment_circulating","segments":[49]}"#,
            r#"{"ip":"192.168.0.10","experiment":"x3_segment_circulating","states":[[267,56,100,0]]}"#,
            r#"{"ip":"192.168.0.10","experiment":"x4a_rainbow_transition_3000","transition":1500}"#,
            r#"{"ip":"192.168.0.10","experiment":"x5_direction"}"#,
            r#"{"ip":"192.168.0.10","experiment":"x5a_rainbow_direction_3","direction":1}"#,
        ] {
            assert!(serde_json::from_str::<ExperimentRequest>(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn experiments_and_capabilities_need_an_explicit_ip() {
        assert_eq!(explicit_ip(" 192.168.0.10 "), Ok("192.168.0.10"));
        assert!(explicit_ip("").is_err());
        assert!(explicit_ip("   ").is_err());
    }

    #[test]
    fn experiment_response_contract() {
        let res = ExperimentResponse {
            success: true, experiment: device::Experiment::X1StaticRedWhiteBlue, elapsed_ms: 90,
        };
        assert_eq!(serde_json::to_value(res).unwrap(), serde_json::json!({
            "success": true, "experiment": "x1_static_red_white_blue", "elapsed_ms": 90
        }));
    }

    #[test]
    fn rainbow_speed_request_contract() {
        let req = effect_req(r#"{"name":"rainbow","ip":"192.168.0.10","brightness":30,"rainbow_speed":"slow"}"#);
        let (effect, brightness) = validate_effect_request(&req).unwrap();
        assert_eq!((effect.as_str(), brightness), ("rainbow", Some(30)));
        assert_eq!(validate_rainbow_speed(&effect, req.rainbow_speed), Ok(Some(RainbowSpeed::Slow)));
        let plain = effect_req(r#"{"name":"rainbow","ip":null}"#);
        assert_eq!(validate_rainbow_speed("rainbow", plain.rainbow_speed), Ok(None));
        let other = effect_req(r#"{"name":"aurora","ip":null,"rainbow_speed":"fast"}"#);
        assert!(validate_rainbow_speed("aurora", other.rainbow_speed).is_err());
        for bad in [
            r#"{"name":"rainbow","ip":null,"rainbow_speed":"turbo"}"#,
            r#"{"name":"rainbow","ip":null,"rainbow_speed":1500}"#,
            r#"{"name":"rainbow","ip":null,"rainbow_speed":"Fast"}"#,
        ] {
            assert!(serde_json::from_str::<EffectRequest>(bad).is_err(), "{bad}");
        }
        let res = EffectResponse {
            success: true, kind: "classic", effect: "rainbow".into(), brightness: Some(30),
            elapsed_ms: 80, rainbow_speed: Some(RainbowSpeed::Slow),
        };
        assert_eq!(serde_json::to_value(res).unwrap()["rainbow_speed"], "slow");
    }

    fn custom_req(json: &str) -> Result<CustomEffectRequest, serde_json::Error> {
        serde_json::from_str(json)
    }

    const RWB: &str = r#"[{"hue":359,"saturation":85,"value":100},{"hue":0,"saturation":0,"value":100},{"hue":236,"saturation":72,"value":100}]"#;

    #[test]
    fn custom_effect_request_contract() {
        let req = custom_req(&format!(r#"{{"ip":"192.168.0.10","mode":"static","colors":{RWB},"brightness":60}}"#)).unwrap();
        assert_eq!(
            validate_custom_effect_request(&req),
            Ok((CustomMode::Static, [[359, 85, 100], [0, 0, 100], [236, 72, 100]], 60)),
        );
        let req = custom_req(&format!(r#"{{"ip":"192.168.0.10","mode":"sequence","colors":{RWB},"brightness":1}}"#)).unwrap();
        assert_eq!(validate_custom_effect_request(&req).unwrap().0, CustomMode::Sequence);
        let res = CustomEffectResponse {
            success: true, mode: CustomMode::Sequence, colors: [[1, 2, 3]; 3], brightness: 60, elapsed_ms: 140,
        };
        assert_eq!(serde_json::to_value(res).unwrap(), serde_json::json!({
            "success": true, "mode": "sequence", "colors": [[1, 2, 3], [1, 2, 3], [1, 2, 3]],
            "brightness": 60, "elapsed_ms": 140
        }));
    }

    #[test]
    fn custom_effect_request_rejects_invalid_input() {
        let two = r#"[{"hue":1,"saturation":1,"value":1},{"hue":1,"saturation":1,"value":1}]"#;
        let bad_value = r#"[{"hue":1,"saturation":1,"value":0},{"hue":1,"saturation":1,"value":1},{"hue":1,"saturation":1,"value":1}]"#;
        let bad_hue = r#"[{"hue":400,"saturation":1,"value":1},{"hue":1,"saturation":1,"value":1},{"hue":1,"saturation":1,"value":1}]"#;
        for json in [
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{two},"brightness":50}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{bad_value},"brightness":50}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{bad_hue},"brightness":50}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{RWB},"brightness":0}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{RWB},"brightness":101}}"#),
        ] {
            let req = custom_req(&json).unwrap();
            assert!(validate_custom_effect_request(&req).is_err(), "{json}");
        }
        for json in [
            format!(r#"{{"ip":"1.2.3.4","mode":"pulse","colors":{RWB},"brightness":50}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{RWB}}}"#),
            format!(r#"{{"mode":"static","colors":{RWB},"brightness":50}}"#),
            format!(r#"{{"ip":null,"mode":"static","colors":{RWB},"brightness":50}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{RWB},"brightness":50,"segments":[0]}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"sequence","colors":{RWB},"brightness":50,"duration":10}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"sequence","colors":{RWB},"brightness":50,"direction":4}}"#),
            format!(r#"{{"ip":"1.2.3.4","mode":"static","colors":{RWB},"brightness":50,"display_colors":[[0,0,0]]}}"#),
            r#"{"ip":"1.2.3.4","mode":"static","colors":[[0,0,100],[0,0,100],[0,0,100]],"brightness":50}"#.to_string(),
        ] {
            assert!(custom_req(&json).is_err(), "{json}");
        }
    }

    #[tokio::test]
    async fn custom_effects_are_enabled_in_every_build_profile() {
        let devices = device::new_device_manager();
        let req = custom_req(&format!(r#"{{"ip":"10.99.1.1","mode":"sequence","colors":{RWB},"brightness":50}}"#)).unwrap();
        let err = run_custom_effect_request(req, &devices).await.err().unwrap();
        assert_eq!(err, "Device 10.99.1.1 is not connected");
        let invalid = custom_req(&format!(r#"{{"ip":"10.99.1.1","mode":"static","colors":{RWB},"brightness":0}}"#)).unwrap();
        let err = run_custom_effect_request(invalid, &devices).await.err().unwrap();
        assert!(err.contains("brightness"), "{err}");
        let blank_ip = custom_req(&format!(r#"{{"ip":" ","mode":"static","colors":{RWB},"brightness":50}}"#)).unwrap();
        let err = run_custom_effect_request(blank_ip, &devices).await.err().unwrap();
        assert!(err.contains("explicit device IP"), "{err}");
    }

    #[tokio::test]
    async fn diagnostics_follow_the_build_profile() {
        let devices = device::new_device_manager();
        let req: ExperimentRequest = serde_json::from_str(
            r#"{"ip":"10.99.1.2","experiment":"x1_static_red_white_blue"}"#).unwrap();
        let err = run_experiment_request(req, &devices).await.err().unwrap();
        if cfg!(debug_assertions) {
            assert_eq!(err, "Device 10.99.1.2 is not connected");
        } else {
            assert_eq!(err, "Lighting experiments are only available in development builds");
        }
    }

    #[test]
    fn effect_response_contract() {
        let res = EffectResponse { success: true, kind: "segment", effect: "disco".into(), brightness: Some(40), elapsed_ms: 120, rainbow_speed: None };
        let v = serde_json::to_value(res).unwrap();
        assert_eq!(v, serde_json::json!({
            "success": true, "kind": "segment", "effect": "disco", "brightness": 40, "elapsed_ms": 120
        }));
    }
}
