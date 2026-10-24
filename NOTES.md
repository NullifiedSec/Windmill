2026-10-25 - project - Initialized Windmill in Documents - local-first media center foundation, no upstream fork
2026-10-25 - src - Implemented local catalog and CLI, deterministic recommender, integrations - first verifiable vertical slice
2026-10-25 - docs - Recorded architecture, scope and roadmap - preserve intent and non-obvious decisions
2026-10-25 - src/lyrics.rs, src/main.rs - Added LRC parser and timestamped lyric preview - foundation for player-neutral synchronized lyrics
2026-10-25 - docs/MPD-SETUP.md - Documented user-scoped MPD/PipeWire configuration and localhost endpoint - reproducible integration without system service
2026-10-25 - upstream/, docs/UPSTREAM.md, .gitignore, README.md - Added isolated shallow reference clones and pinned revisions - inspect integrations without vendoring upstream or modifying host packages
2026-10-25 - feature/windmill-tui-playback - Added ratatui UI, MPD/mpv adapters, public YouTube discovery and linked offline assets - interactive vertical slice; remaining limitations in docs
2026-10-25 - feature/windmill-mpris - Added one MPRIS D-Bus player with playerctl forwarding to MPD/mpv, metadata, volume, seeking, and silent end-to-end tests - no system package changes or upstream forks
2026-10-25 - feature/windmill-lyrics - Added LRCLIB auto lookup/private cache, enhanced LRC highlighting, offset controls and lyric navigation - tested live provider and silent TUI
2026-10-25 - feature/lyrics-video-offset - Implemented precise video lyric shift via o prompt, shared online/offline identity, previous offset migration and music-video runtime mismatch handling - verified with isolated video and lyrics tests
2026-10-25 - feature/lyrics-tap-sync - Added guided listen-and-tap lyric alignment, live MPD/mpv clock capture, persisted offset, undo and cancel - verified with silent PTY and unit tests
2026-10-25 - feature/easy-playback-seeking - Added 5/30/60s navigation, typed jump-to-time, clickable progress bar, undo, and mpv error propagation - verified by isolated MPD/mpv integration tests
