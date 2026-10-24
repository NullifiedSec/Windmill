# Windmill lyrics (v0.3.1)

## Source priority

1. Local `file.lrc` alongside the playing file, including MPD library songs.
2. Offline SHA-256-keyed private cache at `~/.local/share/windmill/lyrics/`, or `$WINDMILL_DATA_DIR/lyrics/`.
3. LRCLIB public artist/title lookup in an asynchronous worker, with title decorations removed and duration matching when available.

Windmill uses TLS-verified curl with timeouts, response-size limits, and no Google login. Cache files have mode 0600. Matching validates both normalized artist and title; mismatched-duration recordings are rejected. No lyric content is committed to source control. An unsuccessful lookup displays a message instead of fabricating lyrics. Untimed/plain lyrics render without fake synchronization.

## Controls (Now Playing)

| Key | Action |
|---|---|
| `o` | Set precise offset, e.g. `+12.5` seconds or `-00:03.250` |
| `[` | Advance lyrics 250 ms |
| `]` | Delay lyrics 250 ms |
| `0` | Reset timing offset |
| `j/k`, Up/Down | Browse lyric lines |
| Enter | Seek to selected timestamped line |
| `a` | Return to auto-follow |
| `L` | Refresh lookup manually |

**Example:** A music video with a 13-second spoken intro needs `+13`.
Press `o` and enter `+13` or `+00:13.000`. This delays the lyrics,
without skipping video content. `-2.5` displays lyrics 2.5 seconds earlier.

Inputs *replace* the current offset; they do not add to it. They accept
signed seconds or mm:ss.sss, are rounded to 1 ms and bounded by ±60 minutes.
Invalid input leaves the previous offset unchanged. The Now Playing
lyrics panel always shows the signed offset; `[` / `]` fine-tune it.

Timing adjustments persist in SQLite without modifying the media file or .lrc sidecar.
Catalog video offsets are keyed to the original URL, even when mpv plays a
downloaded file. Existing offsets saved against local file paths migrate on
first access; explicit zeroes remain zero. Intros/outros make music videos
longer than the album recording, so videos aren't rejected from LRCLIB for
differing track duration. Audio-only files still use duration matching.

A single offset shifts the entire song timeline. It cannot handle extra
footage inserted *mid-song*; that would need segment-specific timing.



The current line is centered/highlighted, while neighboring lines fade. Enhanced LRC with inline `<mm:ss.xx>` segment timestamps enables word/segment highlighting. Normal LRCLIB results are usually line-timed, so per-word accuracy cannot be invented.

## Tests

`cargo test --locked` (offline unit tests).
`cargo test --locked -- --ignored real_provider_fetches_and_reuses_private_cache` (opt-in live LRCLIB request and cache test).
`python3 tests/lyrics_integration.py` (silent real-terminal TUI with sidecar and offset persistence).
`dbus-run-session -- python3 tests/mpris_integration.py` (desktop media-control regression).

## Not yet implemented

Manual LRCLIB result selection, artist/title correction UI, cover artwork, audio fingerprinting and richer word-timed provider formats. Metadata quality and external service availability limit remote lyric matching.
