# Windmill v0.5.0 — Music center expansion

## 1. Public playlist import

From Discover, press `P` and paste a public YouTube or YouTube Music playlist URL. A metadata-only preview of up to 250 items appears. Use `a` to toggle a song, `A` to select all, `I` to import selected tracks into a collection, `B` for explicit background audio batch download, and `V` for video batch download.

No downloads occur during preview/import. Reimporting is atomic, idempotent and preserves original library tags and titles. Batch jobs run sequentially in a worker, skip existing assets and report failure counts. Interrupted-job recovery and cancellation remain future work.

## 2. Queue across MPD and mpv

Press `6` to open Queue. Press `e` on Library/Discover/For You to append a track, or `Q` on Collections to queue an entire collection. Inside Queue, Enter/`p` plays, `n`/`b` moves next/previous, Delete removes an entry, `<`/`>` reorders, `S` shuffles, `R` toggles repeat and `C` clears. Queue entries persist in SQLite. Playback resumes manually across app sessions; automatic advancement happens during active sessions. An intentional Stop prevents autoplay.

## 3. Public YouTube Music search

In Discover press `Y` and enter a query such as `songs:Counting Stars`, `albums:Gorillaz`, `artists:Gorillaz` or `playlists:Gorillaz`. Press Enter on albums or artists to browse tracks, which can be selectively imported or downloaded; playlists open the existing preview. Song results can play directly. An optional isolated virtual environment supplies ytmusicapi; set it up with `./scripts/setup-ytmusic`. No user login is required. API availability is controlled by YouTube Music and may change.

## 4. Truecolor artwork

A canonical YouTube video identifier yields a public thumbnail from a fixed HTTPS image host. Local tracks can yield embedded artwork through FFmpeg. Windmill stores private on-disk artwork caches and renders decoded colors using terminal half-block cells. Artwork lookup is asynchronous; missing covers never block playback.

## 5. Live speaker spectrum

While playback is active, Windmill uses `pactl` and `parec` to capture PCM samples from the *default speaker/output sink monitor*. Rust Goertzel processing computes 16 spectral bands, with no recording to disk, network transfer, or microphone capture. The monitor reflects *all audio on that sink*, not exclusively Windmill. If the output monitor is absent the visualizer is unavailable; playback remains functional. Tests disable actual capture and use a fake PCM source instead.

## 6. Local listening recommendations

For You uses tag overlaps, local listening/quick-skip counts, and explicit likes/dislikes, stored only in the catalog SQLite database. In Library, For You, Queue or Now Playing, press `+` to like, `-` to dislike, or `=` to reset. Details explain the inputs to scoring. Dislikes strongly suppress recommendations.

## Tests and limitations

`cargo test --locked`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --locked`.

`python3 tests/playlist_integration.py`: fake yt-dlp, preview, import and batch downloads.
`dbus-run-session -- python3 tests/queue_integration.py`: automatic mpv queue advance and manual stop.
`python3 tests/visualizer_integration.py`: synthetic PCM, speaker monitor and TUI spectrum.
`dbus-run-session -- python3 tests/mpris_integration.py`: MPD/mpv playback and MPRIS.
`python3 tests/lyrics_integration.py` and `python3 tests/video_lyrics_offset_integration.py`: lyrics and time offsets.

Remaining for future: interrupted download recovery, cancellation and fine-grained progress, local media scanning, metadata correction, broader cover overrides, and fully audible output-device testing.
