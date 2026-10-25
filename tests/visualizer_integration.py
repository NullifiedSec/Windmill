"""Isolated real PCM monitor -> Windmill spectrum renderer regression.

This test DOES NOT capture microphone or speaker audio. A fake parec stream
supplies a deterministic sine wave and fake pactl advertises a test monitor.
"""
import fcntl,os,pathlib,pty,select,struct,subprocess,tempfile,termios,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
BIN=ROOT/"target/debug/windmill"
def capture(fd,p,seen,secs):
    end=time.monotonic()+secs
    while time.monotonic()<end and p.poll() is None:
        if select.select([fd],[],[],.1)[0]:
            try:out=os.read(fd,65536)
            except OSError:break
            seen.extend(out)
            if b"\x1b[6n" in out:os.write(fd,b"\x1b[1;1R")
def main():
    with tempfile.TemporaryDirectory(prefix="windmill-viz-") as tmp:
        root=pathlib.Path(tmp)
        bin_dir=root/"bin";bin_dir.mkdir()
        pactl=bin_dir/"pactl";pactl.write_text("#!/bin/sh\necho fixture_sink\n");pactl.chmod(0o755)
        parec=bin_dir/"parec";parec.write_text("""#!/usr/bin/env python3
import math,sys,time
assert any("fixture_sink.monitor" in a for a in sys.argv)
frame=bytearray()
for j in range(2048):
    x=int(16000*math.sin(2*math.pi*450*j/22050))
    frame+=x.to_bytes(2,"little",signed=True)
for i in range(70):
    sys.stdout.buffer.write(frame)
    sys.stdout.buffer.flush()
    time.sleep(.025)
""");parec.chmod(0o755)
        mpv=bin_dir/"mpv";mpv.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n');mpv.chmod(0o755)
        wav=root/"song.wav"
        subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i",
            "anullsrc=r=44100:cl=stereo","-t","5",str(wav)],check=True,timeout=10)
        env={**os.environ,
            "WINDMILL_DATA_DIR":str(root/"data"),
            "WINDMILL_MPD_ADDR":"127.0.0.1:0",
            "PATH":str(bin_dir)+":"+os.environ.get("PATH","/usr/bin"),
            "TERM":"xterm-256color"}
        env.pop("WINDMILL_DISABLE_VISUALIZER",None)
        result=subprocess.run([str(BIN),"add",str(wav),"Viz Fixture","Example"],
            env=env,capture_output=True,text=True)
        assert result.returncode==0,result.stderr
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",40,125,0,0))
        proc=subprocess.Popen(["dbus-run-session","--",str(BIN),"ui"],
            stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True)
        os.close(slave);out=bytearray()
        try:
            capture(master,proc,out,.55)
            os.write(master,b"p")
            capture(master,proc,out,1.4)
            assert b"Output spectrum" in out,"Real PCM spectrum panel not rendered"
            assert b"\xe2\x96\x88" in out,"Audio spectrum contains no active bars"
            os.write(master,b"q")
            assert proc.wait(timeout=5)==0
            print("PASS: non-microphone fake PCM samples create audio-reactive spectrum bars in the real TUI")
        finally:
            if proc.poll() is None:proc.terminate();proc.wait(timeout=5)
            os.close(master)
if __name__=="__main__":main()
