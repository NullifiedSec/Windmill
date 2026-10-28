// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mashrur Rahman Rawnok (NullifiedSec)
//! User-initiated yt-dlp downloads. Side effects run on a worker thread.
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Audio,
    Video,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }
}

/// Download only on explicit user action; never run arbitrary titles through a shell.
/// Returns the actual local path from yt-dlp's after_move hook.
pub fn fetch(id: i64, url: &str, kind: Kind, data_dir: &Path) -> Result<PathBuf> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("download requires a web media source".into());
    }
    let folder = data_dir.join("downloads");
    fs::create_dir_all(&folder)?;
    let template = folder.join(format!("{id}-%(title).150B-%(id)s.%(ext)s"));
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-playlist")
        .arg("--no-overwrites")
        .arg("--quiet")
        .arg("--no-warnings")
        .arg("--socket-timeout")
        .arg("10")
        .arg("--retries")
        .arg("2")
        .arg("--print")
        .arg("after_move:filepath")
        .arg("-o")
        .arg(&template);
    if let Kind::Audio = kind {
        cmd.args(["-x", "--audio-format", "best"]);
    }
    let result = cmd.arg(url).output()?;
    if !result.status.success() {
        let err = String::from_utf8_lossy(&result.stderr);
        return Err(format!(
            "download failed: {}",
            err.chars().take(500).collect::<String>()
        )
        .into());
    }
    let out = String::from_utf8(result.stdout)?;
    let file = out
        .lines()
        .rev()
        .find(|s| !s.is_empty())
        .ok_or("yt-dlp reported no output file")?;
    let path = PathBuf::from(file).canonicalize()?;
    if !path.starts_with(folder.canonicalize()?) || !path.is_file() {
        return Err("yt-dlp returned unexpected output path".into());
    }
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds_are_stable_database_labels() {
        assert_eq!(Kind::Audio.as_str(), "audio");
        assert_eq!(Kind::Video.as_str(), "video");
    }
}
