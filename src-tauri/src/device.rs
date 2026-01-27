use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tapo::{ApiClient, requests::LightingEffectPreset};

// ============ Device Handle ============

/// Wrapper enum for different Tapo LED strip device types.
/// L930/L920 = RgbicLightStripHandler (supports lighting effects)
/// L900 = RgbLightStripHandler
pub enum DeviceHandle {
    Rgbic(tapo::RgbicLightStripHandler),
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

impl TapoDevice {
    pub fn new(email: &str, password: &str) -> Self {
        Self {
            email: email.to_string(),
            password: password.to_string(),
            device_ip: None,
            handle: None,
        }
    }

    pub async fn connect(&mut self, ip: &str) -> Result<DeviceInfo, String> {
        let client = ApiClient::new(self.email.clone(), self.password.clone());

        // Try L930 first (most features), fallback to L920, then L900
        // ApiClient::l9xx() consumes self, so we clone between attempts
        let handle = match client.clone().l930(ip).await {
            Ok(device) => DeviceHandle::Rgbic(device),
            Err(_) => match client.clone().l920(ip).await {
                Ok(device) => DeviceHandle::Rgbic(device),
                Err(_) => match client.l900(ip).await {
                    Ok(device) => DeviceHandle::Rgb(device),
                    Err(e) => return Err(format!("Failed to connect to {}: {}", ip, e)),
                },
            },
        };

        self.device_ip = Some(ip.to_string());
        self.handle = Some(handle);
        self.get_info().await
    }

    pub async fn get_info(&self) -> Result<DeviceInfo, String> {
        let ip = self.device_ip.clone().unwrap_or_default();
        match &self.handle {
            Some(DeviceHandle::Rgbic(device)) => {
                let info = device.get_device_info().await
                    .map_err(|e| format!("Failed to get device info: {}", e))?;
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
                    .map_err(|e| format!("Failed to get device info: {}", e))?;
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
            Some(DeviceHandle::Rgbic(d)) => d.on().await.map_err(|e| format!("{}", e)),
            Some(DeviceHandle::Rgb(d)) => d.on().await.map_err(|e| format!("{}", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn turn_off(&self) -> Result<(), String> {
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.off().await.map_err(|e| format!("{}", e)),
            Some(DeviceHandle::Rgb(d)) => d.off().await.map_err(|e| format!("{}", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_brightness(&self, brightness: u8) -> Result<(), String> {
        let brightness = brightness.clamp(1, 100);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_brightness(brightness).await.map_err(|e| format!("{}", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_brightness(brightness).await.map_err(|e| format!("{}", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_color(&self, hue: u16, saturation: u8) -> Result<(), String> {
        let hue = hue.min(360);
        let saturation = saturation.min(100);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| format!("{}", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| format!("{}", e)),
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn set_color_temperature(&self, kelvin: u16) -> Result<(), String> {
        let kelvin = kelvin.clamp(2500, 6500);
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => d.set_color_temperature(kelvin).await.map_err(|e| format!("{}", e)),
            Some(DeviceHandle::Rgb(d)) => d.set_color_temperature(kelvin).await.map_err(|e| format!("{}", e)),
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
            Some(DeviceHandle::Rgbic(d)) => d.set_lighting_effect(effect).await.map_err(|e| format!("{}", e)),
            _ => Err("Effects only supported on L920/L930 devices".to_string()),
        }
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

pub type DeviceManager = Arc<RwLock<HashMap<String, TapoDevice>>>;

pub fn new_device_manager() -> DeviceManager {
    Arc::new(RwLock::new(HashMap::new()))
}
