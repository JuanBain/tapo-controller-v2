use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tapo::{ApiClient, requests::{LightingEffect, LightingEffectPreset, LightingEffectType, SegmentEffect, SegmentEffectPreset, SegmentEffectType}};

use crate::capabilities::{self, LightingCapabilities};
use crate::custom_effects::{self, CustomMode, Hsv, RainbowSpeed};
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

// ============ Command gate ============

/// Serializes write commands to one device so two requests (e.g. an effect
/// and a color sent from the UI and the scheduler) never interleave. Each
/// device has its own gate, so separate strips stay independent.
#[derive(Default)]
pub struct CommandGate(Mutex<()>);

impl CommandGate {
    pub async fn run<F: Future>(&self, command: F) -> F::Output {
        let _guard = self.0.lock().await;
        command.await
    }
}

/// One gate per device IP for the whole process. Reconnecting replaces the
/// `TapoDevice` in the map, but a command still running on the replaced
/// connection holds this same gate, so writes to one strip never overlap.
fn gate_for(ip: &str) -> Arc<CommandGate> {
    static GATES: OnceLock<std::sync::Mutex<HashMap<String, Arc<CommandGate>>>> = OnceLock::new();
    let mut gates = GATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    gates.entry(ip.to_string()).or_default().clone()
}

// ============ TapoDevice ============

