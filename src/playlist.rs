// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mashrur Rahman Rawnok (NullifiedSec)
//! Public playlist metadata import, never implicit download or authentication.
//! Limits entries so a malicious/huge playlist cannot allocate indefinitely.
use crate::youtube::Video;
use serde_json::Value;
use std::{error::Error, process::Command};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const MAX_ENTRIES: usize = 250;
#[derive(Clone, Debug)]
pub struct Playlist {
    pub title: String,
    pub url: String,
    pub entries: Vec<Video>,
}
pub fn playlist_id(url: &str) -> Result<String> {
    let url = url.trim();
    let (host, query) = if let Some(s) = url
        .strip_prefix("https://www.youtube.com/")
        .or_else(|| url.strip_prefix("https://youtube.com/"))
        .or_else(|| url.strip_prefix("https://music.youtube.com/"))
    {
        s.split_once('?').ok_or("Missing playlist query")?
    } else {
        return Err("Only HTTPS YouTube or YouTube Music playlist URLs are supported".into());
    };
    if !(host == "playlist" || host == "watch") {
        return Err("Expected a playlist/watch URL".into());
    }
    let list = query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(k, v)| (k == "list").then_some(v))
        .ok_or("Missing list= playlist identifier")?;
    if list.len() < 10
        || list.len() > 96
        || !list
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Invalid playlist identifier".into());
    }
    Ok(list.into())
}
pub fn canonical_url(input: &str) -> Result<String> {
    Ok(format!(
        "https://www.youtube.com/playlist?list={}",
        playlist_id(input)?
    ))
}
pub fn parse_playlist(json: &str, url: &str) -> Result<Playlist> {
    let root: Value = serde_json::from_str(json)?;
    let entries = root
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("Missing playlist entries")?;
    let videos: Vec<Video> = entries
        .iter()
        .take(MAX_ENTRIES)
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            if id.len() != 11
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return None;
            }
            Some(Video {
                id: id.into(),
                title: item
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled")
                    .chars()
                    .take(200)
                    .collect(),
                creator: item
                    .get("channel")
                    .or_else(|| item.get("uploader"))
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown creator")
                    .chars()
                    .take(150)
                    .collect(),
                url: format!("https://www.youtube.com/watch?v={id}"),
                duration_seconds: item
                    .get("duration")
                    .and_then(Value::as_u64)
                    .filter(|n| *n > 0 && *n < 86400),
            })
        })
        .collect();
    if videos.is_empty() {
        return Err("No accessible public videos in this playlist".into());
    }
    let title = root
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Imported playlist")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    Ok(Playlist {
        title: if title.is_empty() {
            "Imported playlist".into()
        } else {
            title
        },
        url: url.into(),
        entries: videos,
    })
}
pub fn fetch(url: &str) -> Result<Playlist> {
    let url = canonical_url(url)?;
    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--dump-single-json",
            "--no-warnings",
            "--no-download",
            "--ignore-errors",
            "--socket-timeout",
            "10",
            "--retries",
            "1",
            "--playlist-end",
            "250",
            "--",
        ])
        .arg(&url)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Playlist lookup failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(250)
                .collect::<String>()
        )
        .into());
    }
    if output.stdout.len() > 4_000_000 {
        return Err("Playlist response too large".into());
    }
    parse_playlist(&String::from_utf8(output.stdout)?, &url)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_public_playlist_url_shapes() {
        let pid = "PLABCDEFGHI123456";
        assert_eq!(
            playlist_id(&format!("https://music.youtube.com/playlist?list={pid}")).unwrap(),
            pid
        );
        assert_eq!(
            playlist_id(&format!(
                "https://www.youtube.com/watch?v=HyHNuVaZJ-k&list={pid}"
            ))
            .unwrap(),
            pid
        );
        for url in [
            "https://evil.youtube.com/playlist?list=PLABCDEFGHI123456",
            "http://www.youtube.com/playlist?list=PLABCDEFGHI123456",
            "https://youtube.com/other?list=PLABCDEFGHI123456",
            "https://youtube.com/playlist?list=BAD!",
            "https://youtube.com/playlist?list=short",
        ] {
            assert!(canonical_url(url).is_err(), "{url}");
        }
    }
    #[test]
    fn safely_parses_and_ignores_unavailable_or_malicious_entries() {
        let payload = r#"{"title":"  Study Mix  ","entries":[
            {"id":"HyHNuVaZJ-k","title":"Counting Stars","channel":"OneRepublic","url":"https://evil.example"},
            null,{"id":"bad/../../xy","title":"bad"},{"title":"Private video"},
            {"id":"abcdefABC_1","title":"B","uploader":"Someone"}]}"#;
        let got = parse_playlist(payload, "test").unwrap();
        assert_eq!(got.title, "Study Mix");
        assert_eq!(got.entries.len(), 2);
        assert_eq!(
            got.entries[0].url,
            "https://www.youtube.com/watch?v=HyHNuVaZJ-k"
        );
        assert_eq!(got.entries[1].creator, "Someone");
    }
}
