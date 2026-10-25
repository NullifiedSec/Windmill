//! Offline album art rendering as true-color half-block terminal cells.
//! Only canonical validated YouTube video IDs can trigger thumbnail fetches.
use image::{DynamicImage, imageops::FilterType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub fn video_id(source: &str) -> Option<&str> {
    let query = source
        .strip_prefix("https://www.youtube.com/watch?")
        .or_else(|| source.strip_prefix("https://youtube.com/watch?"))
        .or_else(|| source.strip_prefix("https://music.youtube.com/watch?"))?;
    let id = query
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find_map(|(name, value)| (name == "v").then_some(value))?;
    if id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Some(id)
    } else {
        None
    }
}
pub fn render(img: &DynamicImage, columns: u32, rows: u32) -> Vec<Line<'static>> {
    let columns = columns.clamp(2, 60);
    let rows = rows.clamp(1, 24);
    let scaled = img
        .resize_exact(columns, rows * 2, FilterType::Triangle)
        .to_rgb8();
    (0..rows)
        .map(|row| {
            let spans = (0..columns)
                .map(|col| {
                    let top = scaled.get_pixel(col, row * 2).0;
                    let bottom = scaled.get_pixel(col, row * 2 + 1).0;
                    Span::styled(
                        "▀",
                        Style::default()
                            .fg(Color::Rgb(top[0], top[1], top[2]))
                            .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                    )
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}
fn art_path(data: &Path, source: &str) -> PathBuf {
    let key = hex_sha(source);
    data.join("art").join(format!("{key}.jpg"))
}
fn hex_sha(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}
/// Best-effort fetch. No album artwork is downloaded when merely browsing.
pub fn get(source: &str, data: &Path) -> Result<Option<Vec<Line<'static>>>> {
    let path = art_path(data, source);
    if !path.is_file() {
        fs::create_dir_all(data.join("art"))?;
        if let Some(id) = video_id(source) {
            let url = format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg");
            let mut temp = tempfile::NamedTempFile::new_in(data.join("art"))?;
            let response = Command::new("curl")
                .args([
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--max-time",
                    "12",
                    "--connect-timeout",
                    "5",
                    "--max-filesize",
                    "4194304",
                    "--url",
                    &url,
                    "--output",
                ])
                .arg(temp.path())
                .output()?;
            if !response.status.success() {
                return Ok(None);
            }
            let bytes = fs::read(temp.path())?;
            let _ = image::load_from_memory(&bytes)?;
            temp.as_file_mut().flush()?;
            temp.persist(&path)?;
        } else {
            // Local audio's embedded cover, or a local video's first frame.
            let original = Path::new(source);
            if !original.is_file() {
                return Ok(None);
            }
            let mut output = Command::new("ffmpeg");
            output
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
                .arg(original)
                .args(["-map", "0:v:0", "-frames:v", "1", "-y"])
                .arg(&path);
            let result = output.output()?;
            if !result.status.success() || !path.is_file() {
                return Ok(None);
            }
        }
    }
    let bytes = fs::read(path)?;
    if bytes.len() > 4_194_304 {
        return Ok(None);
    }
    Ok(Some(render(&image::load_from_memory(&bytes)?, 24, 8)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requests one real public YouTube thumbnail"]
    fn downloads_and_caches_real_cover_art() {
        let temp = tempfile::tempdir().unwrap();
        let source = "https://www.youtube.com/watch?v=HyHNuVaZJ-k";
        let first = get(source, temp.path()).unwrap().unwrap();
        assert_eq!(first.len(), 8);
        assert!(art_path(temp.path(), source).is_file());
        let from_cache = get(source, temp.path()).unwrap().unwrap();
        assert_eq!(from_cache.len(), first.len());
    }
    #[test]
    fn reject_untrusted_sources() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=HyHNuVaZJ-k"),
            Some("HyHNuVaZJ-k")
        );
        for url in [
            "https://evil.youtube.com/watch?v=HyHNuVaZJ-k",
            "http://www.youtube.com/watch?v=HyHNuVaZJ-k",
            "https://www.youtube.com/watch?v=../../../../",
            "https://www.youtube.com/playlist?list=HyHNuVaZJ-k",
        ] {
            assert_eq!(video_id(url), None)
        }
    }
    #[test]
    fn half_block_renders_expected_number_of_cells() {
        let img = DynamicImage::new_rgb8(10, 20);
        let lines = render(&img, 28, 8);
        assert_eq!(lines.len(), 8);
        assert!(lines.iter().all(|l| l.spans.len() == 28));
    }
}
