# Test strategy

`cargo test --locked` tests catalog persistence, tag matching, assets, yt-dlp JSON decoding, MPD response parsing, LRC synchronization and ratatui test-backend rendering.

The development workstation also passed independent pseudo-terminal checks: start the real TUI with a disposable `WINDMILL_DATA_DIR`, render a saved item, quit with q; perform live public YouTube search, save the result using s and verify SQLite; load the For You tab with tagged data and apply a tag filter.

These checks do not verify actual audio output or downloaded file contents. Use legally playable/downloadable media in future end-to-end tests.

Run the UI with `./scripts/dev`.

## Live MPRIS integration (silent and isolated)

Run `python3 tests/mpris_integration.py` after `cargo build --locked`.

This launches a disposable MPD daemon with a null output and an isolated mpv wrapper with null audio/video output. It verifies D-Bus registration and external `playerctl` metadata, play/pause, volume, seek, MPD next/previous and stop, plus TUI shutdown/unregistration. It does not touch normal MPD or speakers. Another Windmill instance must not already own the D-Bus name.


## Lyrics integration

Run `python3 tests/lyrics_integration.py`: silent mpv, private D-Bus, temporary catalog and .lrc. Validates rendering, timing persistence, navigation, seeking and clean exit.

The real LRCLIB HTTP test is opt-in:
`cargo test --locked -- --ignored real_provider_fetches_and_reuses_private_cache`.
The test uses temporary storage and never commits lyric content.
