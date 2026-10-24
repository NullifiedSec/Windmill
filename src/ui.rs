//! Interactive Windmill terminal front end. No application-specific background daemon.
use crate::{
    download::{self, Kind},
    lyric_offset::{MAX_ABS_OFFSET_MS, format_offset_ms, offset_from_cue, parse_offset_ms},
    lyric_source::{self, Document, Loaded, Request},
    mpris::{self, Control, Snapshot},
    player::{self, Engine, Playback, Status},
    seek::{self, clamp_target, format_time, parse_time_ms},
    store::{Media, Store},
    youtube::{self, Video},
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use std::{
    error::Error,
    io::{self, IsTerminal},
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

const TITLES: [&str; 5] = [
    "Library",
    "Discover",
    "Collections",
    "For You",
    "Now Playing",
];
const ACCENT: Color = Color::Rgb(186, 216, 165);
const MUTED: Color = Color::Rgb(150, 156, 171);
const HIGHLIGHT: Color = Color::Rgb(246, 210, 143);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Library,
    Discover,
    Collections,
    ForYou,
    Playing,
}
impl Tab {
    fn index(self) -> usize {
        match self {
            Self::Library => 0,
            Self::Discover => 1,
            Self::Collections => 2,
            Self::ForYou => 3,
            Self::Playing => 4,
        }
    }
    fn from_index(i: usize) -> Self {
        match i % 5 {
            0 => Self::Library,
            1 => Self::Discover,
            2 => Self::Collections,
            3 => Self::ForYou,
            _ => Self::Playing,
        }
    }
}
#[derive(Debug)]
enum Prompt {
    Search(String),
    Tag(String),
    Collection(String),
    Filter(String),
    LyricOffset(String),
    JumpTime(String),
}
impl Prompt {
    fn text(&self) -> &str {
        match self {
            Self::Search(t)
            | Self::Tag(t)
            | Self::Collection(t)
            | Self::Filter(t)
            | Self::LyricOffset(t)
            | Self::JumpTime(t) => t,
        }
    }
    fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Search(t)
            | Self::Tag(t)
            | Self::Collection(t)
            | Self::Filter(t)
            | Self::LyricOffset(t)
            | Self::JumpTime(t) => t,
        }
    }
    fn label(&self) -> &str {
        match self {
            Self::Search(_) => "YouTube search",
            Self::Tag(_) => "Add tag",
            Self::Collection(_) => "Create & add collection",
            Self::Filter(_) => "Recommendation tags, comma-separated",
            Self::LyricOffset(_) => "Lyrics offset (+seconds or -mm:ss.sss) — Enter saves",
            Self::JumpTime(_) => "Jump to playback time (seconds, m:ss, h:mm:ss)",
        }
    }
}
struct LyricResponse {
    key: String,
    result: std::result::Result<Option<Loaded>, String>,
}
struct DownloadResponse {
    media_id: i64,
    kind: Kind,
    result: std::result::Result<std::path::PathBuf, String>,
}
struct SearchResponse {
    query: String,
    result: std::result::Result<Vec<Video>, String>,
}
pub struct App {
    store: Store,
    tab: Tab,
    selected: [usize; 4],
    media: Vec<Media>,
    collections: Vec<String>,
    recommendations: Vec<(i64, Media)>,
    filter_tags: Vec<String>,
    found: Vec<Video>,
    search: String,
    pending: bool,
    prompt: Option<Prompt>,
    status: String,
    player: Playback,
    mpd: Option<Status>,
    elapsed_ms: Option<i64>,
    lyrics: Option<Document>,
    lyric_origin: String,
    lyric_status: String,
    lyric_key: Option<String>,
    lyric_source: String,
    lyric_offset_ms: i64,
    lyric_cursor: usize,
    lyric_follow: bool,
    tap_sync: bool,
    sync_undo: Option<(String, i64)>,
    seek_undo: Option<(String, i64)>,
    lyric_pending: bool,
    lyric_tx: Sender<LyricResponse>,
    lyric_rx: Receiver<LyricResponse>,
    duration_ms: Option<i64>,
    rx: Receiver<SearchResponse>,
    tx: Sender<SearchResponse>,
    last_tick: Instant,
    download_tx: Sender<DownloadResponse>,
    download_rx: Receiver<DownloadResponse>,
}
impl App {
    fn new(store: Store) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (download_tx, download_rx) = mpsc::channel();
        let (lyric_tx, lyric_rx) = mpsc::channel();
        let media = store.all()?;
        let collections = store.collections()?;
        let tags = store.top_tags(6)?;
        let recommendations = store.recommend(tags.iter().map(String::as_str))?;
        Ok(Self {
            store,
            tab: Tab::Library,
            selected: [0; 4],
            media,
            collections,
            recommendations,
            filter_tags: vec![],
            found: vec![],
            search: String::new(),
            pending: false,
            prompt: None,
            status: "Welcome to Windmill · local-first media center".into(),
            player: Playback::new(),
            mpd: None,
            elapsed_ms: None,
            lyrics: None,
            lyric_origin: String::new(),
            lyric_status: "Select a song to see its lyrics".into(),
            lyric_key: None,
            lyric_source: String::new(),
            lyric_offset_ms: 0,
            lyric_cursor: 0,
            lyric_follow: true,
            tap_sync: false,
            sync_undo: None,
            seek_undo: None,
            lyric_pending: false,
            lyric_tx,
            lyric_rx,
            duration_ms: None,
            tx,
            rx,
            download_tx,
            download_rx,
            last_tick: Instant::now() - Duration::from_secs(2),
        })
    }
    fn rebuild_recommendations(&mut self) -> Result<()> {
        let tags = if self.filter_tags.is_empty() {
            self.store.top_tags(6)?
        } else {
            self.filter_tags.clone()
        };
        self.recommendations = self.store.recommend(tags.iter().map(String::as_str))?;
        self.selected[3] = self.selected[3].min(self.recommendations.len().saturating_sub(1));
        Ok(())
    }
    fn reload(&mut self) -> Result<()> {
        self.media = self.store.all()?;
        self.collections = self.store.collections()?;
        self.rebuild_recommendations()?;
        for (i, len) in [
            self.media.len(),
            self.found.len(),
            self.collections.len(),
            self.recommendations.len(),
        ]
        .into_iter()
        .enumerate()
        {
            self.selected[i] = self.selected[i].min(len.saturating_sub(1));
        }
        Ok(())
    }
    fn selected_media(&self) -> Option<&Media> {
        self.media.get(self.selected[0])
    }
    fn selected_video(&self) -> Option<&Video> {
        self.found.get(self.selected[1])
    }
    fn message_error(&mut self, err: impl std::fmt::Display) {
        self.status = format!("Error: {err}");
    }
    fn choose(&mut self, delta: isize) {
        let (idx, len) = match self.tab {
            Tab::Library => (0, self.media.len()),
            Tab::Discover => (1, self.found.len()),
            Tab::Collections => (2, self.collections.len()),
            Tab::ForYou => (3, self.recommendations.len()),
            Tab::Playing => return,
        };
        if len == 0 {
            self.selected[idx] = 0;
            return;
        }
        self.selected[idx] =
            (self.selected[idx] as isize + delta).clamp(0, (len - 1) as isize) as usize;
    }
    fn active_media(&self) -> Option<Media> {
        match self.tab {
            Tab::Library => self.selected_media().cloned(),
            Tab::Discover => self.selected_video().map(|v| Media {
                id: 0,
                source: v.url.clone(),
                title: v.title.clone(),
                creator: v.creator.clone(),
            }),
            Tab::ForYou => self
                .recommendations
                .get(self.selected[3])
                .map(|(_, m)| m.clone()),
            _ => None,
        }
    }
    fn current_lyric_request(&self) -> Option<Request> {
        if self.player.engine == Some(Engine::Mpv) {
            let media = self.player.active.as_ref()?;
            // MPV can play a downloaded copy, but it is the SAME catalog video.
            // Keep offset and lyrics cache under its original URL/identity.
            let original = if media.id > 0 {
                self.store.get(media.id).ok().map(|m| m.source)
            } else {
                None
            };
            let source = original.unwrap_or_else(|| media.source.clone());
            let file = Path::new(&media.source);
            let ext = file
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let video = source.starts_with("https://")
                || source.starts_with("http://")
                || ["mp4", "mkv", "webm", "mov", "avi", "m4v"]
                    .contains(&ext.to_ascii_lowercase().as_str());
            return Some(Request {
                source,
                title: media.title.clone(),
                artist: media.creator.clone(),
                // A music-video intro/outro makes VIDEO duration longer than
                // the song's LRCLIB duration. Don't reject a correct match.
                duration_ms: if video { None } else { self.duration_ms },
                sidecar: file.is_file().then(|| file.with_extension("lrc")),
            });
        }
        let mpd = self.mpd.as_ref()?;
        let file = mpd.file.as_ref()?;
        let origin = format!("mpd:{file}");
        let mut request = Request {
            source: origin,
            title: mpd.title.clone().unwrap_or_default(),
            artist: mpd.artist.clone().unwrap_or_default(),
            duration_ms: mpd.duration_ms,
            sidecar: player::music_dir()
                .ok()
                .map(|root| root.join(file).with_extension("lrc")),
        };
        // If this track was selected inside Windmill, keep its user-maintained
        // artist/title metadata rather than requiring tags embedded in audio files.
        if let Some(media) = self.player.active.as_ref()
            && player::mpd_relative(&media.source).ok().as_deref() == Some(file)
        {
            request.title = media.title.clone();
            request.artist = media.creator.clone();
        }
        Some(request)
    }
    fn load_lyrics(&mut self, refresh: bool) {
        let Some(request) = self.current_lyric_request() else {
            self.lyrics = None;
            self.lyric_key = None;
            self.tap_sync = false;
            self.lyric_status = "Nothing playing".into();
            self.lyric_pending = false;
            return;
        };
        let key = format!(
            "{}\0{}\0{}\0{:?}",
            request.source,
            request.title,
            request.artist,
            request.duration_ms.map(|ms| ms / 5000)
        );
        if !refresh && self.lyric_key.as_deref() == Some(&key) {
            return;
        }
        let same_track = self.lyric_key.as_deref() == Some(&key);
        self.lyric_key = Some(key.clone());
        self.lyric_source = request.source.clone();
        // A manual refresh shouldn't blank an existing lyric screen if the
        // upstream service has temporarily become unreachable.
        if !same_track {
            self.tap_sync = false;
            self.lyrics = None;
            self.lyric_cursor = 0;
            self.lyric_follow = true;
        }
        self.lyric_pending = true;
        self.lyric_status = "Finding synchronized lyrics...".into();
        self.lyric_origin.clear();
        let legacy = self.player.active.as_ref().map(|m| m.source.as_str());
        self.lyric_offset_ms = self
            .store
            .lyrics_offset_with_legacy(&request.source, legacy)
            .unwrap_or(0);
        let data_dir = match crate::data_dir() {
            Ok(dir) => dir,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        let tx = self.lyric_tx.clone();
        std::thread::spawn(move || {
            let result =
                lyric_source::load(&request, &data_dir, true, refresh).map_err(|e| e.to_string());
            let _ = tx.send(LyricResponse { key, result });
        });
    }
    fn set_lyric_offset(&mut self, offset_ms: i64) {
        if self.lyric_key.is_none() {
            self.status = "Play a song before adjusting its lyrics".into();
            return;
        }
        let offset_ms = offset_ms.clamp(-MAX_ABS_OFFSET_MS, MAX_ABS_OFFSET_MS);
        match self.store.set_lyrics_offset(&self.lyric_source, offset_ms) {
            Ok(()) => {
                self.lyric_offset_ms = offset_ms;
                self.status = format!(
                    "Lyrics offset {} (saved for this recording)",
                    format_offset_ms(offset_ms)
                );
            }
            Err(e) => self.message_error(e),
        }
    }
    fn nudge_lyrics(&mut self, delta_ms: i64) {
        self.set_lyric_offset(self.lyric_offset_ms.saturating_add(delta_ms));
    }
    /// Begin guided sync, picking the first meaningful line as the default
    /// cue. Nothing is persisted until the listener taps at the heard moment.
    fn toggle_tap_sync(&mut self) {
        if self.tap_sync {
            self.tap_sync = false;
            self.lyric_follow = true;
            self.status = "Tap Sync cancelled. Playback unchanged.".into();
            return;
        }
        let Some(Document::Synced(lyrics)) = self.lyrics.as_ref() else {
            self.status =
                "Tap Sync needs timestamped lyrics. Play a track with synced lyrics first.".into();
            return;
        };
        let Some((first, _)) = lyrics
            .lines()
            .iter()
            .enumerate()
            .find(|(_, line)| !line.text.trim().is_empty())
        else {
            self.status = "No lyric lines available to mark".into();
            return;
        };
        self.lyric_cursor = first;
        self.lyric_follow = false;
        self.tap_sync = true;
        self.status="TAP SYNC: choose the line with j/k; press SPACE exactly when you hear it begin. Esc cancels.".into();
    }
    /// Real-time handler obtains a fresh playback position rather than using
    /// the previous UI refresh (which may be up to 350ms stale).
    fn tap_lyric_now(&mut self) {
        let position = if self.player.engine.is_some() {
            self.player.elapsed_ms()
        } else {
            player::mpd_status()
                .ok()
                .filter(|s| s.state == "play")
                .map(|s| s.elapsed_ms)
        };
        if self.player.paused
            || self
                .mpd
                .as_ref()
                .is_some_and(|s| self.player.engine != Some(Engine::Mpv) && s.state == "pause")
        {
            self.status = "Resume playback before tapping a lyric cue".into();
            return;
        }
        let Some(position) = position else {
            self.status = "Cannot read live player position. Start playback and try again.".into();
            return;
        };
        self.apply_lyric_cue(position);
    }
    fn apply_lyric_cue(&mut self, player_position_ms: i64) {
        if !self.tap_sync {
            return;
        }
        let Some(Document::Synced(lyrics)) = self.lyrics.as_ref() else {
            return;
        };
        let Some(cue_ms) = lyrics.line_position_ms(self.lyric_cursor, 0) else {
            self.status = "Invalid lyric cue; choose another line".into();
            return;
        };
        match offset_from_cue(player_position_ms, cue_ms) {
            Ok(offset) => {
                let previous = self.lyric_offset_ms;
                match self.store.set_lyrics_offset(&self.lyric_source, offset) {
                    Ok(()) => {
                        self.sync_undo = Some((self.lyric_source.clone(), previous));
                        self.lyric_offset_ms = offset;
                        self.tap_sync = false;
                        self.lyric_follow = true;
                        self.elapsed_ms = Some(player_position_ms);
                        self.status = format!(
                            "Synced at {}! Offset {} saved. Press u to undo, [ / ] to fine-tune.",
                            format_offset_ms(player_position_ms),
                            format_offset_ms(offset)
                        );
                    }
                    Err(e) => self.message_error(e),
                }
            }
            Err(e) => self.status = format!("Unable to sync this line: {e}"),
        }
    }
    fn undo_tap_sync(&mut self) {
        let Some((source, previous)) = self.sync_undo.as_ref() else {
            self.status = "No Tap Sync change to undo".into();
            return;
        };
        if *source != self.lyric_source {
            self.status = "Tap Sync undo belongs to a different recording".into();
            return;
        }
        match self.store.set_lyrics_offset(source, *previous) {
            Ok(()) => {
                self.lyric_offset_ms = *previous;
                self.sync_undo = None;
                self.status = format!(
                    "Tap Sync undone; previous offset {} restored",
                    format_offset_ms(self.lyric_offset_ms)
                );
            }
            Err(e) => self.message_error(e),
        }
    }
    fn move_lyric_cursor(&mut self, delta: isize) {
        let size = match self.lyrics.as_ref() {
            Some(Document::Synced(lrc)) => lrc.lines().len(),
            Some(Document::Plain(text)) => text.lines().count(),
            None => 0,
        };
        if size == 0 {
            return;
        }
        let active = self
            .lyrics
            .as_ref()
            .and_then(|doc| match doc {
                Document::Synced(lrc) => {
                    lrc.active_index(self.elapsed_ms.unwrap_or(0) - self.lyric_offset_ms)
                }
                Document::Plain(_) => None,
            })
            .unwrap_or(0);
        let origin = if self.lyric_follow {
            active
        } else {
            self.lyric_cursor
        };
        self.lyric_cursor =
            (origin as isize + delta).clamp(0, size.saturating_sub(1) as isize) as usize;
        self.lyric_follow = false;
    }
    fn seek_to_lyric(&mut self) {
        if let Some(Document::Synced(lrc)) = self.lyrics.as_ref()
            && let Some(position_ms) = lrc.line_position_ms(self.lyric_cursor, self.lyric_offset_ms)
        {
            let position_us = position_ms.max(0).saturating_mul(1000);
            match self.player.seek_absolute(position_us) {
                Ok(()) => {
                    self.elapsed_ms = Some(position_ms.max(0));
                    self.lyric_follow = true;
                    self.status = "Jumped to selected lyric".into();
                }
                Err(e) => self.message_error(e),
            }
        }
    }
    fn playback_clock(&mut self) -> Option<i64> {
        if self.player.engine.is_some() {
            self.player.elapsed_ms()
        } else {
            player::mpd_status()
                .ok()
                .filter(|s| s.file.is_some())
                .map(|s| s.elapsed_ms)
        }
    }
    fn seek_to(&mut self, target_ms: i64) {
        let snap = self.mpris_snapshot();
        if !snap.can_seek || snap.source.is_empty() {
            self.status = "Start a playable track before seeking".into();
            return;
        }
        let Some(previous) = self.playback_clock() else {
            self.status =
                "Playback position unavailable; try again after starting the player".into();
            return;
        };
        let duration = self.player.duration_ms().or(self.duration_ms);
        let target = clamp_target(target_ms, 0, duration);
        if (target - previous).abs() < 100 {
            return;
        }
        match self.player.seek_absolute(target.saturating_mul(1000)) {
            Ok(()) => {
                self.seek_undo = Some((snap.source, previous));
                self.elapsed_ms = Some(target);
                if !self.tap_sync {
                    self.lyric_follow = true
                }
                self.status = format!(
                    "Jumped to {} (from {}) · z to undo",
                    format_time(target),
                    format_time(previous)
                );
                self.last_tick = Instant::now() - Duration::from_secs(2);
            }
            Err(e) => self.message_error(e),
        }
    }
    fn skip_by(&mut self, delta_ms: i64) {
        let Some(now) = self.playback_clock() else {
            self.status = "Start playback before skipping".into();
            return;
        };
        let duration = self.player.duration_ms().or(self.duration_ms);
        self.seek_to(clamp_target(now, delta_ms, duration));
    }
    fn undo_seek(&mut self) {
        let Some((source, previous)) = self.seek_undo.take() else {
            self.status = "No previous skip to undo".into();
            return;
        };
        let snap = self.mpris_snapshot();
        if source != snap.source || !snap.can_seek {
            self.status = "Can't undo a skip from another track".into();
            return;
        }
        match self.player.seek_absolute(previous.saturating_mul(1000)) {
            Ok(()) => {
                self.elapsed_ms = Some(previous);
                self.lyric_follow = true;
                self.status = format!("Skip undone · back at {}", format_time(previous));
                self.last_tick = Instant::now() - Duration::from_secs(2);
            }
            Err(e) => {
                self.seek_undo = Some((source, previous));
                self.message_error(e)
            }
        }
    }
    fn mouse(&mut self, event: MouseEvent, area: Rect) {
        if self.tab != Tab::Playing || self.prompt.is_some() {
            return;
        }
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let progress = progress_area(area);
        if event.row < progress.y || event.row >= progress.y.saturating_add(progress.height) {
            return;
        }
        let duration = self.player.duration_ms().or(self.duration_ms).unwrap_or(0);
        match seek::progress_target(event.column, progress.x, progress.width, duration) {
            Some(target) => self.seek_to(target),
            None => self.status = "Click the progress bar when the track duration is known".into(),
        }
    }
    fn download(&mut self, kind: Kind) {
        let Some(media) = self.active_media() else {
            self.status = "Select media to download first".into();
            return;
        };
        if !media.source.starts_with("https://") && !media.source.starts_with("http://") {
            self.status = "Only online media can be downloaded".into();
            return;
        }
        let id = if media.id > 0 {
            media.id
        } else {
            match self.store.add(&media.source, &media.title, &media.creator) {
                Ok(id) => id,
                Err(e) => {
                    self.message_error(e);
                    return;
                }
            }
        };
        if let Err(e) = self.reload() {
            self.message_error(e);
            return;
        }
        if self
            .store
            .asset(id, kind.as_str())
            .ok()
            .flatten()
            .is_some_and(|p| Path::new(&p).is_file())
        {
            self.status = format!("{} is already available offline", media.title);
            return;
        }
        let data_dir = match crate::data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        let tx = self.download_tx.clone();
        let url = media.source.clone();
        self.status = format!("Downloading {} as {}...", media.title, kind.as_str());
        std::thread::spawn(move || {
            let result = download::fetch(id, &url, kind, &data_dir).map_err(|e| e.to_string());
            let _ = tx.send(DownloadResponse {
                media_id: id,
                kind,
                result,
            });
        });
    }
    fn play(&mut self, mpd: bool) {
        if let Some(mut media) = self.active_media() {
            if !mpd && media.id > 0 {
                let file = self
                    .store
                    .asset(media.id, "video")
                    .ok()
                    .flatten()
                    .or_else(|| self.store.asset(media.id, "audio").ok().flatten());
                if let Some(file) = file.filter(|s| Path::new(s).is_file()) {
                    media.source = file;
                }
            }
            let result = if mpd {
                self.player.play_mpd(&media)
            } else {
                self.player.play_mpv(&media)
            };
            match result {
                Ok(()) => {
                    self.tap_sync = false;
                    self.seek_undo = None;
                    self.duration_ms = None;
                    self.mpd = player::mpd_status().ok();
                    self.load_lyrics(false);
                    self.status = format!(
                        "Playing: {} [{}]",
                        media.title,
                        if mpd { "MPD" } else { "mpv" }
                    );
                    self.tab = Tab::Playing;
                }
                Err(e) => self.message_error(e),
            }
        } else {
            self.status = "Select a media item first".into();
        }
    }
    fn search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.search = query.clone();
        self.tab = Tab::Discover;
        self.pending = true;
        self.found.clear();
        self.status = format!("Searching YouTube: {query}");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = youtube::search(&query).map_err(|e| e.to_string());
            let _ = tx.send(SearchResponse { query, result });
        });
    }
    fn submit_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let value = prompt.text().trim();
        if value.is_empty() {
            return;
        }
        match prompt {
            Prompt::Search(value) => self.search(value),
            Prompt::Tag(value) => {
                if let Some(id) = self.selected_media().map(|m| m.id) {
                    match self.store.tag(id, value.trim()) {
                        Ok(()) => self.status = format!("Tag added: {}", value.trim()),
                        Err(e) => self.message_error(e),
                    }
                }
            }
            Prompt::Filter(value) => {
                self.filter_tags = value
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                match self.rebuild_recommendations() {
                    Ok(()) => {
                        self.status = format!("For You · {} matches", self.recommendations.len())
                    }
                    Err(e) => self.message_error(e),
                }
            }
            Prompt::JumpTime(value) => match parse_time_ms(&value) {
                Ok(ms) => self.seek_to(ms),
                Err(error) => self.status = format!("Invalid seek time: {error}"),
            },
            Prompt::LyricOffset(value) => match parse_offset_ms(&value) {
                Ok(ms) => self.set_lyric_offset(ms),
                Err(problem) => self.status = format!("Invalid lyrics offset: {problem}"),
            },
            Prompt::Collection(value) => {
                if let Some(id) = self.selected_media().map(|m| m.id) {
                    let result = self
                        .store
                        .create_collection(value.trim())
                        .and_then(|_| self.store.add_to_collection(value.trim(), id));
                    match result {
                        Ok(()) => self.status = format!("Added to collection: {}", value.trim()),
                        Err(e) => self.message_error(e),
                    }
                    if let Err(e) = self.reload() {
                        self.message_error(e)
                    }
                }
            }
        }
    }
    fn mpris_snapshot(&self) -> Snapshot {
        let mut snap = Snapshot::default();
        let mpd = self.mpd.as_ref();
        if self.player.engine == Some(Engine::Mpv) {
            if let Some(media) = self.player.active.as_ref() {
                snap.title = media.title.clone();
                snap.artist = media.creator.clone();
                snap.source = media.source.clone();
            }
            snap.state = if self.player.paused {
                "Paused"
            } else {
                "Playing"
            };
            snap.can_play = self.player.active.is_some();
            snap.can_seek = snap.can_play;
            snap.volume = self.player.volume().unwrap_or(1.0);
        } else {
            // Always source MPD metadata from its actual current song. The daemon's
            // track can change via Next/Previous without changing Windmill's selection.
            if let Some(mpd) = mpd {
                snap.title = mpd.title.clone().unwrap_or_default();
                snap.artist = mpd.artist.clone().unwrap_or_default();
                if let Some(file) = mpd.file.as_ref() {
                    snap.source = format!("mpd:{file}");
                }
                // Preserve custom Windmill metadata for the item that is actually
                // in MPD; after Next, fall back to the daemon's new track metadata.
                if let Some(media) = self.player.active.as_ref()
                    && player::mpd_relative(&media.source).ok().as_deref() == mpd.file.as_deref()
                {
                    snap.title = media.title.clone();
                    snap.artist = media.creator.clone();
                    snap.source = media.source.clone();
                }
                snap.duration_us = mpd.duration_ms.map(|v| v.saturating_mul(1000));
                snap.state = match mpd.state.as_str() {
                    "play" => "Playing",
                    "pause" => "Paused",
                    _ => "Stopped",
                };
                snap.can_play = mpd.file.is_some();
                snap.can_seek = snap.can_play;
                let length = mpd.playlist_length.unwrap_or(0);
                let position = mpd.song;
                snap.can_go_next = position.is_some_and(|p| p.saturating_add(1) < length);
                snap.can_go_previous = position.is_some_and(|p| p > 0);
                snap.volume = mpd.volume.map(|v| f64::from(v) / 100.0).unwrap_or(1.0);
            }
        }
        let elapsed = if self.player.engine == Some(Engine::Mpv) {
            self.elapsed_ms
        } else {
            self.elapsed_ms.or_else(|| mpd.map(|s| s.elapsed_ms))
        };
        snap.position_us = elapsed.unwrap_or(0).saturating_mul(1000);
        snap
    }
    fn mpris_command(&mut self, control: Control) -> Option<i64> {
        let snapshot = self.mpris_snapshot();
        match control {
            Control::Play | Control::Pause | Control::Toggle if !snapshot.can_play => return None,
            Control::Next if !snapshot.can_go_next => return None,
            Control::Previous if !snapshot.can_go_previous => return None,
            Control::Seek(_) | Control::SetPosition(_) if !snapshot.can_seek => return None,
            _ => {}
        }
        let mut seeked = false;
        let result: std::result::Result<(), Box<dyn Error>> = match control {
            Control::Play => self.player.play(),
            Control::Pause => self.player.pause(),
            Control::Toggle => self.player.toggle(),
            Control::Stop => {
                self.player.stop();
                self.elapsed_ms = None;
                self.status = "Playback stopped by media control".into();
                Ok(())
            }
            Control::Next => self.player.next(),
            Control::Previous => self.player.previous(),
            Control::Seek(offset) => {
                seeked = true;
                self.player.seek_relative(offset)
            }
            Control::SetPosition(position) => {
                seeked = true;
                self.player.seek_absolute(position)
            }
            Control::Volume(volume) => self.player.set_volume(volume),
        };
        if let Err(e) = result {
            self.message_error(e)
        }
        self.last_tick = Instant::now() - Duration::from_secs(2);
        self.tick();
        if seeked {
            self.elapsed_ms.map(|ms| ms.saturating_mul(1000))
        } else {
            None
        }
    }
    fn key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        if let Some(ref mut prompt) = self.prompt {
            match key.code {
                KeyCode::Esc => self.prompt = None,
                KeyCode::Enter => self.submit_prompt(),
                KeyCode::Backspace => {
                    prompt.text_mut().pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    prompt.text_mut().push(c)
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => return true,
            KeyCode::Esc if self.tap_sync => {
                self.tap_sync = false;
                self.lyric_follow = true;
                self.status = "Tap Sync cancelled. No offset changed.".into();
            }
            KeyCode::Char('s') if self.tab == Tab::Playing => self.toggle_tap_sync(),
            KeyCode::Char('u') if self.tab == Tab::Playing => self.undo_tap_sync(),
            KeyCode::Char(' ') | KeyCode::Enter if self.tab == Tab::Playing && self.tap_sync => {
                self.tap_lyric_now()
            }
            KeyCode::Left => self.skip_by(if key.modifiers.contains(KeyModifiers::CONTROL) {
                -60_000
            } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                -30_000
            } else {
                -5_000
            }),
            KeyCode::Right => self.skip_by(if key.modifiers.contains(KeyModifiers::CONTROL) {
                60_000
            } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                30_000
            } else {
                5_000
            }),
            KeyCode::PageUp => self.skip_by(-60_000),
            KeyCode::PageDown => self.skip_by(60_000),
            KeyCode::Home => self.seek_to(0),
            KeyCode::End => {
                if let Some(duration) = self.player.duration_ms().or(self.duration_ms) {
                    self.seek_to(duration.saturating_sub(1000))
                } else {
                    self.status = "This track has no known duration".into()
                }
            }
            KeyCode::Char('g') => self.prompt = Some(Prompt::JumpTime(String::new())),
            KeyCode::Char('z') => self.undo_seek(),
            KeyCode::Tab => self.tab = Tab::from_index(self.tab.index() + 1),
            KeyCode::BackTab => self.tab = Tab::from_index(self.tab.index() + 4),
            KeyCode::Char('1'..='5') => {
                if let KeyCode::Char(c) = key.code {
                    self.tab = Tab::from_index((c as u8 - b'1') as usize)
                }
            }
            KeyCode::Down | KeyCode::Char('j') if self.tab == Tab::Playing => {
                self.move_lyric_cursor(1)
            }
            KeyCode::Up | KeyCode::Char('k') if self.tab == Tab::Playing => {
                self.move_lyric_cursor(-1)
            }
            KeyCode::Enter if self.tab == Tab::Playing => self.seek_to_lyric(),
            KeyCode::Char('a') if self.tab == Tab::Playing => {
                self.lyric_follow = true;
                self.status = "Lyrics auto-follow enabled".into();
            }
            KeyCode::Char('L') if self.tab == Tab::Playing => self.load_lyrics(true),
            KeyCode::Char('o') if self.tab == Tab::Playing => {
                if self.lyric_key.is_some() {
                    self.prompt = Some(Prompt::LyricOffset(String::new()));
                    self.status =
                        format!("Current offset: {}", format_offset_ms(self.lyric_offset_ms));
                } else {
                    self.status = "Play a song before setting lyrics offset".into();
                }
            }
            KeyCode::Char('[') if self.tab == Tab::Playing => self.nudge_lyrics(-250),
            KeyCode::Char(']') if self.tab == Tab::Playing => self.nudge_lyrics(250),
            KeyCode::Char('0') if self.tab == Tab::Playing => {
                self.nudge_lyrics(-self.lyric_offset_ms)
            }
            KeyCode::Down | KeyCode::Char('j') => self.choose(1),
            KeyCode::Up | KeyCode::Char('k') => self.choose(-1),
            KeyCode::Char('/') => self.prompt = Some(Prompt::Search(self.search.clone())),
            KeyCode::Char('f') if self.tab == Tab::ForYou => {
                self.prompt = Some(Prompt::Filter(self.filter_tags.join(",")))
            }
            KeyCode::Char('t') if self.tab == Tab::Library => {
                self.prompt = Some(Prompt::Tag(String::new()))
            }
            KeyCode::Char('c') if self.tab == Tab::Library => {
                self.prompt = Some(Prompt::Collection(String::new()))
            }
            KeyCode::Char('p')
                if self.tab == Tab::Library
                    || self.tab == Tab::Discover
                    || self.tab == Tab::ForYou =>
            {
                self.play(false)
            }
            KeyCode::Char('m') if self.tab == Tab::Library => self.play(true),
            KeyCode::Char('s') if self.tab == Tab::Discover => {
                if let Some(video) = self.selected_video().cloned() {
                    match self.store.add(&video.url, &video.title, &video.creator) {
                        Ok(id) => {
                            self.status = format!("Saved #{}: {}", id, video.title);
                            if let Err(e) = self.reload() {
                                self.message_error(e)
                            }
                        }
                        Err(e) => self.message_error(e),
                    }
                }
            }
            KeyCode::Char('d')
                if self.tab == Tab::Library
                    || self.tab == Tab::Discover
                    || self.tab == Tab::ForYou =>
            {
                self.download(Kind::Audio);
            }
            KeyCode::Char('v')
                if self.tab == Tab::Library
                    || self.tab == Tab::Discover
                    || self.tab == Tab::ForYou =>
            {
                self.download(Kind::Video);
            }
            KeyCode::Char(' ') => {
                if let Err(e) = self.player.toggle() {
                    self.message_error(e)
                }
            }
            KeyCode::Char('x') => {
                self.tap_sync = false;
                self.seek_undo = None;
                self.player.stop();
                self.elapsed_ms = None;
                self.status = "Playback stopped".into();
            }
            KeyCode::Char('n') => {
                if let Err(e) = self.player.next() {
                    self.message_error(e)
                }
            }
            KeyCode::Char('r') => {
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
            }
            _ => {}
        };
        false
    }
    fn tick(&mut self) {
        while let Ok(response) = self.lyric_rx.try_recv() {
            if self.lyric_key.as_deref() != Some(&response.key) {
                continue;
            }
            self.lyric_pending = false;
            match response.result {
                Ok(Some(Loaded { document, origin })) => {
                    let count = match &document {
                        Document::Synced(lrc) => lrc.lines().len(),
                        Document::Plain(text) => text.lines().count(),
                    };
                    self.lyric_status = format!("{count} lyric lines ready");
                    self.lyric_origin = origin;
                    self.lyrics = Some(document);
                    self.lyric_follow = true;
                }
                Ok(None) => {
                    self.lyric_status = "No matching lyrics found · press L to retry".into();
                }
                Err(err) => {
                    self.lyric_status = format!("Lyrics unavailable: {err}");
                }
            }
        }
        while let Ok(response) = self.download_rx.try_recv() {
            match response.result {
                Ok(file) => {
                    match self
                        .store
                        .save_asset(response.media_id, response.kind.as_str(), &file)
                    {
                        Ok(()) => {
                            self.status = format!(
                                "Saved {} offline: {}",
                                response.kind.as_str(),
                                file.display()
                            )
                        }
                        Err(e) => self.message_error(e),
                    }
                }
                Err(e) => self.message_error(e),
            }
        }
        while let Ok(response) = self.rx.try_recv() {
            if response.query != self.search {
                continue;
            }
            self.pending = false;
            match response.result {
                Ok(videos) => {
                    self.found = videos;
                    self.selected[1] = 0;
                    self.status = format!(
                        "Found {} YouTube results · s saves to catalog, p plays",
                        self.found.len()
                    );
                }
                Err(e) => self.message_error(e),
            }
        }
        if self.last_tick.elapsed() >= Duration::from_millis(350) {
            self.elapsed_ms = self.player.elapsed_ms();
            self.mpd = player::mpd_status().ok();
            self.duration_ms = if self.player.engine == Some(Engine::Mpv) {
                self.player.duration_ms()
            } else {
                self.mpd.as_ref().and_then(|s| s.duration_ms)
            };
            // Track MPD queue changes and reload lyrics for the current actual file.
            self.load_lyrics(false);
            self.last_tick = Instant::now();
        }
    }
}
struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(Box::new(e));
        }
        Ok(Self)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    }
}

