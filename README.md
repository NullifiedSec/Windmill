# Windmill

**Your music. Your terminal. Your rules.**

Windmill is a **local-first music and media center** written in Rust with Ratatui, MPD, and mpv. Browse and organize songs, stream and download media you are allowed to save, synchronize lyrics, and build your own music dashboard—all from the terminal.

No Windmill account, no mandatory cloud backend. Your media catalog, tags, playlists, ratings, and lyric adjustments are stored locally.

**Version:** 0.6.0 — early Linux-first release, primarily tested on Arch Linux with PipeWire and Ghostty. Expect rough edges; see [limitations](#limitations).

## Highlights

| Feature | What Windmill does |
| --- | --- |
| Library and collections | SQLite-backed catalog with tags, custom collections, and separate offline assets |
| Discover | Public YouTube video and playlist search via yt-dlp, and optional YouTube Music discovery |
| Download | Explicit audio/video downloads; playlist preview and track durations before download when available |
| Playback | MPD for files in the music root, mpv for other media, persistent queue with shuffle and repeat |
| Lyrics | Local LRC, LRCLIB, word highlighting, manual LRCLIB track links, per-recording offsets and ear-guided Tap Sync |
| Artwork | Cached covers through terminal-native image graphics such as Kitty/Sixel/iTerm2 when supported |
| Spectrum | Optional CAVA-powered audio-reactive output-sink visualizer |
| Recommendations | Explainable local ranking from tags, ratings and listening statistics |
| Desktop controls | MPRIS integration with media keys and playerctl |

Six views: **Library**, **Discover**, **Collections**, **For You**, **Now Playing**, and **Queue**. You can hide individual panels to focus on artwork, lyrics, or playback information.

## Requirements

- Linux with a modern terminal; Ghostty/Kitty-style keyboard protocols are recommended for modified Tab and image support.
- A Rust toolchain supporting edition 2024, including Cargo.
- `mpv`, `mpd`, `yt-dlp`, `ffmpeg` and `curl` for the corresponding playback, download, and lookup features.
- Optional: `cava` for the spectrum, `playerctl` for media commands, Python 3 plus venv/pip for public YouTube Music browsing.

On Arch Linux:

~~~bash
sudo pacman -S --needed rust mpv mpd mpc yt-dlp ffmpeg curl cava python playerctl
~~~

Use your distribution's packages otherwise. CAVA, playerctl, and the YouTube Music bridge are optional. MPD requires a user daemon/configuration for MPD playback; mpv works separately.

## Installation

~~~bash
git clone https://github.com/NullifiedSec/Windmill.git
cd Windmill

# Build an optimized binary and install in ~/.local/bin (no sudo).
./scripts/install-user

# Optional public YouTube Music lookup in an isolated Python environment:
./scripts/setup-ytmusic

# If MPD is configured, start it:
systemctl --user start mpd.service

windmill
~~~

Ensure `~/.local/bin` is on your `PATH`. Alternatively run `./scripts/dev` or `cargo run --locked` from the repo. See [MPD setup](docs/MPD-SETUP.md) to configure the default music root at `~/Music`.

The optional YouTube Music Python bridge currently uses the build checkout's bundled script/venv; keep the checkout if you want that feature.

## Keyboard quick reference

**Ctrl+Tab** selects the next view; **Ctrl+Shift+Tab** selects the previous one, using Kitty/CSI-u keyboard enhancements. If the terminal reserves those key combinations, configure it to pass them through.

| Context | Keys | Action |
| --- | --- | --- |
| Now Playing | `1`–`6` | Show/hide artwork, CAVA spectrum, lyrics, track info, progress, and controls |
| Other views | `1` / `2` | Show/hide item list and details |
| Everywhere | `j` / `k`, arrows | Navigate |
| Discover | `/` / `Y` / `P` | YouTube search / YouTube Music search / public playlist preview |
| Playlist preview | `a` / `A` / `I` / `B` / `V` | Select one / all / import / download audio / download video |
| Library, Discover, For You | `e` | Add track to queue |
| Collections | `Q` | Queue selected collection |
| Queue | `Enter`, `n` / `b`, `S`, `R` | Play, next/previous, shuffle, repeat |
| Now Playing | `p`, `x`, `V` | Play/pause, stop, toggle visualizer |
| Now Playing | `s`, `o`, `[` / `]` | Tap Sync, precise lyrics offset, timing nudge |
| Now Playing | `M` / `U` / `L` | Pin LRCLIB track URL / unpin / refresh |
| Now Playing | Left/Right, `g`, `z` | Seek ±5 seconds, jump to time, undo seek |
| Everywhere | `q` | Quit |

Controls are context-sensitive. In Tap Sync, Space/Enter mark a lyric time rather than toggle playback; press `p` to play/pause. Details: [lyrics](docs/LYRICS.md), [seeking](docs/SEEKING.md).

## Examples

~~~bash
windmill list
windmill add "/home/me/Music/song.flac" "My Song" "An Artist"
windmill tag 1 dreamy
windmill collection create "Late Night"
windmill collection add "Late Night" 1
windmill queue collection "Late Night"

windmill playlist preview "https://www.youtube.com/playlist?list=..."
windmill playlist import "https://www.youtube.com/playlist?list=..."
windmill playlist download "https://www.youtube.com/playlist?list=..." audio

windmill music search songs "Feel Good Inc"
playerctl -p windmill play-pause
~~~

Previews/imports do not automatically download media. Third-party provider responses can be unavailable or change without notice. Download content only with permission.

## Data, privacy, and configuration

Windmill stores its catalog, offline media assets, cover cache and lyrics cache under `$XDG_DATA_HOME/windmill` (normally `~/.local/share/windmill`). Set `WINDMILL_DATA_DIR` for a separate profile, `WINDMILL_MUSIC_ROOT` to change the MPD music root, or `WINDMILL_MPD_ADDR` to change the MPD server (default `127.0.0.1:6600`).

The optional spectrum captures **the default speaker output sink monitor**, which may also include audio from other programs on that sink. It does not deliberately capture the microphone or write PCM samples to disk. Disable it with `WINDMILL_DISABLE_VISUALIZER=1`. Use `WINDMILL_DISABLE_ARTWORK=1` to skip terminal graphics detection.

Windmill doesn't host a central profile or tracking server. Online discovery, playback, LRCLIB, and artwork nevertheless contact third-party services; online requests are not anonymous.

## Limitations

This is an early source-based release, not a universal packaged desktop app. Interrupted download recovery/cancellation, automatic folder scanning, robust queue error skipping, cross-terminal graphics reliability, and real-device audible testing need more work. CAVA must be installed separately. The unofficial public YouTube Music bridge depends on an API that may change. No prebuilt binaries are published yet.

## Documentation

- [Developer guide](docs/DEVELOPMENT.md): build, test, architecture, workflows
- [Contributing](CONTRIBUTING.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Lyrics / Tap Sync](docs/LYRICS.md)
- [Playlist support](docs/PLAYLISTS.md)
- [MPD setup](docs/MPD-SETUP.md) · [MPRIS](docs/MPRIS.md) · [Seeking](docs/SEEKING.md)
- [Roadmap](docs/ROADMAP.md)

## License

**GNU GPL v3 or later** ([full license](LICENSE), SPDX: GPL-3.0-or-later).

**Original author: Mashrur Rahman Rawnok (NullifiedSec)**.
Please preserve the original attribution in [NOTICE](NOTICE) and applicable
copyright notices when redistributing Windmill or GPL-covered derivatives.
GPL also requires covered distributed derivatives to provide corresponding
source code and identify modifications. Crediting Windmill visibly in an
About page or README is appreciated, though the GPL does not always
require a visible credit screen.

Earlier published MIT versions remain usable under their original terms.
Third-party dependencies retain their own licenses.
