# Local MPD configuration (Arch / PipeWire)

On the development machine MPD 0.24.17 and MPC 0.35 were installed using pacman on 2026-10-25.

Created only when absent:

- `~/.config/mpd/mpd.conf` (permissions 0600)
- `~/Music/` (library root, currently empty)
- `~/.local/share/mpd/playlists/`
- `~/.local/state/mpd/`

MPD audio output uses the native `pipewire` plugin. Control binds to `127.0.0.1:6600`. The packaged **user** service was started with:

```sh
systemctl --user start mpd.service
mpc outputs
mpc status
```

The service is **not enabled** to autostart. Start it on demand as shown above; stop it with `systemctl --user stop mpd.service`.

MPD's library is rooted at `~/Music`; it expects library-relative URIs. Windmill's own catalog is separate and may reference arbitrary local paths and URLs; future playback orchestration must bridge those identities safely without moving original files.

`mpc update` updates MPD's library database after copying files into `~/Music`. Never assume MPD recognizes arbitrary files referenced only by Windmill's catalog.

## Verified

- `systemctl --user is-active mpd.service` = active
- `mpc status` succeeded
- `mpc outputs` returned `Output 1 (PipeWire) is enabled`
- MPD listens on loopback port 6600

No actual audio was played, so end-to-end audio output has not been verified.
