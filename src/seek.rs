// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mashrur Rahman Rawnok (NullifiedSec)
//! Pure timeline conversions shared by shortcuts, jump-to-time and mouse seek.
/// An absolute input such as 1:23.5 or 83.5; no negative or NaN values.
pub fn parse_time_ms(input: &str) -> Result<i64, &'static str> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a time such as 1:23 or 83.5");
    }
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|s| s.is_empty()) {
        return Err("Use seconds, m:ss or h:mm:ss");
    }
    let seconds = parts
        .last()
        .unwrap()
        .parse::<f64>()
        .map_err(|_| "Invalid seconds")?;
    if !seconds.is_finite() || seconds < 0.0 || (parts.len() > 1 && seconds >= 60.0) {
        return Err("Invalid seconds");
    }
    let mut total = seconds;
    if parts.len() >= 2 {
        let minutes = parts[parts.len() - 2]
            .parse::<u64>()
            .map_err(|_| "Invalid minutes")?;
        if parts.len() == 3 && minutes >= 60 {
            return Err("Minutes must be below 60");
        }
        total += minutes as f64 * 60.0;
    }
    if parts.len() == 3 {
        let hours = parts[0].parse::<u64>().map_err(|_| "Invalid hours")?;
        total += hours as f64 * 3600.0;
    }
    if !total.is_finite() || total > 86_400.0 {
        return Err("Time exceeds 24 hours");
    }
    Ok((total * 1000.0).round() as i64)
}
pub fn clamp_target(current: i64, delta: i64, duration: Option<i64>) -> i64 {
    let destination = current.saturating_add(delta).max(0);
    match duration.filter(|d| *d > 0) {
        Some(d) => destination.min(d),
        None => destination,
    }
}
pub fn progress_target(column: u16, area_x: u16, area_width: u16, duration_ms: i64) -> Option<i64> {
    if duration_ms <= 0 || area_width < 3 {
        return None;
    }
    let inner_x = area_x.saturating_add(1);
    let end = area_x.saturating_add(area_width).saturating_sub(2);
    if column < inner_x || column > end {
        return None;
    }
    let length = end.saturating_sub(inner_x);
    if length == 0 {
        return None;
    }
    Some(((u128::from(column - inner_x) * duration_ms as u128) / u128::from(length)) as i64)
}
pub fn format_time(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_jump_time_formats() {
        assert_eq!(parse_time_ms("90"), Ok(90_000));
        assert_eq!(parse_time_ms("1:23.5"), Ok(83_500));
        assert_eq!(parse_time_ms("0:02:00"), Ok(120_000));
        assert_eq!(parse_time_ms("02:03:04.125"), Ok(7_384_125));
        assert_eq!(parse_time_ms("0"), Ok(0));
        for invalid in [
            "",
            "-10",
            "NaN",
            "Infinity",
            "1:80",
            "4::20",
            "9:3:10:40",
            "2:60:00",
            "1e309",
            "1000000",
        ] {
            assert!(parse_time_ms(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn seeks_saturate_at_track_edges() {
        assert_eq!(clamp_target(2_000, -10_000, Some(200_000)), 0);
        assert_eq!(clamp_target(195_000, 30_000, Some(200_000)), 200_000);
        assert_eq!(clamp_target(100_000, 10_000, None), 110_000);
    }
    #[test]
    fn mouse_seeking_is_safe_and_precise() {
        assert_eq!(progress_target(1, 0, 102, 200_000), Some(0));
        assert!((progress_target(51, 0, 102, 200_000).unwrap() - 100_000).abs() <= 1_100);
        assert_eq!(progress_target(100, 0, 102, 200_000), Some(200_000));
        assert_eq!(progress_target(0, 0, 102, 200_000), None);
        assert_eq!(progress_target(50, 0, 102, 0), None);
        assert_eq!(progress_target(1, 0, 2, 200_000), None);
    }
}
