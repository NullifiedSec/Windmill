// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mashrur Rahman Rawnok (NullifiedSec)
//! Basic synchronized LRC lyrics, kept independent of playback engines.
//! The timestamp used for comparisons is milliseconds. Positive [offset:...] delays lyrics.
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub at_ms: i64,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub at_ms: i64,
    pub text: String,
    pub words: Vec<Word>,
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
                let (text, words) = parse_enhanced(remaining.trim(), stamp);
                lyrics.lines.push(Line {
                    at_ms: stamp,
                    text,
                    words,
                });
            }
        }
        lyrics.lines.sort_by_key(|line| line.at_ms);
        lyrics
    }
    /// A synchronized transcript can end before the actual media does.
    /// Explicitly append an empty timestamp so the last lyric stops
    /// highlighting during an instrumental/outro. Never alter media bytes.
    pub fn pad_blank_to(&mut self, duration_ms: i64) {
        if duration_ms <= 0 || duration_ms > 7_200_000 {
            return;
        }
        if self
            .lines
            .last()
            .is_some_and(|last| last.at_ms < duration_ms)
        {
            self.lines.push(Line {
                at_ms: duration_ms,
                text: String::new(),
                words: vec![],
            });
        }
    }
    /// Used by the TUI for non-destructive alignment and virtual padding.
    /// A positive shift creates blank intro time; the remaining media after
    /// the transcript is blank outro; lyrics past EOF remain on a virtual
    /// timeline, they do not extend actual audio playback.
    pub fn padding(&self, media_duration_ms: Option<i64>, user_offset_ms: i64) -> TimelinePadding {
        let first = self.lines.first().map_or(0, |l| {
            l.at_ms
                .saturating_add(self.offset_ms)
                .saturating_add(user_offset_ms)
        });
        let end = self.lines.last().map_or(0, |l| {
            l.at_ms
                .saturating_add(self.offset_ms)
                .saturating_add(user_offset_ms)
        });
        let media_end = media_duration_ms.unwrap_or(0).max(0);
        TimelinePadding {
            blank_intro_ms: first.max(0),
            blank_outro_ms: (media_end - end).max(0),
            lyrics_past_eof_ms: if media_end > 0 {
                (end - media_end).max(0)
            } else {
                0
            },
            virtual_end_ms: media_end.max(end).max(0),
        }
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
    pub fn timeline_ms(&self, elapsed_ms: i64) -> i64 {
        elapsed_ms.saturating_sub(self.offset_ms)
    }
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
    pub fn line_position_ms(&self, index: usize, user_offset_ms: i64) -> Option<i64> {
        self.lines.get(index).map(|line| {
            line.at_ms
                .saturating_add(self.offset_ms)
                .saturating_add(user_offset_ms)
        })
    }
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelinePadding {
    pub blank_intro_ms: i64,
    pub blank_outro_ms: i64,
    pub lyrics_past_eof_ms: i64,
    pub virtual_end_ms: i64,
}
/// Enhanced LRC: parse inline <mm:ss.xx> segments while keeping a clean
/// text fallback. Malformed angle brackets remain literal text.
fn parse_enhanced(input: &str, initial_ms: i64) -> (String, Vec<Word>) {
    let mut cursor = input;
    let mut text = String::new();
    let mut words = Vec::new();
    let mut at_ms = initial_ms;
    let mut seen_tag = false;
    loop {
        let Some(open) = cursor.find('<') else {
            if !cursor.is_empty() {
                text.push_str(cursor);
                words.push(Word {
                    at_ms,
                    text: cursor.to_string(),
                });
            }
            break;
        };
        let before = &cursor[..open];
        if !before.is_empty() {
            text.push_str(before);
            words.push(Word {
                at_ms,
                text: before.to_string(),
            });
        }
        let after = &cursor[open + 1..];
        if let Some(close) = after.find('>')
            && let Some(time) = parse_stamp(&after[..close])
        {
            seen_tag = true;
            at_ms = time;
            cursor = &after[close + 1..];
            continue;
        }
        // It wasn't a timestamp. Preserve the character without losing text.
        text.push('<');
        words.push(Word {
            at_ms,
            text: "<".into(),
        });
        cursor = after;
    }
    if !seen_tag {
        (text, vec![])
    } else {
        (text, words)
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
    fn enhanced_lrc_cleans_tags_and_preserves_word_timing() {
        let l = Lyrics::parse("[00:01.00]<00:01.00>Good <00:01.35>morning");
        assert_eq!(l.lines()[0].text, "Good morning");
        assert_eq!(l.lines()[0].words.len(), 2);
        assert_eq!(l.lines()[0].words[0].at_ms, 1_000);
        assert_eq!(l.lines()[0].words[1].at_ms, 1_350);
        let plain = Lyrics::parse("[00:02.00]Hello world");
        assert!(plain.lines()[0].words.is_empty());
    }
    #[test]
    fn seeks_respect_document_and_manual_offsets() {
        let l = Lyrics::parse("[offset:100]\n[00:01.00] First");
        assert_eq!(l.line_position_ms(0, 250), Some(1_350));
        assert_eq!(l.line_position_ms(3, 0), None);
        assert_eq!(l.active_index(1350), Some(0));
    }
    #[test]
    fn virtual_padding_adds_silence_in_ui_not_in_actual_media() {
        let mut lyric = Lyrics::parse("[00:20.00]Vocal starts\n[03:30.00]Last line");
        lyric.pad_blank_to(237_000);
        assert_eq!(lyric.lines().last().unwrap().text, "");
        let layout = lyric.padding(Some(310_000), 20_000);
        assert_eq!(layout.blank_intro_ms, 40_000);
        assert_eq!(layout.blank_outro_ms, 53_000);
        assert_eq!(layout.lyrics_past_eof_ms, 0);
        assert_eq!(layout.virtual_end_ms, 310_000);
        let too_short = lyric.padding(Some(220_000), 20_000);
        assert_eq!(too_short.lyrics_past_eof_ms, 37_000);
        assert_eq!(too_short.virtual_end_ms, 257_000);
        assert_eq!(lyric.active_index(236_999), Some(1));
        assert_eq!(lyric.active_index(237_000), Some(2));
    }
    #[test]
    fn empty_and_invalid_are_safe() {
        let l = Lyrics::parse("[meta:abc]\n[01:99.0]bad\n[00:23.0000]bad\n");
        assert!(l.is_empty());
        assert_eq!(l.active_index(0), None);
        assert_eq!(l.window(0, 3, 3).1.len(), 0);
    }
}
