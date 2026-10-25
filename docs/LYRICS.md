# Windmill lyrics (v0.5.1)

## Source priority

If you have explicitly pinned an LRCLIB track, it overrides normal
matching, including local sidecar files. Otherwise, order is:

1. Local `file.lrc` alongside the playing file, including MPD library songs.
2. Offline SHA-256-keyed private cache at `~/.local/share/windmill/lyrics/`, or `$WINDMILL_DATA_DIR/lyrics/`.
3. LRCLIB public artist/title lookup in an asynchronous worker, with title decorations removed and duration matching when available.

Windmill uses TLS-verified curl with timeouts, response-size limits, and no Google login. Cache files have mode 0600. Matching validates both normalized artist and title; mismatched-duration recordings are rejected. No lyric content is committed to source control. An unsuccessful lookup displays a message instead of fabricating lyrics. Untimed/plain lyrics render without fake synchronization.

## Controls (Now Playing)

| Key | Action |
|---|---|
| `M` | Paste LRCLIB track link or numeric ID |
| `U` | Unpin manual LRCLIB record; return to automatic matching |
| `s` | Toggle guided Tap Sync mode |
| `p` | Play/pause without exiting Tap Sync |
| `Space` / `Enter` | During Tap Sync, mark the start of the selected lyric at the current live playback time |
| `Esc` | Cancel Tap Sync before marking |
| `u` | Undo the previous successful Tap Sync correction |
| `o` | Set precise offset, e.g. `+12.5` seconds or `-00:03.250` |
| `[` | Advance lyrics 250 ms |
| `]` | Delay lyrics 250 ms |
| `0` | Reset timing offset |
| `j/k`, Up/Down | Browse lyric lines |
| Enter | Seek to selected timestamped line |
| `a` | Return to auto-follow |
| `L` | Refresh lookup manually |

### Manual LRCLIB ID and virtual blank padding

Press `M` while playing, and paste a record link like
`https://lrclib.net/tracks/36174111`. Windmill fetches exactly the
record identified by that numeric ID, even when titles, artists or
durations do not match. It saves the override only after a usable
response, in the local SQLite database keyed by original media source.
Downloaded copies of the same video reuse that choice. Private per-ID
caching enables offline playback; press `U` to restore auto/local lyrics.

For a shorter lyric transcript, blank timestamps clear the last sung
line during the instrumental outro. The display also leaves blank space
before vocals begin. If a transcript is longer than the recording,
Windmill displays its remaining duration as a *virtual lyric tail*;
it does not add actual silence to the audio and will not seek past EOF.
You can shift lyric timing using `s` Tap Sync, `o` exact offset, or
`[`/`]` fine controls. Extra dialogue inserted in the *middle*
of a song still needs segment-specific timing.

Links are restricted to the official LRCLIB domain or a numeric ID.
The app never fetches arbitrary pasted URLs and doesn't overwrite
a working pinned record if a new request fails.

### Guided Tap Sync (recommended)

With the song playing, press `s` in Now Playing. Windmill chooses the
first nonempty timestamped lyric line; select a different one using
`j/k` or the arrow keys. **Press Space at the instant you hear the selected
line begin.** Enter can also mark. This does not pause or seek the song.

**Need to pause while lining things up?** Press `p` to pause and `p`
again to resume. Space is reserved for the lyric marker in this
mode. The controls pane and footer explain this mapping.
Outside Tap Sync, Space also works as a play/pause toggle.

Windmill reads a fresh timestamp from MPD/mpv, then computes:

`new_offset_ms = live_playback_position_ms − (LRC_timestamp_ms + LRC_document_offset_ms)`

It persists the correction under the media source ID. When a video contains
a long intro, the resulting offset is positive; negative offsets work too.
The selected line remains fixed during marking even while the currently
active lyric changes. Windmill visually distinguishes it with `◎`.
Use `u` to restore the previous offset, or `Esc` to cancel before tapping.
Tap Sync requires timed LRC; plain lyrics cannot be synchronized by
guessing timestamps.

A human keyboard tap and player IPC have some latency. You can refine
afterward with `[`/`]` in 250 ms steps, or enter a precise value with `o`.

**Manual alternative:** A music video with a 13-second spoken intro needs `+13`.
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

Artist/title correction UI, audio fingerprinting, genuine silent audio extension and richer word-timed provider formats. Metadata quality and external service availability limit remote lyric matching.
