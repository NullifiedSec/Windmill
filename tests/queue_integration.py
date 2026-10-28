"""Persisted cross-engine queue and mpv automatic advance smoke test.

Run: dbus-run-session -- python3 tests/queue_integration.py
"""
import fcntl,os,pathlib,pty,select,struct,subprocess,tempfile,termios,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
BIN=ROOT/"target/debug/windmill"
def capture(fd,p,buf,seconds):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline and p.poll() is None:
        if select.select([fd],[],[],.08)[0]:
            try:piece=os.read(fd,65536)
            except OSError:break
            buf.extend(piece)
            if b"\x1b[6n" in piece:os.write(fd,b"\x1b[1;1R")
def run(env,*cmd):
    result=subprocess.run(cmd,env=env,text=True,capture_output=True,timeout=8)
    assert result.returncode==0,(cmd,result.stderr)
    return result.stdout
def main():
    with tempfile.TemporaryDirectory(prefix="windmill-queue-") as folder:
        root=pathlib.Path(folder)
        path=root/"bin";path.mkdir()
        wrapper=path/"mpv"
        wrapper.write_text('#!/bin/sh\nexec /usr/bin/mpv --vo=null --ao=null "$@"\n')
        wrapper.chmod(0o755)
        env={**os.environ,"WINDMILL_DATA_DIR":str(root/"profile"),
             "WINDMILL_MPD_ADDR":"127.0.0.1:0","WINDMILL_DISABLE_VISUALIZER":"1","WINDMILL_DISABLE_ARTWORK":"1",
             "PATH":str(path)+":"+os.environ.get("PATH","/usr/bin"),
             "TERM":"xterm-256color"}
        for i,duration in [(1,2),(2,10)]:
            file=root/f"song{i}.wav"
            subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i",
                "anullsrc=r=44100:cl=stereo","-t",str(duration),str(file)],check=True,timeout=20)
            run(env,str(BIN),"add",str(file),f"Track {i}","Artist")
            run(env,str(BIN),"queue","add",str(i))
        assert "Track 1" in run(env,str(BIN),"queue","list")
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",35,115,0,0))
        tui=subprocess.Popen([str(BIN),"ui"],
            stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True)
        os.close(slave);output=bytearray()
        try:
            capture(master,tui,output,.65)
            for _ in range(5):
                os.write(master,b"\x1b[9;5u")  # Ctrl+Tab CSI-u
                capture(master,tui,output,.22)
            capture(master,tui,output,.35)
            assert b"Queue controls" in output and b"Track 1" in output, "Ctrl+Tab did not enter Queue"
            os.write(master,b"\r")
            capture(master,tui,output,.55)
            assert b"Track 1" in output
            # First file is two seconds; automatic advance must play second.
            capture(master,tui,output,2.5)
            title=run(env,"playerctl","-p","windmill","metadata","--format","{{title}}").strip()
            assert title=="Track 2",f"Queue did not auto-advance; actual title={title!r}"
            os.write(master,b"x")
            capture(master,tui,output,.4)
            assert run(env,"playerctl","-p","windmill","status").strip()=="Stopped"
            os.write(master,b"q")
            assert tui.wait(timeout=5)==0
            assert "Track 1" in run(env,str(BIN),"queue","list")
            assert "Track 2" in run(env,str(BIN),"queue","list")
            assert b"Error:" not in output
            print("PASS: queue persists, draws in sixth tab, mpv plays next track automatically, manual stop prevents autoplay")
        finally:
            if tui.poll() is None:tui.terminate();tui.wait(timeout=5)
            os.close(master)
if __name__=="__main__":main()
