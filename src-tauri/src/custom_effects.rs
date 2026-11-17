//! Configurable lighting effects, limited to structures already seen working
//! on an L930-5: the custom `Static` (X1) and `Sequence` (X2) payloads with
//! the caller's three colors, and Rainbow with one of the `transition` values
//! tried in X4. Nothing else in the payloads comes from the caller.

use serde::{Deserialize, Serialize};
use tapo::requests::{LightingEffect, LightingEffectPreset, LightingEffectType};

use crate::validation;

/// `[hue, saturation, value]`, the color layout of `LightingEffect`.
pub type Hsv = [u16; 3];

pub const COLOR_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomMode {
    Static,
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectColor {
    pub hue: i32,
    pub saturation: i32,
    pub value: i32,
}

/// Only `{ "hue", "saturation", "value" }`; derived impls would also accept an array.
impl<'de> Deserialize<'de> for EffectColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            hue: i32,
            saturation: i32,
            value: i32,
        }
        let map = serde_json::Map::deserialize(deserializer)?;
        let f: Fields = serde_json::from_value(serde_json::Value::Object(map))
            .map_err(serde::de::Error::custom)?;
        Ok(EffectColor { hue: f.hue, saturation: f.saturation, value: f.value })
    }
}

/// Only these three values; X4 showed 750 faster and 3000 slower than the
/// preset's 1500 on one L930-5. Their units are not documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RainbowSpeed {
    Fast,
    Normal,
    Slow,
}

impl RainbowSpeed {
    pub fn transition(self) -> u16 {
        match self {
            RainbowSpeed::Fast => 750,
            RainbowSpeed::Normal => 1500,
            RainbowSpeed::Slow => 3000,
        }
    }
}

pub fn validate_color(color: &EffectColor) -> Result<Hsv, String> {
    Ok([
        validation::hue(color.hue)?,
        validation::effect_saturation(color.saturation)?,
        validation::effect_value(color.value)?,
    ])
}

pub fn validate_colors(colors: &[EffectColor]) -> Result<[Hsv; COLOR_COUNT], String> {
    if colors.len() != COLOR_COUNT {
        return Err(format!("Exactly {COLOR_COUNT} colors are required (got {})", colors.len()));
    }
    Ok([validate_color(&colors[0])?, validate_color(&colors[1])?, validate_color(&colors[2])?])
}

/// The X1/X2 payload with the given colors. `sequence` carries the three
/// colors and `display_colors` the first one, as in the `tapo` example.
/// `segments: [0, 1, 2]` is copied from that example; it is not a claim
/// about three physical zones.
pub fn build_custom(mode: CustomMode, colors: [Hsv; COLOR_COUNT], brightness: u8) -> LightingEffect {
    let (name, effect_type) = match mode {
        CustomMode::Static => ("My Custom Static Effect", LightingEffectType::Static),
        CustomMode::Sequence => ("My Custom Sequence Effect", LightingEffectType::Sequence),
    };
    let effect = LightingEffect::new(name, effect_type, true, true, brightness, vec![colors[0]])
        .with_expansion_strategy(1)
        .with_segments(vec![0, 1, 2])
        .with_sequence(colors.to_vec());
    match mode {
        CustomMode::Static => effect,
        CustomMode::Sequence => effect.with_direction(1).with_duration(50),
    }
}

