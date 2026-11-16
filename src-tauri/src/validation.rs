//! Validation for values crossing the IPC boundary.
//!
//! Ranges mirror the checks performed by the `tapo` crate (0.8.x) so invalid
//! input is rejected before any request is sent to a device.

use std::net::IpAddr;

pub const BRIGHTNESS_RANGE: (i32, i32) = (1, 100);
pub const HUE_RANGE: (i32, i32) = (0, 360);
pub const SATURATION_RANGE: (i32, i32) = (1, 100);
pub const COLOR_TEMPERATURE_RANGE: (i32, i32) = (2500, 6500);

pub const SCHEDULE_ACTIONS: [&str; 3] = ["on", "off", "preset"];

fn in_range(field: &str, value: i32, (min, max): (i32, i32)) -> Result<i32, String> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{field} must be between {min} and {max} (got {value})"))
    }
}

pub fn brightness(value: i32) -> Result<u8, String> {
    let v = in_range("brightness", value, BRIGHTNESS_RANGE)?;
    u8::try_from(v).map_err(|_| "brightness out of range".to_string())
}

pub fn hue(value: i32) -> Result<u16, String> {
    let v = in_range("hue", value, HUE_RANGE)?;
    u16::try_from(v).map_err(|_| "hue out of range".to_string())
}

pub fn saturation(value: i32) -> Result<u8, String> {
    let v = in_range("saturation", value, SATURATION_RANGE)?;
    u8::try_from(v).map_err(|_| "saturation out of range".to_string())
}

pub fn color_temperature(value: i32) -> Result<u16, String> {
    let v = in_range("color temperature", value, COLOR_TEMPERATURE_RANGE)?;
    u16::try_from(v).map_err(|_| "color temperature out of range".to_string())
}

/// Accepts an IPv4/IPv6 literal and returns its canonical textual form.
pub fn device_ip(value: &str) -> Result<String, String> {
    value
        .trim()
        .parse::<IpAddr>()
        .map(|ip| ip.to_string())
        .map_err(|_| format!("Invalid device IP address: {}", value.trim()))
}

pub fn email(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return Err("A valid Tapo account email is required".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn password(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("Password is required".to_string());
    }
    Ok(())
}

/// Parses `HH:MM` (24h) into `(hour, minute)`.
pub fn schedule_time(value: &str) -> Result<(u8, u8), String> {
    let err = || format!("Schedule time must use HH:MM 24h format (got {value})");
    let (h, m) = value.split_once(':').ok_or_else(err)?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return Err(err());
    }
    let hour: u8 = h.parse().map_err(|_| err())?;
    let minute: u8 = m.parse().map_err(|_| err())?;
    if hour > 23 || minute > 59 {
        return Err(err());
    }
    Ok((hour, minute))
}

/// Days use 0 = Monday … 6 = Sunday.
pub fn schedule_days(days: &[i32]) -> Result<(), String> {
    match days.iter().find(|d| !(0..=6).contains(*d)) {
        Some(d) => Err(format!("Schedule day must be between 0 (Mon) and 6 (Sun) (got {d})")),
        None => Ok(()),
    }
}

pub fn schedule_action(action: &str) -> Result<(), String> {
    if SCHEDULE_ACTIONS.contains(&action) {
        Ok(())
    } else {
        Err(format!("Unknown schedule action: {action}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_accepts_bounds_and_rejects_outside() {
        assert_eq!(brightness(1), Ok(1));
        assert_eq!(brightness(100), Ok(100));
        assert!(brightness(0).is_err());
        assert!(brightness(101).is_err());
        assert!(brightness(-1).is_err());
        assert!(brightness(300).is_err());
    }

    #[test]
    fn hue_accepts_bounds_and_rejects_outside() {
        assert_eq!(hue(0), Ok(0));
        assert_eq!(hue(360), Ok(360));
        assert!(hue(361).is_err());
        assert!(hue(-10).is_err());
    }

    #[test]
    fn saturation_matches_tapo_range() {
        assert_eq!(saturation(1), Ok(1));
        assert_eq!(saturation(100), Ok(100));
        assert!(saturation(0).is_err());
        assert!(saturation(101).is_err());
        assert!(saturation(i32::MIN).is_err());
    }

    #[test]
    fn color_temperature_matches_tapo_range() {
        assert_eq!(color_temperature(2500), Ok(2500));
        assert_eq!(color_temperature(6500), Ok(6500));
        assert!(color_temperature(2499).is_err());
        assert!(color_temperature(6501).is_err());
        assert!(color_temperature(i32::MAX).is_err());
    }

    #[test]
    fn device_ip_normalizes_and_rejects_garbage() {
        assert_eq!(device_ip(" 192.168.1.20 ").as_deref(), Ok("192.168.1.20"));
        assert!(device_ip("192.168.1.256").is_err());
        assert!(device_ip("http://192.168.1.20").is_err());
        assert!(device_ip("").is_err());
    }

    #[test]
    fn email_and_password_require_content() {
        assert_eq!(email(" user@example.com ").as_deref(), Ok("user@example.com"));
        assert!(email("").is_err());
        assert!(email("not-an-email").is_err());
        assert!(password("").is_err());
        assert!(password(" ").is_ok());
    }

    #[test]
    fn schedule_time_parses_24h() {
        assert_eq!(schedule_time("07:05"), Ok((7, 5)));
        assert_eq!(schedule_time("7:05"), Ok((7, 5)));
        assert_eq!(schedule_time("23:59"), Ok((23, 59)));
        assert!(schedule_time("24:00").is_err());
        assert!(schedule_time("12:60").is_err());
        assert!(schedule_time("12").is_err());
        assert!(schedule_time("12:5").is_err());
        assert!(schedule_time("1 2:30").is_err());
        assert!(schedule_time("*/5:00").is_err());
    }

    #[test]
    fn schedule_days_and_action() {
        assert!(schedule_days(&[0, 3, 6]).is_ok());
        assert!(schedule_days(&[]).is_ok());
        assert!(schedule_days(&[7]).is_err());
        assert!(schedule_days(&[-1]).is_err());
        assert!(schedule_action("on").is_ok());
        assert!(schedule_action("preset").is_ok());
        assert!(schedule_action("reboot").is_err());
    }
}
