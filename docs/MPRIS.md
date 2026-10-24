# MPRIS desktop integration

Windmill v0.2.1 registers one desktop player, `org.mpris.MediaPlayer2.windmill`, while the interactive TUI is running. MPD and mpv remain internal, independently maintained engines.

## Architecture

- `src/mpris.rs`: zbus D-Bus methods, current metadata snapshot, PropertiesChanged and Seeked signals, and a channel carrying commands to the TUI thread.
- `src/ui.rs`: accepts controls on the TUI event loop and updates the correct playback engine.
- `src/player.rs`: native MPD TCP commands and mpv JSON IPC for play/pause, stop, previous/next, seeking and volume.

Properties include playback state, title/artist, track ID, actual MPD item, known length, position, volume and capability flags. Changing MPD queue items refreshes metadata. If the playing MPD file matches Windmill's catalog selection, its custom title is preferred.

External controls are marshalled onto the application event loop: D-Bus callbacks do not directly modify media engine state.

## Verify

```sh
cargo build --locked
python3 tests/mpris_integration.py
```

Requires existing mpd, mpc, playerctl and ffmpeg. Tests are user-scoped with temporary files, silent MPD null output and an mpv wrapper disabling video/audio hardware. The bus name is released when Windmill closes.

## Limitations

- No standalone headless MPRIS server yet: close the TUI, and Windmill unregisters from D-Bus. MPD may continue playing independently.
- No unified media queue; previous/next for single-item mpv playback cannot advance to a different item.
- Loop/shuffle and independent playlist operations aren't implemented in the facade.
- Only one instance can own the well-known bus name; a second TUI runs without registering Windmill again.
- Position is refreshed at the TUI polling cadence rather than a hard real-time feed.
- If Firefox or Spotify is also running, use `playerctl -p windmill` to select this player.
