# Windmill

**Windmill is a local-first personal media center**, not a YouTube client. It manages its own media catalog, custom tags, collections, discovery, recommendations and playback state. MPD handles music-library audio; mpv handles video and other sources.

## Current status: v0.6.0 — video durations, native terminal image covers and CAVA visualization

The Rust terminal interface has six working views:

- **Library:** saved media, source information, offline availability, user tags.
- **Discover:** public YouTube discovery, dedicated YouTube Music search (songs/albums/artists/playlists) and selectable public playlist downloads.
- **Collections:** any media entry can belong to arbitrarily many user-named collections.
- **For You:** explainable local ranking from tags, likes/dislikes, listening history and quick skips.
- **Now Playing:** synchronized lyrics, Tap Sync, offline lyric cache, progress, cached truecolor artwork and real audio-reactive output spectrum.
- **Queue:** persistent MPD/mpv song queue with continuous playback, reorder, shuffle and repeat.
- **MPRIS:** exposes Windmill to desktop media keys/playerctl, regardless of playback engine.

The CLI and TUI support user-initiated offline audio/video downloads. Downloaded files are linked to their original media entry as assets instead of duplicating it.

**Not finished:** resumable/cancellable batch jobs, local media scanning, more robust queue error recovery, and fully audible output-device testing. YouTube Music lookup uses an optional isolated public API bridge.

## Install for everyday use (Arch Linux)

The standard development checkout is `~/Documents/windmill`. To install
the tested release under your own account (no root or system packages):

~~~sh
cd ~/Documents/windmill
./scripts/setup-ytmusic        # optional, public YouTube Music browsing
./scripts/install-user         # optimized Rust binary in ~/.local/bin
windmill                       # starts the TUI
~~~

Artwork is fetched and cached automatically when a song is playing;
the speaker-output spectrum starts automatically when playback is active.
Press `V` in Now Playing to toggle it. It observes the current default
speaker monitor, **not a microphone**. Set
`WINDMILL_DISABLE_VISUALIZER=1` if you want to disable any real PCM
monitor capture entirely.

Data is stored separately in `~/.local/share/windmill`, and upgrading
the executable does not delete your media, lyric offsets or collections.
Back up the SQLite library before major upgrades. If your user-level
MPD service is stopped, run `systemctl --user start mpd.service`.
Run `./scripts/dev` to compile and launch directly from the repository.

## Run on the development workstation

~~~sh
cd ~/Documents/windmill-expansion
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
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab using CSI-u/Kitty keyboard protocol |
| `1`–`6` (Now Playing) | Hide/show artwork, spectrum, lyrics, track info, progress, controls |
| `1` / `2` (other tabs) | Hide/show list or details |
| `j` / `k`, arrows | Navigate |
| `/` | Search public YouTube |
| `P` | Preview a public playlist (Discover/Collections) |
| `Y` | Search public YouTube Music (songs, albums, artists, playlists) |
| `6` | Open persistent Queue tab |
| `V` (Now Playing) | Toggle the real-time speaker-output spectrum |
| `e` | Enqueue current media |
| `Q` | In Collections, replace queue with the selected collection |
| `n` / `b` | Next / previous queued track |
| `<` / `>` | Move selected Queue entry up / down |
| `S` / `R` | Shuffle queue / toggle repeat |
| `+` / `-` / `=` | Like / dislike / reset saved-song rating |
| `a` / `A` | Toggle one / select all playlist tracks in preview |
| `I` | Import selected playlist tracks into a Windmill collection |
| `B` / `V` | Explicit batch download of selected audio / video |
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
| `p` | Play/pause in Now Playing, including during Tap Sync (Space remains the lyric marker) |
| `Space` / `Enter` | In Tap Sync mode, mark the moment you hear the selected lyric |
| `Esc` | Cancel Tap Sync without changing the offset |
| `u` | Undo the most recent successful Tap Sync mark |
| `o` | Set exact signed lyrics offset (e.g. `+12.5` or `-00:03.250`) |
| `M` (Now Playing) | Paste a specific LRCLIB track page link |
| `U` (Now Playing) | Remove the pinned link; use automatic/local lyrics |
| `[` / `]` | Lyrics earlier/later in 250 ms steps (Now Playing) |
| `0` | Reset lyric offset (Now Playing) |
| `j/k`, `Enter` | Browse lyric lines and seek to selected timed line (Now Playing) |
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

**If playback is paused while setting up Tap Sync, press `p` to resume.**
`p` toggles playback in normal Now Playing and Tap Sync modes;
`Space` stays reserved for marking the cue while Tap Sync is active.
This avoids interrupting playback when capturing the exact moment.

