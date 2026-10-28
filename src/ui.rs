//! Interactive Windmill terminal front end. No application-specific background daemon.
use crate::{
    art,
    download::{self, Kind},
    lyric_offset::{MAX_ABS_OFFSET_MS, format_offset_ms, offset_from_cue, parse_offset_ms},
    lyric_source::{self, Document, Loaded, Request},
    mpris::{self, Control, Snapshot},
    player::{self, Engine, Playback, Status},
    playlist::{self, Playlist},
    seek::{self, clamp_target, format_time, parse_time_ms},
    store::{Media, Store},
    visualizer,
    youtube::{self, Video},
    ytmusic::{self, MusicItem},
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
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
use ratatui_image::{Image as TerminalImage, Resize, picker::Picker, protocol::Protocol};
use std::{
    collections::BTreeSet,
    error::Error,
    io::{self, IsTerminal},
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

const TITLES: [&str; 6] = [
    "Library",
    "Discover",
    "Collections",
    "For You",
    "Now Playing",
    "Queue",
];
const ACCENT: Color = Color::Rgb(186, 216, 165);
const MUTED: Color = Color::Rgb(150, 156, 171);
const HIGHLIGHT: Color = Color::Rgb(246, 210, 143);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Tab {
    Library,
    Discover,
    Collections,
    ForYou,
    Playing,
    Queue,
}
impl Tab {
    fn index(self) -> usize {
        match self {
            Self::Library => 0,
            Self::Discover => 1,
            Self::Collections => 2,
            Self::ForYou => 3,
            Self::Playing => 4,
            Self::Queue => 5,
        }
    }
    fn from_index(i: usize) -> Self {
        match i % 6 {
            0 => Self::Library,
            1 => Self::Discover,
            2 => Self::Collections,
            3 => Self::ForYou,
            4 => Self::Playing,
            _ => Self::Queue,
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
    ManualLyrics(String),
    JumpTime(String),
    PlaylistUrl(String),
    MusicQuery(String),
}
impl Prompt {
    fn text(&self) -> &str {
        match self {
            Self::Search(t)
            | Self::Tag(t)
            | Self::Collection(t)
            | Self::Filter(t)
            | Self::LyricOffset(t)
            | Self::ManualLyrics(t)
            | Self::JumpTime(t)
            | Self::PlaylistUrl(t)
            | Self::MusicQuery(t) => t,
        }
    }
    fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Search(t)
            | Self::Tag(t)
            | Self::Collection(t)
            | Self::Filter(t)
            | Self::LyricOffset(t)
            | Self::ManualLyrics(t)
            | Self::JumpTime(t)
            | Self::PlaylistUrl(t)
            | Self::MusicQuery(t) => t,
        }
    }
    fn label(&self) -> &str {
        match self {
            Self::Search(_) => "YouTube search",
            Self::Tag(_) => "Add tag",
            Self::Collection(_) => "Create & add collection",
            Self::Filter(_) => "Recommendation tags, comma-separated",
            Self::LyricOffset(_) => "Lyrics offset (+seconds or -mm:ss.sss) — Enter saves",
            Self::ManualLyrics(_) => "Paste LRCLIB track link (https://lrclib.net/tracks/12345)",
            Self::JumpTime(_) => "Jump to playback time (seconds, m:ss, h:mm:ss)",
            Self::PlaylistUrl(_) => "Public YouTube / YouTube Music playlist URL",
            Self::MusicQuery(_) => "YouTube Music search (songs:/albums:/artists:/playlists:query)",
        }
    }
}
struct LyricResponse {
    key: String,
    generation: u64,
    manual_id: Option<u64>,
    pin_on_success: bool,
    result: std::result::Result<Option<Loaded>, String>,
}
struct DownloadResponse {
    media_id: i64,
    kind: Kind,
    result: std::result::Result<std::path::PathBuf, String>,
    batch: Option<(usize, usize)>,
}
struct MusicResponse {
    query: String,
    result: std::result::Result<Vec<MusicItem>, String>,
}
struct MusicBrowseResponse {
    key: String,
    result: std::result::Result<(String, Vec<Video>), String>,
}
struct ArtResponse {
    key: String,
    result: std::result::Result<Option<Protocol>, String>,
}
struct PlaylistResponse {
    url: String,
    result: std::result::Result<Playlist, String>,
}
struct SearchResponse {
    query: String,
    result: std::result::Result<Vec<Video>, String>,
}
pub struct App {
    store: Store,
    tab: Tab,
    selected: [usize; 4],
    visible_panels: [[bool; 6]; 6],
    queue_selected: usize,
    queue: Vec<(i64, Media)>,
    queue_active: Option<i64>,
    queue_repeat: bool,
    queue_shuffle_tick: u64,
    listening: Option<i64>,
    listened_ms: i64,
    listening_last: Instant,
    media: Vec<Media>,
    collections: Vec<String>,
    recommendations: Vec<(i64, Media)>,
    filter_tags: Vec<String>,
    found: Vec<Video>,
    playlist_preview: Option<Playlist>,
    music_items: Option<Vec<MusicItem>>,
    music_pending: bool,
    music_query: String,
    music_tx: Sender<MusicResponse>,
    music_rx: Receiver<MusicResponse>,
    browse_tx: Sender<MusicBrowseResponse>,
    browse_rx: Receiver<MusicBrowseResponse>,
    browse_key: String,
    playlist_selected: BTreeSet<usize>,
    playlist_loading: bool,
    playlist_url: String,
    playlist_tx: Sender<PlaylistResponse>,
    playlist_rx: Receiver<PlaylistResponse>,
    batch_running: bool,
    batch_failed: usize,
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
    lyric_generation: u64,
    lyrics_duration_ms: Option<i64>,
    lyric_tx: Sender<LyricResponse>,
    lyric_rx: Receiver<LyricResponse>,
    duration_ms: Option<i64>,
    art_key: Option<String>,
    art_protocol: Option<Protocol>,
    art_picker: Picker,
    art_pending: bool,
    art_tx: Sender<ArtResponse>,
    art_rx: Receiver<ArtResponse>,
    spectrum: Option<visualizer::Spectrum>,
    spectrum_enabled: bool,
    spectrum_attempted: bool,
    spectrum_bands: [f32; 16],
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
        let (playlist_tx, playlist_rx) = mpsc::channel();
        let (art_tx, art_rx) = mpsc::channel();
        let (music_tx, music_rx) = mpsc::channel();
        let (browse_tx, browse_rx) = mpsc::channel();
        let media = store.all()?;
        let queue = store.queue_entries()?;
        let collections = store.collections()?;
        let tags = store.top_tags(6)?;
        let recommendations = store.recommend(tags.iter().map(String::as_str))?;
        Ok(Self {
            store,
            tab: Tab::Library,
            selected: [0; 4],
            visible_panels: [[true; 6]; 6],
            queue_selected: 0,
            queue,
            queue_active: None,
            queue_repeat: false,
            queue_shuffle_tick: 0,
            listening: None,
            listened_ms: 0,
            listening_last: Instant::now(),
            media,
            collections,
            recommendations,
            filter_tags: vec![],
            found: vec![],
            playlist_preview: None,
            music_items: None,
            music_pending: false,
            music_query: String::new(),
            music_tx,
            music_rx,
            browse_tx,
            browse_rx,
            browse_key: String::new(),
            playlist_selected: BTreeSet::new(),
            playlist_loading: false,
            playlist_url: String::new(),
            playlist_tx,
            playlist_rx,
            batch_running: false,
            batch_failed: 0,
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
            lyric_generation: 0,
            lyrics_duration_ms: None,
            lyric_tx,
            lyric_rx,
            duration_ms: None,
            art_key: None,
            art_protocol: None,
            art_picker: Picker::halfblocks(),
            art_pending: false,
            art_tx,
            art_rx,
            spectrum: None,
            spectrum_enabled: true,
            spectrum_attempted: false,
            spectrum_bands: [0.; 16],
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
        self.queue = self.store.queue_entries()?;
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        if self
            .queue_active
            .is_some_and(|id| !self.queue.iter().any(|(row, _)| *row == id))
        {
            self.queue_active = None;
        }
        self.collections = self.store.collections()?;
        self.rebuild_recommendations()?;
        for (i, len) in [
            self.media.len(),
            self.playlist_preview
                .as_ref()
                .map_or(self.found.len(), |p| p.entries.len()),
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
        if let Some(playlist) = self.playlist_preview.as_ref() {
            playlist.entries.get(self.selected[1])
        } else if let Some(items) = self.music_items.as_ref() {
            items.get(self.selected[1]).and_then(|i| i.song.as_ref())
        } else {
            self.found.get(self.selected[1])
        }
    }
    fn playlist_entries_selected(&self) -> Vec<Video> {
        let Some(playlist) = self.playlist_preview.as_ref() else {
            return vec![];
        };
        self.playlist_selected
            .iter()
            .filter_map(|i| playlist.entries.get(*i).cloned())
            .collect()
    }
    fn start_music_search(&mut self, input: String) {
        let input = input.trim();
        let (kind, query) = input
            .split_once(':')
            .filter(|(part, _)| ["songs", "albums", "artists", "playlists"].contains(part))
            .unwrap_or(("songs", input));
        if query.trim().is_empty() {
            self.status = "Enter a YouTube Music query".into();
            return;
        }
        self.music_query = format!("{kind}:{query}");
        self.music_pending = true;
        self.playlist_preview = None;
        self.playlist_selected.clear();
        self.tab = Tab::Discover;
        self.status = format!("Searching YouTube Music {kind}...");
        let request = self.music_query.clone();
        let tx = self.music_tx.clone();
        let query = query.to_owned();
        let kind = kind.to_owned();
        std::thread::spawn(move || {
            let result = ytmusic::search(&kind, &query).map_err(|e| e.to_string());
            let _ = tx.send(MusicResponse {
                query: request,
                result,
            });
        });
    }
    fn open_music_selection(&mut self) {
        let Some(item) = self
            .music_items
            .as_ref()
            .and_then(|r| r.get(self.selected[1]))
            .cloned()
        else {
            return;
        };
        match item.kind.as_str() {
            "songs" => self.play(false),
            "playlists" => self.start_playlist_preview(item.url),
            "albums" | "artists" => {
                self.browse_key = format!("{}:{}", item.kind, item.id);
                self.status = format!("Fetching public {} tracks: {}", item.kind, item.title);
                let tx = self.browse_tx.clone();
                let key = self.browse_key.clone();
                std::thread::spawn(move || {
                    let result = ytmusic::browse(&item.kind, &item.id).map_err(|e| e.to_string());
                    let _ = tx.send(MusicBrowseResponse { key, result });
                });
            }
            _ => {}
        }
    }
    fn start_playlist_preview(&mut self, url: String) {
        let canonical = match playlist::canonical_url(&url) {
            Ok(value) => value,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        self.playlist_loading = true;
        self.music_items = None;
        self.playlist_url = canonical.clone();
        self.status = "Fetching playlist metadata (no downloads)...".into();
        self.tab = Tab::Discover;
        let tx = self.playlist_tx.clone();
        std::thread::spawn(move || {
            let result = playlist::fetch(&canonical).map_err(|e| e.to_string());
            let _ = tx.send(PlaylistResponse {
                url: canonical,
                result,
            });
        });
    }
    fn import_playlist_selection(&mut self) -> Option<Vec<(i64, Video)>> {
        let selected = self.playlist_entries_selected();
        let Some(playlist) = self.playlist_preview.as_ref() else {
            self.status = "Paste a playlist URL with P first".into();
            return None;
        };
        if selected.is_empty() {
            self.status = "Select tracks with a / A before importing".into();
            return None;
        }
        let name = playlist.title.clone();
        match self.store.import_playlist(&name, &selected) {
            Ok(added) => {
                let tracks = selected
                    .into_iter()
                    .filter_map(|video| {
                        // Import just committed, so IDs must be present in the store.
                        self.store
                            .all()
                            .ok()?
                            .into_iter()
                            .find(|m| m.source == video.url)
                            .map(|m| (m.id, video))
                    })
                    .collect::<Vec<_>>();
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
                self.status = format!(
                    "Imported {} selected tracks into {name} ({} new)",
                    tracks.len(),
                    added
                );
                Some(tracks)
            }
            Err(e) => {
                self.message_error(e);
                None
            }
        }
    }
    fn batch_download(&mut self, kind: Kind) {
        if self.batch_running {
            self.status = "An existing batch download is still running".into();
            return;
        }
        let Some(items) = self.import_playlist_selection() else {
            return;
        };
        let filtered = items
            .into_iter()
            .filter(|(id, _)| {
                !self
                    .store
                    .asset(*id, kind.as_str())
                    .ok()
                    .flatten()
                    .is_some_and(|f| Path::new(&f).is_file())
            })
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            self.status = "Every selected item is already downloaded".into();
            return;
        }
        let data_dir = match crate::data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        self.batch_running = true;
        self.batch_failed = 0;
        let total = filtered.len();
        self.status = format!(
            "Downloading {total} {} tracks in the background...",
            kind.as_str()
        );
        let tx = self.download_tx.clone();
        std::thread::spawn(move || {
            for (index, (id, video)) in filtered.into_iter().enumerate() {
                let result =
                    download::fetch(id, &video.url, kind, &data_dir).map_err(|e| e.to_string());
                if tx
                    .send(DownloadResponse {
                        media_id: id,
                        kind,
                        result,
                        batch: Some((index + 1, total)),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    fn message_error(&mut self, err: impl std::fmt::Display) {
        self.status = format!("Error: {err}");
    }
    fn choose(&mut self, delta: isize) {
        let (idx, len) = match self.tab {
            Tab::Library => (0, self.media.len()),
            Tab::Discover => (
                1,
                self.playlist_preview
                    .as_ref()
                    .map_or(self.found.len(), |p| p.entries.len()),
            ),
            Tab::Collections => (2, self.collections.len()),
            Tab::ForYou => (3, self.recommendations.len()),
            Tab::Playing => return,
            Tab::Queue => {
                if !self.queue.is_empty() {
                    self.queue_selected = (self.queue_selected as isize + delta)
                        .clamp(0, self.queue.len().saturating_sub(1) as isize)
                        as usize;
                }
                return;
            }
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
            Tab::Queue => self.queue.get(self.queue_selected).map(|(_, m)| m.clone()),
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
    fn load_art(&mut self) {
        let source = if self.player.engine == Some(Engine::Mpv) {
            self.player.active.as_ref().map(|m| {
                if m.id > 0 {
                    self.store
                        .get(m.id)
                        .map(|v| v.source)
                        .unwrap_or_else(|_| m.source.clone())
                } else {
                    m.source.clone()
                }
            })
        } else {
            self.mpd
                .as_ref()
                .and_then(|s| s.file.as_ref())
                .and_then(|file| {
                    player::music_dir()
                        .ok()
                        .map(|root| root.join(file).to_string_lossy().into_owned())
                })
        };
        let Some(source) = source else {
            self.art_protocol = None;
            self.art_key = None;
            self.art_pending = false;
            return;
        };
        if self.art_key.as_ref() == Some(&source) {
            return;
        }
        self.art_key = Some(source.clone());
        self.art_protocol = None;
        self.art_pending = true;
        let data = match crate::data_dir() {
            Ok(dir) => dir,
            Err(_) => {
                self.art_pending = false;
                return;
            }
        };
        let tx = self.art_tx.clone();
        let picker = self.art_picker.clone();
        std::thread::spawn(move || {
            let result = (|| -> std::result::Result<Option<Protocol>, String> {
                let Some(path) = art::get(&source, &data).map_err(|e| e.to_string())? else {
                    return Ok(None);
                };
                let image = image::ImageReader::open(path)
                    .map_err(|e| e.to_string())?
                    .decode()
                    .map_err(|e| e.to_string())?;
                let protocol = picker
                    .new_protocol(image, ratatui::layout::Size::new(25, 10), Resize::Fit(None))
                    .map_err(|e| e.to_string())?;
                Ok(Some(protocol))
            })();
            let _ = tx.send(ArtResponse {
                key: source,
                result,
            });
        });
    }
    fn pin_manual_lyrics(&mut self, link: &str) {
        if self.lyric_key.is_none() {
            self.status = "Play a song before choosing an LRCLIB link".into();
            return;
        }
        let id = match lyric_source::parse_manual_link(link) {
            Ok(id) => id,
            Err(e) => {
                self.status = format!("Invalid LRCLIB link: {e}");
                return;
            }
        };
        let dir = match crate::data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        self.lyric_generation = self.lyric_generation.wrapping_add(1);
        let generation = self.lyric_generation;
        let Some(key) = self.lyric_key.clone() else {
            return;
        };
        let tx = self.lyric_tx.clone();
        self.lyric_pending = true;
        self.lyric_status = format!("Fetching selected LRCLIB #{id}...");
        self.status = format!("Checking LRCLIB #{id} (will save only if usable)");
        std::thread::spawn(move || {
            let result =
                lyric_source::load_manual(id, &dir, true, false).map_err(|e| e.to_string());
            let _ = tx.send(LyricResponse {
                key,
                generation,
                manual_id: Some(id),
                pin_on_success: true,
                result,
            });
        });
    }
    fn restore_auto_lyrics(&mut self) {
        if self.lyric_key.is_none() {
            self.status = "Play a song before clearing an LRCLIB link".into();
            return;
        }
        match self.store.clear_manual_lyrics_id(&self.lyric_source) {
            Ok(()) => {
                self.lyric_generation = self.lyric_generation.wrapping_add(1);
                self.status = "Manual lyrics removed; returning to automatic matching".into();
                self.load_lyrics(true);
            }
            Err(e) => self.message_error(e),
        }
    }
    fn load_lyrics(&mut self, refresh: bool) {
        let Some(request) = self.current_lyric_request() else {
            self.lyrics = None;
            self.lyric_key = None;
            self.tap_sync = false;
            self.lyric_status = "Nothing playing".into();
            self.lyric_pending = false;
            self.lyric_generation = self.lyric_generation.wrapping_add(1);
            self.lyrics_duration_ms = None;
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
            self.lyrics_duration_ms = None;
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
        let manual_id = match self.store.manual_lyrics_id(&request.source) {
            Ok(id) => id,
            Err(e) => {
                self.message_error(e);
                return;
            }
        };
        self.lyric_generation = self.lyric_generation.wrapping_add(1);
        let generation = self.lyric_generation;
        let tx = self.lyric_tx.clone();
        std::thread::spawn(move || {
            let result = match manual_id {
                Some(id) => lyric_source::load_manual(id, &data_dir, true, refresh),
                None => lyric_source::load(&request, &data_dir, true, refresh),
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(LyricResponse {
                key,
                generation,
                manual_id,
                pin_on_success: false,
                result,
            });
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
        self.status =
            "TAP SYNC: p play/pause, j/k choose lyric, SPACE mark when heard, Esc cancel.".into();
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
                batch: None,
            });
        });
    }
    fn update_listening(&mut self) {
        let now = Instant::now();
        let active = match self.player.engine {
            Some(Engine::Mpv) => !self.player.paused,
            Some(Engine::Mpd) => self.mpd.as_ref().is_some_and(|s| s.state == "play"),
            None => false,
        };
        if self.listening.is_some() && active {
            self.listened_ms = self.listened_ms.saturating_add(
                now.duration_since(self.listening_last)
                    .as_millis()
                    .min(1200) as i64,
            );
        }
        self.listening_last = now;
    }
    fn finish_listening(&mut self) {
        if let Some(media_id) = self.listening.take() {
            let seconds = self.listened_ms / 1000;
            if seconds >= 1 {
                if let Err(e) = self.store.record_listen(media_id, seconds) {
                    self.message_error(e)
                }
                let _ = self.rebuild_recommendations();
            }
        }
        self.listened_ms = 0;
        self.listening_last = Instant::now();
    }
    fn rate_media(&mut self, rating: i64) {
        let selected = if self.tab == Tab::Playing {
            self.player.active.clone()
        } else {
            self.active_media()
        };
        let Some(media) = selected else {
            self.status = "Select a saved song to rate".into();
            return;
        };
        let id = if media.id > 0 {
            media.id
        } else {
            self.store
                .all()
                .ok()
                .and_then(|all| {
                    all.into_iter()
                        .find(|m| m.source == media.source)
                        .map(|m| m.id)
                })
                .unwrap_or(0)
        };
        if id <= 0 {
            self.status = "Save this song to the library before rating".into();
            return;
        }
        match self.store.rate(id, rating) {
            Ok(()) => {
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
                self.status = format!(
                    "{}: {}",
                    match rating {
                        1 => "Liked",
                        -1 => "Disliked",
                        _ => "Reset rating",
                    },
                    media.title
                );
            }
            Err(e) => self.message_error(e),
        }
    }
    fn start_media(&mut self, mut media: Media, mpd: bool) -> bool {
        if !mpd && media.id > 0 {
            let file = self
                .store
                .asset(media.id, "video")
                .ok()
                .flatten()
                .filter(|f| Path::new(f).is_file())
                .or_else(|| {
                    self.store
                        .asset(media.id, "audio")
                        .ok()
                        .flatten()
                        .filter(|f| Path::new(f).is_file())
                });
            if let Some(file) = file {
                media.source = file
            }
        }
        self.finish_listening();
        let original_id = media.id;
        let result = if mpd {
            self.player.play_mpd(&media)
        } else {
            self.player.play_mpv(&media)
        };
        match result {
            Ok(()) => {
                self.listening = (original_id > 0).then_some(original_id);
                self.listened_ms = 0;
                self.listening_last = Instant::now();
                self.tap_sync = false;
                self.seek_undo = None;
                self.duration_ms = None;
                self.elapsed_ms = Some(0);
                self.mpd = player::mpd_status().ok();
                self.load_lyrics(false);
                self.status = format!(
                    "Playing: {} [{}]",
                    media.title,
                    if mpd { "MPD" } else { "mpv" }
                );
                self.tab = Tab::Playing;
                true
            }
            Err(e) => {
                self.message_error(e);
                false
            }
        }
    }
    fn play(&mut self, mpd: bool) {
        let Some(media) = self.active_media() else {
            self.status = "Select a media item first".into();
            return;
        };
        if self.start_media(media, mpd) {
            self.queue_active = None; // Standalone playback is independent of queued items.
        }
    }
    fn play_queue(&mut self, index: usize) -> bool {
        let Some((row, media)) = self.queue.get(index).cloned() else {
            self.status = "The queue is empty".into();
            return false;
        };
        let mpd = std::path::Path::new(&media.source).is_file()
            && player::mpd_relative(&media.source).is_ok();
        if self.start_media(media, mpd) {
            self.queue_active = Some(row);
            self.queue_selected = index;
            true
        } else {
            false
        }
    }
    fn enqueue_current(&mut self) {
        let Some(media) = self.active_media() else {
            self.status = "Select a track to enqueue".into();
            return;
        };
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
        match self.store.queue_add(id) {
            Ok(_) => {
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
                self.status = format!(
                    "Added to queue: {} ({} tracks)",
                    media.title,
                    self.queue.len()
                );
            }
            Err(e) => self.message_error(e),
        }
    }
    fn queue_step(&mut self, direction: isize) {
        if self.queue.is_empty() {
            if let Err(e) = if direction > 0 {
                self.player.next()
            } else {
                self.player.previous()
            } {
                self.message_error(e)
            }
            return;
        }
        let position = self
            .queue_active
            .and_then(|id| self.queue.iter().position(|(row, _)| *row == id));
        let target = match position {
            Some(index) => {
                let next = index as isize + direction;
                if next < 0 {
                    if self.queue_repeat {
                        self.queue.len() - 1
                    } else {
                        0
                    }
                } else if next >= self.queue.len() as isize {
                    if self.queue_repeat {
                        0
                    } else {
                        self.queue_active = None;
                        self.finish_listening();
                        self.player.stop();
                        self.status = "End of queue. Press 6 to replay.".into();
                        return;
                    }
                } else {
                    next as usize
                }
            }
            None => {
                if direction >= 0 {
                    0
                } else {
                    self.queue.len() - 1
                }
            }
        };
        self.play_queue(target);
    }
    fn reorder_queue(&mut self, step: isize) {
        if self.queue.len() < 2 {
            return;
        }
        let here = self.queue_selected;
        let to = (here as isize + step).clamp(0, self.queue.len() as isize - 1) as usize;
        if here == to {
            return;
        }
        let mut ids = self.queue.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        ids.swap(here, to);
        match self.store.queue_reorder(&ids) {
            Ok(()) => {
                self.queue_selected = to;
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
                self.status = "Queue order saved".into();
            }
            Err(e) => self.message_error(e),
        }
    }
    fn shuffle_queue(&mut self) {
        if self.queue.len() < 2 {
            return;
        }
        use std::hash::{Hash, Hasher};
        self.queue_shuffle_tick = self.queue_shuffle_tick.saturating_add(1);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut ids = self.queue.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        ids.sort_by_key(|id| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (id, nonce, self.queue_shuffle_tick).hash(&mut h);
            h.finish()
        });
        match self.store.queue_reorder(&ids) {
            Ok(()) => {
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
                self.status = "Queue shuffled (saved)".into();
            }
            Err(e) => self.message_error(e),
        }
    }
    fn search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.search = query.clone();
        self.playlist_preview = None;
        self.music_items = None;
        self.playlist_selected.clear();
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
            Prompt::PlaylistUrl(url) => self.start_playlist_preview(url),
            Prompt::MusicQuery(input) => self.start_music_search(input),
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
            Prompt::ManualLyrics(link) => self.pin_manual_lyrics(&link),
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
        if let Some(row) = self.queue_active
            && let Some(pos) = self.queue.iter().position(|(id, _)| *id == row)
        {
            snap.can_go_next = pos + 1 < self.queue.len() || self.queue_repeat;
            snap.can_go_previous = pos > 0 || self.queue_repeat;
        }
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
                self.finish_listening();
                self.player.stop();
                self.queue_active = None;
                self.elapsed_ms = None;
                self.status = "Playback stopped by media control".into();
                Ok(())
            }
            Control::Next => {
                if self.queue_active.is_some() {
                    self.queue_step(1);
                    Ok(())
                } else {
                    self.player.next()
                }
            }
            Control::Previous => {
                if self.queue_active.is_some() {
                    self.queue_step(-1);
                    Ok(())
                } else {
                    self.player.previous()
                }
            }
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
    fn toggle_playback(&mut self) {
        if !self.mpris_snapshot().can_play {
            self.status = "No playable track. Choose a song first.".into();
            return;
        }
        match self.player.toggle() {
            Ok(()) => {
                self.mpd = player::mpd_status().ok();
                let paused = if self.player.engine == Some(Engine::Mpv) {
                    self.player.paused
                } else {
                    self.mpd
                        .as_ref()
                        .is_some_and(|status| status.state == "pause")
                };
                self.status = if paused {
                    "Playback paused. Press p again to resume (Space marks lyrics in Tap Sync)."
                } else {
                    "Playback resumed. Press Space when the selected lyric starts."
                }
                .into();
                self.last_tick = Instant::now() - Duration::from_secs(2);
            }
            Err(e) => self.message_error(e),
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
            KeyCode::Char('q') => return true,
            KeyCode::Char('Q') if self.tab == Tab::Collections => {
                if let Some(name) = self.collections.get(self.selected[2]).cloned() {
                    match self.store.queue_collection(&name) {
                        Ok(count) => {
                            self.queue_active = None;
                            if let Err(e) = self.reload() {
                                self.message_error(e)
                            }
                            self.tab = Tab::Queue;
                            self.status = format!("Queued {count} tracks from collection: {name}");
                        }
                        Err(e) => self.message_error(e),
                    }
                }
            }
            KeyCode::Esc if self.tap_sync => {
                self.tap_sync = false;
                self.lyric_follow = true;
                self.status = "Tap Sync cancelled. No offset changed.".into();
            }
            KeyCode::Char('p') if self.tab == Tab::Playing => self.toggle_playback(),
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
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tab = Tab::from_index(
                    self.tab.index()
                        + if key.modifiers.contains(KeyModifiers::SHIFT) {
                            5
                        } else {
                            1
                        },
                );
            }
            KeyCode::BackTab if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tab = Tab::from_index(self.tab.index() + 5);
            }
            KeyCode::Char('1'..='6')
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                if let KeyCode::Char(c) = key.code {
                    let idx = (c as u8 - b'1') as usize;
                    if self.tab != Tab::Playing && idx > 1 {
                        return false;
                    }
                    self.visible_panels[self.tab.index()][idx] =
                        !self.visible_panels[self.tab.index()][idx];
                    let panel = if self.tab == Tab::Playing {
                        [
                            "Artwork",
                            "Spectrum",
                            "Lyrics",
                            "Track information",
                            "Progress",
                            "Controls",
                        ][idx]
                    } else if idx == 0 {
                        "List"
                    } else if idx == 1 {
                        "Details"
                    } else {
                        "Now Playing panel"
                    };
                    self.status = format!(
                        "{panel}: {}",
                        if self.visible_panels[self.tab.index()][idx] {
                            "shown"
                        } else {
                            "hidden"
                        }
                    );
                }
            }
            KeyCode::Char('+')
                if matches!(
                    self.tab,
                    Tab::Library | Tab::ForYou | Tab::Queue | Tab::Playing
                ) =>
            {
                self.rate_media(1)
            }
            KeyCode::Char('-')
                if matches!(
                    self.tab,
                    Tab::Library | Tab::ForYou | Tab::Queue | Tab::Playing
                ) =>
            {
                self.rate_media(-1)
            }
            KeyCode::Char('=')
                if matches!(
                    self.tab,
                    Tab::Library | Tab::ForYou | Tab::Queue | Tab::Playing
                ) =>
            {
                self.rate_media(0)
            }
            KeyCode::Char('e')
                if matches!(
                    self.tab,
                    Tab::Library | Tab::Discover | Tab::ForYou | Tab::Queue
                ) =>
            {
                self.enqueue_current()
            }
            KeyCode::Enter if self.tab == Tab::Queue => {
                self.play_queue(self.queue_selected);
            }
            KeyCode::Char('p') if self.tab == Tab::Queue => {
                self.play_queue(self.queue_selected);
            }
            KeyCode::Delete | KeyCode::Backspace if self.tab == Tab::Queue => {
                if let Some((row, _)) = self.queue.get(self.queue_selected) {
                    match self.store.queue_remove(*row) {
                        Ok(()) => {
                            if let Err(e) = self.reload() {
                                self.message_error(e)
                            }
                            self.status = "Queue entry removed".into();
                        }
                        Err(e) => self.message_error(e),
                    }
                }
            }
            KeyCode::Char('<') if self.tab == Tab::Queue => self.reorder_queue(-1),
            KeyCode::Char('>') if self.tab == Tab::Queue => self.reorder_queue(1),
            KeyCode::Char('S') if self.tab == Tab::Queue => self.shuffle_queue(),
            KeyCode::Char('R') if self.tab == Tab::Queue => {
                self.queue_repeat = !self.queue_repeat;
                self.status = format!(
                    "Queue repeat: {}",
                    if self.queue_repeat { "ON" } else { "OFF" }
                );
            }
            KeyCode::Char('C') if self.tab == Tab::Queue => match self.store.queue_clear() {
                Ok(()) => {
                    self.queue_active = None;
                    if let Err(e) = self.reload() {
                        self.message_error(e)
                    }
                    self.status = "Queue cleared. Current playback continues.".into();
                }
                Err(e) => self.message_error(e),
            },
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
            KeyCode::Char('M') if self.tab == Tab::Playing => {
                if self.lyric_key.is_some() {
                    self.prompt = Some(Prompt::ManualLyrics(String::new()));
                    self.status =
                        "Paste an LRCLIB track page link; your selection will be remembered".into();
                } else {
                    self.status = "Play a song before choosing lyrics".into()
                }
            }
            KeyCode::Char('U') if self.tab == Tab::Playing => self.restore_auto_lyrics(),
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
            KeyCode::Char('Y') if self.tab == Tab::Discover => {
                self.prompt = Some(Prompt::MusicQuery(String::new()))
            }
            KeyCode::Enter if self.tab == Tab::Discover && self.music_items.is_some() => {
                self.open_music_selection()
            }
            KeyCode::Char('P') if self.tab == Tab::Discover || self.tab == Tab::Collections => {
                self.prompt = Some(Prompt::PlaylistUrl(String::new()))
            }
            KeyCode::Char('a') if self.tab == Tab::Discover && self.playlist_preview.is_some() => {
                let idx = self.selected[1];
                if !self.playlist_selected.insert(idx) {
                    self.playlist_selected.remove(&idx);
                }
                self.status = format!(
                    "{} of {} playlist tracks selected",
                    self.playlist_selected.len(),
                    self.playlist_preview.as_ref().unwrap().entries.len()
                );
            }
            KeyCode::Char('A') if self.tab == Tab::Discover && self.playlist_preview.is_some() => {
                let len = self.playlist_preview.as_ref().unwrap().entries.len();
                self.playlist_selected = (0..len).collect();
                self.status = format!("All {len} playlist tracks selected");
            }
            KeyCode::Char('I') if self.tab == Tab::Discover && self.playlist_preview.is_some() => {
                self.import_playlist_selection();
            }
            KeyCode::Char('B') if self.tab == Tab::Discover && self.playlist_preview.is_some() => {
                self.batch_download(Kind::Audio)
            }
            KeyCode::Char('V') if self.tab == Tab::Discover && self.playlist_preview.is_some() => {
                self.batch_download(Kind::Video)
            }
            KeyCode::Char('V') if self.tab == Tab::Playing => self.toggle_spectrum(),
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
            KeyCode::Char(' ') => self.toggle_playback(),
            KeyCode::Char('x') => {
                self.tap_sync = false;
                self.seek_undo = None;
                self.finish_listening();
                self.player.stop();
                self.queue_active = None;
                self.elapsed_ms = None;
                self.status = "Playback stopped".into();
            }
            KeyCode::Char('n') => self.queue_step(1),
            KeyCode::Char('b') => self.queue_step(-1),
            KeyCode::Char('r') => {
                if let Err(e) = self.reload() {
                    self.message_error(e)
                }
            }
            _ => {}
        };
        false
    }
    fn toggle_spectrum(&mut self) {
        self.spectrum_enabled = !self.spectrum_enabled;
        self.spectrum = None;
        self.spectrum_bands = [0.; 16];
        self.spectrum_attempted = false;
        self.status = format!(
            "Audio-reactive output spectrum: {}",
            if self.spectrum_enabled { "ON" } else { "OFF" }
        );
    }
    fn tick(&mut self) {
        let active = self.player.engine.is_some()
            || self
                .mpd
                .as_ref()
                .is_some_and(|status| status.state == "play");
        if active
            && self.spectrum.is_none()
            && !self.spectrum_attempted
            && self.spectrum_enabled
            && self.visible_panels[Tab::Playing.index()][1]
            && std::env::var_os("WINDMILL_DISABLE_VISUALIZER").is_none()
        {
            self.spectrum_attempted = true;
            match visualizer::Spectrum::start() {
                Ok(spectrum) => self.spectrum = Some(spectrum),
                Err(e) => {
                    self.spectrum = None;
                    if self.tab == Tab::Playing {
                        self.status = format!("CAVA visualizer unavailable: {e}");
                    }
                }
            }
        }
        if !active || !self.spectrum_enabled || !self.visible_panels[Tab::Playing.index()][1] {
            self.spectrum = None;
            self.spectrum_bands = [0.; 16];
            self.spectrum_attempted = false;
        }
        if let Some(spectrum) = self.spectrum.as_ref()
            && let Some(data) = spectrum.newest()
        {
            self.spectrum_bands = data;
        }
        while let Ok(response) = self.music_rx.try_recv() {
            if response.query != self.music_query {
                continue;
            }
            self.music_pending = false;
            match response.result {
                Ok(items) => {
                    self.selected[1] = 0;
                    self.status = format!(
                        "YouTube Music: {} results · Enter open · p play songs",
                        items.len()
                    );
                    self.music_items = Some(items);
                }
                Err(e) => self.message_error(e),
            }
        }
        while let Ok(response) = self.browse_rx.try_recv() {
            if response.key != self.browse_key {
                continue;
            }
            match response.result {
                Ok((name, entries)) => {
                    let len = entries.len();
                    self.playlist_selected = (0..len).collect();
                    self.music_items = None;
                    self.selected[1] = 0;
                    self.playlist_preview = Some(Playlist {
                        title: name,
                        url: format!("ytmusic:{}", response.key),
                        entries,
                    });
                    self.status =
                        format!("Public album/artist tracks loaded: {len} · I import · B download");
                }
                Err(e) => self.message_error(e),
            }
        }
        while let Ok(response) = self.art_rx.try_recv() {
            if self.art_key.as_deref() != Some(&response.key) {
                continue;
            }
            self.art_pending = false;
            if let Ok(Some(protocol)) = response.result {
                self.art_protocol = Some(protocol);
            }
        }
        while let Ok(response) = self.playlist_rx.try_recv() {
            if response.url != self.playlist_url {
                continue;
            }
            self.playlist_loading = false;
            match response.result {
                Ok(preview) => {
                    let len = preview.entries.len();
                    self.playlist_selected = (0..len).collect();
                    self.selected[1] = 0;
                    self.status = format!(
                        "Preview: {} · {len} items selected · a toggle · I import · B audio / V video download",
                        preview.title
                    );
                    self.playlist_preview = Some(preview);
                }
                Err(e) => self.message_error(e),
            }
        }
        while let Ok(response) = self.lyric_rx.try_recv() {
            if self.lyric_key.as_deref() != Some(&response.key)
                || self.lyric_generation != response.generation
            {
                continue;
            }
            self.lyric_pending = false;
            match response.result {
                Ok(Some(Loaded {
                    document,
                    origin,
                    lyrics_duration_ms,
                })) => {
                    if response.pin_on_success
                        && let Some(id) = response.manual_id
                    {
                        if let Err(e) = self.store.set_manual_lyrics_id(&self.lyric_source, id) {
                            self.message_error(e);
                            continue;
                        }
                        self.status =
                            format!("Pinned LRCLIB #{id} to this recording. Tap Sync to align.");
                    }
                    self.lyrics_duration_ms = lyrics_duration_ms;
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
                    self.lyric_status =
                        "No lyrics found. Press M to paste an LRCLIB track link.".into();
                    if response.pin_on_success {
                        self.status="That LRCLIB record has no usable lyrics; your previous selection is unchanged".into();
                    }
                }
                Err(err) => {
                    self.lyric_status = format!("Lyrics unavailable: {err}");
                    self.status = format!("LRCLIB lookup failed: {err}");
                }
            }
        }
        while let Ok(response) = self.download_rx.try_recv() {
            let batch = response.batch;
            if response.result.is_err() && batch.is_some() {
                self.batch_failed += 1
            }
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
            if let Some((done, total)) = batch {
                if done == total {
                    self.batch_running = false;
                }
                self.status = format!(
                    "Batch {}: {done}/{total} processed · {} errors{}",
                    if self.batch_running {
                        "progress"
                    } else {
                        "complete"
                    },
                    self.batch_failed,
                    if self.batch_running {
                        ""
                    } else {
                        " · see Library for offline assets"
                    }
                );
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
            let before_engine = self.player.engine;
            self.elapsed_ms = self.player.elapsed_ms();
            self.mpd = player::mpd_status().ok();
            self.update_listening();
            if before_engine == Some(Engine::Mpv)
                && self.player.engine.is_none()
                && self.queue_active.is_none()
            {
                self.finish_listening();
            }
            let queue_finished = self.queue_active.is_some()
                && ((before_engine == Some(Engine::Mpv) && self.player.engine.is_none())
                    || (before_engine == Some(Engine::Mpd)
                        && self.mpd.as_ref().is_some_and(|mpd| mpd.state == "stop")));
            if queue_finished {
                self.queue_step(1);
                self.mpd = player::mpd_status().ok();
            }
            self.duration_ms = if self.player.engine == Some(Engine::Mpv) {
                self.player.duration_ms()
            } else {
                self.mpd.as_ref().and_then(|s| s.duration_ms)
            };
            // Track MPD queue changes and reload lyrics for the current actual file.
            self.load_lyrics(false);
            self.load_art();
            self.last_tick = Instant::now();
        }
    }
}
struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        ) {
            let _ = disable_raw_mode();
            return Err(Box::new(e));
        }
        Ok(Self)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
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
    // Probe terminal capabilities before the TUI event loop; Ghostty's Kitty
    // image support is preferred to character-cell approximation.
    if std::env::var_os("WINDMILL_DISABLE_ARTWORK").is_none() {
        app.art_picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    }
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
        if event::poll(Duration::from_millis(35))? {
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
    app.finish_listening();
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
        .constraints(if app.tab == Tab::Playing {
            if app.visible_panels[Tab::Playing.index()][2] {
                [Constraint::Percentage(56), Constraint::Percentage(44)]
            } else {
                [Constraint::Percentage(100), Constraint::Length(0)]
            }
        } else {
            match (
                app.visible_panels[app.tab.index()][0],
                app.visible_panels[app.tab.index()][1],
            ) {
                (true, true) => [Constraint::Percentage(56), Constraint::Percentage(44)],
                (true, false) => [Constraint::Percentage(100), Constraint::Length(0)],
                (false, true) => [Constraint::Length(0), Constraint::Percentage(100)],
                (false, false) => [Constraint::Length(0), Constraint::Length(0)],
            }
        })
        .split(layout[1]);
    if app.tab != Tab::Playing
        && !app.visible_panels[app.tab.index()][0]
        && !app.visible_panels[app.tab.index()][1]
    {
        frame.render_widget(
            Paragraph::new("Both panels hidden. Press 1 or 2 to restore.")
                .alignment(Alignment::Center),
            layout[1],
        );
    }
    match app.tab {
        Tab::Library => draw_library(frame, app, content[0], content[1]),
        Tab::Discover => draw_discover(frame, app, content[0], content[1]),
        Tab::Collections => draw_collections(frame, app, content[0], content[1]),
        Tab::ForYou => draw_recommendations(frame, app, content[0], content[1]),
        Tab::Playing => draw_playing(frame, app, content[0], content[1]),
        Tab::Queue => draw_queue(frame, app, content[0], content[1]),
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
    let bottom_hint = if app.tap_sync {
        "TAP SYNC: p play/pause · SPACE mark lyric · Esc cancel · ←/→ seek · q quit"
    } else {
        "Ctrl+Tab / Ctrl+Shift+Tab = tabs · 1-6 = panels (Now Playing) · 1/2 = list/details · q quit"
    };
    frame.render_widget(
        Paragraph::new(bottom_hint)
            .style(Style::default().fg(MUTED))
            .block(panel(" Keys ")),
        layout[3],
    );
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
    if let Some(items) = app.music_items.as_ref() {
        let entries = items
            .iter()
            .map(|item| ListItem::new(format!("{} | {} — {}", item.kind, item.title, item.creator)))
            .collect();
        select_list(
            frame,
            entries,
            app.selected[1],
            &format!(" YouTube Music · {} results ", items.len()),
            left,
        );
        let desc = items
            .get(app.selected[1])
            .map(|m| {
                format!(
                    "{}\\nCreator: {}\\nType: {}\\nURL: {}",
                    m.title, m.creator, m.kind, m.url
                )
            })
            .unwrap_or_default();
        let text = format!(
            "{}\\n\\nEnter open album/artist/playlist or play song\\n            p play individual song · s save song\\n            Y search again (songs:,albums:,artists:,playlists:)\\n            P public playlist URL · / regular video search",
            desc
        );
        frame.render_widget(
            Paragraph::new(text.replace("\\n", "\n"))
                .block(panel(" YouTube Music "))
                .wrap(Wrap { trim: true }),
            right,
        );
        return;
    }
    if let Some(playlist) = app.playlist_preview.as_ref() {
        let items = playlist
            .entries
            .iter()
            .enumerate()
            .map(|(index, video)| {
                ListItem::new(format!(
                    "{} [{}] {} — {}",
                    if app.playlist_selected.contains(&index) {
                        "[x]"
                    } else {
                        "[ ]"
                    },
                    youtube::duration_label(video.duration_seconds),
                    video.title,
                    video.creator
                ))
            })
            .collect();
        select_list(
            frame,
            items,
            app.selected[1],
            &format!(
                " {} · {} selected / {} ",
                playlist.title,
                app.playlist_selected.len(),
                playlist.entries.len()
            ),
            left,
        );
        let current = app
            .selected_video()
            .map(|v| {
                format!(
                    "{}\\nBy: {}\\nDuration: {}\\nSource: {}",
                    v.title,
                    v.creator,
                    youtube::duration_label(v.duration_seconds),
                    v.url
                )
            })
            .unwrap_or_default();
        let text = format!(
            "{}\\n\\nURL: {}\\nSelected: {} / {}\\n\\n            j/k navigate · a toggle · A select all\\n            I import selected into collection\\n            B download selected audio · V video\\n            P open another playlist · / normal search\\n            p play current · s save current\\n\\n            Nothing downloads until you press B or V.",
            current,
            playlist.url,
            app.playlist_selected.len(),
            playlist.entries.len()
        );
        frame.render_widget(
            Paragraph::new(text.replace("\\n", "\n"))
                .block(panel(" Playlist preview "))
                .wrap(Wrap { trim: true }),
            right,
        );
        return;
    }
    let items = app
        .found
        .iter()
        .map(|v| {
            ListItem::new(format!(
                "[{}] {} — {}",
                youtube::duration_label(v.duration_seconds),
                v.title,
                v.creator
            ))
        })
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
            "{}\\n\\nBy: {}\\nDuration: {}\\nVideo ID: {}\\n\\n{}\\n\\nStatus: {}\\n\\nPress p to play. s to save. d audio/v video.\\nPress P to preview a public playlist.",
            v.title,
            v.creator,
            youtube::duration_label(v.duration_seconds),
            v.id,
            v.url,
            if app.store.has(&v.url).unwrap_or(false) {
                "In library"
            } else {
                "Not saved"
            }
        )
    } else if app.playlist_loading {
        "Fetching playlist metadata, no downloads started...".into()
    } else if app.pending {
        "Searching YouTube...".into()
    } else {
        "Press / to search videos or P to paste a public playlist URL.\\n        Preview and select before importing or downloading.".into()
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
        " For You · local taste ranked ",
        left,
    );
    let filter = if app.filter_tags.is_empty() {
        app.store.top_tags(6).unwrap_or_default().join(", ")
    } else {
        app.filter_tags.join(", ")
    };
    let text = if let Some((score, media)) = app.recommendations.get(app.selected[3]) {
        format!(
            "{}\\n\\nBy: {}\\nScore: {}\\n\\nTags: {}\\n\\n{}\\n\\n+ like · - dislike · = neutral\\np play · f edit tags\\nAll preference data stays local.",
            media.title,
            media.creator,
            score,
            filter,
            app.store.preference_reason(media.id).unwrap_or_default()
        )
    } else {
        format!(
            "No recommendations yet.\\n\\nTag songs using t, like them with +, or listen for a while.\\nPress f to choose tags.\\nCurrent tags: {filter}"
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
fn draw_queue(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let items = app
        .queue
        .iter()
        .enumerate()
        .map(|(index, (row, media))| {
            ListItem::new(format!(
                "{} {:>3}. {} — {}",
                if app.queue_active == Some(*row) {
                    "♫"
                } else {
                    " "
                },
                index + 1,
                media.title,
                media.creator
            ))
        })
        .collect();
    select_list(
        frame,
        items,
        app.queue_selected,
        &format!(" Queue · {} tracks ", app.queue.len()),
        left,
    );
    let current = app
        .queue
        .get(app.queue_selected)
        .map(|(_, m)| format!("{}\\nBy {}\\n\\n{}", m.title, m.creator, m.source))
        .unwrap_or_else(|| "Queue empty. Press e from Library, Discover, or For You.".into());
    let text = format!(
        "{}\\n\\n        Enter/p play selected · e duplicate item\\n        n/b next/previous · Delete remove\\n        </> reorder · S shuffle · R repeat {}\\n        C clear queue · Q from Collections loads whole collection\\n\\n        Queue persists across Windmill restarts.",
        current,
        if app.queue_repeat { "ON" } else { "OFF" }
    );
    frame.render_widget(
        Paragraph::new(text.replace("\\n", "\n"))
            .block(panel(" Queue controls "))
            .wrap(Wrap { trim: true }),
        right,
    );
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
    let flags = &app.visible_panels[Tab::Playing.index()];
    let info = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            if flags[3] || flags[0] {
                Constraint::Min(7)
            } else {
                Constraint::Length(0)
            },
            if flags[4] {
                Constraint::Length(3)
            } else {
                Constraint::Length(0)
            },
            if flags[5] {
                Constraint::Length(7)
            } else {
                Constraint::Length(0)
            },
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
        Line::from(format!("♫ {engine} · {}", snap.state)),
        Line::raw(""),
        Line::from(format!("{minute:02}:{second:02} / {duration_label}")),
    ];
    let has_spectrum = flags[1] && app.spectrum.is_some();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(7),
            Constraint::Length(if has_spectrum { 6 } else { 0 }),
        ])
        .split(info[0]);
    if flags[0] && app.art_protocol.is_some() {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if flags[3] {
                [Constraint::Percentage(44), Constraint::Percentage(56)]
            } else {
                [Constraint::Percentage(100), Constraint::Length(0)]
            })
            .split(sections[0]);
        frame.render_widget(panel(" Cover "), columns[0]);
        if let Some(protocol) = app.art_protocol.as_ref() {
            frame.render_widget(
                TerminalImage::new(protocol),
                columns[0].inner(ratatui::layout::Margin::new(1, 1)),
            );
        }
        if flags[3] {
            frame.render_widget(
                Paragraph::new(details)
                    .block(panel(" Now Playing "))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true }),
                columns[1],
            );
        }
    } else if flags[3] {
        frame.render_widget(
            Paragraph::new(details)
                .block(panel(" Now Playing "))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            sections[0],
        );
    }
    if has_spectrum {
        let mut bars = Vec::new();
        for row in (0..4).rev() {
            let mut spans = Vec::new();
            for (idx, value) in app.spectrum_bands.iter().enumerate() {
                let height = value.clamp(0., 1.) * 4.0;
                let filled = ((height - row as f32) * 8.).clamp(0., 8.) as usize;
                let color = if idx < 6 {
                    ACCENT
                } else if idx < 11 {
                    HIGHLIGHT
                } else {
                    Color::Rgb(144, 178, 205)
                };
                let glyph = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"][filled];
                spans.push(Span::styled(
                    format!("{glyph} "),
                    Style::default().fg(color),
                ));
            }
            bars.push(Line::from(spans));
        }
        frame.render_widget(
            Paragraph::new(bars)
                .block(panel(" Output spectrum · live "))
                .alignment(Alignment::Center),
            sections[1],
        );
    }
    let ratio = if duration > 0 {
        (elapsed as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if flags[4] {
        frame.render_widget(
            Gauge::default()
                .block(panel(" Playback progress "))
                .gauge_style(Style::default().fg(ACCENT))
                .ratio(ratio)
                .label(format!("{}%", (ratio * 100.0).round() as u8)),
            info[1],
        );
    }
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
            "SELECTED: {cue}\np PLAY / PAUSE · j/k choose a lyric\nSPACE / Enter = MARK when heard\nEsc cancels without changing offset"
        )
    } else {
        let timeline = match app.lyrics.as_ref() {
            Some(Document::Synced(lrc)) => {
                let pad = lrc.padding(app.duration_ms, app.lyric_offset_ms);
                if pad.lyrics_past_eof_ms > 0 {
                    format!(
                        "virtual lyric tail +{} (no audio modified)",
                        format_time(pad.lyrics_past_eof_ms)
                    )
                } else if pad.blank_outro_ms > 1500 {
                    format!(
                        "blank lyric outro {} (audio unchanged)",
                        format_time(pad.blank_outro_ms)
                    )
                } else if pad.blank_intro_ms > 1500 {
                    format!("blank lyric intro {}", format_time(pad.blank_intro_ms))
                } else {
                    "Lyrics align within media time".into()
                }
            }
            _ => "Paste manual LRCLIB track link with M".into(),
        };
        format!(
            "←/→ ±5s · g jump · z undo skip\ns Tap Sync · M paste LRCLIB · U auto\no offset · L refresh · V spectrum\np/Space pause · j/k lyrics · u undo tap\n{timeline}"
        )
    };
    if flags[5] {
        frame.render_widget(
            Paragraph::new(controls)
                .block(panel(if app.tap_sync {
                    " TAP SYNC · p play/pause, SPACE mark "
                } else {
                    " Lyrics controls "
                }))
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(if app.tap_sync { HIGHLIGHT } else { MUTED })),
            info[2],
        );
    }
    if flags[2] {
        draw_lyrics(frame, app, right, elapsed);
    } else if right.width > 0 && right.height > 0 {
        frame.render_widget(
            Paragraph::new("Lyrics hidden · press 3 to restore").alignment(Alignment::Center),
            right,
        );
    }
}
fn draw_lyrics(frame: &mut Frame, app: &App, area: Rect, elapsed_ms: i64) {
    let state = if app.lyric_pending {
        "searching..."
    } else if app.lyric_origin.is_empty() {
        "Lyrics"
    } else {
        app.lyric_origin.as_str()
    };
    let padding = app.lyrics.as_ref().and_then(|d| {
        if let Document::Synced(lrc) = d {
            Some(lrc.padding(app.duration_ms, app.lyric_offset_ms))
        } else {
            None
        }
    });
    let timing_info = padding
        .map(|p| {
            if p.lyrics_past_eof_ms > 0 {
                format!(
                    " · virtual lyric tail +{} (no extra audio)",
                    format_time(p.lyrics_past_eof_ms)
                )
            } else if p.blank_outro_ms > 1500 {
                format!(" · blank outro {}", format_time(p.blank_outro_ms))
            } else if p.blank_intro_ms > 1500 {
                format!(" · blank intro {}", format_time(p.blank_intro_ms))
            } else {
                String::new()
            }
        })
        .unwrap_or_default();
    let title = format!(
        " {} · offset {}{} ",
        if app.tap_sync {
            "TAP SYNC: choose a line"
        } else {
            state
        },
        format_offset_ms(app.lyric_offset_ms),
        timing_info
    );
    let Some(doc) = app.lyrics.as_ref() else {
        let text = format!(
            "\n\n{}\n\nPress M to paste an LRCLIB link.\nPress L to search again.",
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
            if active.is_none() && app.lyric_follow {
                rendered.push(Line::from(Span::styled(
                    "♪ Blank intro / waiting for vocals",
                    Style::default().fg(MUTED),
                )));
                rendered.push(Line::raw(""));
            }
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
                        format!(
                            "{mark}{}",
                            if line.text.trim().is_empty() && current {
                                "♪ Instrumental / blank lyrics"
                            } else {
                                line.text.as_str()
                            }
                        ),
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
    fn ctrl_tab_switches_views_and_numbers_toggle_panels() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        assert_eq!(app.tab, Tab::Library);
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.tab, Tab::Library, "plain Tab must not switch tabs");
        app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
        assert_eq!(app.tab, Tab::Discover);
        app.key(KeyEvent::new(
            KeyCode::Tab,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(app.tab, Tab::Library);
        app.key(KeyEvent::new(
            KeyCode::BackTab,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(app.tab, Tab::Queue);
        app.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(!app.visible_panels[Tab::Queue.index()][0]);
        assert_eq!(app.tab, Tab::Queue);
        app.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(app.visible_panels[Tab::Queue.index()][0]);
        app.tab = Tab::Playing;
        for index in 0..6 {
            let c = (b'1' + index as u8) as char;
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            assert!(!app.visible_panels[Tab::Playing.index()][index]);
        }
        let backend = TestBackend::new(110, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        for index in 0..6 {
            let c = (b'1' + index as u8) as char;
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            assert!(app.visible_panels[Tab::Playing.index()][index]);
        }
        terminal.draw(|frame| draw(frame, &app)).unwrap();
    }
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
            Tab::Queue,
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
    fn visualizer_toggle_resets_capture_and_levels() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let mut app = App::new(store).unwrap();
        app.tab = Tab::Playing;
        app.spectrum_bands[2] = 0.95;
        assert!(app.spectrum_enabled);
        app.key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::NONE));
        assert!(!app.spectrum_enabled);
        assert_eq!(app.spectrum_bands, [0.; 16]);
        app.key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::NONE));
        assert!(app.spectrum_enabled);
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
