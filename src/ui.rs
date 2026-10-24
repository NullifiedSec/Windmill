//! Interactive Windmill terminal front end. No application-specific background daemon.
use crate::{
    download::{self, Kind},
    lyrics::Lyrics,
    mpris::{self, Control, Snapshot},
    player::{self, Engine, Playback, Status},
    store::{Media, Store},
    youtube::{self, Video},
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use std::{
    error::Error,
    fs,
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
}
impl Prompt {
    fn text(&self) -> &str {
        match self {
            Self::Search(t) | Self::Tag(t) | Self::Collection(t) | Self::Filter(t) => t,
        }
    }
    fn text_mut(&mut self) -> &mut String {
        match self {
            Self::Search(t) | Self::Tag(t) | Self::Collection(t) | Self::Filter(t) => t,
        }
    }
    fn label(&self) -> &str {
        match self {
            Self::Search(_) => "YouTube search",
            Self::Tag(_) => "Add tag",
            Self::Collection(_) => "Create & add collection",
            Self::Filter(_) => "Recommendation tags, comma-separated",
        }
    }
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
    lyrics: Option<Lyrics>,
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
    fn find_sidecar(&mut self, media: &Media) {
        self.lyrics = None;
        if media.source.starts_with("http://") || media.source.starts_with("https://") {
            return;
        }
        let path = Path::new(&media.source).with_extension("lrc");
        if let Ok(text) = fs::read_to_string(path) {
            let lyrics = Lyrics::parse(&text);
            if !lyrics.is_empty() {
                self.lyrics = Some(lyrics);
            }
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
                    self.find_sidecar(&media);
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
            KeyCode::Tab => self.tab = Tab::from_index(self.tab.index() + 1),
            KeyCode::BackTab => self.tab = Tab::from_index(self.tab.index() + 4),
            KeyCode::Char('1'..='5') => {
                if let KeyCode::Char(c) = key.code {
                    self.tab = Tab::from_index((c as u8 - b'1') as usize)
                }
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
        if self.last_tick.elapsed() >= Duration::from_millis(900) {
            self.elapsed_ms = self.player.elapsed_ms();
            self.mpd = player::mpd_status().ok();
            self.last_tick = Instant::now();
        }
    }
}
struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(Box::new(e));
        }
        Ok(Self)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
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
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && app.key(key)
        {
            break;
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
    frame.render_widget(Paragraph::new("1-5 tabs · j/k select · / search · p play · m MPD · d audio download · v video download · s save · t tag · c collect · space pause · q quit")
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
fn draw_playing(frame: &mut Frame, app: &App, left: Rect, right: Rect) {
    let snap = app.mpris_snapshot();
    let title = if snap.title.is_empty() {
        "Nothing playing"
    } else {
        &snap.title
    };
    let creator = &snap.artist;
    let engine = match app.player.engine {
        Some(Engine::Mpv) => "mpv",
        Some(Engine::Mpd) => "MPD",
        None => "MPD / mpv",
    };
    let state = snap.state;
    let elapsed = snap.position_us / 1000;
    let minutes = elapsed.max(0) / 60_000;
    let seconds = (elapsed.max(0) / 1_000) % 60;
    let text = format!(
        "{title}\\n\\n{creator}\\n\\nEngine: {engine}\\nTime: {minutes:02}:{seconds:02}\\nMPD: {state}\\n\\nSpace to pause/resume\\nx to stop · n for next\\n\\nLocal .lrc sidecars are automatically loaded."
    );
    frame.render_widget(
        Paragraph::new(text.replace("\\n", "\n"))
            .block(panel(" Now playing "))
            .wrap(Wrap { trim: false }),
        left,
    );
    let mut lyric_lines = Vec::new();
    if let Some(ref lyrics) = app.lyrics {
        let (active_index, window) = lyrics.window(elapsed, 4, 6);
        for (i, line) in window.iter().enumerate() {
            let style = if Some(i) == active_index {
                Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            lyric_lines.push(Line::from(Span::styled(
                format!(
                    "{} {}",
                    if Some(i) == active_index { "♫" } else { " " },
                    line.text
                ),
                style,
            )));
            lyric_lines.push(Line::raw(""));
        }
    } else {
        lyric_lines.push(Line::from("No timed lyrics available for this item."));
        lyric_lines.push(Line::from(
            "Add a matching .lrc alongside the local audio file.",
        ));
        lyric_lines.push(Line::from("LRCLIB fetch and word-level sync are planned."));
    }
    frame.render_widget(
        Paragraph::new(lyric_lines)
            .block(panel(" Synchronized lyrics "))
            .wrap(Wrap { trim: false }),
        right,
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
