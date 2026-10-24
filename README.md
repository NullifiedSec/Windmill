# Windmill

A local-first personal media center. Audio playback is delegated to MPD; video playback to mpv. YouTube discovery and downloading use external tools. Windmill owns flexible collections, tags, ranking, and history.

## Status
First vertical slice: a working **CLI**, not yet a TUI. Local catalog, tags, collections, deterministic tag-based recommendations, yt-dlp downloads, mpv playback and MPD control via native TCP are implemented. Music browsing and synced-lyrics UI come later.

## Development
Install Rust and Cargo, then `cargo test` and `cargo run -- help`. On this workstation a project-local toolchain is in `.tools/` (gitignored).

## Commands
```sh
windmill add /path/to/song.flac "Feel Good Inc." "Gorillaz"
windmill add https://www.youtube.com/watch?v=... "Video" "Channel"
windmill list
windmill tag <id> melancholic
windmill tag <id> windmill
windmill collection create "Peaceful songs"
windmill collection add "Peaceful songs" <id>
windmill collection show "Peaceful songs"
windmill recommend windmill,melancholic
windmill play <id>                 # mpv (video / direct media)
windmill download <id> audio       # yt-dlp, downloaded into a managed directory
windmill download <id> video
windmill lyrics /path/to/lyrics.lrc 90500   # timestamped lyric preview (milliseconds)
windmill mpd status                # MPD native TCP status, no mpc dependency
windmill mpd play /path/to/file   # MPD must be installed and configured
```

By default the database lives in `$XDG_DATA_HOME/windmill/library.db` or `~/.local/share/windmill/library.db`; downloaded files live alongside it in `downloads/`. Override with `WINDMILL_DATA_DIR` for tests and portability.

MPD playback accepts only MPD-library URIs, not arbitrary absolute paths; ensure the audio exists in MPD's music directory. In this initial slice, `mpd play` forwards the supplied URI to MPD and reports errors.

No YouTube sign-in is used; online requests and streaming/downloading are not anonymous. Only download media you have the rights to save and comply with applicable provider terms.

See [architecture](docs/ARCHITECTURE.md), [roadmap](docs/ROADMAP.md) and [local MPD setup](docs/MPD-SETUP.md).

Development reference checkouts (ignored by Git) are documented in [upstream sources](docs/UPSTREAM.md).
