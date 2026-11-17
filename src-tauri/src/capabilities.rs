//! Read-only lighting capability report built from `get_device_info_json`.
//!
//! The raw device info contains network and account data (IP, MAC, SSID,
//! device and hardware ids, nickname, location). Only the values of the
//! fields listed here leave this module; every other key is reported by name
//! only, so new lighting fields can be spotted without exposing their values.

use serde::Serialize;
use serde_json::{Map, Value};

/// Top-level fields whose values may be returned.
const ALLOWED_FIELDS: &[&str] = &[
    "model",
    "type",
    "hw_ver",
    "fw_ver",
    "device_on",
    "brightness",
    "color_temp",
    "color_temp_range",
    "hue",
    "saturation",
    "overheated",
];

/// Objects whose listed sub-fields may be returned. Effect `id`s are left out.
const ALLOWED_OBJECTS: &[(&str, &[&str])] = &[
    ("lighting_effect", &["enable", "custom", "name", "type", "brightness", "segments"]),
];

#[derive(Debug, Serialize, PartialEq)]
pub struct LightingCapabilities {
    /// Values of the allowed fields present in the response.
    pub fields: Map<String, Value>,
    /// Names of every other key (nested ones as `parent.key`); values withheld.
    pub unlisted_keys: Vec<String>,
}

fn is_plain(value: &Value) -> bool {
    match value {
        Value::Object(_) => false,
        Value::Array(items) => items.iter().all(|v| !v.is_object() && !v.is_array()),
        _ => true,
    }
}

pub fn filter(raw: &Value) -> LightingCapabilities {
    let mut fields = Map::new();
    let mut unlisted_keys = Vec::new();
    let Some(object) = raw.as_object() else {
        return LightingCapabilities { fields, unlisted_keys };
    };

    for (key, value) in object {
        if ALLOWED_FIELDS.contains(&key.as_str()) && is_plain(value) {
            fields.insert(key.clone(), value.clone());
        } else if let (Some((_, allowed)), Some(inner)) =
            (ALLOWED_OBJECTS.iter().find(|(name, _)| name == key), value.as_object())
        {
            let mut kept = Map::new();
            for (sub_key, sub_value) in inner {
                if allowed.contains(&sub_key.as_str()) && is_plain(sub_value) {
                    kept.insert(sub_key.clone(), sub_value.clone());
                } else {
                    unlisted_keys.push(format!("{key}.{sub_key}"));
                }
            }
            fields.insert(key.clone(), Value::Object(kept));
        } else {
            unlisted_keys.push(key.clone());
        }
    }
    unlisted_keys.sort();
    LightingCapabilities { fields, unlisted_keys }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "device_id": "dummy-device-id",
            "hw_id": "dummy-hw-id",
            "fw_id": "dummy-fw-id",
            "oem_id": "dummy-oem-id",
            "mac": "00-00-00-00-00-00",
            "ip": "192.0.2.10",
            "ssid": "ZHVtbXktc3NpZA==",
            "nickname": "ZHVtbXktbmFtZQ==",
            "latitude": 1,
            "longitude": 2,
            "region": "Dummy/Region",
            "rssi": -40,
            "model": "L930",
            "type": "SMART.TAPOBULB",
            "fw_ver": "1.0.0",
            "hw_ver": "1.0",
            "device_on": true,
            "brightness": 50,
            "color_temp_range": [2500, 6500],
            "segment_info": {"count": 3},
            "lighting_effect": {
                "enable": 1,
                "id": "TapoStrip_dummy",
                "name": "Rainbow",
                "segments": [0],
                "sequence": [[0, 100, 100]]
            }
        })
    }

    #[test]
    fn sensitive_values_never_leave_the_filter() {
        let report = filter(&sample());
        let text = serde_json::to_string(&report).unwrap();
        for secret in [
            "dummy-device-id", "dummy-hw-id", "dummy-fw-id", "dummy-oem-id",
            "00-00-00-00-00-00", "192.0.2.10", "ZHVtbXktc3NpZA==", "ZHVtbXktbmFtZQ==",
            "Dummy/Region", "TapoStrip_dummy",
        ] {
            assert!(!text.contains(secret), "{secret} leaked");
        }
    }

    #[test]
    fn allowed_fields_are_returned_with_their_values() {
        let report = filter(&sample());
        assert_eq!(report.fields["model"], "L930");
        assert_eq!(report.fields["brightness"], 50);
        assert_eq!(report.fields["color_temp_range"], json!([2500, 6500]));
        assert_eq!(
            report.fields["lighting_effect"],
            json!({"enable": 1, "name": "Rainbow", "segments": [0]})
        );
    }

    #[test]
    fn other_keys_are_listed_by_name_only() {
        let report = filter(&sample());
        for key in ["ip", "mac", "ssid", "segment_info", "lighting_effect.id", "lighting_effect.sequence"] {
            assert!(report.unlisted_keys.contains(&key.to_string()), "{key}");
        }
        assert!(!report.fields.contains_key("segment_info"));
        let mut sorted = report.unlisted_keys.clone();
        sorted.sort();
        assert_eq!(report.unlisted_keys, sorted);
    }

    #[test]
    fn allowed_names_holding_objects_are_withheld() {
        let report = filter(&json!({"model": {"nested": "x"}, "brightness": [[1]]}));
        assert!(report.fields.is_empty());
        assert_eq!(report.unlisted_keys, vec!["brightness", "model"]);
    }

    #[test]
    fn non_object_responses_yield_an_empty_report() {
        assert_eq!(
            filter(&json!([1, 2])),
            LightingCapabilities { fields: Map::new(), unlisted_keys: vec![] }
        );
    }
}