No need to estimate intro lengths. For finer control, `[` / `]` still
adjust the result by 250 ms. This requires synchronized LRC/lyrics, not
plain untimed text.

### Manually pick LRCLIB lyrics, even if the durations differ

If lyrics are missing or the wrong version was selected, press `M`
in Now Playing and paste `https://lrclib.net/tracks/36174111`.
Windmill fetches that record by ID, bypassing exact artist/title and
duration matching, and remembers it for that recording. Offline cached
lyrics work on downloaded copies too. `U` unpins it and returns to
automatic search and local sidecar files.

The lyric **display** has blank intro and instrumental/outro regions.
If the chosen LRCLIB transcript is longer than the playing recording,
Windmill indicates a *virtual lyric tail* beyond the real file.
This does **not** extend or alter the actual music. Use Tap Sync or
the offset editor to line up the words. See [lyrics guide](docs/LYRICS.md).

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

No extra root packages were needed on the development machine. The bus name only exists **while Windmill TUI is running**; headless control is future work. Queue navigation and shuffle/repeat are supported in Windmill. With multiple media players installed, specify `-p windmill`.

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


## Public playlists

From Discover, press `P` to preview a public YouTube playlist.
Select entries with `a`/`A` and import with `I` or explicitly
download with `B`/`V`. Nothing downloads during preview/import.
The CLI supports `windmill playlist preview/import/download`.
See [playlist guide](docs/PLAYLISTS.md).


## v0.5 expansion

Run `./scripts/setup-ytmusic` in the expansion worktree to install the
optional YouTube Music dependency into its own private Python venv.
Press `Y` in Discover, search `songs:Counting Stars` or `albums:Gorillaz`,
and press Enter to browse an album or artist.

The Queue tab (`6`) enables continuous MPD/mpv playback. Artwork is cached
and drawn with real terminal pixels; the spectrum processes PCM from the
default output sink monitor, not the microphone. Rate saved songs with
`+`, `-` or `=` to influence purely local recommendations.

See [full expansion guide](docs/EXPANSION.md).


## v0.6: Actual terminal graphics and fast CAVA spectrum

- **Duration before download:** YouTube search and playlist preview show
  each track's duration in `m:ss`/`h:mm:ss` when provided by YouTube.
  In Discover the selected item's details also display its duration.
  If a provider omits duration, Windmill explicitly says `unknown` instead
  of inventing it. Nothing downloads during preview.
- **Real album images:** Windmill uses `ratatui-image` and terminal graphics
  protocols (Ghostty uses Kitty graphics) to display the cached cover as
  an image rather than converting to unicode pixel mosaics. The encoding
  happens in a worker thread, not on every redraw. Terminals without image
  support may still use a fallback, and terminal graphics should be
  visually inspected in your own terminal.
- **Fast spectrum:** CAVA computes the frequency bars outside Windmill;
  the TUI reads 8-bit heights and draws smooth 8-step vertical glyphs
  with a 40 FPS CAVA source (terminal refresh is roughly 28 FPS). Unlike the older implementation, Windmill does not
  recompute slow per-frequency FFT/Goertzel windows itself.

Install CAVA once on Arch Linux (requires your own sudo approval):

~~~sh
sudo pacman -S cava
~~~

Then launch `windmill` and press `V` in Now Playing to enable or disable
the CAVA spectrum. CAVA is optional: Windmill still plays music if it
isn't installed, but the spectrum panel remains unavailable and a
status message explains why. Audio capture is from the default speaker
output monitor, which also reflects other apps playing on that sink,
not the microphone. No audio is stored. For test or privacy use,
set `WINDMILL_DISABLE_VISUALIZER=1`. A separate
`WINDMILL_DISABLE_ARTWORK=1` disables the graphics protocol probe.


## Panel visibility and enhanced keyboard navigation

Windmill uses Kitty keyboard protocol / CSI-u (via Crossterm keyboard
enhancements) in modern terminals such as Ghostty, allowing modified Tab
combinations to reach the application. Press `Ctrl+Tab` for the next
tab and `Ctrl+Shift+Tab` for the previous tab. Bare Tab does not switch
tabs. Your terminal must pass the key combination to Windmill; customize
terminal shortcuts if it intercepts Ctrl+Tab before applications see it.

**In Now Playing**, number keys toggle the associated panel:

- `1` artwork
- `2` CAVA spectrum (hiding it also stops CAVA capture)
- `3` lyrics
- `4` track information
- `5` playback progress bar
- `6` lyric/Tap Sync control instructions

In Library, Discover, Collections, For You and Queue, `1` toggles
the item list and `2` the information/details pane. When one pane
disappears, the other takes the available width. The toggles are independent per tab and session-local; next launch
begins with everything visible.
These shortcuts never pause playback or modify the queue/lyrics.
