//! Public YouTube search via yt-dlp, with no account or cloud profile.
//! The search process is run off the TUI event loop.
use serde_json::Value;
use std::{error::Error, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub creator: String,
    pub url: String,
}

fn safe_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Reject external URLs from yt-dlp's JSON; reconstruct only canonical YouTube URLs.
pub fn parse_results(json: &str) -> Result<Vec<Video>> {
    let value: Value = serde_json::from_str(json)?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("no search entries")?;
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?;
            if !safe_id(id) {
                return None;
            }
            let title = entry
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string();
            let creator = entry
                .get("channel")
                .or_else(|| entry.get("uploader"))
                .and_then(Value::as_str)
                .unwrap_or("Unknown creator")
                .to_string();
            Some(Video {
                id: id.to_string(),
                title,
                creator,
                url: format!("https://www.youtube.com/watch?v={id}"),
            })
        })
        .collect())
}

pub fn search(query: &str) -> Result<Vec<Video>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }
    if query.chars().count() > 180 {
        return Err("search query too long".into());
    }
    let output = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--dump-single-json",
            "--no-warnings",
            "--socket-timeout",
            "8",
            "--retries",
            "1",
            "--playlist-end",
            "8",
        ])
        .arg(format!("ytsearch8:{query}"))
        .output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "yt-dlp search failed: {}",
            err.chars().take(280).collect::<String>()
        )
        .into());
    }
    parse_results(&String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_urls_and_metadata() {
        let json = r#"{"entries":[{"id":"HyHNuVaZJ-k","title":"Feel Good Inc.","channel":"Gorillaz","url":"https://evil.example/abc"},{"id":"bad/../../abc","title":"bad"}]}"#;
        let hits = parse_results(json).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://www.youtube.com/watch?v=HyHNuVaZJ-k");
        assert_eq!(hits[0].title, "Feel Good Inc.");
    }
    #[test]
    fn malformed_feed_is_rejected() {
        assert!(parse_results("{}").is_err());
    }
}
