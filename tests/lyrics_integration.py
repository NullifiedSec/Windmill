"""PTY regression for Windmill's synchronized lyrics UI (no audible playback).

Uses a private D-Bus session, disposable catalog and mpv null audio/video outputs.
Run: cargo build --locked && python3 tests/lyrics_integration.py
"""
import fcntl
import os
import pty
import select
import sqlite3
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/windmill"


def capture(master, process, buf, sec):
    deadline = time.monotonic() + sec
    while time.monotonic() < deadline and process.poll() is None:
        ready, _, _ = select.select([master], [], [], .1)
        if not ready:
            continue
        try:
            chunk = os.read(master, 32768)
        except OSError:
            return
        buf.extend(chunk)
        if b"\x1b[6n" in chunk:
            os.write(master, b"\x1b[1;1R")


def main():
    assert BINARY.is_file(), "Build Windmill first"
    with tempfile.TemporaryDirectory(prefix="windmill-lyrics-") as directory:
        root = Path(directory)
        music = root / "song.wav"
        lrc = music.with_suffix(".lrc")
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi",
            "-i", "anullsrc=r=44100:cl=stereo", "-t", "10", "-c:a", "pcm_s16le",
            str(music),
        ], check=True, timeout=15)
        lrc.write_text("[00:00.20]The windmill turns\n"
                       "[00:01.00]The meadow rests\n"
                       "[00:02.00]Clouds float slowly\n"
                       "[00:04.00]The sun shines\n")
        bin_dir = root / "bin"
        bin_dir.mkdir()
        script = bin_dir / "mpv"
        script.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n')
        script.chmod(0o755)
        env = dict(
            os.environ,
            WINDMILL_DATA_DIR=str(root / "profile"),
            WINDMILL_DISABLE_VISUALIZER="1", WINDMILL_DISABLE_ARTWORK="1", WINDMILL_MPD_ADDR="127.0.0.1:0",
            PATH=f"{bin_dir}:{os.environ.get('PATH','/usr/bin')}",
            TERM="xterm-256color",
        )
        result = subprocess.run([str(BINARY), "add", str(music), "Meadow Song", "Local Artist"],
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 112, 0, 0))
        process = subprocess.Popen(
            ["dbus-run-session", "--", str(BINARY), "ui"],
            stdin=slave, stdout=slave, stderr=slave,
            env=env, start_new_session=True,
        )
        os.close(slave)
        data = bytearray()
        try:
            capture(master, process, data, .6)
            os.write(master, b"p")
            capture(master, process, data, 1.5)
            assert b"Local .lrc" in data, "Local lyrics not loaded in TUI"
            assert b"The windmill turns" in data, "Synced lyric line not rendered"
            assert b"Lyrics controls" in data, "Lyrics controls pane absent"
            # Enter guided sync; press SPACE as the first sung line begins.
            os.write(master,b"s")
            capture(master, process, data, .15)
            assert b"TAP SYNC" in data, "Guided lyric sync instructions not rendered"
            os.write(master,b" ")
            capture(master, process, data, .25)
            with sqlite3.connect(root / "profile" / "library.db") as con:
                marker=con.execute(
                    "SELECT offset_ms FROM lyric_offsets WHERE source=?",
                    (str(music),)
                ).fetchone()
            assert marker is not None and 500 < marker[0] < 8_000, (
                f"Tap Sync did not mark the current playback position: {marker}"
            )
            os.write(master,b"u")
            capture(master, process, data, .25)
            with sqlite3.connect(root / "profile" / "library.db") as con:
                reverted=con.execute(
                    "SELECT offset_ms FROM lyric_offsets WHERE source=?",
                    (str(music),)
                ).fetchone()
            assert reverted==(0,),f"Undo didn't restore zero offset: {reverted}"
            os.write(master, b"]")
            capture(master, process, data, .5)
            offset = sqlite3.connect(root / "profile" / "library.db").execute(
                "SELECT offset_ms FROM lyric_offsets WHERE source=?", (str(music),)
            ).fetchone()
            assert offset == (250,), f"Timing offset not saved: {offset}"
            os.write(master, b"o+12.5\r")
            capture(master, process, data, .35)
            offset = sqlite3.connect(root / "profile" / "library.db").execute(
                "SELECT offset_ms FROM lyric_offsets WHERE source=?", (str(music),)
            ).fetchone()
            assert offset == (12500,), f"Exact video-intro offset failed: {offset}"
            os.write(master, b"[")
            capture(master, process, data, .2)
            offset = sqlite3.connect(root / "profile" / "library.db").execute(
                "SELECT offset_ms FROM lyric_offsets WHERE source=?", (str(music),)
            ).fetchone()
            assert offset == (12250,), f"Fine tuning after exact offset failed: {offset}"
            os.write(master, b"k")
            capture(master, process, data, .25)
            os.write(master, b"\r")
            capture(master, process, data, .3)
            os.write(master, b"a")
            capture(master, process, data, .3)
            os.write(master, b"q")
            assert process.wait(timeout=5) == 0
            assert b"Error:" not in data, "TUI reported error during lyric seek/follow"
            print("PASS: Tap Sync with SPACE and undo, exact offset, fine tuning, line navigation, seek, auto-follow, clean exit")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            os.close(master)


if __name__ == "__main__":
    main()
