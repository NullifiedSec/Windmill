# Windmill seeking and navigation (v0.3.3)

## Controls

| Action | Shortcut |
|---|---|
| Rewind / forward 5 sec | Left / Right |
| Rewind / forward 30 sec | Shift+Left / Shift+Right |
| Rewind / forward 60 sec | Ctrl+Left / Ctrl+Right, PageUp / PageDown |
| Seek to time | `g`, type `1:23.5`, `83.5` or `1:02:03` |
| Go to beginning / near end | Home / End |
| Undo the most recent seek | `z` |
| Click seek | Mouse click on Now Playing progress bar |

Keyboard shortcuts work outside text-entry prompts while the Windmill
terminal has focus. Terminal mouse capture is enabled while the TUI runs
and disabled on exit. Clicking elsewhere in the UI does not seek.

Seeking clamps at 0 and at the known track duration. End goes one
second before the end to avoid immediately terminating mpv at EOF.
Unknown-duration streams still support arrows and absolute input where
the backend permits them; the progress bar and End need a duration.

The previous playback position is kept in memory for one-step undo.
The undo record is bound to the current track identity and is discarded
when playback switches tracks. Windmill never edits media files or
moves lyric timing offsets when seeking.

Tap Sync has priority for Space/Enter marking; seeking remains separate.
MPRIS/playerctl continues to reflect playback time on the normal UI
polling cadence. Failed mpv IPC commands report an error.

## Tests

`cargo test --locked` checks parsing, bounds and geometry.
`dbus-run-session -- python3 tests/mpris_integration.py` tests silent
MPD/mpv playback with actual arrow keys, jump-to-time, progress-bar
mouse clicks and undo. It does not use your real music library.
