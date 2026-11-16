use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tapo::{ApiClient, requests::LightingEffectPreset};

use crate::storage::Preset;
use crate::validation;

/// Per-request timeout for device communication (the `tapo` default is 30 s).
const DEVICE_TIMEOUT: Duration = Duration::from_secs(10);

// ============ Device Handle ============

/// Wrapper enum for different Tapo LED strip device types.
/// L930/L920 = RgbicLightStripHandler (supports lighting effects)
/// L900 = RgbLightStripHandler
pub enum DeviceHandle {
    Rgbic(tapo::RgbicLightStripHandler),
    // `ApiClient::l930()`, `l920()` and `l900()` perform the same login without
    // checking the model, so `connect` cannot tell an L900 apart yet.
    #[allow(dead_code)]
    Rgb(tapo::RgbLightStripHandler),
}

// ============ TapoDevice ============

pub struct TapoDevice {
    pub email: String,
    pub password: String,
    pub device_ip: Option<String>,
    pub handle: Option<DeviceHandle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub nickname: String,
    pub model: String,
    pub device_on: bool,
    pub brightness: u8,
    pub hue: Option<u16>,
    pub saturation: Option<u16>,
    pub color_temp: Option<u16>,
    pub ip: String,
}

/// Removes any occurrence of the given secrets from a message.
pub fn redact(message: &str, secrets: &[&str]) -> String {
    secrets
        .iter()
        .filter(|s| !s.is_empty())
        .fold(message.to_string(), |msg, s| msg.replace(s, "<redacted>"))
}

impl TapoDevice {
    pub fn new(email: &str, password: &str) -> Self {
        Self {
            email: email.to_string(),
            password: password.to_string(),
            device_ip: None,
            handle: None,
        }
    }

    fn err(&self, context: &str, e: impl std::fmt::Display) -> String {
        let msg = format!("{}: {}", context, e);
        redact(&msg, &[&self.password, &self.email])
    }

    pub async fn connect(&mut self, ip: &str) -> Result<DeviceInfo, String> {
        let client = ApiClient::new(self.email.clone(), self.password.clone())
            .with_timeout(DEVICE_TIMEOUT);

        let device = client
            .l930(ip)
            .await
            .map_err(|e| self.err(&format!("Failed to connect to {}", ip), e))?;

        self.device_ip = Some(ip.to_string());
        self.handle = Some(DeviceHandle::Rgbic(device));
        self.get_info().await
    }

