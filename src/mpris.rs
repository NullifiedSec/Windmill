//! The session-bus MPRIS facade for the whole Windmill media center.
//! MPD and mpv are implementation details: only Windmill is exposed to playerctl.
//! D-Bus handlers enqueue commands; all player mutations happen on the TUI thread.
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{
        Arc, RwLock,
        mpsc::{self, Receiver, Sender},
    },
};
use zbus::{
    blocking::{Connection, connection::Builder},
    interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    Seek(i64),        // relative microseconds
    SetPosition(i64), // absolute microseconds
    Volume(f64),      // 0.0..1.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub state: &'static str,
    pub title: String,
    pub artist: String,
    pub source: String,
    pub position_us: i64,
    pub duration_us: Option<i64>,
    pub volume: f64,
    pub can_play: bool,
    pub can_seek: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: "Stopped",
            title: String::new(),
            artist: String::new(),
            source: String::new(),
            position_us: 0,
            duration_us: None,
            volume: 1.0,
            can_play: false,
            can_seek: false,
            can_go_next: false,
            can_go_previous: false,
        }
    }
}

impl Snapshot {
    fn track_id(&self) -> OwnedObjectPath {
        if self.source.is_empty() {
            return OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack")
                .expect("static DBus object path");
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.source.hash(&mut hasher);
        OwnedObjectPath::try_from(format!("/dev/windmill/track/t{:016x}", hasher.finish()))
            .expect("generated DBus object path")
    }

    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        if self.source.is_empty() {
            return map;
        }
        map.insert(
            "mpris:trackid".into(),
            OwnedValue::try_from(Value::from(self.track_id())).expect("owned DBus object path"),
        );
        map.insert("xesam:title".into(), string_value(&self.title));
        map.insert(
            "xesam:artist".into(),
            array_value(vec![self.artist.clone()]),
        );
        if let Some(length) = self.duration_us {
            map.insert("mpris:length".into(), OwnedValue::from(length));
        }
        // We don't guess album without actual metadata.
        map
    }
}

fn string_value(v: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(v.to_owned())).expect("owned DBus string")
}
fn array_value(v: Vec<String>) -> OwnedValue {
    OwnedValue::try_from(Value::new(v)).expect("owned DBus string array")
}

struct Root;
#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {} // TUI cannot meaningfully raise an existing terminal window.
    fn quit(&self) {} // CanQuit is false: never kill the user's terminal.
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Windmill"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        vec![]
    }
    #[zbus(property)]
    fn fullscreen(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_set_fullscreen(&self) -> bool {
        false
    }
}

struct Player {
    commands: Sender<Control>,
    snapshot: Arc<RwLock<Snapshot>>,
}
#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        let _ = self.commands.send(Control::Next);
    }
    fn previous(&self) {
        let _ = self.commands.send(Control::Previous);
    }
    fn pause(&self) {
        let _ = self.commands.send(Control::Pause);
    }
    fn play_pause(&self) {
        let _ = self.commands.send(Control::Toggle);
    }
    fn stop(&self) {
        let _ = self.commands.send(Control::Stop);
    }
    fn play(&self) {
        let _ = self.commands.send(Control::Play);
    }
    fn seek(&self, offset: i64) {
        let _ = self.commands.send(Control::Seek(offset));
    }
    fn set_position(&self, track_id: OwnedObjectPath, position: i64) {
        let current = self.snapshot.read().expect("snapshot lock");
        if current.can_seek && track_id == current.track_id() && position >= 0 {
            let _ = self.commands.send(Control::SetPosition(position));
        }
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.snapshot.read().expect("snapshot lock").state.into()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.snapshot.read().expect("snapshot lock").metadata()
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.snapshot.read().expect("snapshot lock").position_us
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.snapshot.read().expect("snapshot lock").volume
    }
    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        if volume.is_finite() {
            let _ = self.commands.send(Control::Volume(volume.clamp(0.0, 1.0)));
        }
    }
    #[zbus(property)]
    fn loop_status(&self) -> &str {
        "None"
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.snapshot.read().expect("snapshot lock").can_play
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.snapshot.read().expect("snapshot lock").can_play
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.snapshot.read().expect("snapshot lock").can_seek
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.snapshot.read().expect("snapshot lock").can_go_next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.snapshot.read().expect("snapshot lock").can_go_previous
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub struct Bridge {
    connection: Connection,
    state: Arc<RwLock<Snapshot>>,
    commands: Receiver<Control>,
}

impl Bridge {
    pub fn new() -> zbus::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(RwLock::new(Snapshot::default()));
        let connection = Builder::session()?
            .serve_at(PATH, Root)?
            .serve_at(
                PATH,
                Player {
                    commands: tx,
                    snapshot: state.clone(),
                },
            )?
            .name("org.mpris.MediaPlayer2.windmill")?
            .build()?;
        Ok(Self {
            connection,
            state,
            commands: rx,
        })
    }
    pub fn commands(&self) -> &Receiver<Control> {
        &self.commands
    }

    pub fn update(&self, next: Snapshot) -> zbus::Result<()> {
        let previous = {
            let mut s = self.state.write().expect("snapshot lock");
            std::mem::replace(&mut *s, next.clone())
        };
        let mut changed = HashMap::<&str, Value>::new();
        if previous.state != next.state {
            changed.insert("PlaybackStatus", Value::from(next.state));
        }
        if previous.source != next.source
            || previous.title != next.title
            || previous.artist != next.artist
            || previous.duration_us != next.duration_us
        {
            changed.insert("Metadata", Value::new(next.metadata()));
        }
        if (previous.volume - next.volume).abs() > 0.005 {
            changed.insert("Volume", Value::from(next.volume));
        }
        for (key, from, to) in [
            ("CanPlay", previous.can_play, next.can_play),
            ("CanPause", previous.can_play, next.can_play),
            ("CanSeek", previous.can_seek, next.can_seek),
            ("CanGoNext", previous.can_go_next, next.can_go_next),
            (
                "CanGoPrevious",
                previous.can_go_previous,
                next.can_go_previous,
            ),
        ] {
            if from != to {
                changed.insert(key, Value::from(to));
            }
        }
        if !changed.is_empty() {
            self.connection.emit_signal(
                None::<&str>,
                PATH,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(IFACE, changed, Vec::<String>::new()),
            )?;
        }
        Ok(())
    }
    pub fn seeked(&self, position_us: i64) -> zbus::Result<()> {
        self.connection
            .emit_signal(None::<&str>, PATH, IFACE, "Seeked", &(position_us,))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_and_ids_follow_media() {
        let mut s = Snapshot::default();
        assert!(s.metadata().is_empty());
        s.source = "https://example.com/watch".into();
        s.title = "Feel Good Inc.".into();
        s.artist = "Gorillaz".into();
        let id = s.track_id();
        assert!(id.as_str().starts_with("/dev/windmill/track/t"));
        assert_eq!(s.track_id(), id);
        assert!(s.metadata().contains_key("xesam:title"));
    }
}
