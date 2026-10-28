#!/usr/bin/env python3
"""Small public YouTube Music discovery bridge. No authentication required."""
import json
import re
import sys

from ytmusicapi import YTMusic

IDENTIFIER = re.compile(r"^[A-Za-z0-9_-]{10,120}$")
VIDEO = re.compile(r"^[A-Za-z0-9_-]{11}$")

def artists(item):
    values = item.get("artists") or []
    return ", ".join(a.get("name", "") for a in values if isinstance(a, dict)) or item.get("artist") or item.get("author") or "Unknown artist"

def song(item):
    vid = item.get("videoId")
    if not isinstance(vid,str) or not VIDEO.fullmatch(vid):
        return None
    return {
        "kind": "songs",
        "id": vid,
        "title": str(item.get("title") or "Untitled")[:200],
        "creator": artists(item)[:150],
        "duration": item.get("duration_seconds") or item.get("duration"),
        "url": "https://www.youtube.com/watch?v=" + vid,
    }

def result(item, category):
    if category=="songs":
        return song(item)
    token=item.get("browseId")
    if not isinstance(token,str) or not IDENTIFIER.fullmatch(token):
        return None
    title=str(item.get("title") or item.get("artist") or "Untitled")[:200]
    if category=="playlists":
        pid=token.removeprefix("VL")
        if not IDENTIFIER.fullmatch(pid): return None
        url="https://music.youtube.com/playlist?list="+pid
    else:
        url="https://music.youtube.com/browse/"+token
    return {
        "kind":category,"id":token,"title":title,
        "creator":artists(item)[:150],"url":url
    }

def main():
    command=sys.argv[1]
    api=YTMusic()
    if command=="search":
        category,query=sys.argv[2:4]
        if category not in ("songs","albums","artists","playlists"):
            raise ValueError("Invalid public search category")
        if not query.strip() or len(query)>180:raise ValueError("Invalid search length")
        found=api.search(query,filter=category,limit=20)
        print(json.dumps({"entries":[r for x in found if (r:=result(x,category)) is not None][:20]}))
    elif command=="browse":
        category,token=sys.argv[2:4]
        if category not in ("albums","artists") or not IDENTIFIER.fullmatch(token):
            raise ValueError("Invalid public browse target")
        if category=="albums":
            data=api.get_album(token)
            title=data.get("title") or "Album"
            tracks=data.get("tracks") or []
        else:
            data=api.get_artist(token)
            title=data.get("name") or "Artist"
            tracks=(data.get("songs") or {}).get("results") or []
        print(json.dumps({"title":str(title)[:120],
            "entries":[r for x in tracks if (r:=song(x)) is not None][:250]}))
    else:
        raise ValueError("Invalid command")
if __name__=="__main__":
    try: main()
    except Exception as exc:
        print(f"ytmusic: {type(exc).__name__}: {str(exc)[:220]}",file=sys.stderr)
        sys.exit(2)
