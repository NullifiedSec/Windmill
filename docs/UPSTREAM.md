# Upstream reference clones

Shallow Git clones are stored in `upstream/`, which is ignored by Windmill Git. They are **not** submodules, vendored dependencies, or edited forks. Windmill builds without them. No source has been copied into Windmill.

| Project | Upstream | Observed HEAD (2026-10-25) | Reason |
|---|---|---|---|
| `youtube-tui` | [https://github.com/Siriusmart/youtube-tui](https://github.com/Siriusmart/youtube-tui) | `800b65178f3c6cc2b2afdfc28b3e04e0fc212769` | Online video discovery/provider implementation reference. Relevant: src/global/structs/providers/rustypipe.rs, src/global/traits/search_provider.rs. |
| `mopytui` | [https://github.com/prietus/mopytui](https://github.com/prietus/mopytui) | `2bb7ed6df4fd7293d0e1db590fd1e2ea815d5c62` | TUI inspiration and study of lyrics, art, and layout; uses Mopidy JSON-RPC. Relevant: src/lyrics.rs, src/ui/now_playing.rs, src/mopidy/client.rs. |
| `MPD` | [https://github.com/MusicPlayerDaemon/MPD](https://github.com/MusicPlayerDaemon/MPD) | `7c3d5737bf4228f6799e64275a3b544daaf1d01d` | Protocol and daemon behavior reference; Windmill uses installed MPD binary without forking. |
| `mpv` | [https://github.com/mpv-player/mpv](https://github.com/mpv-player/mpv) | `703e59b0ecf659ba8062ecb876fb8ec25f73726f` | Playback IPC and player behavior reference; Windmill uses installed mpv binary without forking. |

## Maintenance

- Reference clones retain their own `.git` and `origin` remotes. Never commit them to the Windmill repository.
- Before experiments in a reference checkout, check `git status`; prefer a separate branch/worktree and preserve any upstream-local changes.
- Re-clone into a missing `upstream/<name>` directory or use explicit `git -C upstream/<name> fetch`; avoid unreviewed pulls and version drift.
- Do not treat any one of these revisions as tested against Windmill; source/API compatibility must be verified in integration tests.
- Licenses differ, and merely cloning is not permission to combine code indiscriminately; review copyright and reciprocal licensing when copying or integrating implementations.
- MPD and mpv are external runtime tools. Rebuilding them from these clones is not currently part of Windmill development.
