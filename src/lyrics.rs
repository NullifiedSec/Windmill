//! Basic synchronized LRC lyrics, kept independent of playback engines.
//! The timestamp used for comparisons is milliseconds. Positive [offset:...] delays lyrics.
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub at_ms: i64,
    pub text: String,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Lyrics {
    lines: Vec<Line>,
    offset_ms: i64,
}
impl Lyrics {
    pub fn parse(text: &str) -> Self {
        let mut lyrics = Self::default();
        for raw in text.lines() {
            let mut remaining = raw.trim();
            if remaining.starts_with("[offset:") && remaining.ends_with(']') {
                if let Some(value) = remaining
                    .strip_prefix("[offset:")
                    .and_then(|x| x.strip_suffix(']'))
                    .and_then(|x| i64::from_str(x.trim()).ok())
                {
                    lyrics.offset_ms = value;
                }
                continue;
            }
            let mut stamps = Vec::new();
            while let Some(rest) = remaining.strip_prefix('[') {
                let Some(close) = rest.find(']') else { break };
                let (stamp, tail) = rest.split_at(close);
                match parse_stamp(stamp) {
                    Some(ms) => {
                        stamps.push(ms);
                        remaining = &tail[1..];
                    }
                    None => break,
                };
            }
            for stamp in stamps {
                lyrics.lines.push(Line {
                    at_ms: stamp,
                    text: remaining.trim().to_string(),
                });
            }
        }
        lyrics.lines.sort_by_key(|line| line.at_ms);
        lyrics
    }
    pub fn active_index(&self, elapsed_ms: i64) -> Option<usize> {
        let timeline_ms = elapsed_ms.saturating_sub(self.offset_ms);
        let next = self.lines.partition_point(|line| line.at_ms <= timeline_ms);
        next.checked_sub(1)
    }
    pub fn window(&self, elapsed_ms: i64, before: usize, after: usize) -> (Option<usize>, &[Line]) {
        let active = self.active_index(elapsed_ms);
        let center = active.unwrap_or(0);
        let first = center.saturating_sub(before);
        let last = (center.saturating_add(after).saturating_add(1)).min(self.lines.len());
        (active.map(|v| v - first), &self.lines[first..last])
    }
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
fn parse_stamp(input: &str) -> Option<i64> {
    let (minutes, seconds) = input.split_once(':')?;
    let minutes = minutes.parse::<i64>().ok()?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let seconds = seconds.parse::<i64>().ok()?;
    if !(0..60).contains(&seconds) || minutes < 0 {
        return None;
    }
    let millis = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()? * 100,
        2 => fraction.parse::<i64>().ok()? * 10,
        3 => fraction.parse::<i64>().ok()?,
        _ => return None,
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1_000)?
        .checked_add(millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_sorts_multitimestamps_and_metadata() {
        let l = Lyrics::parse("[ar:Some Artist]\n[01:20.50][00:12.4] same\n[00:15.500] next\n");
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].at_ms, 12_400);
        assert_eq!(l.lines[2].at_ms, 80_500);
        assert_eq!(l.active_index(14_000), Some(0));
        assert_eq!(l.active_index(16_000), Some(1));
        assert_eq!(l.active_index(12_399), None);
    }
    #[test]
    fn offsets_and_window_keep_order() {
        let l =
            Lyrics::parse("[offset:250]\n[00:01.00] first\n[00:02.00] second\n[00:03.00] third");
        assert_eq!(l.active_index(1_100), None);
        assert_eq!(l.active_index(1_250), Some(0));
        let (active, around) = l.window(2_300, 1, 1);
        assert_eq!(active, Some(1));
        assert_eq!(
            around.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }
    #[test]
    fn empty_and_invalid_are_safe() {
        let l = Lyrics::parse("[meta:abc]\n[01:99.0]bad\n[00:23.0000]bad\n");
        assert!(l.is_empty());
        assert_eq!(l.active_index(0), None);
        assert_eq!(l.window(0, 3, 3).1.len(), 0);
    }
}
