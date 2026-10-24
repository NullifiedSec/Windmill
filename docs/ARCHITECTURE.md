# Windmill architecture (v0.2)

Windmill owns the **catalog and user experience**. MPD and mpv remain unmodified playback engines. No upstream source is vendored.

## Components

- `src/store.rs`: SQLite media, tags, collections, assets and deterministic tag scoring. Collections do not map to physical directories.
- `src/ui.rs`: Ratatui interface with Library, Discover, Collections, For You and Now Playing views. Online search and downloads use worker channels.
- `src/youtube.rs`: public yt-dlp search, strict video ID validation and canonical YouTube URLs.
- `src/download.rs`: user-initiated audio/video downloads via subprocess argument arrays, output-path verification and managed downloads.
- `src/player.rs`: native MPD TCP protocol, mpv child management and IPC playback-position lookup; one active playback engine at a time.
- `src/seek.rs`: timestamp parsing, bounded seek targets, mouse progress geometry.
- `src/lyric_offset.rs`: input validation, signed offsets, and pure Tap Sync cue calculation from current player time and lyric timestamp.
- `src/lyrics.rs`: LRC and enhanced per-word timestamp parsing, document and user offsets.
- `src/lyric_source.rs`: local sidecars, private cache and verified LRCLIB lookup on a worker thread.
- `src/main.rs`: CLI commands and default TUI entrypoint.

## Catalog identity

A media entry is distinct from its sources. The entry has an original URL or local path, plus optional `assets` with downloaded audio or video paths. One entry can join many collections and many tags, independently of an Artist → Album → Track hierarchy.

Downloaded files are checked for existence before being preferred for mpv playback. Multiple recordings, playlists and cross-source canonicalization are future work.

## Safety / limitations

- No shell interpolation for online titles or URLs.
- MPD commands reject newlines and quote arguments.
- MPD library playback only supports files under its configured music root. Arbitrary catalog paths are not imported into MPD.
- Never auto-scrape or download on import.
- Local metadata is owned by Windmill, but network requests are not anonymous.
- Playback and downloads need additional genuine integration tests on real files.
- Mopytui uses Mopidy JSON-RPC, so Windmill has its own MPD adapter, not incompatible copied client code.

See `docs/UPSTREAM.md` for pinned upstream reference revisions and license guidance.


## Music-video lyrics identity

The lyric-source identity for an MPV item follows the catalog's original
URL, even when the actual playback file is a downloaded video. Existing
file-path offset values migrate to their stable source identity on first use.
Video duration does not exclude song-length LRCLIB matches: an intro or
outro may extend a music video without changing the audio recording.


## Guided Tap Sync

The TUI maintains a guided sync flag, a selected timed line, and one
in-memory undo entry. Space (or Enter) while in sync mode reads the live
player clock through the existing MPD/mpv adapter. Offset calculation is a
pure bounded operation: live clock minus line timestamp including the
LRC document offset. The corrected timing is persisted under the source
identity in SQLite, without seeking or pausing playback. Cancelling the
mode has no storage side effects. Plain-lyrics records cannot enter this
mode because no trustworthy timestamp is available.


## Timeline seeking

Terminal key and mouse events become absolute seeks through the existing
MPD or mpv playback adapter. The previous position is held in a
track-scoped, one-step undo. The progress bar's rectangle is calculated
using the same Ratatui layout constraints as the renderer. Backend
errors, including mpv JSON IPC errors, are surfaced to the user.
