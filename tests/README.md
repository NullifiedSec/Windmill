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


## Music-video offset integration

Run `python3 tests/video_lyrics_offset_integration.py` after building.
It creates a silent test video, links it to a mock YouTube media entry,
sets `+12.5` in the real TUI, restarts the application, and confirms
the offset is saved under the original URL. It does not access the
user's library, audio devices, or online sources.

`tests/lyrics_integration.py` also enters an exact offset with `o` then
fine-tunes with `[`. Parser tests cover decimal seconds, mm:ss.sss,
invalid text and out-of-range offsets.

## Guided Tap Sync regression

The offline Rust tests cover positive/negative cue offsets, preserving LRC
document offsets, skipping blank first lines, choosing a later line,
persistence/undo, out-of-range rejection and cancellation. The silent
PTY test in `tests/lyrics_integration.py` enters `s` mode, taps Space at
the live cue, verifies the stored offset is nonzero, presses `u` to
restore the old value, then verifies the original precise-offset
and seek controls still function.


## Timeline seeking

The `src/seek.rs` tests cover timestamp expressions and progress-bar
coordinates. `tests/mpris_integration.py` sends real arrow and mouse
events to silent MPD/mpv, tests a jump to timestamp, and verifies `z`
restores previous playback positions.


## Tap Sync pause/resume regression

The isolated MPRIS integration test also enters Tap Sync during both
silent MPD and mpv playback and presses p twice. It verifies paused
then playing states through playerctl, ensuring the Space cue marker
remains separate.


## Public playlists

`python3 tests/playlist_integration.py` tests playlist preview,
atomic import, idempotency, playlist order, explicit batch downloading
and skipping completed files using fake yt-dlp and a temporary catalog.