pub fn run(store: Store) -> Result<()> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return Err("Windmill TUI requires a terminal".into());
    }
    let mut app = App::new(store)?;
    let mpris = match mpris::Bridge::new() {
        Ok(server) => Some(server),
        Err(e) => {
            app.status = format!("MPRIS unavailable (TUI works): {e}");
            None
        }
    };
    let _screen = Screen::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    loop {
        app.tick();
        if let Some(bridge) = mpris.as_ref() {
            while let Ok(command) = bridge.commands().try_recv() {
                if let Some(position) = app.mpris_command(command) {
                    let _ = bridge.seeked(position);
                }
            }
            if let Err(e) = bridge.update(app.mpris_snapshot()) {
                app.status = format!("MPRIS signal error: {e}");
            }
        }
        terminal.draw(|f| draw(f, &app))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.key(key) {
                        break;
                    }
                }
                Event::Mouse(mouse) => app.mouse(mouse, terminal.size()?.into()),
                _ => {}
            }
        }
    }
    Ok(())
}
fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MUTED))
}
fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);
    let tabs = Tabs::new(TITLES.iter().map(|s| Line::from(*s)).collect::<Vec<_>>())
        .block(panel(" WINDMILL  /  personal media center "))
        .select(app.tab.index())
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, layout[0]);
    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(layout[1]);
    match app.tab {
        Tab::Library => draw_library(frame, app, content[0], content[1]),
        Tab::Discover => draw_discover(frame, app, content[0], content[1]),
        Tab::Collections => draw_collections(frame, app, content[0], content[1]),
        Tab::ForYou => draw_recommendations(frame, app, content[0], content[1]),
        Tab::Playing => draw_playing(frame, app, content[0], content[1]),
    }
    let status = if let Some(ref prompt) = app.prompt {
        format!("{}: {}_", prompt.label(), prompt.text())
    } else {
        app.status.clone()
    };
    frame.render_widget(
        Paragraph::new(status)
            .style(Style::default().fg(HIGHLIGHT))
            .block(panel(" Status "))
            .wrap(Wrap { trim: true }),
        layout[2],
    );
    frame.render_widget(Paragraph::new("←/→ ±5s · Shift+←/→ ±30s · g jump · z undo · click progress · 1-5 tabs · / search · Space pause · q quit")
        .style(Style::default().fg(MUTED)).block(panel(" Keys ")),layout[3]);
}
fn select_list(
    frame: &mut Frame,
    lines: Vec<ListItem<'_>>,
    selected: usize,
    title: &str,
    area: Rect,
) {
    let mut state = ListState::default().with_selected(if lines.is_empty() {
        None
    } else {
        Some(selected.min(lines.len() - 1))
    });
    let list = List::new(lines)
        .block(panel(title))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(51, 65, 57))
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ▸ ");
    frame.render_stateful_widget(list, area, &mut state);
}
fn draw_library(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let items = app
        .media
        .iter()
        .map(|m| ListItem::new(format!("{}  —  {}", m.title, m.creator)))
        .collect();
    select_list(
        frame,
        items,
        app.selected[0],
        &format!(" Library · {} items ", app.media.len()),
        left,
    );
    let text=match app.selected_media() {
        Some(m)=>{
            let tags=app.store.tags_for(m.id).unwrap_or_default();
            let offline=["audio","video"].iter()
                .filter(|kind|app.store.asset(m.id,kind).ok().flatten().is_some_and(|p|Path::new(&p).is_file()))
                .copied().collect::<Vec<_>>().join(", ");
            format!("{}\\n\\nCreator: {}\\nSource: {}\\nOffline: {}\\n\\nTags: {}\\n\\nPress p for mpv video/audio\\nPress m for MPD library playback\\nPress d or v to download\\nPress t to tag · c to collect",
                m.title,m.creator,m.source,
                if offline.is_empty() {"none"}else{&offline},
                if tags.is_empty() {"(none)".into()}else{tags.join(" · ")})
        }
        None=>"Empty library. Press / to discover videos, then s to save.\\n\\nOr use: windmill add <file> <title> <creator>".into(),
    };
    frame.render_widget(
        Paragraph::new(text.replace("\\n", "\n"))
            .wrap(Wrap { trim: false })
            .block(panel(" Details ")),
        right,
    );
}
fn draw_discover(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let items = app
        .found
        .iter()
        .map(|v| ListItem::new(format!("{}  —  {}", v.title, v.creator)))
        .collect();
    select_list(
        frame,
        items,
        app.selected[1],
        &format!(
            " YouTube · {} ",
            if app.pending {
                "searching..."
            } else {
                "results"
            }
        ),
        left,
    );
    let description = if let Some(v) = app.selected_video() {
        format!(
            "{}\\n\\nBy: {}\\nVideo ID: {}\\n\\n{}\\n\\nStatus: {}\\n\\nPress p to play via mpv.\\nPress s to save to local catalog.\\nPress d to download audio or v for video.",
            v.title,
            v.creator,
            v.id,
            v.url,
            if app.store.has(&v.url).unwrap_or(false) {
                "In library"
            } else {
                "Not saved"
            }
        )
    } else if app.pending {
        "Searching public YouTube metadata...".into()
    } else {
        "Press / and enter a search query.\\nResults come from yt-dlp with no Google sign-in.\\nYour Windmill catalog stays local.".into()
    };
    frame.render_widget(
        Paragraph::new(description.replace("\\n", "\n"))
            .block(panel(" Discovery "))
            .wrap(Wrap { trim: false }),
        right,
    );
}
fn draw_collections(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let list = app
        .collections
        .iter()
        .map(|s| ListItem::new(s.as_str()))
        .collect();
    select_list(frame, list, app.selected[2], " Collections ", left);
    let text = app
        .collections
        .get(app.selected[2])
        .map(|name| {
            let members = app.store.collection_items(name).unwrap_or_default();
            format!(
                "{}\\n\\n{}",
                name,
                if members.is_empty() {
                    "No items yet".into()
                } else {
                    members
                        .iter()
                        .map(|m| format!("• {}", m.title))
                        .collect::<Vec<_>>()
                        .join("\\n")
                }
            )
        })
        .unwrap_or_else(|| "No collections yet. From Library, select a song and press c.".into());
    frame.render_widget(
        Paragraph::new(text.replace("\\n", "\n"))
            .block(panel(" Collection contents "))
            .wrap(Wrap { trim: false }),
        right,
    );
}
fn draw_recommendations(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let items = app
        .recommendations
        .iter()
        .map(|(score, m)| ListItem::new(format!("[{score}] {} — {}", m.title, m.creator)))
        .collect();
    select_list(
        frame,
        items,
        app.selected[3],
        " For You · tag ranked ",
        left,
    );
    let filter = if app.filter_tags.is_empty() {
        app.store.top_tags(6).unwrap_or_default().join(", ")
    } else {
        app.filter_tags.join(", ")
    };
    let text = if let Some((score, media)) = app.recommendations.get(app.selected[3]) {
        format!(
            "{}\\n\\nBy: {}\\nTag score: {}\\n\\nRanked using: {}\\n\\nPress p to play. Press f to change tags.\\nFor You is based entirely on your local tags.",
            media.title, media.creator, score, filter
        )
    } else {
        format!(
            "No matches yet.\\n\\nTag items in Library using t, then come back here.\\n\\nPress f to choose your own tags.\\nCurrent tags: {filter}"
        )
    };
    frame.render_widget(
        Paragraph::new(text.replace("\\n", "\n"))
            .block(panel(" Explainable recommendations "))
            .wrap(Wrap { trim: false }),
        right,
    );
}
fn progress_area(area: Rect) -> Rect {
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(main[1]);
    let panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(7),
        ])
        .split(halves[0]);
    panels[1]
}
fn draw_playing(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let snap = app.mpris_snapshot();
    let title = if snap.title.is_empty() {
        "Nothing playing"
    } else {
        snap.title.as_str()
    };
    let engine = match app.player.engine {
        Some(Engine::Mpv) => "mpv",
        Some(Engine::Mpd) => "MPD",
        None => "MPD / mpv",
    };
    let elapsed = (snap.position_us / 1000).max(0);
    let duration = app
        .duration_ms
        .or_else(|| snap.duration_us.map(|t| t / 1000))
        .unwrap_or(0);
    let minute = elapsed / 60_000;
    let second = (elapsed / 1000) % 60;
    let duration_label = if duration > 0 {
        format!("{:02}:{:02}", duration / 60_000, (duration / 1000) % 60)
    } else {
        "--:--".into()
    };
    let info = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(7),
        ])
        .split(left);
    let details = vec![
        Line::raw(""),
        Line::from(Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(snap.artist, Style::default().fg(MUTED))),
        Line::raw(""),
        Line::from(format!("♫  {engine}  ·  {}", snap.state)),
        Line::raw(""),
        Line::from(format!("  {minute:02}:{second:02} / {duration_label}")),
    ];
    frame.render_widget(
        Paragraph::new(details)
            .block(panel(" Now Playing "))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        info[0],
    );
    let ratio = if duration > 0 {
        (elapsed as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    frame.render_widget(
        Gauge::default()
            .block(panel(" Playback progress "))
            .gauge_style(Style::default().fg(ACCENT))
            .ratio(ratio)
            .label(format!("{}%", (ratio * 100.0).round() as u8)),
        info[1],
    );
    let controls = if app.tap_sync {
        let cue = match app.lyrics.as_ref() {
            Some(Document::Synced(lrc)) => lrc.lines().get(app.lyric_cursor).map(|line| {
                format!(
                    "{}  [lyric {:02}:{:02}]",
                    line.text,
                    line.at_ms / 60_000,
                    (line.at_ms / 1_000) % 60
                )
            }),
            _ => None,
        }
        .unwrap_or_else(|| "Choose a lyric line".into());
        format!(
            "SELECTED: {cue}\n↑/↓ or j/k choose another lyric\nSPACE / Enter = TAP when you HEAR it\nEsc = cancel without changing offset"
        )
    } else {
        "←/→ ±5s · Shift+arrows ±30s\ng jump · z undo skip · click progress\ns TAP SYNC · o lyric offset · Space pause\nj/k lyrics · Enter line · u undo tap".into()
    };
    frame.render_widget(
        Paragraph::new(controls)
            .block(panel(if app.tap_sync {
                " TAP SYNC · playback keeps running "
            } else {
                " Lyrics controls "
            }))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(if app.tap_sync { HIGHLIGHT } else { MUTED })),
        info[2],
    );
    draw_lyrics(frame, app, right, elapsed);
}
fn draw_lyrics(frame: &mut Frame, app: &App, area: Rect, elapsed_ms: i64) {
    let state = if app.lyric_pending {
        "searching..."
    } else if app.lyric_origin.is_empty() {
        "Lyrics"
    } else {
        app.lyric_origin.as_str()
    };
    let title = format!(
        " {} · offset {} ",
        if app.tap_sync {
            "TAP SYNC: choose a line"
        } else {
            state
        },
        format_offset_ms(app.lyric_offset_ms)
    );
    let Some(doc) = app.lyrics.as_ref() else {
        let text = format!(
            "\n\n{}\n\nLocal .lrc files are preferred.\nLRCLIB results are cached offline.\n\nPress L to search again.",
            app.lyric_status
        );
        frame.render_widget(
            Paragraph::new(text)
                .block(panel(&title))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(MUTED)),
            area,
        );
        return;
    };
    let visible = ((area.height.saturating_sub(2) as usize) / 2).max(3);
    let mut rendered: Vec<Line> = Vec::new();
    match doc {
        Document::Synced(lrc) => {
            let offset_elapsed = elapsed_ms.saturating_sub(app.lyric_offset_ms);
            let active = lrc.active_index(offset_elapsed);
            let center = if app.lyric_follow {
                active.unwrap_or(0)
            } else {
                app.lyric_cursor
            };
            let before = visible / 2;
            let first = center.saturating_sub(before);
            let last = (first + visible).min(lrc.lines().len());
            for (idx, line) in lrc.lines()[first..last].iter().enumerate() {
                let at = first + idx;
                let distance = at.abs_diff(center);
                let current = Some(at) == active;
                let browsed = !app.lyric_follow && at == app.lyric_cursor;
                let shade = if current || (app.tap_sync && browsed) {
                    HIGHLIGHT
                } else if browsed {
                    ACCENT
                } else {
                    match distance {
                        0 | 1 => Color::Rgb(172, 186, 181),
                        2 => Color::Rgb(125, 138, 137),
                        _ => Color::Rgb(89, 100, 105),
                    }
                };
                let mark = if app.tap_sync && browsed {
                    "◎ "
                } else if current {
                    "♫ "
                } else if browsed {
                    "> "
                } else {
                    "  "
                };
                let style = if current {
                    Style::default().fg(shade).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(shade)
                };
                if current && !line.words.is_empty() {
                    let time = lrc.timeline_ms(offset_elapsed);
                    let mut spans = vec![Span::styled(mark, style)];
                    for word in &line.words {
                        let sung = word.at_ms <= time;
                        spans.push(Span::styled(
                            word.text.clone(),
                            if sung {
                                Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(MUTED)
                            },
                        ));
                    }
                    rendered.push(Line::from(spans));
                } else {
                    rendered.push(Line::from(Span::styled(
                        format!("{mark}{}", line.text),
                        style,
                    )));
                }
                rendered.push(Line::raw(""));
            }
        }
        Document::Plain(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let cursor = if app.lyric_follow {
                0
            } else {
                app.lyric_cursor.min(lines.len().saturating_sub(1))
            };
            let first = cursor.saturating_sub(visible / 3);
            for (idx, line) in lines.iter().enumerate().skip(first).take(visible) {
                let style = if !app.lyric_follow && idx == cursor {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(MUTED)
                };
                rendered.push(Line::from(Span::styled(*line, style)));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(rendered)
            .block(panel(&title))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    #[test]
    fn draws_all_tabs_without_media() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        let backend = TestBackend::new(100, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        for tab in [
            Tab::Library,
            Tab::Discover,
            Tab::Collections,
            Tab::ForYou,
            Tab::Playing,
        ] {
            app.tab = tab;
            terminal.draw(|frame| draw(frame, &app)).unwrap();
        }
    }
    #[test]
    fn tap_sync_chooses_line_marks_and_undoes_without_playback_seek() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        app.tab = Tab::Playing;
        app.lyric_source = "https://example.test/video".into();
        app.lyric_key = Some("test-track".into());
        app.lyrics = Some(Document::Synced(crate::lyrics::Lyrics::parse(
            "[00:00.00]\n[00:02.00]First lyric\n[00:05.00]Second lyric",
        )));
        app.toggle_tap_sync();
        assert!(app.tap_sync);
        assert_eq!(app.lyric_cursor, 1, "should skip blank opening line");
        app.move_lyric_cursor(1);
        assert_eq!(app.lyric_cursor, 2);
        // Pressing at 20.750s should align the selected 5.000s lyric there.
        app.apply_lyric_cue(20_750);
        assert_eq!(app.lyric_offset_ms, 15_750);
        assert!(!app.tap_sync);
        assert!(app.lyric_follow);
        assert_eq!(app.store.lyrics_offset(&app.lyric_source).unwrap(), 15_750);
        app.undo_tap_sync();
        assert_eq!(app.store.lyrics_offset(&app.lyric_source).unwrap(), 0);
        assert!(app.sync_undo.is_none());
    }
    #[test]
    fn tap_sync_rejects_out_of_range_cues() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        app.tab = Tab::Playing;
        app.lyric_source = "source".into();
        app.lyric_key = Some("test-track".into());
        app.lyrics = Some(Document::Synced(crate::lyrics::Lyrics::parse(
            "[00:01.00]First lyric",
        )));
        app.toggle_tap_sync();
        app.apply_lyric_cue(4_000_000);
        assert!(app.tap_sync, "invalid mark stays in guided mode");
        assert_eq!(app.store.lyrics_offset("source").unwrap(), 0);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.tap_sync);
        assert_eq!(app.store.lyrics_offset("source").unwrap(), 0);
    }
    #[test]
    fn video_lyrics_use_original_identity_and_allow_precise_edits() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let url = "https://www.youtube.com/watch?v=HyHNuVaZJ-k";
        let id = store.add(url, "Feel Good Inc.", "Gorillaz").unwrap();
        let mut app = App::new(store).unwrap();
        app.player.active = Some(Media {
            id,
            source: "/tmp/video-download.mp4".into(),
            title: "Feel Good Inc.".into(),
            creator: "Gorillaz".into(),
        });
        app.player.engine = Some(Engine::Mpv);
        app.duration_ms = Some(295_000);
        let req = app.current_lyric_request().unwrap();
        assert_eq!(req.source, url);
        // Music-video intros/outros should not exclude LRCLIB's shorter recording.
        assert_eq!(req.duration_ms, None);
        app.tab = Tab::Playing;
        app.lyric_key = Some("fixture".into());
        app.lyric_source = req.source;
        assert!(!app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)));
        assert!(matches!(app.prompt, Some(Prompt::LyricOffset(_))));
        for c in "+12.5".chars() {
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.lyric_offset_ms, 12_500);
        assert_eq!(app.store.lyrics_offset(url).unwrap(), 12_500);
        app.key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        assert_eq!(app.store.lyrics_offset(url).unwrap(), 12_250);
    }
    #[test]
    fn input_state_and_selection() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        app.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(matches!(app.prompt, Some(Prompt::Tag(_))));
        app.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(app.prompt.as_ref().unwrap().text(), "x");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.prompt.is_none());
    }
}
