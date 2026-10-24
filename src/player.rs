//! MPD control and mpv IPC, with playback state independent of the UI.
use crate::store::Media;
use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub state: String,
    pub elapsed_ms: i64,
    pub volume: Option<u8>,
    pub title: Option<String>,
}

pub fn mpd_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Strictly one response per MPD command. Never interpolate unescaped user input.
pub fn mpd_request(lines: &[&str]) -> Result<String> {
    let addr = env::var("WINDMILL_MPD_ADDR").unwrap_or_else(|_| "127.0.0.1:6600".into());
    let mut stream = TcpStream::connect(&addr)?;
    stream.set_read_timeout(Some(Duration::from_millis(700)))?;
    stream.set_write_timeout(Some(Duration::from_millis(700)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut greeting = String::new();
    reader.read_line(&mut greeting)?;
    if !greeting.starts_with("OK MPD ") {
        return Err("invalid MPD greeting".into());
    }
    let mut output = String::new();
    for line in lines {
        if line.contains(['\n', '\r']) {
            return Err("invalid MPD command".into());
        }
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        loop {
            let mut response = String::new();
            if reader.read_line(&mut response)? == 0 {
                return Err("MPD closed connection".into());
            }
            if response.starts_with("ACK ") {
                return Err(format!("MPD: {response}").into());
            }
            if response == "OK\n" {
                break;
            }
            output.push_str(&response);
        }
    }
    Ok(output)
}

pub fn mpd_status() -> Result<Status> {
    let raw = mpd_request(&["status", "currentsong"])?;
    Ok(parse_status(&raw))
}
fn parse_status(raw: &str) -> Status {
    let mut result = Status::default();
    for line in raw.lines() {
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };
        match key {
            "state" => result.state = value.to_string(),
            "elapsed" => {
                result.elapsed_ms = (value.parse::<f64>().unwrap_or(0.0) * 1000.0).max(0.0) as i64
            }
            "volume" => result.volume = value.parse().ok(),
            "Title" => result.title = Some(value.to_string()),
            "file" if result.title.is_none() => result.title = Some(value.to_string()),
            _ => {}
        }
    }
    result
}
pub fn mpd_toggle() -> Result<()> {
    let status = mpd_status()?;
    if status.state == "play" {
        mpd_request(&["pause 1"])?;
    } else if status.state == "pause" {
        mpd_request(&["pause 0"])?;
    } else {
        mpd_request(&["play"])?;
    }
    Ok(())
}
pub fn mpd_play(uri: &str) -> Result<()> {
    let quoted = mpd_quote(uri);
    mpd_request(&["clear", &format!("add {quoted}"), "play"])?;
    Ok(())
}

fn music_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("WINDMILL_MUSIC_ROOT") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from(env::var("HOME")?).join("Music"))
}
fn mpd_relative(path: &str) -> Result<String> {
    let root = music_dir()?.canonicalize()?;
    let file = Path::new(path).canonicalize()?;
    let relative = file
        .strip_prefix(&root)
        .map_err(|_| "file is outside the configured MPD music directory")?;
    if !file.is_file() {
        return Err("not a regular media file".into());
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Mpv,
    Mpd,
}

pub struct Playback {
    child: Option<Child>,
    socket_dir: Option<tempfile::TempDir>,
    pub active: Option<Media>,
    pub engine: Option<Engine>,
    pub paused: bool,
}
impl Playback {
    pub fn new() -> Self {
        Self {
            child: None,
            socket_dir: None,
            active: None,
            engine: None,
            paused: false,
        }
    }
    fn close_mpv(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.socket_dir = None;
    }
    pub fn play_mpv(&mut self, media: &Media) -> Result<()> {
        let _ = mpd_request(&["pause 1"]); // Never blast two players simultaneously.
        self.close_mpv();
        let dir = tempfile::tempdir()?;
        let socket = dir.path().join("mpv.sock");
        let mut cmd = Command::new("mpv");
        cmd.arg(format!("--input-ipc-server={}", socket.display()))
            .arg("--no-terminal")
            .arg("--")
            .arg(&media.source);
        let child = cmd.spawn()?;
        self.child = Some(child);
        self.socket_dir = Some(dir);
        self.active = Some(media.clone());
        self.engine = Some(Engine::Mpv);
        self.paused = false;
        Ok(())
    }
    pub fn play_mpd(&mut self, media: &Media) -> Result<()> {
        let uri = mpd_relative(&media.source)?;
        mpd_play(&uri)?;
        self.close_mpv();
        self.active = Some(media.clone());
        self.engine = Some(Engine::Mpd);
        self.paused = false;
        Ok(())
    }
    fn ipc(&self, command: Value) -> Result<Value> {
        let path = self
            .socket_dir
            .as_ref()
            .ok_or("mpv not running")?
            .path()
            .join("mpv.sock");
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_millis(250)))?;
        stream.set_write_timeout(Some(Duration::from_millis(250)))?;
        let mut payload = serde_json::to_vec(&json!({"command":command,"request_id":1}))?;
        payload.push(b'\n');
        stream.write_all(&payload)?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }
    pub fn toggle(&mut self) -> Result<()> {
        match self.engine {
            Some(Engine::Mpv) => {
                let value = self.ipc(json!(["get_property", "pause"]))?;
                let paused = value["data"].as_bool().unwrap_or(false);
                self.ipc(json!(["set_property", "pause", !paused]))?;
                self.paused = !paused;
            }
            _ => {
                mpd_toggle()?;
                self.paused = mpd_status()?.state == "pause";
            }
        }
        Ok(())
    }
    pub fn stop(&mut self) {
        if self.engine == Some(Engine::Mpd) {
            let _ = mpd_request(&["stop"]);
        }
        self.close_mpv();
        self.active = None;
        self.engine = None;
        self.paused = false;
    }
    pub fn elapsed_ms(&mut self) -> Option<i64> {
        match self.engine {
            Some(Engine::Mpv) => {
                if let Some(child) = self.child.as_mut()
                    && child.try_wait().ok().flatten().is_some()
                {
                    self.close_mpv();
                    self.engine = None;
                    self.active = None;
                    return None;
                }
                self.ipc(json!(["get_property", "time-pos"]))
                    .ok()
                    .and_then(|r| r["data"].as_f64())
                    .map(|s| (s * 1000.0) as i64)
            }
            Some(Engine::Mpd) => mpd_status().ok().map(|s| s.elapsed_ms),
            None => None,
        }
    }
    pub fn next(&mut self) -> Result<()> {
        match self.engine {
            Some(Engine::Mpv) => {
                self.ipc(json!(["playlist-next", "weak"]))?;
            }
            Some(Engine::Mpd) => {
                mpd_request(&["next"])?;
            }
            None => {}
        }
        Ok(())
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.close_mpv();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpd_escapes_untrusted_names() {
        assert_eq!(mpd_quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
    #[test]
    fn parsed_status_uses_milliseconds() {
        let s =
            parse_status("volume: 70\nstate: play\nelapsed: 12.750\nfile: a.mp3\nTitle: Song\n");
        assert_eq!(s.elapsed_ms, 12_750);
        assert_eq!(s.volume, Some(70));
        assert_eq!(s.title.as_deref(), Some("Song"));
    }
}
