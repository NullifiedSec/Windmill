//! Per-recording lyric timing offsets. Positive offsets delay the displayed
//! lyrics, e.g. +12.5 means a 12.5-second music-video intro.
pub const MAX_ABS_OFFSET_MS: i64 = 3_600_000;

/// Accept signed decimal seconds, e.g. +12.5 / -3.25, or signed mm:ss.sss.
/// The value replaces the current offset, rather than being added to it.
pub fn parse_offset_ms(input: &str) -> Result<i64, &'static str> {
    let input = input.trim();
    let (sign, value) = if let Some(value) = input.strip_prefix('+') {
        (1, value)
    } else if let Some(value) = input.strip_prefix('-') {
        (-1, value)
    } else {
        (1, input)
    };
    let value = value.trim().trim_end_matches(['s', 'S']);
    if value.is_empty() {
        return Err("Enter seconds like +12.5 or -00:03.250");
    }
    let seconds = if let Some((minutes, secs)) = value.split_once(':') {
        if minutes.is_empty() || secs.is_empty() || secs.contains(':') {
            return Err("Expected mm:ss.sss (e.g. +01:12.500)");
        }
        let minutes: u32 = minutes.parse().map_err(|_| "Invalid minutes")?;
        let seconds: f64 = secs.parse().map_err(|_| "Invalid seconds")?;
        if !(0.0..60.0).contains(&seconds) || !seconds.is_finite() {
            return Err("Seconds must be between 0 and 59.999");
        }
        f64::from(minutes) * 60.0 + seconds
    } else {
        value
            .parse::<f64>()
            .map_err(|_| "Invalid number of seconds")?
    };
    if !seconds.is_finite() || !(0.0..=3600.0).contains(&seconds) {
        return Err("Offset must be within ±60 minutes");
    }
    let offset = (seconds * 1000.0).round() as i64 * sign;
    Ok(offset.clamp(-MAX_ABS_OFFSET_MS, MAX_ABS_OFFSET_MS))
}

pub fn format_offset_ms(ms: i64) -> String {
    let sign = if ms < 0 { '-' } else { '+' };
    let abs = ms.unsigned_abs();
    format!(
        "{sign}{}:{:02}.{:03}",
        abs / 60_000,
        (abs / 1000) % 60,
        abs % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_precise_music_video_offsets() {
        assert_eq!(parse_offset_ms("+12.5"), Ok(12_500));
        assert_eq!(parse_offset_ms("12.500s"), Ok(12_500));
        assert_eq!(parse_offset_ms("-3.25"), Ok(-3_250));
        assert_eq!(parse_offset_ms("01:12.250"), Ok(72_250));
        assert_eq!(parse_offset_ms("-00:03.250"), Ok(-3_250));
        assert_eq!(parse_offset_ms("0"), Ok(0));
        assert_eq!(parse_offset_ms("1.2345"), Ok(1_235));
        assert_eq!(parse_offset_ms("+60:00.000"), Ok(MAX_ABS_OFFSET_MS));
    }

    #[test]
    fn rejects_invalid_and_excessive_offsets() {
        for entry in [
            "", "+", "--2", "foo", "NaN", "Infinity", "1e999", "1:60", "1:2:3", "1:-3", "-62:00",
            "3600.001", "0:99", ".",
        ] {
            assert!(parse_offset_ms(entry).is_err(), "accepted {entry}");
        }
    }

    #[test]
    fn displayed_values_round_trip() {
        for value in [0, 12_500, -3_250, 3_600_000, -3_600_000] {
            assert_eq!(parse_offset_ms(&format_offset_ms(value)), Ok(value));
        }
    }
}
