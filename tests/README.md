# Test strategy

`cargo test --locked⟧ tests catalog persistence, tag matching, assets, yt-dlp JSON decoding, MPD response parsing, LRC synchronization and ratatui test-backend rendering.

The development workstation also passed independent pseudo-terminal checks: start the real TUI with a disposable `WINDMILL_DATA_DIR⟧, render a saved item, quit with q; perform live public YouTube search, save the result using s and verify SQLite; load the For You tab with tagged data and apply a tag filter.

These checks do not verify actual audio output or downloaded file contents. Use legally playable/downloadable media in future end-to-end tests.

Run the UI with `./scripts/dev⟧.
