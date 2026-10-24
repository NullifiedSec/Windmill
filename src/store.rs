use rusqlite::{Connection, Result, params};
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
            CREATE TABLE IF NOT EXISTS collections (name TEXT PRIMARY KEY COLLATE NOCASE);
            CREATE TABLE IF NOT EXISTS membership (
              collection_name TEXT NOT NULL REFERENCES collections(name) ON DELETE CASCADE,
              media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
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
    pub fn collection_items(&self, name: &str) -> Result<Vec<Media>> {
        let mut q = self.conn.prepare(
            "SELECT m.id,m.source,m.title,m.creator FROM media m
          JOIN membership c ON c.media_id=m.id WHERE c.collection_name=?1 ORDER BY m.id",
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
            let score = requested
                .iter()
                .filter(|t| existing.iter().any(|e| e.to_lowercase() == **t))
                .count() as i64;
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
        assert_eq!(s.recommend(["windmill"].into_iter()).unwrap()[0].0, 1);
        s.untag(id, "windmill").unwrap();
        assert!(s.recommend(["windmill"].into_iter()).unwrap().is_empty());
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
            vec![(2, b), (1, a)]
        );
    }
}
