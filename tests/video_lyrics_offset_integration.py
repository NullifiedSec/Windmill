"""End-to-end test of per-video offsets surviving downloaded/offline playback.

Starts a real Windmill TUI in a private DBus/PTTY with an mpv null-output
wrapper. Does not access the user's playback, files or network.
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
BIN = ROOT / "target/debug/windmill"
URL = "https://www.youtube.com/watch?v=HyHNuVaZJ-k"


def capture(master, process, chunks, seconds):
    until = time.monotonic() + seconds
    while time.monotonic() < until and process.poll() is None:
        ready, _, _ = select.select([master], [], [], .1)
        if ready:
            try:
                chunk = os.read(master, 32768)
            except OSError:
                return
            chunks.extend(chunk)
            if b"\x1b[6n" in chunk:
                os.write(master, b"\x1b[1;1R")


def session(env, commands, delay=.55):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 38, 115, 0, 0))
    proc = subprocess.Popen(
        ["dbus-run-session", "--", str(BIN), "ui"],
        stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True,
    )
    os.close(slave)
    output = bytearray()
    try:
        capture(master, proc, output, .6)
        for command in commands:
            os.write(master, command)
            capture(master, proc, output, delay)
        os.write(master, b"q")
        assert proc.wait(timeout=5) == 0
        assert b"Error:" not in output, "TUI displayed a playback or lyrics error"
        return bytes(output)
    finally:
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)
        os.close(master)


def main():
    assert BIN.is_file()
    with tempfile.TemporaryDirectory(prefix="windmill-video-offset-") as folder:
        root = Path(folder)
        video = root / "downloaded.mp4"
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "color=c=black:s=64x64:r=5",
            "-f", "lavfi", "-i", "anullsrc=r=44100:cl=stereo",
            "-t", "20", "-shortest", "-c:v", "mpeg4", "-c:a", "aac",
            str(video),
        ], check=True, timeout=20)
        video.with_suffix(".lrc").write_text(
            "[00:00.50]Testing a video intro\n[00:01.50]Lyrics after the intro\n"
        )
        bin_dir = root / "bin"
        bin_dir.mkdir()
        wrapper = bin_dir / "mpv"
        wrapper.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n')
        wrapper.chmod(0o755)
        data = root / "data"
        env = dict(os.environ, WINDMILL_DATA_DIR=str(data),
                   WINDMILL_DISABLE_VISUALIZER="1", WINDMILL_DISABLE_ARTWORK="1", WINDMILL_MPD_ADDR="127.0.0.1:0",
                   PATH=f"{bin_dir}:{os.environ.get('PATH','/usr/bin')}",
                   TERM="xterm-256color")
        result = subprocess.run([str(BIN), "add", URL, "Local Intro Test", "Example Artist"],
                                env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        with sqlite3.connect(data / "library.db") as connection:
            connection.execute(
                "INSERT INTO assets(media_id,kind,path) VALUES(1,'video',?)",
                (str(video),),
            )
        output = session(env, [b"p", b"o+00:12.500\r"])
        assert b"Testing a video intro" in output, "Local downloaded-video lyrics not rendered"
        with sqlite3.connect(data / "library.db") as connection:
            values = connection.execute(
                "SELECT source,offset_ms FROM lyric_offsets"
            ).fetchall()
        assert values == [(URL, 12_500)], f"Offset incorrectly tied to downloaded path: {values}"
        again = session(env, [b"p"], delay=.8)
        assert b"+0:12.500" in again, "Saved video offset not restored"
        print("PASS: real offline video uses original URL for lyrics offset; exact 12.5s offset survives restart")


if __name__ == "__main__":
    main()
