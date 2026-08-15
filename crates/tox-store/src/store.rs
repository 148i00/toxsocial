//! SQLite store implementation.

use rusqlite::{params, Connection, Result, OptionalExtension};
use std::path::Path;

/// Kind of a timeline entry (mirrors TSP/1 kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostKind {
    Post = 0,
    Comment = 1,
    Reaction = 2,
}

impl PostKind {
    pub fn from_raw(raw: i64) -> Self {
        match raw {
            1 => PostKind::Comment,
            2 => PostKind::Reaction,
            _ => PostKind::Post,
        }
    }
}

/// Source of a timeline entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostSource {
    SelfPublished = 0,
    FriendDirect = 1,
    Channel = 2,
}

/// A row in the `posts` table.
#[derive(Debug, Clone, PartialEq)]
pub struct PostRow {
    pub id: String,
    pub author: String,
    pub kind: PostKind,
    pub parent_id: Option<String>,
    pub text: Option<String>,
    pub emoji: Option<String>,
    pub ts: i64,
    pub received_at: i64,
    pub source: PostSource,
    pub channel_id: Option<String>,
}

/// A row in the `friends` table (a friend == someone you follow).
#[derive(Debug, Clone, PartialEq)]
pub struct FriendRow {
    pub toxid: String,
    pub nospam: String,
    pub name: String,
    pub status: i64, // 0 offline, 1 online, 2 blocked
    pub added_at: i64,
    pub last_seen: Option<i64>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS friends (
  toxid       TEXT PRIMARY KEY,
  nospam      TEXT,
  name        TEXT,
  status      INTEGER NOT NULL DEFAULT 0,
  added_at    INTEGER NOT NULL,
  last_seen   INTEGER
);
CREATE TABLE IF NOT EXISTS posts (
  id           TEXT PRIMARY KEY,
  author       TEXT NOT NULL,
  kind         INTEGER NOT NULL,
  parent_id    TEXT,
  text         TEXT,
  emoji        TEXT,
  ts           INTEGER NOT NULL,
  received_at  INTEGER NOT NULL,
  source       INTEGER NOT NULL DEFAULT 1,
  channel_id   TEXT,
  UNIQUE(id, author)
);
CREATE INDEX IF NOT EXISTS idx_posts_author ON posts(author);
CREATE INDEX IF NOT EXISTS idx_posts_ts     ON posts(ts DESC);
CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(parent_id);
";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// Open an in-memory database (used by tests and embedded scenarios).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    // --- kv ---------------------------------------------------------------

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
    }

    // --- friends -------------------------------------------------------------

    pub fn friend_upsert(&self, f: &FriendRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO friends(toxid, nospam, name, status, added_at, last_seen)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(toxid) DO UPDATE SET
               nospam = excluded.nospam,
               name   = excluded.name,
               status = excluded.status,
               last_seen = excluded.last_seen",
            params![f.toxid, f.nospam, f.name, f.status, f.added_at, f.last_seen],
        )?;
        Ok(())
    }

    pub fn friend_list(&self) -> Result<Vec<FriendRow>> {
        let mut stmt =
            self.conn
                .prepare("SELECT toxid, nospam, name, status, added_at, last_seen FROM friends")?;
        let rows = stmt.query_map([], |r| {
            Ok(FriendRow {
                toxid: r.get(0)?,
                nospam: r.get(1)?,
                name: r.get(2)?,
                status: r.get(3)?,
                added_at: r.get(4)?,
                last_seen: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn friend_remove(&self, toxid: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM friends WHERE toxid = ?1", params![toxid])?;
        Ok(())
    }

    // --- posts -----------------------------------------------------------------

    /// Insert a timeline entry. Returns `false` if the (id, author) pair
    /// already exists (idempotent de-duplication).
    pub fn post_upsert(&self, p: &PostRow) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO posts
               (id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                p.id,
                p.author,
                p.kind as i64,
                p.parent_id,
                p.text,
                p.emoji,
                p.ts,
                p.received_at,
                p.source as i64,
                p.channel_id,
            ],
        )?;
        Ok(n > 0)
    }

    pub fn post_get(&self, id: &str) -> Result<Option<PostRow>> {
        self.conn
            .query_row(
                "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id
                 FROM posts WHERE id = ?1",
                params![id],
                row_to_post,
            )
            .optional()
    }

    /// Timeline: newest-first posts by the given authors (following feed).
    pub fn timeline(&self, authors: &[String], limit: u32) -> Result<Vec<PostRow>> {
        let mut sql = String::from(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id
             FROM posts WHERE kind = 0 AND author IN (",
        );
        let placeholders: Vec<String> = (1..=authors.len()).map(|i| format!("?{i}")).collect();
        sql.push_str(&placeholders.join(","));
        sql.push_str(&format!(") ORDER BY ts DESC LIMIT {limit}"));

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            authors.iter().map(|a| a as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), row_to_post)?;
        rows.collect()
    }

    /// All comments/reactions attached to a post, oldest first.
    pub fn thread_for(&self, post_id: &str) -> Result<Vec<PostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id
             FROM posts WHERE parent_id = ?1 ORDER BY ts ASC",
        )?;
        let rows = stmt.query_map(params![post_id], row_to_post)?;
        rows.collect()
    }

    /// Latest timestamp ever stored for an author (posts/comments/reactions).
    /// Used as the `since` cursor when requesting offline backfill.
    pub fn latest_ts_for_author(&self, author: &str) -> Result<Option<i64>> {
        self.conn.query_row(
            "SELECT MAX(ts) FROM posts WHERE author = ?1",
            params![author],
            |r| r.get::<_, Option<i64>>(0),
        )
    }

    /// Rows authored by `author` with `ts > since`, oldest first.
    /// Used to build `sync_posts` responses.
    pub fn posts_by_author_since(
        &self,
        author: &str,
        since: i64,
        limit: u32,
    ) -> Result<Vec<PostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id
             FROM posts WHERE author = ?1 AND ts > ?2
             ORDER BY ts ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![author, since, limit], row_to_post)?;
        rows.collect()
    }
}

