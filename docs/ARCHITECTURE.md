# Windmill architecture (v0.2)

Windmill owns the **catalog and user experience**. MPD and mpv remain unmodified playback engines. No upstream source is vendored.

## Components

- `src/store.rs`: SQLite media, tags, collections, assets and deterministic tag scoring. Collections do not map to physical directories.
- `src/ui.rs`: Ratatui interface with Library, Discover, Collections, For You and Now Playing views. Online search and downloads use worker channels.
- `src/youtube.rs`: public yt-dlp search, strict video ID validation and canonical YouTube URLs.
- `src/download.rs`: user-initiated audio/video downloads via subprocess argument arrays, output-path verification and managed downloads.
- `src/player.rs`: native MPD TCP protocol, mpv child management and IPC playback-position lookup; one active playback engine at a time.
- `src/lyrics.rs`: basic line-timed LRC parser with offsets and active-line lookup; local sidecar support.
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