/// The Rainbow preset with only `transition` replaced.
pub fn rainbow(speed: RainbowSpeed) -> LightingEffect {
    LightingEffect::from(LightingEffectPreset::Rainbow).with_transition(speed.transition())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Experiment, PreparedExperiment};

    const RED_WHITE_BLUE: [Hsv; 3] = [[359, 85, 100], [0, 0, 100], [236, 72, 100]];

    fn color(hue: i32, saturation: i32, value: i32) -> EffectColor {
        EffectColor { hue, saturation, value }
    }

    fn json_without_id(effect: LightingEffect) -> serde_json::Value {
        let mut v = serde_json::to_value(effect).unwrap();
        let id = v.as_object_mut().unwrap().remove("id").unwrap();
        assert_eq!(id.as_str().unwrap().len(), 32);
        v
    }

    fn experiment_json(experiment: Experiment) -> serde_json::Value {
        match experiment.prepare() {
            PreparedExperiment::Lighting(effect) => json_without_id(effect),
            PreparedExperiment::Segment(_) => panic!("{experiment:?} is a segment effect"),
        }
    }

    #[test]
    fn colors_are_validated_per_component() {
        assert_eq!(validate_color(&color(0, 0, 1)), Ok([0, 0, 1]));
        assert_eq!(validate_color(&color(360, 100, 100)), Ok([360, 100, 100]));
        for bad in [color(-1, 50, 50), color(361, 50, 50), color(10, -1, 50), color(10, 101, 50),
                    color(10, 50, 0), color(10, 50, 101), color(i32::MAX, 50, 50)] {
            assert!(validate_color(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn exactly_three_colors_are_required() {
        let c = color(10, 20, 30);
        assert_eq!(validate_colors(&[c, c, c]), Ok([[10, 20, 30]; 3]));
        assert!(validate_colors(&[c, c]).is_err());
        assert!(validate_colors(&[c, c, c, c]).is_err());
        assert!(validate_colors(&[]).is_err());
        assert!(validate_colors(&[c, color(10, 20, 0), c]).is_err());
    }

    #[test]
    fn color_requests_reject_unknown_fields_and_types() {
        let ok: EffectColor = serde_json::from_str(r#"{"hue":1,"saturation":2,"value":3}"#).unwrap();
        assert_eq!(ok, color(1, 2, 3));
        for bad in [
            r#"{"hue":1,"saturation":2}"#,
            r#"{"hue":1,"saturation":2,"value":3,"segment":0}"#,
            r#"{"hue":"red","saturation":2,"value":3}"#,
            r#"{"hue":1.5,"saturation":2,"value":3}"#,
            r#"[1,2,3]"#,
        ] {
            assert!(serde_json::from_str::<EffectColor>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn red_white_blue_static_matches_the_x1_payload() {
        let built = json_without_id(build_custom(CustomMode::Static, RED_WHITE_BLUE, 100));
        assert_eq!(built, experiment_json(Experiment::X1StaticRedWhiteBlue));
    }

    #[test]
    fn red_white_blue_sequence_matches_the_x2_payload() {
        let built = json_without_id(build_custom(CustomMode::Sequence, RED_WHITE_BLUE, 100));
        assert_eq!(built, experiment_json(Experiment::X2SequenceRedWhiteBlue));
    }

    #[test]
    fn custom_payloads_carry_only_the_chosen_colors_and_brightness() {
        let colors = [[120, 100, 100], [45, 60, 80], [300, 0, 50]];
        for mode in [CustomMode::Static, CustomMode::Sequence] {
            let v = json_without_id(build_custom(mode, colors, 40));
            assert_eq!(v["brightness"], 40);
            assert_eq!(v["custom"], 1);
            assert_eq!(v["enable"], 1);
            assert_eq!(v["display_colors"], serde_json::json!([colors[0]]));
            assert_eq!(v["sequence"], serde_json::json!(colors));
            assert_eq!(v["segments"], serde_json::json!([0, 1, 2]));
            assert_eq!(v["expansion_strategy"], 1);
            let base = match mode {
                CustomMode::Static => Experiment::X1StaticRedWhiteBlue,
                CustomMode::Sequence => Experiment::X2SequenceRedWhiteBlue,
            };
            let mut expected = experiment_json(base);
            expected["brightness"] = serde_json::json!(40);
            expected["display_colors"] = serde_json::json!([colors[0]]);
            expected["sequence"] = serde_json::json!(colors);
            assert_eq!(v, expected, "{mode:?}");
        }
        let sequence = json_without_id(build_custom(CustomMode::Sequence, colors, 40));
        assert_eq!(sequence["type"], "sequence");
        assert_eq!(sequence["direction"], 1);
        assert_eq!(sequence["duration"], 50);
        let fixed = json_without_id(build_custom(CustomMode::Static, colors, 40));
        assert_eq!(fixed["type"], "static");
        assert!(fixed.get("direction").is_none());
        assert!(fixed.get("duration").is_none());
    }

    #[test]
    fn modes_are_a_closed_set() {
        assert_eq!(serde_json::from_str::<CustomMode>(r#""static""#).unwrap(), CustomMode::Static);
        assert_eq!(serde_json::from_str::<CustomMode>(r#""sequence""#).unwrap(), CustomMode::Sequence);
        for bad in [r#""pulse""#, r#""random""#, r#""circulating""#, r#""Static""#, r#""""#] {
            assert!(serde_json::from_str::<CustomMode>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rainbow_speeds_map_to_the_x4_transitions_only() {
        assert_eq!(RainbowSpeed::Fast.transition(), 750);
        assert_eq!(RainbowSpeed::Normal.transition(), 1500);
        assert_eq!(RainbowSpeed::Slow.transition(), 3000);
        for bad in [r#""faster""#, r#""1500""#, "1500", r#""Fast""#, r#""""#] {
            assert!(serde_json::from_str::<RainbowSpeed>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rainbow_speed_changes_only_the_transition() {
        let preset = serde_json::to_value(LightingEffect::from(LightingEffectPreset::Rainbow)).unwrap();
        assert_eq!(serde_json::to_value(rainbow(RainbowSpeed::Normal)).unwrap(), preset);
        for speed in [RainbowSpeed::Fast, RainbowSpeed::Slow] {
            let mut expected = preset.clone();
            expected["transition"] = serde_json::json!(speed.transition());
            assert_eq!(serde_json::to_value(rainbow(speed)).unwrap(), expected, "{speed:?}");
        }
        let fast = serde_json::to_value(rainbow(RainbowSpeed::Fast)).unwrap();
        let x4b = serde_json::to_value(match Experiment::X4bRainbowTransition750.prepare() {
            PreparedExperiment::Lighting(effect) => effect,
            PreparedExperiment::Segment(_) => unreachable!(),
        }).unwrap();
        assert_eq!(fast, x4b);
    }

    #[test]
    fn rainbow_speed_survives_a_brightness_change() {
        let v = serde_json::to_value(rainbow(RainbowSpeed::Slow).with_brightness(30)).unwrap();
        assert_eq!(v["transition"], 3000);
        assert_eq!(v["brightness"], 30);
        assert_eq!(v["direction"], 1);
    }
}