fn row_to_post(r: &rusqlite::Row) -> Result<PostRow> {
    Ok(PostRow {
        id: r.get(0)?,
        author: r.get(1)?,
        kind: PostKind::from_raw(r.get(2)?),
        parent_id: r.get(3)?,
        text: r.get(4)?,
        emoji: r.get(5)?,
        ts: r.get(6)?,
        received_at: r.get(7)?,
        source: match r.get::<_, i64>(8)? {
            0 => PostSource::SelfPublished,
            2 => PostSource::Channel,
            _ => PostSource::FriendDirect,
        },
        channel_id: r.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    #[test]
    fn post_upsert_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        let p = PostRow {
            id: "post-1".into(),
            author: "aabb".into(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some("hello".into()),
            emoji: None,
            ts: now(),
            received_at: now(),
            source: PostSource::FriendDirect,
            channel_id: None,
        };
        assert!(store.post_upsert(&p).unwrap());
        assert!(!store.post_upsert(&p).unwrap()); // duplicate
        assert_eq!(store.post_get("post-1").unwrap().unwrap(), p);
    }

    #[test]
    fn timeline_orders_by_ts_desc_and_filters_authors() {
        let store = Store::open_in_memory().unwrap();
        let mk = |id: &str, author: &str, ts: i64| PostRow {
            id: id.into(),
            author: author.into(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some("t".into()),
            emoji: None,
            ts,
            received_at: ts,
            source: PostSource::FriendDirect,
            channel_id: None,
        };
        store.post_upsert(&mk("a", "x", 3)).unwrap();
        store.post_upsert(&mk("b", "y", 5)).unwrap();
        store.post_upsert(&mk("c", "x", 1)).unwrap();
        let authors = vec!["x".to_string()];
        let tl = store.timeline(&authors, 10).unwrap();
        assert_eq!(tl.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
    }

    #[test]
    fn thread_groups_by_parent() {
        let store = Store::open_in_memory().unwrap();
        let c = PostRow {
            id: "c1".into(),
            author: "z".into(),
            kind: PostKind::Comment,
            parent_id: Some("post-1".into()),
            text: Some("nice".into()),
            emoji: None,
            ts: 2,
            received_at: 2,
            source: PostSource::FriendDirect,
            channel_id: None,
        };
        store.post_upsert(&c).unwrap();
        let thread = store.thread_for("post-1").unwrap();
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].id, "c1");
    }

    #[test]
    fn kv_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        store.kv_set("nickname", "Alice").unwrap();
        assert_eq!(store.kv_get("nickname").unwrap().as_deref(), Some("Alice"));
        store.kv_set("nickname", "Bob").unwrap();
        assert_eq!(store.kv_get("nickname").unwrap().as_deref(), Some("Bob"));
    }

    #[test]
    fn latest_ts_for_author_uses_all_kinds() {
        let store = Store::open_in_memory().unwrap();
        let mk = |id: &str, author: &str, kind: PostKind, ts: i64| PostRow {
            id: id.into(),
            author: author.into(),
            kind,
            parent_id: None,
            text: Some("t".into()),
            emoji: None,
            ts,
            received_at: ts,
            source: PostSource::SelfPublished,
            channel_id: None,
        };
        store.post_upsert(&mk("p1", "alice", PostKind::Post, 10)).unwrap();
        store.post_upsert(&mk("c1", "alice", PostKind::Comment, 20)).unwrap();
        store.post_upsert(&mk("r1", "bob", PostKind::Reaction, 30)).unwrap();
        assert_eq!(store.latest_ts_for_author("alice").unwrap(), Some(20));
        assert_eq!(store.latest_ts_for_author("bob").unwrap(), Some(30));
        assert_eq!(store.latest_ts_for_author("nobody").unwrap(), None);
    }

    #[test]
    fn posts_by_author_since_filters_and_orders() {
        let store = Store::open_in_memory().unwrap();
        let mk = |id: &str, author: &str, kind: PostKind, ts: i64| PostRow {
            id: id.into(),
            author: author.into(),
            kind,
            parent_id: None,
            text: Some("t".into()),
            emoji: None,
            ts,
            received_at: ts,
            source: PostSource::SelfPublished,
            channel_id: None,
        };
        store.post_upsert(&mk("a", "x", PostKind::Post, 1)).unwrap();
        store.post_upsert(&mk("b", "x", PostKind::Comment, 2)).unwrap();
        store.post_upsert(&mk("c", "x", PostKind::Post, 3)).unwrap();
        store.post_upsert(&mk("d", "y", PostKind::Post, 4)).unwrap();

        let rows = store.posts_by_author_since("x", 1, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        assert_eq!(rows[0].kind, PostKind::Comment);

        let limited = store.posts_by_author_since("x", 0, 2).unwrap();
        assert_eq!(limited.len(), 2);
    }
}
