//! Shared lyrics acquisition: local .lrc > offline cache > user-initiated LRCLIB.
//! No lyrics are bundled with Windmill. Never execute media metadata as shell code.
use crate::lyrics::Lyrics;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub source: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: Option<i64>,
    pub sidecar: Option<PathBuf>,
}
impl Request {
    pub fn cache_key(&self) -> String {
        let title = normalize(&clean_title(&self.title, &self.artist));
        let artist = normalize(&self.artist);
        let duration_bucket = self
            .duration_ms
            .filter(|d| *d > 0)
            .map(|d| (d / 5000).to_string())
            .unwrap_or_default();
        let input = format!("{title}\0{artist}\0{duration_bucket}");
        format!("{:x}", Sha256::digest(input.as_bytes()))
    }
}
#[derive(Debug)]
pub enum Document {
    Synced(Lyrics),
    Plain(String),
}
#[derive(Debug)]
pub struct Loaded {
    pub document: Document,
    pub origin: String,
}
fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}
pub fn clean_title(raw: &str, artist: &str) -> String {
    let mut title = raw.trim().to_string();
    if let Some((prefix, rest)) = title.split_once(" - ")
        && normalize(prefix) == normalize(artist)
    {
        title = rest.trim().to_owned();
    }
    // YouTube commonly decorates music video titles. Only remove well-known
    // suffixes; never guess song titles from arbitrary URLs.
    const SUFFIXES: [&str; 10] = [
        " (official music video)",
        " (official video)",
        " [official video]",
        " (official audio)",
        " [official audio]",
        " (lyrics)",
        " [lyrics]",
        " (lyric video)",
        " (visualizer)",
        " (audio)",
    ];
    loop {
        let lower = title.to_lowercase();
        let Some(suffix) = SUFFIXES.iter().find(|ending| lower.ends_with(**ending)) else {
            break;
        };
        title.truncate(title.len() - suffix.len());
        title = title.trim_end().to_string();
    }
    title
}
fn sidecar_document(path: &Path) -> Option<Loaded> {
    let text = fs::read_to_string(path).ok()?;
    let lyrics = Lyrics::parse(&text);
    (!lyrics.is_empty()).then_some(Loaded {
        document: Document::Synced(lyrics),
        origin: "Local .lrc".into(),
    })
}
fn parse_item(item: &Value, origin: &str) -> Option<Loaded> {
    if item.get("instrumental").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if let Some(text) = item.get("syncedLyrics").and_then(Value::as_str) {
        let lrc = Lyrics::parse(text);
        if !lrc.is_empty() {
            return Some(Loaded {
                document: Document::Synced(lrc),
                origin: origin.into(),
            });
        }
    }
    item.get("plainLyrics")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Loaded {
            document: Document::Plain(s.to_owned()),
            origin: origin.into(),
        })
}
fn select_match<'a>(json: &'a Value, req: &Request) -> Option<&'a Value> {
    let title = normalize(&clean_title(&req.title, &req.artist));
    let artist = normalize(&req.artist);
    if title.is_empty() || artist.is_empty() {
        return None;
    }
    json.as_array()?
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let t = candidate.get("trackName").and_then(Value::as_str)?;
            let a = candidate.get("artistName").and_then(Value::as_str)?;
            if normalize(t) != title || normalize(a) != artist {
                return None;
            }
            let synced = candidate
                .get("syncedLyrics")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty());
            let plain = candidate
                .get("plainLyrics")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty());
            if !synced && !plain {
                return None;
            }
            let diff = req
                .duration_ms
                .zip(candidate.get("duration").and_then(Value::as_f64))
                .map(|(d, s)| ((d as f64 / 1000.0) - s).abs());
            // A known duration helps reject similarly named remixes/live versions.
            if diff.is_some_and(|d| d > 18.0) {
                return None;
            }
            let score =
                if synced { 10_000i64 } else { 0 } - diff.map(|d| (d * 100.0) as i64).unwrap_or(0);
            Some((score, index, candidate))
        })
        .max_by_key(|(score, index, _)| (*score, -(*index as i64)))
        .map(|(_, _, item)| item)
}
fn cache_path(data_dir: &Path, req: &Request) -> PathBuf {
    data_dir
        .join("lyrics")
        .join(format!("{}.json", req.cache_key()))
}
pub fn load(req: &Request, data_dir: &Path, online: bool, refresh: bool) -> Result<Option<Loaded>> {
    if let Some(file) = req.sidecar.as_ref()
        && let Some(document) = sidecar_document(file)
    {
        return Ok(Some(document));
    }
    let path = cache_path(data_dir, req);
    if !refresh
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(data) = serde_json::from_str::<Value>(&content)
        && let Some(document) = parse_item(&data, "Cached LRCLIB")
    {
        return Ok(Some(document));
    }
    if !online {
        return Ok(None);
    }
    let title = clean_title(&req.title, &req.artist);
    if title.is_empty() || req.artist.trim().is_empty() {
        return Ok(None);
    }
    if title.chars().count() > 180 || req.artist.chars().count() > 180 {
        return Err("track metadata too long for lyrics search".into());
    }
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "12",
            "--connect-timeout",
            "5",
            "--max-filesize",
            "524288",
            "--compressed",
            "--get",
            "--url",
            "https://lrclib.net/api/search",
            "--user-agent",
            "Windmill/0.3.0 (local-first personal media center)",
            "--data-urlencode",
            &format!("track_name={title}"),
            "--data-urlencode",
            &format!("artist_name={}", req.artist),
        ])
        .output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "LRCLIB lookup failed: {}",
            err.chars().take(180).collect::<String>()
        )
        .into());
    }
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let Some(selected) = select_match(&result, req) else {
        return Ok(None);
    };
    let Some(loaded) = parse_item(selected, "LRCLIB") else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(serde_json::to_string(selected)?.as_bytes())?;
        // The cache stays private to the current user.
        temp.as_file()
            .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        temp.persist(&path)?;
    }
    Ok(Some(loaded))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_only_known_decorations() {
        assert_eq!(
            clean_title("Gorillaz - Feel Good Inc. (Official Video)", "Gorillaz"),
            "Feel Good Inc."
        );
        assert_eq!(clean_title("Same - Name (lyrics)", "Other"), "Same - Name");
        assert_eq!(clean_title("Home (Live)", "Creator"), "Home (Live)");
    }
    #[test]
    fn correct_artist_and_duration_win() {
        let fixtures = serde_json::json!([
            {"trackName":"Blue Sky","artistName":"Wrong","duration":100,"syncedLyrics":"[00:01.00]wrong"},
            {"trackName":"Blue Sky","artistName":"Singer","duration":200,"syncedLyrics":"[00:01.00]wrong version"},
            {"trackName":"Blue Sky","artistName":"Singer","duration":100,"syncedLyrics":"[00:01.00]correct"}
        ]);
        let q = Request {
            source: "one".into(),
            title: "Singer - Blue Sky (Official Video)".into(),
            artist: "Singer".into(),
            duration_ms: Some(100_000),
            sidecar: None,
        };
        let match_found = select_match(&fixtures, &q).unwrap();
        let Document::Synced(lines) = parse_item(match_found, "Test").unwrap().document else {
            panic!()
        };
        assert_eq!(lines.lines()[0].text, "correct");
        assert_eq!(q.cache_key().len(), 64);
    }
    #[test]
    fn sidecar_precedes_cache_and_missing_is_offline_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let sidecar = tmp.path().join("file.lrc");
        fs::write(&sidecar, "[00:01.50]Example\\n").unwrap();
        let q = Request {
            source: "file.mp3".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            duration_ms: None,
            sidecar: Some(sidecar),
        };
        let found = load(&q, tmp.path(), false, false).unwrap().unwrap();
        assert_eq!(found.origin, "Local .lrc");
        let q = Request {
            source: "other.mp3".into(),
            sidecar: None,
            ..q
        };
        assert!(load(&q, tmp.path(), false, false).unwrap().is_none());
    }
    #[test]
    #[ignore = "performs a real HTTP request to LRCLIB"]
    fn real_provider_fetches_and_reuses_private_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let req = Request {
            source: "fixture:gorillaz".into(),
            artist: "Gorillaz".into(),
            title: "Gorillaz - Feel Good Inc. (Official Video)".into(),
            duration_ms: None,
            sidecar: None,
        };
        let remote = load(&req, tmp.path(), true, false).unwrap().unwrap();
        assert_eq!(remote.origin, "LRCLIB");
        assert!(matches!(remote.document, Document::Synced(_)));
        let cached = load(&req, tmp.path(), false, false).unwrap().unwrap();
        assert_eq!(cached.origin, "Cached LRCLIB");
    }
    #[test]
    fn plain_fallback_and_instrumental_are_safe() {
        let example = serde_json::json!({"plainLyrics":"These are example words"});
        assert!(matches!(
            parse_item(&example, "Test").unwrap().document,
            Document::Plain(_)
        ));
        let example = serde_json::json!({"instrumental":true,"syncedLyrics":"[00:01.00]foo"});
        assert!(parse_item(&example, "Test").is_none());
    }
}
