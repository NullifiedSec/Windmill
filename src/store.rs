// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mashrur Rahman Rawnok (NullifiedSec)
use rusqlite::{Connection, OptionalExtension, Result, params};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Media {
    pub id: i64,
    pub source: String,
    pub title: String,
    pub creator: String,
}
pub struct Store {
    conn: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS media (
              id INTEGER PRIMARY KEY, source TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL, creator TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tags (
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
              tag TEXT NOT NULL COLLATE NOCASE, PRIMARY KEY(media_id,tag)
            );
            CREATE TABLE IF NOT EXISTS lyric_offsets (
              source TEXT PRIMARY KEY,
              offset_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS lyric_manual_sources (
              source TEXT PRIMARY KEY,
              lrclib_id INTEGER NOT NULL CHECK(lrclib_id > 0)
            );
            CREATE TABLE IF NOT EXISTS assets (
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
              kind TEXT NOT NULL CHECK(kind IN ('audio','video')),
              path TEXT NOT NULL,
              PRIMARY KEY(media_id,kind)
            );
            CREATE TABLE IF NOT EXISTS collections (name TEXT PRIMARY KEY COLLATE NOCASE);
            CREATE TABLE IF NOT EXISTS membership (
              collection_name TEXT NOT NULL REFERENCES collections(name) ON DELETE CASCADE,
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
              PRIMARY KEY(collection_name,media_id)
            );
            CREATE TABLE IF NOT EXISTS media_preferences (
              media_id INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
              rating INTEGER NOT NULL CHECK(rating IN(-1,0,1))
            );
            CREATE TABLE IF NOT EXISTS listen_stats (
              media_id INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
              listens INTEGER NOT NULL DEFAULT 0,
              skips INTEGER NOT NULL DEFAULT 0,
              total_seconds INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS play_queue (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
              sort_index INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlist_order (
              collection_name TEXT NOT NULL REFERENCES collections(name) ON DELETE CASCADE,
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
              ordinal INTEGER NOT NULL,
              PRIMARY KEY(collection_name,media_id)
            );",
        )?;
        Ok(Self { conn })
    }
    pub fn add(&self, source: &str, title: &str, creator: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO media(source,title,creator) VALUES (?1,?2,?3)
           ON CONFLICT(source) DO UPDATE SET title=excluded.title,creator=excluded.creator",
            params![source, title, creator],
        )?;
        self.conn
            .query_row("SELECT id FROM media WHERE source=?1", [source], |r| {
                r.get(0)
            })
    }
    pub fn get(&self, id: i64) -> Result<Media> {
        self.conn.query_row(
            "SELECT id,source,title,creator FROM media WHERE id=?1",
            [id],
            |r| {
                Ok(Media {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    title: r.get(2)?,
                    creator: r.get(3)?,
                })
            },
        )
    }
    pub fn all(&self) -> Result<Vec<Media>> {
        let mut q = self
            .conn
            .prepare("SELECT id,source,title,creator FROM media ORDER BY id")?;
        q.query_map([], |r| {
            Ok(Media {
                id: r.get(0)?,
                source: r.get(1)?,
                title: r.get(2)?,
                creator: r.get(3)?,
            })
        })?
        .collect()
    }
    pub fn tags_for(&self, id: i64) -> Result<Vec<String>> {
        let mut q = self
            .conn
            .prepare("SELECT tag FROM tags WHERE media_id=?1 ORDER BY tag COLLATE NOCASE")?;
        q.query_map([id], |r| r.get(0))?.collect()
    }
    pub fn top_tags(&self, limit: usize) -> Result<Vec<String>> {
        let mut q = self.conn.prepare(
            "SELECT tag FROM tags
            GROUP BY tag COLLATE NOCASE
            ORDER BY COUNT(*) DESC, tag COLLATE NOCASE
            LIMIT ?1",
        )?;
        q.query_map([limit as i64], |r| r.get(0))?.collect()
    }
    pub fn collections(&self) -> Result<Vec<String>> {
        let mut q = self
            .conn
            .prepare("SELECT name FROM collections ORDER BY name COLLATE NOCASE")?;
        q.query_map([], |r| r.get(0))?.collect()
    }
    pub fn has(&self, source: &str) -> Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM media WHERE source=?1)",
            [source],
            |r| r.get(0),
        )
    }
    pub fn lyrics_offset_optional(&self, source: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT offset_ms FROM lyric_offsets WHERE source=?1",
                [source],
                |r| r.get(0),
            )
            .optional()
    }
    pub fn lyrics_offset(&self, source: &str) -> Result<i64> {
        Ok(self.lyrics_offset_optional(source)?.unwrap_or(0))
    }
    /// When a previously-streamed track gets downloaded, keep its existing
    /// timing correction under the original media identity. Existing explicit
    /// zero offsets are respected, not mistaken for missing data.
    pub fn lyrics_offset_with_legacy(&self, source: &str, previous: Option<&str>) -> Result<i64> {
        if let Some(value) = self.lyrics_offset_optional(source)? {
            return Ok(value);
        }
        if let Some(previous) = previous.filter(|s| *s != source)
            && let Some(value) = self.lyrics_offset_optional(previous)?
        {
            self.set_lyrics_offset(source, value)?;
            return Ok(value);
        }
        self.lyrics_offset(source)
    }
    /// An explicit LRCLIB track choice belongs to the canonical recording URL.
    /// Manual selections override auto search (and local sidecars) until reset.
    pub fn manual_lyrics_id(&self, source: &str) -> Result<Option<u64>> {
        self.conn
            .query_row(
                "SELECT lrclib_id FROM lyric_manual_sources WHERE source=?1",
                [source],
                |row| row.get::<_, u64>(0),
            )
            .optional()
    }
    pub fn set_manual_lyrics_id(&self, source: &str, id: u64) -> Result<()> {
        if source.is_empty() || id == 0 || id > i64::MAX as u64 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.conn.execute(
            "INSERT INTO lyric_manual_sources(source,lrclib_id) VALUES(?1,?2)
             ON CONFLICT(source) DO UPDATE SET lrclib_id=excluded.lrclib_id",
            params![source, id as i64],
        )?;
        Ok(())
    }
    pub fn clear_manual_lyrics_id(&self, source: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM lyric_manual_sources WHERE source=?1", [source])?;
        Ok(())
    }
    pub fn set_lyrics_offset(&self, source: &str, ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lyric_offsets(source,offset_ms) VALUES(?1,?2)
            ON CONFLICT(source) DO UPDATE SET offset_ms=excluded.offset_ms",
            params![source, ms],
        )?;
        Ok(())
    }
    pub fn asset(&self, id: i64, kind: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.conn
            .query_row(
                "SELECT path FROM assets WHERE media_id=?1 AND kind=?2",
                params![id, kind],
                |r| r.get(0),
            )
            .optional()
    }
    pub fn save_asset(&self, id: i64, kind: &str, path: &Path) -> Result<()> {
        if !matches!(kind, "audio" | "video") {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let path = path.to_string_lossy();
        self.conn.execute(
            "INSERT INTO assets(media_id,kind,path) VALUES (?1,?2,?3)
            ON CONFLICT(media_id,kind) DO UPDATE SET path=excluded.path",
            params![id, kind, path.as_ref()],
        )?;
        Ok(())
    }
    pub fn tag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO tags(media_id,tag) VALUES (?1,?2)",
            params![id, tag],
        )?;
        Ok(())
    }
    pub fn untag(&self, id: i64, tag: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM tags WHERE media_id=?1 AND tag=?2",
            params![id, tag],
        )?;
        Ok(())
    }
    pub fn create_collection(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO collections(name) VALUES (?1)",
            [name],
        )?;
        Ok(())
    }
    pub fn add_to_collection(&self, name: &str, id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO membership(collection_name,media_id) VALUES (?1,?2)
         ON CONFLICT DO NOTHING",
            params![name, id],
        )?;
        Ok(())
    }
    /// Atomic and idempotent playlist import. Existing media metadata/tags are
    /// preserved, and duplicate videos share one catalog identity.
    /// Queue entries have stable row IDs and may contain duplicate songs.
    pub fn queue_entries(&self) -> Result<Vec<(i64, Media)>> {
        let mut q = self.conn.prepare(
            "SELECT q.id,m.id,m.source,m.title,m.creator
            FROM play_queue q JOIN media m ON m.id=q.media_id
            ORDER BY q.sort_index,q.id",
        )?;
        q.query_map([], |r| {
            Ok((
                r.get(0)?,
                Media {
                    id: r.get(1)?,
                    source: r.get(2)?,
                    title: r.get(3)?,
                    creator: r.get(4)?,
                },
            ))
        })?
        .collect()
    }
    pub fn queue_add(&self, media_id: i64) -> Result<i64> {
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sort_index),-1)+1 FROM play_queue",
            [],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO play_queue(media_id,sort_index) VALUES(?1,?2)",
            params![media_id, next],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    pub fn queue_remove(&self, entry_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM play_queue WHERE id=?1", [entry_id])?;
        Ok(())
    }
    pub fn queue_clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM play_queue", [])?;
        Ok(())
    }
    pub fn queue_reorder(&self, ids: &[i64]) -> Result<()> {
        let old = self.queue_entries()?;
        if old.len() != ids.len()
            || old.iter().any(|(id, _)| !ids.contains(id))
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let tx = self.conn.unchecked_transaction()?;
        for (index, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE play_queue SET sort_index=?1 WHERE id=?2",
                params![index as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn queue_collection(&self, name: &str) -> Result<usize> {
        let items = self.collection_items(name)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM play_queue", [])?;
        for (i, m) in items.iter().enumerate() {
            tx.execute(
                "INSERT INTO play_queue(media_id,sort_index) VALUES(?1,?2)",
                params![m.id, i as i64],
            )?;
        }
        tx.commit()?;
        Ok(items.len())
    }
    pub fn import_playlist(&self, name: &str, videos: &[crate::youtube::Video]) -> Result<usize> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO collections(name) VALUES (?1)",
            [name],
        )?;
        let mut added = 0usize;
        for (ordinal, video) in videos.iter().enumerate() {
            let n = tx.execute(
                "INSERT OR IGNORE INTO media(source,title,creator) VALUES(?1,?2,?3)",
                params![video.url, video.title, video.creator],
            )?;
            added += n;
            let id: i64 =
                tx.query_row("SELECT id FROM media WHERE source=?1", [&video.url], |r| {
                    r.get(0)
                })?;
            tx.execute(
                "INSERT OR IGNORE INTO membership(collection_name,media_id) VALUES(?1,?2)",
                params![name, id],
            )?;
            tx.execute(
                "INSERT INTO playlist_order(collection_name,media_id,ordinal)
                VALUES(?1,?2,?3) ON CONFLICT(collection_name,media_id)
                DO UPDATE SET ordinal=excluded.ordinal",
                params![name, id, ordinal as i64],
            )?;
        }
        tx.commit()?;
        Ok(added)
    }
    pub fn collection_items(&self, name: &str) -> Result<Vec<Media>> {
        let mut q = self.conn.prepare(
            "SELECT m.id,m.source,m.title,m.creator FROM media m
          JOIN membership c ON c.media_id=m.id
          LEFT JOIN playlist_order p ON p.media_id=m.id AND p.collection_name=c.collection_name
          WHERE c.collection_name=?1 ORDER BY p.ordinal IS NULL,p.ordinal,m.id",
        )?;
        q.query_map([name], |r| {
            Ok(Media {
                id: r.get(0)?,
                source: r.get(1)?,
                title: r.get(2)?,
                creator: r.get(3)?,
            })
        })?
        .collect()
    }
    pub fn rate(&self, id: i64, rating: i64) -> Result<()> {
        if !(-1..=1).contains(&rating) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.conn.execute(
            "INSERT INTO media_preferences(media_id,rating)
            VALUES(?1,?2) ON CONFLICT(media_id)
            DO UPDATE SET rating=excluded.rating",
            params![id, rating],
        )?;
        Ok(())
    }
    pub fn rating(&self, id: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT rating FROM media_preferences WHERE media_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    pub fn record_listen(&self, id: i64, seconds: i64) -> Result<()> {
        let seconds = seconds.clamp(0, 86400);
        self.conn.execute(
            "INSERT INTO listen_stats(media_id,listens,skips,total_seconds)
            VALUES(?1,?2,?3,?4) ON CONFLICT(media_id) DO UPDATE SET
              listens=listens+excluded.listens,
              skips=skips+excluded.skips,
              total_seconds=total_seconds+excluded.total_seconds",
            params![
                id,
                if seconds >= 30 { 1 } else { 0 },
                if seconds < 30 { 1 } else { 0 },
                seconds
            ],
        )?;
        Ok(())
    }
    pub fn preference_reason(&self, id: i64) -> Result<String> {
        let rating = self.rating(id)?;
        let (plays, skips, seconds): (i64, i64, i64) = self
            .conn
            .query_row(
                "SELECT listens,skips,total_seconds FROM listen_stats WHERE media_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .unwrap_or((0, 0, 0));
        Ok(format!(
            "{} · {plays} listens · {skips} quick skips · {seconds}s played",
            match rating {
                1 => "Liked",
                -1 => "Disliked",
                _ => "Not rated",
            }
        ))
    }
    pub fn recommend<'a>(&self, tags: impl Iterator<Item = &'a str>) -> Result<Vec<(i64, Media)>> {
        let requested: Vec<String> = tags
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty())
            .collect();
        let mut out = Vec::new();
        for media in self.all()? {
            let mut q = self
                .conn
                .prepare("SELECT tag FROM tags WHERE media_id=?1")?;
            let existing: Vec<String> = q
                .query_map([media.id], |r| r.get::<_, String>(0))?
                .collect::<Result<_>>()?;
            let matched = requested
                .iter()
                .filter(|t| existing.iter().any(|e| e.to_lowercase() == **t))
                .count() as i64;
            let (listens, skips): (i64, i64) = self
                .conn
                .query_row(
                    "SELECT listens,skips FROM listen_stats WHERE media_id=?1",
                    [media.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .unwrap_or((0, 0));
            let rating = self.rating(media.id)?;
            // Weighted but explainable: tags form the basis, explicit dislike
            // strongly suppresses a track; listening history refines ordering.
            let score = matched * 5 + rating * 12 + listens.min(8) * 2 - skips.min(10) * 2;
            if score > 0 {
                out.push((score, media));
            }
        }
        out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_and_listening_statistics_influence_recommendations() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("lib.db");
        {
            let store = Store::open(&db).unwrap();
            let a = store.add("a", "A", "Singer").unwrap();
            let b = store.add("b", "B", "Singer").unwrap();
            store.tag(a, "dreamy").unwrap();
            store.tag(b, "dreamy").unwrap();
            assert_eq!(store.recommend(["dreamy"].into_iter()).unwrap().len(), 2);
            store.rate(a, 1).unwrap();
            store.rate(b, -1).unwrap();
            store.record_listen(a, 80).unwrap();
            store.record_listen(b, 8).unwrap();
            let ranked = store.recommend(["dreamy"].into_iter()).unwrap();
            assert_eq!(ranked.len(), 1);
            assert_eq!(ranked[0].1.source, "a");
        }
        let store = Store::open(&db).unwrap();
        assert!(store.preference_reason(1).unwrap().contains("Liked"));
        assert!(store.recommend(["dreamy"].into_iter()).unwrap()[0].0 > 5);
    }
    #[test]
    fn queue_persists_duplicates_reorder_and_collection_loading() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("db");
        let a;
        let b;
        {
            let store = Store::open(&db).unwrap();
            a = store.add("a", "A", "Artist").unwrap();
            b = store.add("b", "B", "Artist").unwrap();
            store.create_collection("Mix").unwrap();
            store.add_to_collection("Mix", b).unwrap();
            store.add_to_collection("Mix", a).unwrap();
            let first = store.queue_add(a).unwrap();
            let second = store.queue_add(a).unwrap();
            let third = store.queue_add(b).unwrap();
            assert_eq!(store.queue_entries().unwrap().len(), 3);
            store.queue_reorder(&[third, first, second]).unwrap();
            assert!(store.queue_reorder(&[first, second, second]).is_err());
        }
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store
                .queue_entries()
                .unwrap()
                .iter()
                .map(|(_, m)| m.source.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "a"]
        );
        assert_eq!(store.queue_collection("Mix").unwrap(), 2);
        assert_eq!(store.queue_entries().unwrap().len(), 2);
        store.queue_clear().unwrap();
        assert!(store.queue_entries().unwrap().is_empty());
    }
    #[test]
    fn playlist_import_is_atomic_idempotent_and_ordered() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let a = crate::youtube::Video {
            id: "HyHNuVaZJ-k".into(),
            title: "A".into(),
            creator: "X".into(),
            url: "https://www.youtube.com/watch?v=HyHNuVaZJ-k".into(),
            duration_seconds: None,
        };
        let b = crate::youtube::Video {
            id: "abcdefABC_1".into(),
            title: "B".into(),
            creator: "Y".into(),
            url: "https://www.youtube.com/watch?v=abcdefABC_1".into(),
            duration_seconds: None,
        };
        assert_eq!(
            s.import_playlist("My Mix", &[b.clone(), a.clone()])
                .unwrap(),
            2
        );
        s.tag(s.all().unwrap()[0].id, "saved").unwrap();
        assert_eq!(
            s.import_playlist("My Mix", &[b.clone(), a.clone()])
                .unwrap(),
            0
        );
        assert_eq!(
            s.collection_items("My Mix")
                .unwrap()
                .iter()
                .map(|m| m.title.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "A"]
        );
        assert_eq!(s.all().unwrap().len(), 2);
        assert_eq!(s.tags_for(s.all().unwrap()[0].id).unwrap(), vec!["saved"]);
        assert!(s.import_playlist("", &[a]).is_err());
    }
    #[test]
    fn flexible_tags_and_collections_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let id;
        {
            let s = Store::open(&path).unwrap();
            id = s
                .add("https://youtu.be/test", "Feel Good Inc.", "Gorillaz")
                .unwrap();
            assert_eq!(
                s.add("https://youtu.be/test", "Feel Good Inc.", "Gorillaz")
                    .unwrap(),
                id
            );
            s.tag(id, "Windmill").unwrap();
            s.tag(id, "windmill").unwrap();
            s.create_collection("The Windmill").unwrap();
            s.add_to_collection("The Windmill", id).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.collection_items("The Windmill").unwrap().len(), 1);
        assert_eq!(s.recommend(["windmill"].into_iter()).unwrap()[0].0, 5);
        s.untag(id, "windmill").unwrap();
        assert!(s.recommend(["windmill"].into_iter()).unwrap().is_empty());
    }
    #[test]
    fn tag_and_collection_queries_are_independent() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let id = s.add("some-path.mp3", "Song", "Unknown").unwrap();
        s.tag(id, "peaceful").unwrap();
        s.tag(id, "Windmill").unwrap();
        s.create_collection("My Mood").unwrap();
        s.add_to_collection("My Mood", id).unwrap();
        assert_eq!(s.tags_for(id).unwrap(), vec!["peaceful", "Windmill"]);
        assert_eq!(s.collections().unwrap(), vec!["My Mood"]);
        assert!(s.has("some-path.mp3").unwrap());
        assert!(!s.has("missing").unwrap());
    }
    #[test]
    fn manual_lyrics_selection_survives_restart_and_can_be_cleared() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("windmill.sqlite");
        {
            let store = Store::open(&db).unwrap();
            assert_eq!(store.manual_lyrics_id("youtube:video").unwrap(), None);
            store
                .set_manual_lyrics_id("youtube:video", 36174111)
                .unwrap();
            store
                .set_manual_lyrics_id("youtube:video", 36174112)
                .unwrap();
            store.set_lyrics_offset("youtube:video", 1234).unwrap();
            assert!(store.set_manual_lyrics_id("youtube:video", 0).is_err());
        }
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store.manual_lyrics_id("youtube:video").unwrap(),
            Some(36174112)
        );
        assert_eq!(store.lyrics_offset("youtube:video").unwrap(), 1234);
        store.clear_manual_lyrics_id("youtube:video").unwrap();
        assert_eq!(store.manual_lyrics_id("youtube:video").unwrap(), None);
        assert_eq!(store.lyrics_offset("youtube:video").unwrap(), 1234);
    }
    #[test]
    fn video_offset_survives_local_asset_change() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        s.set_lyrics_offset("/tmp/downloaded-music-video.mp4", 13_250)
            .unwrap();
        let original = "https://www.youtube.com/watch?v=HyHNuVaZJ-k";
        assert_eq!(
            s.lyrics_offset_with_legacy(original, Some("/tmp/downloaded-music-video.mp4"))
                .unwrap(),
            13_250
        );
        assert_eq!(s.lyrics_offset(original).unwrap(), 13_250);
        s.set_lyrics_offset(original, 0).unwrap();
        assert_eq!(
            s.lyrics_offset_with_legacy(original, Some("/tmp/downloaded-music-video.mp4"))
                .unwrap(),
            0
        );
        assert_eq!(s.lyrics_offset_with_legacy("other", None).unwrap(), 0);
    }
    #[test]
    fn lyric_adjustment_persists_without_modifying_files() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("windmill.db");
        {
            let store = Store::open(&db).unwrap();
            assert_eq!(store.lyrics_offset("file.flac").unwrap(), 0);
            store.set_lyrics_offset("file.flac", 375).unwrap();
            store.set_lyrics_offset("video", -500).unwrap();
        }
        let store = Store::open(&db).unwrap();
        assert_eq!(store.lyrics_offset("file.flac").unwrap(), 375);
        assert_eq!(store.lyrics_offset("video").unwrap(), -500);
    }
    #[test]
    fn most_used_tags_are_stable_and_local() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let a = s.add("a", "A", "Artist").unwrap();
        let b = s.add("b", "B", "Artist").unwrap();
        s.tag(a, "peaceful").unwrap();
        s.tag(b, "peaceful").unwrap();
        s.tag(a, "night").unwrap();
        assert_eq!(s.top_tags(2).unwrap(), vec!["peaceful", "night"]);
    }
    #[test]
    fn local_assets_keep_one_catalog_identity() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let id = s
            .add(
                "https://www.youtube.com/watch?v=HyHNuVaZJ-k",
                "Video",
                "Gorillaz",
            )
            .unwrap();
        s.save_asset(id, "audio", Path::new("/tmp/example.opus"))
            .unwrap();
        s.save_asset(id, "video", Path::new("/tmp/example.mp4"))
            .unwrap();
        assert_eq!(s.all().unwrap().len(), 1);
        assert_eq!(
            s.asset(id, "audio").unwrap().as_deref(),
            Some("/tmp/example.opus")
        );
        assert_eq!(
            s.asset(id, "video").unwrap().as_deref(),
            Some("/tmp/example.mp4")
        );
        assert!(s.save_asset(id, "banana", Path::new("/tmp/abc")).is_err());
    }
    #[test]
    fn recommendation_is_deterministic() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let a = s.add("a", "A", "X").unwrap();
        let b = s.add("b", "B", "X").unwrap();
        s.tag(a, "calm").unwrap();
        s.tag(b, "calm").unwrap();
        s.tag(b, "instrumental").unwrap();
        let r = s.recommend(["calm", "instrumental"].into_iter()).unwrap();
        assert_eq!(
            r.iter().map(|(n, m)| (*n, m.id)).collect::<Vec<_>>(),
            vec![(10, b), (5, a)]
        );
    }
}
