"""End-to-end playerctl/MPRIS smoke test on a disposable, silent MPD.

Requires existing mpd, mpc, playerctl, ffmpeg and a session D-Bus.
Never touches ~/.config/mpd, your real MPD process, or audio hardware.
Usage: python3 tests/mpris_integration.py
"""
import fcntl
import os
import pty
import select
import socket
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/windmill"


def run(*args, env=None, timeout=5):
    return subprocess.run(args, env=env, text=True, capture_output=True, timeout=timeout)


def capture(master, process, seen, secs):
    end = time.monotonic() + secs
    while time.monotonic() < end and process.poll() is None:
        ready, _, _ = select.select([master], [], [], 0.1)
        if ready:
            try:
                block = os.read(master, 32768)
            except OSError:
                return
            seen.extend(block)
            if b"\x1b[6n" in block:
                os.write(master, b"\x1b[1;1R")


def main():
    if not BINARY.is_file():
        raise RuntimeError("Build Windmill with cargo build --locked first.")
    if "windmill" in run("playerctl", "-l").stdout.splitlines():
        raise RuntimeError("Windmill is already running. Stop it before the isolated MPRIS test.")
    with tempfile.TemporaryDirectory(prefix="windmill-mpris-") as directory:
        root = Path(directory)
        music = root / "music"
        music.mkdir()
        (root / "playlists").mkdir()
        for name in ("first.wav", "second.wav"):
            subprocess.run([
                "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi",
                "-i", "anullsrc=r=44100:cl=stereo", "-t", "20", "-c:a",
                "pcm_s16le", str(music / name),
            ], check=True, timeout=15)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        # Shadow mpv with an isolated no-audio/no-video wrapper for this test.
        # Windmill sees the normal mpv executable name but nothing reaches hardware.
        bin_dir=root / "bin"
        bin_dir.mkdir()
        wrapper=bin_dir / "mpv"
        wrapper.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n')
        wrapper.chmod(0o755)
        config = root / "mpd.conf"
        config.write_text(
            f'music_directory "{music}"\n'
            f'playlist_directory "{root}/playlists"\n'
            f'db_file "{root}/mpd.db"\n'
            f'state_file "{root}/mpd.state"\n'
            'bind_to_address "127.0.0.1"\n'
            f'port "{port}"\n'
            'audio_output {\n'
            ' type "null"\n'
            ' name "Windmill Silent Test"\n'
            ' mixer_type "software"\n'
            '}\n'
        )
        mpd = subprocess.Popen(
            ["mpd", "--no-daemon", "--stderr", str(config)],
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
        tui = None
        master = None
        try:
            for _ in range(30):
                if mpd.poll() is not None:
                    raise RuntimeError(f"Isolated MPD exited ({mpd.returncode})")
                if run("mpc", "-h", "127.0.0.1", "-p", str(port), "status").returncode == 0:
                    break
                time.sleep(0.1)
            else:
                raise RuntimeError("MPD did not start.")
            env = dict(
                os.environ, WINDMILL_DATA_DIR=str(root / "windmill"),
                WINDMILL_MUSIC_ROOT=str(music),
                WINDMILL_MPD_ADDR=f"127.0.0.1:{port}", TERM="xterm-256color",
                PATH=f"{bin_dir}:{os.environ.get('PATH', '/usr/bin')}",
            )
            assert run(str(BINARY), "add", str(music / "first.wav"),
                       "Custom Windmill Title", "Custom Artist", env=env).returncode == 0
            assert run("mpc", "-h", "127.0.0.1", "-p", str(port), "update").returncode == 0
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 35, 110, 0, 0))
            tui = subprocess.Popen([str(BINARY), "ui"], stdin=slave, stdout=slave,
                                   stderr=slave, env=env, start_new_session=True)
            os.close(slave)
            screen = bytearray()
            capture(master, tui, screen, 1)
            def ctl(*args):
                res = run("playerctl", "-p", "windmill", *args, env=env)
                if res.returncode:
                    raise AssertionError(f"playerctl {args}: {res.stderr}")
                return res.stdout.strip()
            assert "windmill" in run("playerctl", "-l", env=env).stdout
            assert ctl("status") == "Stopped"
            os.write(master, b"m")
            capture(master, tui, screen, 1)
            assert ctl("status") == "Playing"
            assert ctl("metadata", "--format", "{{title}}") == "Custom Windmill Title"
            assert ctl("metadata", "--format", "{{artist}}") == "Custom Artist"
            assert float(ctl("position")) >= 0
            ctl("pause")
            capture(master, tui, screen, .5)
            assert ctl("status") == "Paused"
            ctl("play")
            capture(master, tui, screen, .5)
            assert ctl("status") == "Playing"
            ctl("volume", "0.25")
            capture(master, tui, screen, .5)
            assert abs(float(ctl("volume")) - .25) < .02
            ctl("position", "3")
            capture(master, tui, screen, .5)
            assert 2.9 <= float(ctl("position")) <= 5.0
            # Windmill terminal arrows use the MPD clock; z returns to pre-skip.
            os.write(master,b"\x1b[C")
            capture(master,tui,screen,.4)
            assert 8 <= float(ctl("position")) <= 11, "MPD right arrow did not skip 5 seconds"
            os.write(master,b"z")
            capture(master,tui,screen,.4)
            assert 2.9 <= float(ctl("position")) <= 5.5, "MPD skip undo failed"
            # A second MPD queue item must update actual track metadata on Next.
            assert run("mpc", "-h", "127.0.0.1", "-p", str(port),
                       "add", "second.wav").returncode == 0
            capture(master, tui, screen, 1)
            ctl("next")
            capture(master, tui, screen, .7)
            assert "second.wav" in ctl("metadata", "--format", "{{title}}")
            ctl("previous")
            capture(master, tui, screen, .7)
            assert ctl("metadata", "--format", "{{title}}") == "Custom Windmill Title"
            ctl("stop")
            capture(master, tui, screen, .5)
            assert ctl("status") == "Stopped"
            # Switch from MPD to mpv while retaining the same Windmill bus name.
            os.write(master, b"1p")  # Library tab, play selected media through mpv
            capture(master,tui,screen,1.0)
            assert ctl("status") == "Playing", f"mpv state: {ctl('status')}"
            assert ctl("metadata","--format","{{title}}") == "Custom Windmill Title"
            ctl("pause")
            capture(master,tui,screen,.4)
            assert ctl("status")=="Paused"
            ctl("play")
            capture(master,tui,screen,.4)
            assert ctl("status")=="Playing"
            ctl("volume","0.37")
            capture(master,tui,screen,.4)
            assert abs(float(ctl("volume"))-.37)<.03
            ctl("position","2")
            # IPC and the TUI's cached position update asynchronously.
            # Observe convergence rather than assuming a fixed 400ms latency.
            observed=[]
            for _ in range(12):
                capture(master,tui,screen,.2)
                position=float(ctl("position"))
                observed.append(position)
                if 1.9<=position<=6.5:
                    break
            else:
                raise AssertionError(f"mpv position did not converge after seek: {observed}; status={ctl('status')}")
            # Jump to an exact timestamp without going through playerctl.
            os.write(master,b"g0:12\r")
            capture(master,tui,screen,.6)
            assert 11.8<=float(ctl("position"))<=14.0, "mpv jump-to-time failed"
            os.write(master,b"z")
            capture(master,tui,screen,.6)
            assert 1.8<=float(ctl("position"))<=7.0, "mpv seek undo failed"
            # Click the progress bar's midpoint in an 110x35 terminal.
            # Ratatui progress region is in the left half around row 20.
            os.write(master,b"\x1b[<0;32;21M")
            capture(master,tui,screen,.6)
            assert 7.0<=float(ctl("position"))<=14.5, (
                f"mpv mouse progress click failed: {ctl('position')}"
            )
            os.write(master,b"z")
            capture(master,tui,screen,.5)
            assert 1.8<=float(ctl("position"))<=8.0, "mpv mouse click undo failed"
            ctl("stop")
            capture(master,tui,screen,.4)
            assert ctl("status")=="Stopped"
            os.write(master, b"q")
            assert tui.wait(timeout=3) == 0
            assert b"Error:" not in screen, "The TUI reported an operation error"
            assert "windmill" not in run("playerctl", "-l", env=env).stdout.splitlines()
            print("PASS: MPD/mpv: MPRIS and terminal arrows, jump-to-time, mouse progress seek and undo")
        finally:
            if tui is not None and tui.poll() is None:
                tui.terminate()
                tui.wait(timeout=5)
            if master is not None:
                os.close(master)
            if mpd.poll() is None:
                mpd.terminate()
                mpd.wait(timeout=5)


if __name__ == "__main__":
    main()