pub struct TapoDevice {
    pub email: String,
    pub password: String,
    pub device_ip: Option<String>,
    pub handle: Option<DeviceHandle>,
    /// Model reported by the device on connect.
    pub model: Option<String>,
    /// Shared with every other connection to the same IP once connected.
    gate: Arc<CommandGate>,
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
    #[serde(default)]
    pub supports_effects: bool,
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
            model: None,
            gate: Arc::new(CommandGate::default()),
        }
    }

    fn attach_gate(&mut self, ip: &str) {
        self.gate = gate_for(ip);
    }

    fn err(&self, context: &str, e: impl std::fmt::Display) -> String {
        let msg = format!("{}: {}", context, e);
        redact(&msg, &[&self.password, &self.email])
    }

    pub async fn connect(&mut self, ip: &str) -> Result<DeviceInfo, String> {
        self.attach_gate(ip);
        let client = ApiClient::new(self.email.clone(), self.password.clone())
            .with_timeout(DEVICE_TIMEOUT);

        let device = client
            .l930(ip)
            .await
            .map_err(|e| self.err(&format!("Failed to connect to {}", ip), e))?;

        self.device_ip = Some(ip.to_string());
        self.handle = Some(DeviceHandle::Rgbic(device));
        let mut info = self.get_info().await?;
        self.model = Some(info.model.clone());
        info.supports_effects = self.supports_effects();
        Ok(info)
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
                    supports_effects: is_rgbic_model(&info.model),
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
                    supports_effects: false,
                })
            }
            None => Err("Not connected to device".to_string()),
        }
    }

    pub async fn turn_on(&self) -> Result<(), String> {
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.on().await.map_err(|e| self.err("Turn on failed", e)),
                Some(DeviceHandle::Rgb(d)) => d.on().await.map_err(|e| self.err("Turn on failed", e)),
                None => Err("Not connected to device".to_string()),
            }
        }).await
    }

    pub async fn turn_off(&self) -> Result<(), String> {
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.off().await.map_err(|e| self.err("Turn off failed", e)),
                Some(DeviceHandle::Rgb(d)) => d.off().await.map_err(|e| self.err("Turn off failed", e)),
                None => Err("Not connected to device".to_string()),
            }
        }).await
    }

    pub async fn set_brightness(&self, brightness: u8) -> Result<(), String> {
        let brightness = brightness.clamp(1, 100);
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.set_brightness(brightness).await.map_err(|e| self.err("Set brightness failed", e)),
                Some(DeviceHandle::Rgb(d)) => d.set_brightness(brightness).await.map_err(|e| self.err("Set brightness failed", e)),
                None => Err("Not connected to device".to_string()),
            }
        }).await
    }

    pub async fn set_color(&self, hue: u16, saturation: u8) -> Result<(), String> {
        let hue = hue.min(360);
        let saturation = saturation.min(100);
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| self.err("Set color failed", e)),
                Some(DeviceHandle::Rgb(d)) => d.set_hue_saturation(hue, saturation).await.map_err(|e| self.err("Set color failed", e)),
                None => Err("Not connected to device".to_string()),
            }
        }).await
    }

    pub async fn set_color_temperature(&self, kelvin: u16) -> Result<(), String> {
        let kelvin = kelvin.clamp(2500, 6500);
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.set_color_temperature(kelvin).await.map_err(|e| self.err("Set color temperature failed", e)),
                Some(DeviceHandle::Rgb(d)) => d.set_color_temperature(kelvin).await.map_err(|e| self.err("Set color temperature failed", e)),
                None => Err("Not connected to device".to_string()),
            }
        }).await
    }

    /// Effects are only sent when the handle is RGBIC *and* the model reported
    /// by the device is an L920/L930, because `ApiClient::l930()` does not
    /// verify the model on login.
    pub fn supports_effects(&self) -> bool {
        matches!(self.handle, Some(DeviceHandle::Rgbic(_)))
            && self.model.as_deref().is_some_and(is_rgbic_model)
    }

    fn ensure_effects_supported(&self) -> Result<(), String> {
        if self.supports_effects() {
            return Ok(());
        }
        Err(format!(
            "Effects are only supported on RGBIC strips (L920/L930); device model: {}",
            self.model.as_deref().unwrap_or("unknown")
        ))
    }

    /// Activates one of the classic `tapo` presets. `brightness` overrides the
    /// preset's own brightness field; `None` keeps the library default.
    pub async fn set_effect(&self, effect_name: &str, brightness: Option<u8>) -> Result<(), String> {
        self.set_effect_with_speed(effect_name, brightness, None).await
    }

    /// `rainbow_speed` is only accepted for Rainbow; `None` keeps the preset.
    pub async fn set_effect_with_speed(
        &self,
        effect_name: &str,
        brightness: Option<u8>,
        rainbow_speed: Option<RainbowSpeed>,
    ) -> Result<(), String> {
        let effect = classic_effect(effect_name, brightness, rainbow_speed)?;
        self.ensure_effects_supported()?;
        self.send_lighting_effect(effect, "Set effect failed").await
    }

    /// The X1/X2 structure with validated colors; see `custom_effects`.
    pub async fn set_custom_effect(
        &self,
        mode: CustomMode,
        colors: [Hsv; custom_effects::COLOR_COUNT],
        brightness: u8,
    ) -> Result<(), String> {
        self.ensure_effects_supported()?;
        let brightness = validation::brightness(brightness as i32)?;
        let effect = custom_effects::build_custom(mode, colors, brightness);
        self.send_lighting_effect(effect, "Set custom effect failed").await
    }

    async fn send_lighting_effect(&self, effect: LightingEffect, context: &str) -> Result<(), String> {
        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.set_lighting_effect(effect).await.map_err(|e| self.err(context, e)),
                _ => Err("Effects only supported on L920/L930 devices".to_string()),
            }
        }).await
    }

    /// Activates one of the segment presets (`tapo` >= 0.8.9). The preset is a
    /// firmware rule referenced by id; this does not address individual LEDs.
    pub async fn set_segment_effect(&self, effect_name: &str, brightness: Option<u8>) -> Result<(), String> {
        let preset = segment_effect_preset(effect_name)
            .ok_or_else(|| format!("Unknown segment effect: {}", effect_name))?;
        self.ensure_effects_supported()?;
        let mut effect = SegmentEffect::from(preset);
        if let Some(b) = brightness {
            effect = effect.with_brightness(validation::brightness(b as i32)?);
        }

        self.gate.run(async {
            match &self.handle {
                Some(DeviceHandle::Rgbic(d)) => d.set_segment_effect(effect).await.map_err(|e| self.err("Set segment effect failed", e)),
                _ => Err("Effects only supported on L920/L930 devices".to_string()),
            }
        }).await
    }

    /// Read-only: one device-info request; nothing on the device changes.
    /// Only allowlisted fields are returned (see `capabilities`).
    pub async fn lighting_capabilities(&self) -> Result<LightingCapabilities, String> {
        match &self.handle {
            Some(DeviceHandle::Rgbic(d)) => {
                let raw = d.get_device_info_json().await
                    .map_err(|e| self.err("Failed to read device info", e))?;
                Ok(capabilities::filter(&raw))
            }
            Some(DeviceHandle::Rgb(_)) => Err("Capability report is only available for RGBIC strips".to_string()),
            None => Err("Not connected to device".to_string()),
        }
    }

    /// Sends one fixed experimental effect. No values come from the caller.
    /// X1 and X2 use `set_lighting_effect`; X3 uses `set_segment_effect`.
    pub async fn run_experiment(&self, experiment: Experiment) -> Result<(), String> {
        self.ensure_effects_supported()?;
        let unsupported = "Effects only supported on L920/L930 devices";
        match experiment.prepare() {
            PreparedExperiment::Lighting(effect) => self.gate.run(async {
                match &self.handle {
                    Some(DeviceHandle::Rgbic(d)) => d.set_lighting_effect(effect).await.map_err(|e| self.err("Experiment failed", e)),
                    _ => Err(unsupported.to_string()),
                }
            }).await,
            PreparedExperiment::Segment(effect) => self.gate.run(async {
                match &self.handle {
                    Some(DeviceHandle::Rgbic(d)) => d.set_segment_effect(effect).await.map_err(|e| self.err("Experiment failed", e)),
                    _ => Err(unsupported.to_string()),
                }
            }).await,
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
            self.set_effect(effect, None).await?;
        }
        Ok(())
    }

    pub fn get_available_effects() -> Vec<String> {
        EFFECT_CATALOG.iter().map(|(id, _)| id.to_string()).collect()
    }
}

// ============ Effect catalog ============

