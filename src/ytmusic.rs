//! Optional public YouTube Music lookup using isolated ytmusicapi Python venv.
use crate::youtube::Video;
use serde_json::Value;
use std::{env, error::Error, path::PathBuf, process::Command};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
#[derive(Clone, Debug)]
pub struct MusicItem {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub creator: String,
    pub url: String,
    pub song: Option<Video>,
}
fn valid(id: &str) -> bool {
    id.len() >= 10
        && id.len() <= 120
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn parse_items(json: &str) -> Result<Vec<MusicItem>> {
    let root: Value = serde_json::from_str(json)?;
    let entries = root
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("Missing Music results")?;
    Ok(entries
        .iter()
        .take(250)
        .filter_map(|r| {
            let kind = r.get("kind")?.as_str()?;
            if !["songs", "albums", "artists", "playlists"].contains(&kind) {
                return None;
            }
            let id = r.get("id")?.as_str()?;
            if !valid(id) {
                return None;
            }
            let title = r
                .get("title")?
                .as_str()?
                .chars()
                .take(200)
                .collect::<String>();
            let creator = r
                .get("creator")?
                .as_str()?
                .chars()
                .take(150)
                .collect::<String>();
            let url = match kind {
                "songs" if id.len() == 11 => format!("https://www.youtube.com/watch?v={id}"),
                "albums" | "artists" => format!("https://music.youtube.com/browse/{id}"),
                "playlists" => format!(
                    "https://music.youtube.com/playlist?list={}",
                    id.strip_prefix("VL").unwrap_or(id)
                ),
                _ => return None,
            };
            let song = (kind == "songs").then(|| Video {
                id: id.into(),
                title: title.clone(),
                creator: creator.clone(),
                url: url.clone(),
            });
            Some(MusicItem {
                kind: kind.into(),
                id: id.into(),
                title,
                creator,
                url,
                song,
            })
        })
        .collect())
}
fn python_program() -> PathBuf {
    if let Ok(path) = env::var("WINDMILL_YTMUSIC_PYTHON") {
        return path.into();
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tools/ytmusic/bin/python");
    if bundled.is_file() {
        bundled
    } else {
        "python3".into()
    }
}
fn exec(operation: &str, kind: &str, param: &str) -> Result<String> {
    if param.trim().is_empty() || param.len() > 200 {
        return Err("Music query invalid or too long".into());
    }
    let output = Command::new(python_program())
        .arg("-c")
        .arg(include_str!("../scripts/ytmusic_bridge.py"))
        .args([operation, kind, param])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "YouTube Music lookup: {}. Run scripts/setup-ytmusic if the library is missing.",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(350)
                .collect::<String>()
        )
        .into());
    }
    if output.stdout.len() > 1_000_000 {
        return Err("Music response too large".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
pub fn search(kind: &str, query: &str) -> Result<Vec<MusicItem>> {
    if !["songs", "albums", "artists", "playlists"].contains(&kind) {
        return Err("Invalid search filter".into());
    }
    parse_items(&exec("search", kind, query)?)
}
pub fn browse(kind: &str, id: &str) -> Result<(String, Vec<Video>)> {
    if !["albums", "artists"].contains(&kind) || !valid(id) {
        return Err("Invalid public Music browse identifier".into());
    }
    let result = exec("browse", kind, id)?;
    let root: Value = serde_json::from_str(&result)?;
    let title = root
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Music selection")
        .to_string();
    let videos = parse_items(&result)?
        .into_iter()
        .filter_map(|item| item.song)
        .collect();
    Ok((title, videos))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_public_metadata_without_trusting_provider_urls() {
        let json = r#"{"entries":[
            {"kind":"songs","id":"HyHNuVaZJ-k","title":"Feel Good","creator":"Gorillaz","url":"https://evil.test"},
            {"kind":"albums","id":"MPREb_album_123","title":"Album","creator":"Creator","url":"https://malicious.example"},
            {"kind":"songs","id":"../../../bad","title":"No","creator":"No"}
        ]}"#;
        let entries = parse_items(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].url,
            "https://www.youtube.com/watch?v=HyHNuVaZJ-k"
        );
        assert_eq!(
            entries[1].url,
            "https://music.youtube.com/browse/MPREb_album_123"
        );
    }
}
