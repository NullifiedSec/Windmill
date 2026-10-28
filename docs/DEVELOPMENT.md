# Windmill Developer Guide

This guide is for contributors working on the Rust TUI, playback integrations, media catalog, online discovery, lyrics, and terminal graphics.

## Development requirements

Use a current stable Rust toolchain supporting edition 2024. The project is Linux-first; tests expect `mpv`, `ffmpeg` and Python 3. Optional integration components include `mpd`, `playerctl`, `yt-dlp`, `curl`, `cava`, D-Bus, and the public YouTube Music Python bridge. No Docker or root privileges are required to build Rust code.

~~~bash
git clone https://github.com/NullifiedSec/Windmill.git
cd Windmill
cargo build --locked
./scripts/dev
~~~

The app requires a real terminal for the TUI. Use `cargo run --locked -- help` to inspect CLI commands without launching a terminal UI. The installer (`./scripts/install-user`) compiles an optimized build and atomically installs to `~/.local/bin/windmill`.

For development without touching real user data:

~~~bash
export WINDMILL_DATA_DIR="$(mktemp -d)"
export WINDMILL_DISABLE_VISUALIZER=1
export WINDMILL_DISABLE_ARTWORK=1
cargo run --locked -- list
~~~

Delete only the temporary directory you created, not your regular library.

## Code map

| Module | Responsibilities |
| --- | --- |
| `src/main.rs` | Program entry, CLI, profile directory resolution |
| `src/ui.rs` | Ratatui state, terminal events, worker channels, views, controls |
| `src/store.rs` | SQLite tables, migrations, collections, queue, ratings, statistics |
| `src/player.rs` | MPD protocol, mpv child and IPC, engine selection and control |
| `src/mpris.rs` | Desktop D-Bus MPRIS service and media commands |
| `src/youtube.rs` / `src/playlist.rs` | yt-dlp discovery, validated video IDs, public playlist metadata |
| `src/download.rs` | Explicit downloads and managed asset paths |
| `src/ytmusic.rs` / `scripts/ytmusic_bridge.py` | Optional, public YouTube Music lookup via isolated Python |
| `src/lyrics.rs` / `src/lyric_source.rs` / `src/lyric_offset.rs` | LRC parsing, LRCLIB matching, manual selection and Tap Sync |
| `src/seek.rs` | Seek parsing and bounds |
| `src/art.rs` | Verified cover URLs, image download/cache; UI uses ratatui-image protocol encoding |
| `src/visualizer.rs` | External CAVA process with 8-bit raw spectrum frames |

The SQLite catalog owns logical media identities; downloading adds an asset to a media entry rather than a duplicate entry. The queue stores ordered media references. Manual lyric pins and timing offsets belong to the original media source so downloaded copies inherit corrections.

## Development principles

- Keep application-owned data local; keep network requests opt-in and provider-specific.
- Use bounded subprocess arguments, never shell interpolation with untrusted media titles/URLs.
- Validate and canonicalize URLs and IDs before constructing provider requests.
- Keep heavy work off the UI loop: discovery, artwork, downloads, and network lyric fetches run on workers.
- Preserve offline-first behavior where possible and graceful fallback when CAVA, Python, or provider APIs are absent.
- Respect the user's audio: test with disposable profiles, silent mpv output, and fake CAVA/provider fixtures.

## Validation

Required Rust checks:

~~~bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked
~~~

Isolated end-to-end checks (Linux; install Python 3, ffmpeg, mpv, and D-Bus utilities):

~~~bash
python3 tests/playlist_integration.py
python3 tests/manual_lrclib_integration.py
python3 tests/lyrics_integration.py
python3 tests/video_lyrics_offset_integration.py
dbus-run-session -- python3 tests/queue_integration.py
python3 tests/visualizer_integration.py
dbus-run-session -- python3 tests/mpris_integration.py
~~~

The integration scripts start disposable TUI sessions, mock providers, and use silent audio/video fixtures. `tests/README.md` has individual test details. Two opt-in live-network checks are excluded from default test runs; upstream API errors do not mean offline functionality is broken.

## Optional manual checks

- On a Ghostty/Kitty-compatible terminal, confirm that terminal-native artwork displays without ghost images or flicker. PTY tests cannot verify pixels from terminal graphic protocols.
- Press Ctrl+Tab and Ctrl+Shift+Tab; a terminal shortcut may intercept these combinations before the app sees CSI-u input.
- Install `cava` and verify the spectrum reacts to sound through the default playback sink monitor. Turning off the panel must stop the capture process.
- Test MPD playback from its configured music root and mpv playback of an external media file.
- Check manual LRCLIB pins offline, including differing lyric/video durations.
- Do not manually download or test media without permission.

## Configuration and data

The default library is `~/.local/share/windmill/library.db` or `$XDG_DATA_HOME/windmill/library.db`. Relevant variables:

| Variable | Purpose |
| --- | --- |
| `WINDMILL_DATA_DIR` | Override all Windmill-owned local profile data |
| `WINDMILL_MUSIC_ROOT` | MPD library-root path; default `~/Music` |
| `WINDMILL_MPD_ADDR` | MPD TCP address; default `127.0.0.1:6600` |
| `WINDMILL_YTMUSIC_PYTHON` | Use a specific Python executable with ytmusicapi installed |
| `WINDMILL_DISABLE_VISUALIZER` | Skip live sink-monitor capture |
| `WINDMILL_DISABLE_ARTWORK` | Skip terminal graphics capability probe |

Before changing database migrations, back up the user's data. Changes should remain compatible with existing catalogs and never remove downloaded files implicitly.

## Pull requests

Open an issue for nontrivial features, use descriptive branches and small focused commits, document behavior changes, and add Rust or isolated integration coverage. Don't include recorded media, playback databases, OAuth cookies, API keys, private filesystem paths, or proprietary code. Follow [CONTRIBUTING.md](../CONTRIBUTING.md).

Related deeper docs: [architecture](ARCHITECTURE.md), [lyrics](LYRICS.md), [public playlists](PLAYLISTS.md), [MPD](MPD-SETUP.md), [MPRIS](MPRIS.md), and [roadmap](ROADMAP.md).