/// `(id, display name)` for every `LightingEffectPreset` in `tapo` 0.8.10.
/// Ids are part of the IPC contract and of saved presets; do not rename them.
pub const EFFECT_CATALOG: [(&str, &str); 17] = [
    ("aurora", "Aurora"),
    ("bubbling_cauldron", "Bubbling Cauldron"),
    ("candycane", "Candy Cane"),
    ("christmas", "Christmas"),
    ("flicker", "Flicker"),
    ("grandmas_christmas_lights", "Grandma's Christmas Lights"),
    ("hanukkah", "Hanukkah"),
    ("haunted_mansion", "Haunted Mansion"),
    ("icicle", "Icicle"),
    ("lightning", "Lightning"),
    ("ocean", "Ocean"),
    ("rainbow", "Rainbow"),
    ("raindrop", "Raindrop"),
    ("spring", "Spring"),
    ("sunrise", "Sunrise"),
    ("sunset", "Sunset"),
    ("valentines", "Valentines"),
];

pub fn effect_preset(id: &str) -> Option<LightingEffectPreset> {
    Some(match id.trim().to_lowercase().as_str() {
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
        _ => return None,
    })
}

/// `(id, display name)` for every `SegmentEffectPreset` in `tapo` 0.8.10.
/// Ids match the preset names `tapo` sends to the device.
pub const SEGMENT_EFFECT_CATALOG: [(&str, &str); 55] = [
    ("birthday", "Birthday"),
    ("blue", "Blue"),
    ("bonfire", "Bonfire"),
    ("candlelight", "Candlelight"),
    ("carnival", "Carnival"),
    ("cyan", "Cyan"),
    ("dancing", "Dancing"),
    ("dating", "Dating"),
    ("disco", "Disco"),
    ("dreamland", "Dreamland"),
    ("electro_dance", "Electro Dance"),
    ("energetic", "Energetic"),
    ("excited", "Excited"),
    ("fall", "Fall"),
    ("family", "Family"),
    ("fireworks", "Fireworks"),
    ("flower_field", "Flower Field"),
    ("forest", "Forest"),
    ("game", "Game"),
    ("green", "Green"),
    ("halloween", "Halloween"),
    ("happy", "Happy"),
    ("jazz", "Jazz"),
    ("lake", "Lake"),
    ("light_green", "Light Green"),
    ("lyric", "Lyric"),
    ("moonlight", "Moonlight"),
    ("morning", "Morning"),
    ("movie", "Movie"),
    ("new_year", "New Year"),
    ("night", "Night"),
    ("orange", "Orange"),
    ("pink", "Pink"),
    ("purple", "Purple"),
    ("quiet", "Quiet"),
    ("red", "Red"),
    ("relaxed", "Relaxed"),
    ("rock", "Rock"),
    ("siren", "Siren"),
    ("sleep", "Sleep"),
    ("snow", "Snow"),
    ("star", "Star"),
    ("study", "Study"),
    ("summer", "Summer"),
    ("sunny", "Sunny"),
    ("sweet", "Sweet"),
    ("tense", "Tense"),
    ("thinking", "Thinking"),
    ("universe", "Universe"),
    ("volcano", "Volcano"),
    ("warm", "Warm"),
    ("white", "White"),
    ("winter", "Winter"),
    ("work", "Work"),
    ("yellow", "Yellow"),
];

pub fn segment_effect_preset(id: &str) -> Option<SegmentEffectPreset> {
    use SegmentEffectPreset as P;
    Some(match id.trim().to_lowercase().as_str() {
        "birthday" => P::Birthday,
        "blue" => P::Blue,
        "bonfire" => P::Bonfire,
        "candlelight" => P::Candlelight,
        "carnival" => P::Carnival,
        "cyan" => P::Cyan,
        "dancing" => P::Dancing,
        "dating" => P::Dating,
        "disco" => P::Disco,
        "dreamland" => P::Dreamland,
        "electro_dance" => P::ElectroDance,
        "energetic" => P::Energetic,
        "excited" => P::Excited,
        "fall" => P::Fall,
        "family" => P::Family,
        "fireworks" => P::Fireworks,
        "flower_field" => P::FlowerField,
        "forest" => P::Forest,
        "game" => P::Game,
        "green" => P::Green,
        "halloween" => P::Halloween,
        "happy" => P::Happy,
        "jazz" => P::Jazz,
        "lake" => P::Lake,
        "light_green" => P::LightGreen,
        "lyric" => P::Lyric,
        "moonlight" => P::Moonlight,
        "morning" => P::Morning,
        "movie" => P::Movie,
        "new_year" => P::NewYear,
        "night" => P::Night,
        "orange" => P::Orange,
        "pink" => P::Pink,
        "purple" => P::Purple,
        "quiet" => P::Quiet,
        "red" => P::Red,
        "relaxed" => P::Relaxed,
        "rock" => P::Rock,
        "siren" => P::Siren,
        "sleep" => P::Sleep,
        "snow" => P::Snow,
        "star" => P::Star,
        "study" => P::Study,
        "summer" => P::Summer,
        "sunny" => P::Sunny,
        "sweet" => P::Sweet,
        "tense" => P::Tense,
        "thinking" => P::Thinking,
        "universe" => P::Universe,
        "volcano" => P::Volcano,
        "warm" => P::Warm,
        "white" => P::White,
        "winter" => P::Winter,
        "work" => P::Work,
        "yellow" => P::Yellow,
        _ => return None,
    })
}

