# Windmill

**Windmill is a local-first personal media center**, not a YouTube client. It manages its own media catalog, custom tags, collections, discovery, recommendations and playback state. MPD handles music-library audio; mpv handles video and other sources.

## Current status: v0.3.3 — easy timeline seeking and one-tap lyric sync

The Rust terminal interface has five working views:

- **Library:** saved media, source information, offline availability, user tags.
- **Discover:** public YouTube search through yt-dlp, performed in a worker thread.
- **Collections:** any media entry can belong to arbitrarily many user-named collections.
- **For You:** transparent tag-matching ranks, plus editable tag filters.
- **Now Playing:** centered synchronized lyrics, live LRCLIB lookup and offline cache, playback progress, enhanced LRC highlighting and persistent timing correction.
- **MPRIS:** exposes Windmill to desktop media keys/playerctl, regardless of playback engine.

The CLI and TUI support user-initiated offline audio/video downloads. Downloaded files are linked to their original media entry as assets instead of duplicating it.

**Not finished:** no dedicated YouTube Music browsing backend, rich cover art, unified queue, or persistent download-progress dashboard. Actual downloading and audible playback still need end-to-end testing.

## Run on the development workstation

~~~sh
cd ~/Documents/windmill
./scripts/dev
~~~

Or with Rust/Cargo on PATH:

~~~sh
cargo run --locked
~~~

The TUI starts by default and requires a real terminal. For the CLI run `windmill help`.

## Controls

| Key | Action |
|---|---|
| `1`–`5`, `Tab`, `Shift+Tab` | Switch between five views |
| `j` / `k`, arrows | Navigate |
| `/` | Search public YouTube |
| `p` | Play using mpv, prefer a downloaded local asset |
| `m` | Play a local MPD-library file via MPD (Library) |
| `s` | Save discovered media to catalog |
| `d` / `v` | Download audio / video for selected online media |
| `t` | Tag a library item |
| `c` | Create/add to a collection |
| `f` | Filter For You with comma-separated tags |
| `←` / `→` | Skip backward/forward 5 seconds |
| `Shift+←` / `Shift+→` | Skip backward/forward 30 seconds |
| `Ctrl+←` / `Ctrl+→` (or PageUp/PageDown) | Skip backward/forward 60 seconds |
| `g` | Jump to a specific time (e.g. `1:23.5` or `83.5`) |
| `Home` / `End` | Jump to start / near end |
| `z` | Undo the last skip or progress-bar click |
| Mouse click on Now Playing progress bar | Seek to the chosen position |
| `Space`, `n`, `x` | Pause/resume, next, stop |
| `r` | Reload local catalog |
| `q` | Quit (also closes mpv launched by Windmill) |
| `s` | Start/exit guided Tap Sync (Now Playing) |
| `Space` / `Enter` | In Tap Sync mode, mark the moment you hear the selected lyric |
| `Esc` | Cancel Tap Sync without changing the offset |
| `u` | Undo the most recent successful Tap Sync mark |
| `o` | Set exact signed lyrics offset (e.g. `+12.5` or `-00:03.250`) |
| `[` / `]` | Lyrics earlier/later in 250 ms steps (Now Playing) |
| `0` | Reset lyric offset (Now Playing) |
| `j/k`, Enter | Browse lyric lines and seek to selected timed line (Now Playing) |
| `a` / `L` | Auto-follow / refresh LRCLIB lookup (Now Playing) |

MPD must be running (`systemctl --user start mpd.service`). The configured MPD library root is `~/Music`. Files outside MPD's music directory remain playable using mpv.

### Replay and skip sections

Press **Left/Right** for five-second skips, Shift+arrows for 30 seconds,
or Ctrl+arrows for 60 seconds. Press `g` and enter `2:15` (or `135`) to
jump directly. In **Now Playing**, click on the progress bar to seek.

**Overshot? Press `z` to undo** the last jump and return to your
previous playback position. The keyboard shortcuts work while Windmill's
terminal has focus; mouse seeking needs a known track duration.
In Tap Sync mode, Space remains a lyric marker rather than pause.

### Tap to Sync — find the offset by ear

Play a song or music video, open Now Playing (`5`), and press `s` to enter
**Tap Sync**. Windmill highlights the first sung lyric by default; use `j/k`
to pick any unmistakable lyric line. **Press Space when you hear that line
start**. This marks the current live MPD/mpv playback position as the timestamp
for that lyric, calculates the exact offset, and persists it. Playback
continues during marking. `u` restores your previous offset; `Esc` cancels
without changing it.

No need to estimate intro lengths. For finer control, `[` / `]` still
adjust the result by 250 ms. This requires synchronized LRC/lyrics, not
plain untimed text.

### Music-video lyric timing

For music videos with an intro, enter **Now Playing** (press `5`),
press `o`, then type `+12.5` (seconds) or `+00:12.500` (minutes:seconds)
to delay the lyrics by 12.5 seconds. A negative value like `-3.25`
makes them appear earlier. `[` / `]` fine-tune; `0` resets.

Offsets persist per original media URL, even when playing the downloaded copy.
Because intros/outros make videos longer than the actual song, matching
lyrics for videos doesn't require the same recording duration.

## Desktop media controls (playerctl)

While the TUI is open, Windmill registers `org.mpris.MediaPlayer2.windmill` on the user session D-Bus. It uses one playback coordinator for MPD and mpv.

~~~sh
playerctl -p windmill status
playerctl -p windmill play-pause
playerctl -p windmill pause
playerctl -p windmill play
playerctl -p windmill stop
playerctl -p windmill next
playerctl -p windmill previous
playerctl -p windmill position 30
playerctl -p windmill volume 0.5
playerctl -p windmill metadata
~~~

No extra root packages were needed on the development machine. The bus name only exists **while Windmill TUI is running**; headless control is future work. Loop/shuffle commands and global queues are not yet implemented. With several players installed, specify `-p windmill`.

See [MPRIS integration guide](docs/MPRIS.md).

## CLI examples

~~~sh
windmill add /path/to/file.flac "Song" "Artist"
windmill add https://www.youtube.com/watch?v=HyHNuVaZJ-k "Feel Good Inc." "Gorillaz"
windmill list
windmill tag 1 peaceful
windmill collection create "The Windmill"
windmill collection add "The Windmill" 1
windmill collection show "The Windmill"
windmill recommend peaceful,melancholic
windmill lyrics /path/to/song.lrc 90500
windmill mpd status
windmill download 1 audio
windmill download 1 video
~~~

Storage is `~/.local/share/windmill/` (or `$XDG_DATA_HOME/windmill/`), including the SQLite database and managed downloads. Set `WINDMILL_DATA_DIR` for isolated profiles and tests.

## Quality checks

~~~sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
~~~

See [lyrics](docs/LYRICS.md), [architecture](docs/ARCHITECTURE.md), [roadmap](docs/ROADMAP.md), [user MPD setup](docs/MPD-SETUP.md), and [upstream references](docs/UPSTREAM.md).

No Google login is required for public discovery, but online activity is **not anonymous**. Downloads depend on provider availability and should only be used where permitted. Windmill never scrapes or downloads automatically on import.
