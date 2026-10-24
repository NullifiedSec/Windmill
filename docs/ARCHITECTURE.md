# Windmill architecture (initial slice)

## Boundaries
- Catalog: stable local integer IDs for entries; location is separate from tags and collections. An entry may have any number of tags and collections. No artist/album hierarchy is required.
- Playback: current integration uses mpv for local files and online video; native MPD text protocol over TCP for audio library playback. Audio and video should eventually be coordinated by a single playback arbiter to prevent simultaneous output.
- Downloads: yt-dlp is executed with argument arrays (never shell interpolation). Files are kept under a managed directory. A download is currently an external side effect, not a fully transactional job.
- Discovery: v0 scores entries *already in the catalog* by matching tags. External candidate discovery is not yet built.
- Lyrics: shared LRC parser implemented, timestamped CLI preview. Future synchronized UI renderer from LRCLIB/LRC with MPD elapsed time or mpv IPC time-pos. Use available rights/licensing for lyric data.

## Constraints/invariants
- Never overwrite an imported file or rewrite original media tags.
- Collections and tags are orthogonal to physical paths.
- Store all user-organized metadata locally.
- Keep third-party applications unmodified unless a missing capability is demonstrated.
- Avoid interpreting remote titles/URLs as shell commands.
- No automatic network scraping or downloads on import.
- No claims of anonymous streaming: lack of login is not lack of tracking.

## Rejected for now
- Forking MPD, mpv, mopytui: unnecessary initial maintenance and complex integration.
- MPD as the catalog: flexible cross-source collections don't map cleanly to its audio library.
- LLM-based ranking: deterministic tags are inspectable and testable.
- Reusing mopytui directly: its library and control paths target Mopidy JSON-RPC, not native MPD.