/// Experimental effects with fixed payloads copied from `tapo` 0.8.10
/// `examples/tapo_l930.rs`. Nothing here is caller-supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Experiment {
    /// X1: custom `Static` effect. Physically confirmed on one L930-5.
    X1StaticRedWhiteBlue,
    /// X2: the same colors as a `Sequence`, with `direction` 1 and `duration` 50.
    /// Physically confirmed on one L930-5: the colors move.
    X2SequenceRedWhiteBlue,
    /// X3: the custom `Circulating` segment effect from the same example.
    /// `segments: [49]` is copied as-is; it is not a measured segment count.
    /// A purple expansion was observed on one L930-5. Protocol acceptance was
    /// not confirmed separately.
    X3SegmentCirculating,
    /// X4-A: the Rainbow preset with only `transition` set to 3000 (preset: 1500).
    /// On one L930-5 this was slower. The number is still not a unit of speed.
    #[serde(rename = "x4a_rainbow_transition_3000")]
    X4aRainbowTransition3000,
    /// X4-B: the Rainbow preset with only `transition` set to 750.
    /// On one L930-5 this was faster.
    #[serde(rename = "x4b_rainbow_transition_750")]
    X4bRainbowTransition750,
    /// X5-A: the Rainbow preset with only `direction` set to 3 (preset: 1).
    /// Not yet run on hardware. The number has no documented meaning.
    #[serde(rename = "x5a_rainbow_direction_3")]
    X5aRainbowDirection3,
    /// X5-B: the Rainbow preset with only `direction` set to 4.
    /// Not yet run on hardware.
    #[serde(rename = "x5b_rainbow_direction_4")]
    X5bRainbowDirection4,
}

/// A fixed payload ready to send. Lighting and segment effects use different requests.
pub enum PreparedExperiment {
    Lighting(LightingEffect),
    Segment(SegmentEffect),
}

/// Distribution builds are release builds (`debug_assertions` off). Experiments
/// stay callable from `npm run dev` and from tests only.
pub fn experiments_enabled() -> bool {
    cfg!(debug_assertions)
}

impl Experiment {
    pub fn prepare(self) -> PreparedExperiment {
        match self {
            Experiment::X1StaticRedWhiteBlue => PreparedExperiment::Lighting(
                LightingEffect::new(
                    "My Custom Static Effect",
                    LightingEffectType::Static,
                    true,
                    true,
                    100,
                    vec![[359, 85, 100]],
                )
                .with_expansion_strategy(1)
                .with_segments(vec![0, 1, 2])
                .with_sequence(vec![[359, 85, 100], [0, 0, 100], [236, 72, 100]]),
            ),
            Experiment::X2SequenceRedWhiteBlue => PreparedExperiment::Lighting(
                LightingEffect::new(
                    "My Custom Sequence Effect",
                    LightingEffectType::Sequence,
                    true,
                    true,
                    100,
                    vec![[359, 85, 100]],
                )
                .with_expansion_strategy(1)
                .with_segments(vec![0, 1, 2])
                .with_sequence(vec![[359, 85, 100], [0, 0, 100], [236, 72, 100]])
                .with_direction(1)
                .with_duration(50),
            ),
            Experiment::X3SegmentCirculating => PreparedExperiment::Segment(
                SegmentEffect::new(
                    "My Custom Segment Effect",
                    SegmentEffectType::Circulating,
                    true,
                    true,
                    100,
                    vec![[267, 56, 100, 0]],
                )
                .with_segments(vec![49])
                .with_states(vec![[267, 56, 100, 0]]),
            ),
            Experiment::X4aRainbowTransition3000 => {
                PreparedExperiment::Lighting(rainbow_with_transition(3000))
            }
            Experiment::X4bRainbowTransition750 => {
                PreparedExperiment::Lighting(rainbow_with_transition(750))
            }
            Experiment::X5aRainbowDirection3 => {
                PreparedExperiment::Lighting(rainbow_with_direction(3))
            }
            Experiment::X5bRainbowDirection4 => {
                PreparedExperiment::Lighting(rainbow_with_direction(4))
            }
        }
    }
}

/// The installed Rainbow preset (`transition` 1500) with that one field replaced.
/// The preset id and every other field stay as `tapo` defines them.
fn rainbow_with_transition(transition: u16) -> LightingEffect {
    LightingEffect::from(LightingEffectPreset::Rainbow).with_transition(transition)
}

/// The same preset (`direction` 1, `transition` 1500) with only `direction` replaced.
fn rainbow_with_direction(direction: u8) -> LightingEffect {
    LightingEffect::from(LightingEffectPreset::Rainbow).with_direction(direction)
}

/// A classic preset with an optional brightness and, for Rainbow only, a speed.
pub fn classic_effect(
    effect_name: &str,
    brightness: Option<u8>,
    rainbow_speed: Option<RainbowSpeed>,
) -> Result<LightingEffect, String> {
    let preset = effect_preset(effect_name)
        .ok_or_else(|| format!("Unknown effect: {}", effect_name))?;
    let mut effect = match (preset, rainbow_speed) {
        (LightingEffectPreset::Rainbow, Some(speed)) => custom_effects::rainbow(speed),
        (_, Some(_)) => return Err("Speed can only be set for Rainbow".to_string()),
        (preset, None) => LightingEffect::from(preset),
    };
    if let Some(b) = brightness {
        effect = effect.with_brightness(validation::brightness(b as i32)?);
    }
    Ok(effect)
}

