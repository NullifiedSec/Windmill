"""Playlist import/batch download E2E with fake yt-dlp and a disposable catalog."""
import os, pathlib, subprocess, tempfile, sqlite3

ROOT=pathlib.Path(__file__).resolve().parents[1]
BIN=ROOT/"target/debug/windmill"
URL="https://music.youtube.com/playlist?list=PLABCDEFGHI123456"
VIDS=["HyHNuVaZJ-k","abcdefABC_1"]
FAKE=r'''#!/usr/bin/env python3
import json,os,sys,pathlib
args=sys.argv[1:]
log=pathlib.Path(os.environ["FAKE_LOG"])
with log.open("a") as f:f.write(" ".join(args)+"\n")
if "--dump-single-json" in args:
    print(json.dumps({"title":"Fixture Mix","entries":[
        {"id":"HyHNuVaZJ-k","title":"Counting Stars","channel":"OneRepublic","duration":244},
        {"id":"abcdefABC_1","title":"Second","channel":"Somebody"},
        {"title":"Private"}]}))
    sys.exit(0)
if "--print" in args:
    output=pathlib.Path(args[args.index("-o")+1].replace("%(title).150B","Song").replace("%(id)s","fake").replace("%(ext)s","opus"))
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_bytes(b"OggSfake_fixture_data")
    print(output)
    sys.exit(0)
sys.exit(3)
'''

def run(env,*args):
    result=subprocess.run([str(BIN),*args],env=env,text=True,capture_output=True,timeout=15)
    assert result.returncode==0,(args,result.stderr,result.stdout)
    return result.stdout
def main():
    assert BIN.exists(),"cargo build --locked first"
    with tempfile.TemporaryDirectory(prefix="windmill-pl-") as base:
        root=pathlib.Path(base)
        bindir=root/"bin";bindir.mkdir()
        fake=bindir/"yt-dlp";fake.write_text(FAKE);fake.chmod(0o755)
        env={**os.environ,"WINDMILL_DATA_DIR":str(root/"profile"),
             "FAKE_LOG":str(root/"fetch.log"),
             "PATH":str(bindir)+":"+os.environ.get("PATH","/usr/bin")}
        result=run(env,"playlist","preview",URL)
        assert "Fixture Mix" in result and "Counting Stars" in result
        assert "[4:04]" in result, f"Playlist preview must show source duration before download: {result}"
        assert not (root/"profile"/"downloads").exists(),"Preview downloaded media"
        result=run(env,"playlist","import",URL)
        assert "2 tracks" in result
        result=run(env,"playlist","import",URL)
        assert "0 new" in result,"Duplicate import should be idempotent"
        con=sqlite3.connect(root/"profile"/"library.db")
        assert con.execute("SELECT COUNT(*) FROM media").fetchone()==(2,)
        assert con.execute("SELECT COUNT(*) FROM membership").fetchone()==(2,)
        names=con.execute("SELECT m.title FROM media m JOIN playlist_order p ON p.media_id=m.id ORDER BY ordinal").fetchall()
        assert names==[("Counting Stars",),("Second",)]
        con.close()
        result=run(env,"playlist","download",URL,"audio")
        assert "saved" in result
        assert run(env,"playlist","download",URL,"audio").count("already downloaded")==2
        log=(root/"fetch.log").read_text()
        assert log.count("--print")==2,"Expected one download per unique song"
        con=sqlite3.connect(root/"profile"/"library.db")
        assert con.execute("SELECT COUNT(*) FROM assets WHERE kind='audio'").fetchone()==(2,)
        print("PASS: public playlist preview-only, idempotent import/order, explicit batch audio and skip completed files")
if __name__=="__main__":main()
