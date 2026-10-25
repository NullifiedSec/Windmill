"""End-to-end manual LRCLIB link selection, virtual padding and persistence.

Uses disposable media and fake curl: never connects to LRCLIB, and never
touches the user's actual playback/library. A live API test exists in Rust.
"""
import fcntl,os,pty,select,sqlite3,stat,struct,subprocess,tempfile,termios,time,json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT/"target/debug/windmill"
TRACK="https://lrclib.net/tracks/36174111"
SOURCE_ID=36174111

def capture(fd,proc,content,seconds):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline and proc.poll() is None:
        if select.select([fd],[],[],.075)[0]:
            try:
                chunk=os.read(fd,32768)
            except OSError:return
            content.extend(chunk)
            if b"\x1b[6n" in chunk:os.write(fd,b"\x1b[1;1R")

def launch(env):
    master,slave=pty.openpty()
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",41,140,0,0))
    proc=subprocess.Popen(
        ["dbus-run-session","--",str(BIN),"ui"],
        stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True)
    os.close(slave)
    out=bytearray()
    capture(master,proc,out,.5)
    os.write(master,b"p")
    capture(master,proc,out,.7)
    return proc,master,out

def finish(proc,master):
    try:
        if proc.poll() is None:
            os.write(master,b"q")
            assert proc.wait(timeout=5)==0
    finally:
        if proc.poll() is None:proc.terminate();proc.wait(timeout=5)
        os.close(master)

def main():
    assert BIN.is_file(),"cargo build --locked first"
    with tempfile.TemporaryDirectory(prefix="windmill-manual-lrclib-") as folder:
        root=Path(folder)
        media=root/"track.wav"
        subprocess.run(["ffmpeg","-hide_banner","-loglevel","error",
            "-f","lavfi","-i","anullsrc=r=44100:cl=stereo",
            "-t","15",str(media)],check=True,timeout=20)
        media.with_suffix(".lrc").write_text("[00:01.00]LOCAL SIDE-CAR\n")
        bin_dir=root/"bin";bin_dir.mkdir()
        mpv=bin_dir/"mpv"
        mpv.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n')
        mpv.chmod(0o755)
        json_file=root/"fixture.json"
        json_file.write_text(json.dumps({
            "id":SOURCE_ID,"trackName":"A different version","artistName":"No match",
            "duration":45.0,"instrumental":False,
            "syncedLyrics":"[00:01.00]PINNED LRCLIB WORDS\n[00:18.00]Different ending"
        }))
        curl=bin_dir/"curl"
        curl.write_text('#!/bin/sh\n'
            f'if printf "%s " "$@" | grep -q "https://lrclib.net/api/get/{SOURCE_ID}"; then\n'
            f'  cat "{json_file}"\n'
            'else\n  echo "unexpected network request" >&2\n  exit 22\nfi\n')
        curl.chmod(0o755)
        env={**os.environ,"WINDMILL_DATA_DIR":str(root/"data"),
             "WINDMILL_MPD_ADDR":"127.0.0.1:0",
             "WINDMILL_DISABLE_VISUALIZER":"1",
             "PATH":str(bin_dir)+":"+os.environ.get("PATH","/usr/bin"),
             "TERM":"xterm-256color"}
        add=subprocess.run([str(BIN),"add",str(media),"Other title","Other artist"],
            env=env,capture_output=True,text=True)
        assert add.returncode==0,add.stderr
        proc,fd,out=launch(env)
        try:
            assert b"LOCAL SIDE-CAR" in out,"initial local lyrics not loaded"
            os.write(fd,b"M");capture(fd,proc,out,.12)
            os.write(fd,TRACK.encode()+b"\r")
            capture(fd,proc,out,.95)
            assert b"PINNED LRCLIB WORDS" in out,"manual lyrics not shown"
            assert b"Manual LRCLIB" in out,"source provenance not displayed"
            # Ratatui may redraw individual terminal cells between chunks.
            # The complete geometry/offset math is covered by Rust unit tests.
            assert b"virtual" in out,"longer lyric timeline indicator not rendered"
            conn=sqlite3.connect(root/"data"/"library.db")
            got=conn.execute("SELECT source,lrclib_id FROM lyric_manual_sources").fetchall()
            conn.close()
            assert got==[(str(media),SOURCE_ID)],f"manual source not persisted: {got}"
            cache=root/"data"/"lyrics"/f"lrclib-{SOURCE_ID}.json"
            assert cache.is_file() and (stat.S_IMODE(cache.stat().st_mode)&0o077)==0
            # Invalid input must not replace previously pinned ID.
            os.write(fd,b"M");capture(fd,proc,out,.12)
            os.write(fd,b"https://evil.example/tracks/1\r")
            capture(fd,proc,out,.22)
            with sqlite3.connect(root/"data"/"library.db") as c:
                assert c.execute("SELECT lrclib_id FROM lyric_manual_sources").fetchone()==(SOURCE_ID,)
        finally:finish(proc,fd)
        # Fail any network request; the per-ID cache must restore pinned
        # lyrics without contacting LRCLIB or requiring matching metadata.
        curl.write_text('#!/bin/sh\nexit 22\n')
        proc,fd,out=launch(env)
        try:
            assert b"PINNED LRCLIB WORDS" in out,"offline pinned cache not restored"
            assert b"Manual LRCLIB" in out
            os.write(fd,b"U")
            fresh=bytearray()
            capture(fd,proc,fresh,.7)
            assert b"LOCAL SIDE-CAR" in fresh,"unpin did not restore automatic/local lyrics"
            with sqlite3.connect(root/"data"/"library.db") as c:
                assert c.execute("SELECT COUNT(*) FROM lyric_manual_sources").fetchone()==(0,)
        finally:finish(proc,fd)
    print("PASS: TUI manual link pin, mismatch bypass, virtual timeline, cache offline, invalid-link preservation, unpin")

if __name__=="__main__":main()