/// Model strings reported by `get_device_info` for RGBIC strips (e.g. "L930").
pub fn is_rgbic_model(model: &str) -> bool {
    let m = model.trim().to_ascii_uppercase();
    m.starts_with("L920") || m.starts_with("L930")
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

    #[test]
    fn every_catalog_effect_maps_to_a_preset() {
        for (id, label) in EFFECT_CATALOG {
            assert!(effect_preset(id).is_some(), "{id}");
            assert!(!label.is_empty());
        }
        assert_eq!(TapoDevice::get_available_effects().len(), 17);
    }

    #[test]
    fn effect_ids_are_case_and_space_insensitive() {
        assert_eq!(effect_preset(" Aurora "), Some(LightingEffectPreset::Aurora));
        assert_eq!(effect_preset("OCEAN"), Some(LightingEffectPreset::Ocean));
        assert_eq!(effect_preset("disco"), None);
        assert_eq!(effect_preset(""), None);
    }

    #[test]
    fn rgbic_model_detection() {
        assert!(is_rgbic_model("L930"));
        assert!(is_rgbic_model("l920"));
        assert!(!is_rgbic_model("L900"));
        assert!(!is_rgbic_model("L530"));
        assert!(!is_rgbic_model(""));
    }

    #[tokio::test]
    async fn unknown_effect_is_rejected() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let err = d.set_effect("disco", None).await.unwrap_err();
        assert!(err.contains("Unknown effect"), "{err}");
    }

    #[test]
    fn segment_catalog_matches_the_library_presets() {
        let mut ids = std::collections::HashSet::new();
        let mut device_ids = std::collections::HashSet::new();
        for (id, label) in SEGMENT_EFFECT_CATALOG {
            let effect = SegmentEffect::from(segment_effect_preset(id).unwrap_or_else(|| panic!("{id}")));
            assert_eq!(effect.name, id, "catalog id must be the name tapo sends");
            assert!(effect.id.starts_with("TapoStrip_"), "{id}");
            assert!(!effect.is_custom && effect.r#type.is_none() && effect.segments.is_none());
            assert!(!label.is_empty());
            assert!(ids.insert(id), "duplicate id {id}");
            assert!(device_ids.insert(effect.id), "duplicate device id for {id}");
        }
        assert_eq!(ids.len(), 55);
    }

    #[test]
    fn segment_ids_are_validated() {
        assert_eq!(segment_effect_preset(" Electro_Dance "), Some(SegmentEffectPreset::ElectroDance));
        assert_eq!(segment_effect_preset("aurora"), None);
        assert_eq!(segment_effect_preset("led_3"), None);
        assert_eq!(segment_effect_preset(""), None);
    }

    #[test]
    fn classic_catalog_is_unchanged() {
        assert_eq!(EFFECT_CATALOG.len(), 17);
        assert_eq!(EFFECT_CATALOG[0], ("aurora", "Aurora"));
        assert_eq!(EFFECT_CATALOG[16], ("valentines", "Valentines"));
    }

    #[tokio::test]
    async fn unknown_segment_effect_is_rejected() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let err = d.set_segment_effect("disco_inferno", None).await.unwrap_err();
        assert!(err.contains("Unknown segment effect"), "{err}");
    }

    #[tokio::test]
    async fn segment_effect_is_rejected_on_unsupported_devices() {
        let mut d = TapoDevice::new("user@example.com", "dummy-test-secret");
        assert!(d.set_segment_effect("disco", None).await.unwrap_err().contains("RGBIC"));
        d.model = Some("L900".into());
        assert!(d.set_segment_effect("disco", Some(40)).await.unwrap_err().contains("L900"));
    }

    #[test]
    fn protocol_errors_are_reported_and_redacted() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let protocol_error = tapo::Error::Validation {
            field: "segments".into(),
            message: "Required for custom segment effects (user@example.com)".into(),
        };
        let msg = d.err("Set segment effect failed", protocol_error);
        assert!(msg.starts_with("Set segment effect failed"), "{msg}");
        assert!(msg.contains("segments"), "{msg}");
        assert!(!msg.contains("user@example.com"), "{msg}");
    }

    #[tokio::test]
    async fn command_gate_serializes_commands() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let gate = Arc::new(CommandGate::default());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let tasks: Vec<_> = (0..8).map(|_| {
            let (gate, in_flight, max_seen) = (gate.clone(), in_flight.clone(), max_seen.clone());
            tokio::spawn(async move {
                gate.run(async {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                }).await
            })
        }).collect();
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn segment_presets_only_reference_firmware_patterns() {
        let expected = ["brightness", "custom", "display_colors", "enable", "id", "name"];
        for (id, _) in SEGMENT_EFFECT_CATALOG {
            let effect = SegmentEffect::from(segment_effect_preset(id).unwrap());
            let json = serde_json::to_value(&effect).unwrap();
            let mut keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            assert_eq!(keys, expected, "{id}");
        }
    }

    #[test]
    fn brightness_is_the_only_field_changed_on_presets() {
        for (id, _) in EFFECT_CATALOG {
            let preset = || LightingEffect::from(effect_preset(id).unwrap());
            let mut original = serde_json::to_value(preset()).unwrap();
            let changed = serde_json::to_value(preset().with_brightness(37)).unwrap();
            assert_eq!(changed["brightness"], 37, "{id}");
            original["brightness"] = 37.into();
            assert_eq!(changed, original, "{id}");
        }
        for (id, _) in SEGMENT_EFFECT_CATALOG {
            let preset = || SegmentEffect::from(segment_effect_preset(id).unwrap());
            let mut original = serde_json::to_value(preset()).unwrap();
            let changed = serde_json::to_value(preset().with_brightness(37)).unwrap();
            original["brightness"] = 37.into();
            assert_eq!(changed, original, "{id}");
        }
    }

    fn prepared_json(experiment: Experiment) -> serde_json::Value {
        match experiment.prepare() {
            PreparedExperiment::Lighting(effect) => serde_json::to_value(effect).unwrap(),
            PreparedExperiment::Segment(effect) => serde_json::to_value(effect).unwrap(),
        }
    }

    #[test]
    fn x1_payload_matches_the_library_example() {
        let mut payload = prepared_json(Experiment::X1StaticRedWhiteBlue);
        let id = payload.as_object_mut().unwrap().remove("id").unwrap();
        assert_eq!(id.as_str().map(str::len), Some(32), "random uuid id");
        assert_eq!(payload, serde_json::json!({
            "brightness": 100,
            "custom": 1,
            "display_colors": [[359, 85, 100]],
            "enable": 1,
            "name": "My Custom Static Effect",
            "type": "static",
            "expansion_strategy": 1,
            "segments": [0, 1, 2],
            "sequence": [[359, 85, 100], [0, 0, 100], [236, 72, 100]]
        }));
    }

    #[test]
    fn x2_payload_matches_the_library_example() {
        let mut payload = prepared_json(Experiment::X2SequenceRedWhiteBlue);
        let id = payload.as_object_mut().unwrap().remove("id").unwrap();
        assert_eq!(id.as_str().map(str::len), Some(32), "random uuid id");
        assert_eq!(payload, serde_json::json!({
            "brightness": 100,
            "custom": 1,
            "display_colors": [[359, 85, 100]],
            "enable": 1,
            "name": "My Custom Sequence Effect",
            "type": "sequence",
            "direction": 1,
            "duration": 50,
            "expansion_strategy": 1,
            "segments": [0, 1, 2],
            "sequence": [[359, 85, 100], [0, 0, 100], [236, 72, 100]]
        }));
    }

    #[test]
    fn x3_payload_matches_the_library_example() {
        let mut payload = prepared_json(Experiment::X3SegmentCirculating);
        let id = payload.as_object_mut().unwrap().remove("id").unwrap();
        assert_eq!(id.as_str().map(str::len), Some(32), "random uuid id");
        assert_eq!(payload, serde_json::json!({
            "brightness": 100,
            "custom": 1,
            "deviceType": "strip",
            "display_colors": [[267, 56, 100, 0]],
            "enable": 1,
            "name": "My Custom Segment Effect",
            "type": "circulating",
            "segments": [49],
            "states": [[267, 56, 100, 0]]
        }));
        assert!(matches!(
            Experiment::X3SegmentCirculating.prepare(),
            PreparedExperiment::Segment(_)
        ));
    }

    #[test]
    fn x4_changes_only_the_rainbow_transition() {
        let baseline = serde_json::to_value(LightingEffect::from(LightingEffectPreset::Rainbow)).unwrap();
        assert_eq!(baseline["transition"], 1500);
        assert_eq!(baseline["id"], "TapoStrip_7CC5y4lsL8pETYvmz7UOpQ");
        for (experiment, transition) in [
            (Experiment::X4aRainbowTransition3000, 3000),
            (Experiment::X4bRainbowTransition750, 750),
        ] {
            let payload = prepared_json(experiment);
            let mut expected = baseline.clone();
            expected["transition"] = serde_json::json!(transition);
            assert_eq!(payload, expected, "{experiment:?}");
        }
    }

    #[test]
    fn x5_changes_only_the_rainbow_direction() {
        let baseline = serde_json::to_value(LightingEffect::from(LightingEffectPreset::Rainbow)).unwrap();
        assert_eq!(baseline["direction"], 1);
        assert_eq!(baseline["transition"], 1500);
        assert_eq!(baseline["id"], "TapoStrip_7CC5y4lsL8pETYvmz7UOpQ");
        for (experiment, direction) in [
            (Experiment::X5aRainbowDirection3, 3),
            (Experiment::X5bRainbowDirection4, 4),
        ] {
            let payload = prepared_json(experiment);
            let mut expected = baseline.clone();
            expected["direction"] = serde_json::json!(direction);
            assert_eq!(payload, expected, "{experiment:?}");
        }
    }

    #[test]
    fn experiment_ids_are_a_closed_set() {
        let x1: Experiment = serde_json::from_str(r#""x1_static_red_white_blue""#).unwrap();
        let x2: Experiment = serde_json::from_str(r#""x2_sequence_red_white_blue""#).unwrap();
        let x3: Experiment = serde_json::from_str(r#""x3_segment_circulating""#).unwrap();
        assert_eq!(x1, Experiment::X1StaticRedWhiteBlue);
        assert_eq!(x2, Experiment::X2SequenceRedWhiteBlue);
        assert_eq!(x3, Experiment::X3SegmentCirculating);
        let x4a: Experiment = serde_json::from_str(r#""x4a_rainbow_transition_3000""#).unwrap();
        let x4b: Experiment = serde_json::from_str(r#""x4b_rainbow_transition_750""#).unwrap();
        assert_eq!(x4a, Experiment::X4aRainbowTransition3000);
        assert_eq!(x4b, Experiment::X4bRainbowTransition750);
        let x5a: Experiment = serde_json::from_str(r#""x5a_rainbow_direction_3""#).unwrap();
        let x5b: Experiment = serde_json::from_str(r#""x5b_rainbow_direction_4""#).unwrap();
        assert_eq!(x5a, Experiment::X5aRainbowDirection3);
        assert_eq!(x5b, Experiment::X5bRainbowDirection4);
        for unknown in [
            r#""x2_sequence""#, r#""x3_segment""#, r#""x4_transition""#, r#""x5_direction""#,
            r#""X1StaticRedWhiteBlue""#, r#""""#, "1",
        ] {
            assert!(serde_json::from_str::<Experiment>(unknown).is_err(), "{unknown}");
        }
    }

    #[test]
    fn experiments_leave_the_preset_catalogs_unchanged() {
        assert_eq!(EFFECT_CATALOG.len(), 17);
        assert_eq!(SEGMENT_EFFECT_CATALOG.len(), 55);
        assert!(effect_preset("rainbow").is_some());
        let names = [
            Experiment::X1StaticRedWhiteBlue,
            Experiment::X2SequenceRedWhiteBlue,
            Experiment::X3SegmentCirculating,
            Experiment::X4aRainbowTransition3000,
            Experiment::X4bRainbowTransition750,
            Experiment::X5aRainbowDirection3,
            Experiment::X5bRainbowDirection4,
        ].map(|experiment| match experiment.prepare() {
            PreparedExperiment::Lighting(effect) => effect.name,
            PreparedExperiment::Segment(effect) => effect.name,
        });
        assert_eq!(names.len(), 7);
        assert_ne!(names[0], names[1]);
        assert_ne!(names[1], names[2]);
    }

    #[test]
    fn experiment_errors_are_reported_and_redacted() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        let protocol_error = tapo::Error::Validation {
            field: "direction".into(),
            message: "rejected for user@example.com".into(),
        };
        let msg = d.err("Experiment failed", protocol_error);
        assert!(msg.starts_with("Experiment failed"), "{msg}");
        assert!(msg.contains("direction"), "{msg}");
        assert!(!msg.contains("user@example.com"), "{msg}");
        assert!(!msg.contains("dummy-test-secret"), "{msg}");
    }

    #[test]
    fn experiments_follow_the_build_profile() {
        // Debug (`npm run dev`, `cargo test`) enables them. Release installers do not.
        assert_eq!(experiments_enabled(), cfg!(debug_assertions));
    }

    #[test]
    fn an_experiment_selects_only_the_named_strip() {
        let m = map(&["10.0.0.1", "10.0.0.2"]);
        assert_eq!(resolve_target(&m, Some("10.0.0.1")), Ok(&0));
        let missed = resolve_target(&m, Some("10.0.0.9")).unwrap_err();
        assert!(missed.contains("10.0.0.9"), "{missed}");
        assert!(!missed.contains("10.0.0.2"), "{missed}");
    }

    #[tokio::test]
    async fn experiment_is_rejected_on_unsupported_devices() {
        let mut d = TapoDevice::new("user@example.com", "dummy-test-secret");
        d.model = Some("L930".into());
        for experiment in [
            Experiment::X1StaticRedWhiteBlue,
            Experiment::X2SequenceRedWhiteBlue,
            Experiment::X3SegmentCirculating,
            Experiment::X4aRainbowTransition3000,
            Experiment::X4bRainbowTransition750,
            Experiment::X5aRainbowDirection3,
            Experiment::X5bRainbowDirection4,
        ] {
            let err = d.run_experiment(experiment).await.unwrap_err();
            assert!(err.contains("RGBIC"), "{experiment:?}: {err}");
        }
    }

    #[tokio::test]
    async fn capabilities_require_a_connected_device() {
        let d = TapoDevice::new("user@example.com", "dummy-test-secret");
        assert_eq!(d.lighting_capabilities().await, Err("Not connected to device".to_string()));
    }

    #[tokio::test]
    async fn command_gate_runs_commands_in_arrival_order() {
        let gate = Arc::new(CommandGate::default());
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let held = gate.0.lock().await;
        let mut tasks = Vec::new();
        for i in 0..8 {
            let (gate, order) = (gate.clone(), order.clone());
            tasks.push(tokio::spawn(async move {
                gate.run(async { order.lock().unwrap().push(i) }).await
            }));
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        drop(held);
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), (0..8).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn separate_gates_do_not_block_each_other() {
        let a = CommandGate::default();
        let b = CommandGate::default();
        let held = a.0.lock().await;
        let done = tokio::time::timeout(Duration::from_millis(200), b.run(async { 1 })).await;
        assert_eq!(done, Ok(1));
        drop(held);
    }

    #[test]
    fn connections_to_one_ip_share_a_gate() {
        assert!(Arc::ptr_eq(&gate_for("10.99.0.1"), &gate_for("10.99.0.1")));
        assert!(!Arc::ptr_eq(&gate_for("10.99.0.1"), &gate_for("10.99.0.2")));
        let mut old = TapoDevice::new("user@example.com", "dummy-test-secret");
        let mut new = TapoDevice::new("user@example.com", "dummy-test-secret");
        assert!(!Arc::ptr_eq(&old.gate, &new.gate));
        old.attach_gate("10.99.0.3");
        new.attach_gate("10.99.0.3");
        assert!(Arc::ptr_eq(&old.gate, &new.gate));
    }

    #[tokio::test]
    async fn a_reconnected_device_waits_for_the_replaced_connection() {
        let mut replaced = TapoDevice::new("user@example.com", "dummy-test-secret");
        replaced.attach_gate("10.99.0.4");
        let mut current = TapoDevice::new("user@example.com", "dummy-test-secret");
        current.attach_gate("10.99.0.4");
        let current = Arc::new(current);

        // A write still in flight on the replaced connection.
        let in_flight = replaced.gate.0.lock().await;
        let write = tokio::spawn({
            let current = current.clone();
            async move { current.turn_on().await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!write.is_finished(), "a write overlapped the replaced connection");
        drop(in_flight);
        let result = tokio::time::timeout(Duration::from_secs(2), write).await.unwrap().unwrap();
        assert_eq!(result, Err("Not connected to device".to_string()));
    }

    #[tokio::test]
    async fn reconnecting_one_strip_does_not_block_the_other() {
        let mut strip_one = TapoDevice::new("user@example.com", "dummy-test-secret");
        strip_one.attach_gate("10.99.0.5");
        let mut strip_two = TapoDevice::new("user@example.com", "dummy-test-secret");
        strip_two.attach_gate("10.99.0.6");
        let held = strip_one.gate.0.lock().await;
        let done = tokio::time::timeout(Duration::from_millis(500), strip_two.turn_off()).await;
        assert_eq!(done, Ok(Err("Not connected to device".to_string())));
        drop(held);
    }

    #[tokio::test]
    async fn writes_through_a_shared_gate_never_overlap() {
        let gate = gate_for("10.99.0.7");
        let in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..6 {
            // Each task stands for a different connection object to the same strip.
            let (gate, in_flight, max_seen) = (gate_for("10.99.0.7"), in_flight.clone(), max_seen.clone());
            tasks.push(tokio::spawn(async move {
                gate.run(async {
                    let n = in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    max_seen.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                }).await
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(max_seen.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(gate);
    }

    #[tokio::test]
    async fn effect_is_rejected_without_rgbic_handle() {
        let mut d = TapoDevice::new("user@example.com", "dummy-test-secret");
        d.model = Some("L930".into());
        assert!(!d.supports_effects());
        let err = d.set_effect("aurora", None).await.unwrap_err();
        assert!(err.contains("RGBIC"), "{err}");
    }

    #[tokio::test]
    async fn custom_effects_and_rainbow_speed_need_an_rgbic_strip() {
        let mut d = TapoDevice::new("user@example.com", "dummy-test-secret");
        d.model = Some("L930".into());
        let err = d.set_custom_effect(CustomMode::Static, [[0, 0, 100]; 3], 50).await.unwrap_err();
        assert!(err.contains("RGBIC"), "{err}");
        let err = d.set_effect_with_speed("rainbow", None, Some(RainbowSpeed::Slow)).await.unwrap_err();
        assert!(err.contains("RGBIC"), "{err}");
        let err = d.set_effect_with_speed("aurora", None, Some(RainbowSpeed::Slow)).await.unwrap_err();
        assert!(err.contains("Rainbow"), "{err}");
    }

    #[test]
    fn rainbow_speed_is_only_accepted_for_rainbow() {
        let slow = serde_json::to_value(classic_effect("rainbow", Some(30), Some(RainbowSpeed::Slow)).unwrap()).unwrap();
        assert_eq!(slow["transition"], 3000);
        assert_eq!(slow["brightness"], 30);
        assert_eq!(slow["id"], "TapoStrip_7CC5y4lsL8pETYvmz7UOpQ");
        let plain = serde_json::to_value(classic_effect("rainbow", None, None).unwrap()).unwrap();
        assert_eq!(plain, serde_json::to_value(LightingEffect::from(LightingEffectPreset::Rainbow)).unwrap());
        for (name, _) in EFFECT_CATALOG.iter().filter(|(name, _)| *name != "rainbow") {
            assert!(classic_effect(name, None, Some(RainbowSpeed::Fast)).is_err(), "{name}");
            let preset = effect_preset(name).unwrap();
            assert_eq!(
                serde_json::to_value(classic_effect(name, None, None).unwrap()).unwrap(),
                serde_json::to_value(LightingEffect::from(preset)).unwrap(),
                "{name}",
            );
        }
        assert!(classic_effect("rainbow", Some(0), Some(RainbowSpeed::Fast)).is_err());
        assert!(classic_effect("disco", None, None).is_err());
    }
}
