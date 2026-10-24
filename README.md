# Windmill

**Windmill is a local-first personal media center**, not a YouTube client. It manages its own media catalog, custom tags, collections, discovery, recommendations and playback state. MPD handles music-library audio; mpv handles video and other sources.

## Current status: v0.2.0 — first interactive slice

The Rust terminal interface has five working views:

- **Library:** saved media, source information, offline availability, user tags.
- **Discover:** public YouTube search through yt-dlp, performed in a worker thread.
- **Collections:** any media entry can belong to arbitrarily many user-named collections.
- **For You:** transparent tag-matching ranks, plus editable tag filters.
- **Now Playing:** MPD status, mpv IPC playback position, synchronized local .lrc sidecars.

The CLI and TUI support user-initiated offline audio/video downloads. Downloaded files are linked to their original media entry as assets instead of duplicating it.

**Not finished:** no dedicated YouTube Music browsing backend, rich cover art, online lyrics fetching, word-level karaoke, unified queue, or persistent download-progress dashboard. Actual downloading and audible playback still need end-to-end testing.

## Run on the development workstation

~~~sh
cd ~/Documents/windmill
./scripts/dev
~~~

Or with Rust/Cargo on PATH:

~~~sh
cargo run --locked
~~~

The TUI starts by default and requires a real terminal. For the CLI run `windmill help⟧.

## Controls

| Key | Action |
|---|---|
| `1⟧–`5⟧, `Tab⟧, `Shift+Tab⟧ | Switch between five views |
| `j⟧ / `k⟧, arrows | Navigate |
| `/⟧ | Search public YouTube |
| `p⟧ | Play using mpv, prefer a downloaded local asset |
| `m⟧ | Play a local MPD-library file via MPD (Library) |
| `s⟧ | Save discovered media to catalog |
| `d⟧ / `v⟧ | Download audio / video for selected online media |
| `t⟧ | Tag a library item |
| `c⟧ | Create/add to a collection |
| `f⟧ | Filter For You with comma-separated tags |
| `Space⟧, `n⟧, `x⟧ | Pause/resume, next, stop |
| `r⟧ | Reload local catalog |
| `q⟧ | Quit (also closes mpv launched by Windmill) |

MPD must be running (`systemctl --user start mpd.service⟧). The configured MPD library root is `~/Music⟧. Files outside MPD's music directory remain playable using mpv.

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

Storage is `~/.local/share/windmill/⟧ (or `$XDG_DATA_HOME/windmill/⟧), including the SQLite database and managed downloads. Set `WINDMILL_DATA_DIR⟧ for isolated profiles and tests.

## Quality checks

~~~sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
~~~

See [architecture](docs/ARCHITECTURE.md), [roadmap](docs/ROADMAP.md), [user MPD setup](docs/MPD-SETUP.md), and [upstream references](docs/UPSTREAM.md).

No Google login is required for public discovery, but online activity is **not anonymous**. Downloads depend on provider availability and should only be used where permitted. Windmill never scrapes or downloads automatically on import.
