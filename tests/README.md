# Integration checks

Unit tests cover catalog persistence, deterministic scoring and LRC timestamp handling.

Once MPD is installed, validate its user service with a real audio file and an
isolated `WINDMILL_DATA_DIR`. Playback and download commands are not exercised
in automatic tests because they can produce audio and external network traffic.

The `lyrics` CLI supports testing with a local .lrc fixture:
```sh
windmill lyrics /path/to/song.lrc 90500
```