    pub async fn get_info(&self) -> Result<DeviceInfo, String> {
        let ip = self.device_ip.clone().unwrap_or_default();
        match &self.handle {
            Some(DeviceHandle::Rgbic(device)) => {
                let info = device.get_device_info().await
                    .map_err(|e| self.err("Failed to get device info", e))?;
                Ok(DeviceInfo {
                    device_id: info.device_id,
                    nickname: info.nickname,
                    model: info.model,
                    device_on: info.device_on,
                    brightness: info.brightness,
                    hue: info.hue,
                    saturation: info.saturation,
                    color_temp: Some(info.color_temp),
                    ip,
                })
            }
            Some(DeviceHandle::Rgb(device)) => {
                let info = device.get_device_info().await
                    .map_err(|e| self.err("Failed to get device info", e))?;
                Ok(DeviceInfo {
                    device_id: info.device_id,
                    nickname: info.nickname,
                    model: info.model,
                    device_on: info.device_on,
                    brightness: info.brightness,
                    hue: info.hue,
                    saturation: info.saturation,
                    color_temp: Some(info.color_temp),
                    ip,
                })
            }
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn turn_on(&self) -> Result<(), String> {
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.on().await.map_err(|e| self.err("Turn on failed", e)),
            Some(DeviceHandle::Rgb(d)) => d.on().await.map_err(|e| self.err("Turn on failed", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn turn_off(&self) -> Result<(), String> {
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.off().await.map_err(|e| self.err("Turn off failed", e)),
            Some(DeviceHandle::Rgb(d)) => d.off().await.map_err(|e| self.err("Turn off failed", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_brightness(&self, brightness: u8) -> Result<(), String> {
        let brightness = brightness.clamp(1, 100);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_brightness(brightness).await.map_err(|e| self.err("Set brightness failed", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_brightness(brightness).await.map_err(|e| self.err("Set brightness failed", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_color(&self, hue: u16, saturation: u8) -> Result<(), String> {
        let hue = hue.min(360);
        let saturation = saturation.min(100);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| self.err("Set color failed", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| self.err("Set color failed", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_color_temperature(&self, kelvin: u16) -> Result<(), String> {
        let kelvin = kelvin.clamp(2500, 6500);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_color_temperature(kelvin).await.map_err(|e| self.err("Set color temperature failed", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_color_temperature(kelvin).await.map_err(|e| self.err("Set color temperature failed", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_effect(&self, effect_name: &str) -> Result<(), String> {
        let effect = match effect_name.to_lowercase().as_str() {
            "aurora" => LightingEffectPreset::Aurora,
            "bubbling_cauldron" => LightingEffectPreset::BubblingCauldron,
            "candycane" => LightingEffectPreset::CandyCane,
            "christmas" => LightingEffectPreset::Christmas,
            "flicker" => LightingEffectPreset::Flicker,
            "grandmas_christmas_lights" => LightingEffectPreset::GrandmasChristmasLights,
            "hanukkah" => LightingEffectPreset::Hanukkah,
            "haunted_mansion" => LightingEffectPreset::HauntedMansion,
            "icicle" => LightingEffectPreset::Icicle,
            "lightning" => LightingEffectPreset::Lightning,
            "ocean" => LightingEffectPreset::Ocean,
            "rainbow" => LightingEffectPreset::Rainbow,
            "raindrop" => LightingEffectPreset::Raindrop,
            "spring" => LightingEffectPreset::Spring,
            "sunrise" => LightingEffectPreset::Sunrise,
            "sunset" => LightingEffectPreset::Sunset,
            "valentines" => LightingEffectPreset::Valentines,
            _ => return Err(format!("Unknown effect: {}", effect_name)),
        };

        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_lighting_effect(effect).await.map_err(|e| self.err("Set effect failed", e)),
            _ => Err("Effects only supported on L920/L930 devices".to_string()),
        }
    }

    /// Validates every preset field before sending anything to the device.
    pub async fn apply_preset(&self, preset: &Preset) -> Result<(), String> {
        let brightness = validation::brightness(preset.brightness)?;
        let color = match (preset.hue, preset.saturation) {
            (Some(h), Some(s)) => Some((validation::hue(h)?, validation::saturation(s)?)),
            _ => None,
        };
        let temperature = match (color, preset.color_temperature) {
            (None, Some(t)) => Some(validation::color_temperature(t)?),
            _ => None,
        };

        self.turn_on().await?;
        self.set_brightness(brightness).await?;
        if let Some((hue, sat)) = color {
            self.set_color(hue, sat).await?;
        } else if let Some(temp) = temperature {
            self.set_color_temperature(temp).await?;
        }
        if let Some(ref effect) = preset.effect {
            self.set_effect(effect).await?;
        }
        Ok(())
    }

    pub fn get_available_effects() -> Vec<String> {
        vec![
            "aurora".to_string(),
            "bubbling_cauldron".to_string(),
            "candycane".to_string(),
            "christmas".to_string(),
            "flicker".to_string(),
            "grandmas_christmas_lights".to_string(),
            "hanukkah".to_string(),
            "haunted_mansion".to_string(),
            "icicle".to_string(),
            "lightning".to_string(),
            "ocean".to_string(),
            "rainbow".to_string(),
            "raindrop".to_string(),
            "spring".to_string(),
            "sunrise".to_string(),
            "sunset".to_string(),
            "valentines".to_string(),
        ]
    }
}

// ============ Device Manager ============

/// Devices are stored behind `Arc` so callers can clone a handle and release
/// the map lock before doing network I/O.
pub type DeviceManager = Arc<RwLock<HashMap<String, Arc<TapoDevice>>>>;

pub fn new_device_manager() -> DeviceManager {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Picks the device a command applies to. Without an explicit IP the call is
/// only accepted when exactly one device is connected, so a second strip is
/// never controlled by accident.
pub fn resolve_target<'a, T>(devices: &'a HashMap<String, T>, ip: Option<&str>) -> Result<&'a T, String> {
    match ip.filter(|s| !s.is_empty()) {
        Some(ip) => devices
            .get(ip)
            .ok_or_else(|| format!("Device {} is not connected", ip)),
        None => match devices.len() {
            0 => Err("Not connected to device".to_string()),
            1 => Ok(devices.values().next().expect("len checked")),
            _ => Err("Multiple devices are connected; select a target device".to_string()),
        },
    }
}

pub async fn get_target(devices: &DeviceManager, ip: Option<&str>) -> Result<Arc<TapoDevice>, String> {
    let map = devices.read().await;
    resolve_target(&map, ip).cloned()
}

/// Snapshot of connected devices, taken without holding the lock afterwards.
pub async fn connected_devices(devices: &DeviceManager) -> HashMap<String, Arc<TapoDevice>> {
    devices.read().await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(ips: &[&str]) -> HashMap<String, u32> {
        ips.iter().enumerate().map(|(i, ip)| (ip.to_string(), i as u32)).collect()
    }

    #[test]
    fn explicit_target_is_used() {
        let m = map(&["10.0.0.1", "10.0.0.2"]);
        assert_eq!(resolve_target(&m, Some("10.0.0.2")), Ok(&1));
    }

    #[test]
    fn unknown_target_is_error() {
        let m = map(&["10.0.0.1"]);
        assert!(resolve_target(&m, Some("10.0.0.9")).is_err());
    }

    #[test]
    fn implicit_target_with_single_device() {
        let m = map(&["10.0.0.1"]);
        assert_eq!(resolve_target(&m, None), Ok(&0));
        assert_eq!(resolve_target(&m, Some("")), Ok(&0));
    }

    #[test]
    fn implicit_target_with_multiple_devices_is_rejected() {
        let m = map(&["10.0.0.1", "10.0.0.2"]);
        let err = resolve_target(&m, None).unwrap_err();
        assert!(err.contains("Multiple devices"));
    }

    #[test]
    fn no_devices_is_error() {
        let m = map(&[]);
        assert_eq!(resolve_target(&m, None), Err("Not connected to device".to_string()));
    }

    #[test]
    fn redact_removes_secrets() {
        let out = redact("login failed for user@example.com with hunter2", &["hunter2", "user@example.com"]);
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("user@example.com"));
        assert_eq!(redact("nothing here", &[""]), "nothing here");
    }

    #[test]
    fn device_errors_are_redacted() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let msg = d.err("Failed", "echo dummy-test-secret user@example.com");
        assert!(!msg.contains("dummy-test-secret"));
        assert!(!msg.contains("user@example.com"));
    }

    #[tokio::test]
    async fn disconnected_device_reports_not_connected() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        assert_eq!(d.turn_on().await, Err("Not connected to device".to_string()));
        assert!(d.get_info().await.is_err());
    }

    #[tokio::test]
    async fn invalid_preset_is_rejected_before_any_request() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let preset = Preset {
            id: "x".into(), name: "bad".into(), hue: Some(400), saturation: Some(50),
            brightness: 50, color_temperature: None, effect: None, created_at: String::new(),
        };
        let err = d.apply_preset(&preset).await.unwrap_err();
        assert!(err.contains("hue"), "{err}");
    }
}
