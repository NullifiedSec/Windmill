# Windmill roadmap

## Completed first interactive vertical slice

- [x] Local Rust project, developer workflow, clean upstream reference clones.
- [x] SQLite media, custom tags, collections, linked offline audio/video assets.
- [x] CLI and deterministic tag ranking exposed in a For You TUI.
- [x] Ratatui Library, Discover, Collections, For You and Now Playing views.
- [x] Public YouTube search and manual save to catalog.
- [x] Explicit yt-dlp audio/video downloads with background worker.
- [x] Native MPD status/control and local library playback; mpv with IPC time-position.
- [x] Local .lrc timestamped line highlighting.
- [x] Rust unit tests and pseudo-terminal rendering/search/save tests.

## Next vertical slices

1. **Playback reliability:** coordinated queues, stop/pause, playlist integration, resume, audible playback tests.
2. **YouTube Music:** artist, track, album and playlist search, source metadata and identifiers.
3. **Downloader:** progress, cancellation, resuming, file naming, codecs, live download validation.
4. **Rich lyrics/artwork:** cover rendering, LRCLIB matching/cache, lyric offset editing, word timing.
5. **Media imports:** recursive scan, file monitoring, MPD library reconciliation and custom virtual groups.
6. **Recommend:** richer tag weights, negative feedback, diversity and exploration.
7. **Hardening:** SQLite migrations, permissions, portability and error-reporting integration tests.

Do not call the project finished until full playback, download and source integration are verified.
