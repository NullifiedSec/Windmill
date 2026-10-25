# Windmill roadmap

## Completed first interactive vertical slice

- [x] Local Rust project, developer workflow, clean upstream reference clones.
- [x] SQLite media, custom tags, collections, linked offline audio/video assets.
- [x] CLI and deterministic tag ranking exposed in a For You TUI.
- [x] Ratatui Library, Discover, Collections, For You and Now Playing views.
- [x] Public YouTube search and manual save to catalog.
- [x] Public playlist preview, selective collection import and batch audio/video download.
- [x] Persistent MPD/mpv play queue with auto-next, reorder, shuffle and repeat.
- [x] Dedicated public YouTube Music song/album/artist/playlist discovery.
- [x] Cached truecolor artwork from thumbnails and local embedded covers.
- [x] Real audio-reactive output sink spectrum without microphone capture.
- [x] Local ratings, listen/skip counts and explainable recommendations.
- [x] Explicit yt-dlp audio/video downloads with background worker.
- [x] Native MPD status/control and local library playback; mpv with IPC time-position.
- [x] Local .lrc synchronized lyrics, private LRCLIB cache, manual correction, plain fallback and enhanced LRC word highlighting.
- [x] Precise per-video lyrics offset editor, stream/offline identity and music-video intro/outro duration handling.
- [x] Guided Tap Sync workflow: select timestamped lyric, mark current player position without pausing, persisted shift and undo.
- [x] Manual LRCLIB track URL selection, persistent overrides, offline caching and non-destructive lyric timeline padding.
- [x] Tap Sync transport: p pauses and resumes both engines without exiting lyric marking mode.
- [x] Playback navigation: quick skips, direct timeline jumps, clickable progress bar and undo.
- [x] Single MPRIS identity for MPD and mpv, including playerctl controls, metadata, volume and seek.
- [x] Rust unit tests and pseudo-terminal rendering/search/save tests.

## Next vertical slices

1. **Playback reliability:** resume, queue failure handling, and audible real-device tests.
2. **YouTube Music:** metadata depth, upstream resilience and alternative providers.
3. **Downloader:** progress, cancellation, resuming, file naming, codecs, live download validation.
4. **Rich lyrics/artwork:** manual source correction and lyric/art result choice.
5. **Media imports:** recursive scan, file monitoring, MPD library reconciliation and custom virtual groups.
6. **Recommend:** diversity and exploration.
7. **Hardening:** SQLite migrations, permissions, portability and error-reporting integration tests.

Do not call the project finished until full playback, download and source integration are verified.
