# Windmill lyrics (v0.3.0)

## Source priority

1. Local `file.lrc` alongside the playing file, including MPD library songs.
2. Offline SHA-256-keyed private cache at `~/.local/share/windmill/lyrics/`, or `$WINDMILL_DATA_DIR/lyrics/`.
3. LRCLIB public artist/title lookup in an asynchronous worker, with title decorations removed and duration matching when available.

Windmill uses TLS-verified curl with timeouts, response-size limits, and no Google login. Cache files have mode 0600. Matching validates both normalized artist and title; mismatched-duration recordings are rejected. No lyric content is committed to source control. An unsuccessful lookup displays a message instead of fabricating lyrics. Untimed/plain lyrics render without fake synchronization.

## Controls (Now Playing)

| Key | Action |
|---|---|
| `[` | Advance lyrics 250 ms |
| `]` | Delay lyrics 250 ms |
| `0` | Reset timing offset |
| `j/k`, Up/Down | Browse lyric lines |
| Enter | Seek to selected timestamped line |
| `a` | Return to auto-follow |
| `L` | Refresh lookup manually |

Timing adjustments persist in SQLite without modifying the media file or .lrc sidecar.

The current line is centered/highlighted, while neighboring lines fade. Enhanced LRC with inline `<mm:ss.xx>` segment timestamps enables word/segment highlighting. Normal LRCLIB results are usually line-timed, so per-word accuracy cannot be invented.

## Tests

`cargo test --locked` (offline unit tests).
`cargo test --locked -- --ignored real_provider_fetches_and_reuses_private_cache` (opt-in live LRCLIB request and cache test).
`python3 tests/lyrics_integration.py` (silent real-terminal TUI with sidecar and offset persistence).
`dbus-run-session -- python3 tests/mpris_integration.py` (desktop media-control regression).

## Not yet implemented

Manual LRCLIB result selection, artist/title correction UI, cover artwork, audio fingerprinting and richer word-timed provider formats. Metadata quality and external service availability limit remote lyric matching.
