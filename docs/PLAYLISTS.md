# Playlist import and batch downloads

Public YouTube and YouTube Music playlist URLs are accepted: both playlist links and watch links with a list parameter. No account credentials are used.

## TUI

In Discover, press `P` and paste the public playlist URL. The preview is limited to 250 accessible items and does not download anything. Use `j/k` to navigate; `a` toggles one checkbox; `A` selects all. `I` imports selected items into a collection named after the playlist. `B` explicitly downloads selected audio, and `V` downloads selected video. Press `p` to play a highlighted song, `s` to save it individually, and `/` to return to normal search.

Batch downloads run on a worker thread with progress counts. Completed assets are skipped, and errors do not delete successes. Cancellation, persisted download queues, progress percentages and automatic retries remain future work.

## CLI

- `windmill playlist preview URL`
- `windmill playlist import URL`
- `windmill playlist import URL "Collection name"`
- `windmill playlist download URL audio`
- `windmill playlist download URL video`

Both CLI and TUI imports use an atomic SQLite transaction and preserve existing media titles, tags and stable identities. Snapshot playlist ordering is retained separately from arbitrary collection membership.

## Safety and verification

- URLs are restricted to HTTPS YouTube/YouTube Music hosts with validated playlist IDs.
- Video IDs from metadata are validated and converted to canonical links.
- Preview is bounded to 250 entries and a 4MB JSON response.
- Downloading requires an explicit command and each item still invokes yt-dlp with `--no-playlist`; paths are verified inside the managed downloads directory.
- No DRM/authentication bypass; use only with media you are permitted to save.
- `python3 tests/playlist_integration.py` uses fake yt-dlp and a temporary catalog to test preview without downloading, ordered/idempotent import and batch skip-existing behavior.
