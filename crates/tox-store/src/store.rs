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
    pub is_public: bool,
    pub sig: String,
}

/// A row in the `friends` table (a friend == someone you follow).
#[derive(Debug, Clone, PartialEq)]
pub struct FriendRow {
    pub toxid: String,
    pub nospam: String,
    pub name: String,
    pub avatar: String,
    pub bio: String,
    pub status: i64, // 0 offline, 1 online, 2 blocked
    pub added_at: i64,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryEntry {
    pub pubkey: String,
    pub toxid: String,
    pub name: String,
    pub avatar: String,
    pub relay: String,
    pub source: String,
    pub updated_at: i64,
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
  avatar      TEXT DEFAULT '',
  bio         TEXT DEFAULT '',
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
  is_public    INTEGER NOT NULL DEFAULT 0,
  sig          TEXT DEFAULT '',
  UNIQUE(id, author)
);
CREATE INDEX IF NOT EXISTS idx_posts_author ON posts(author);
CREATE INDEX IF NOT EXISTS idx_posts_ts     ON posts(ts DESC);
CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(parent_id);
CREATE TABLE IF NOT EXISTS post_chunks (
  post_id      TEXT NOT NULL,
  author       TEXT NOT NULL,
  idx          INTEGER NOT NULL,
  total        INTEGER NOT NULL,
  ts           INTEGER NOT NULL,
  part         TEXT NOT NULL,
  received_at  INTEGER NOT NULL,
  PRIMARY KEY (post_id, author, idx)
);
CREATE INDEX IF NOT EXISTS idx_post_chunks_post ON post_chunks(post_id, author);
CREATE TABLE IF NOT EXISTS directory (
  pubkey      TEXT PRIMARY KEY,
  toxid       TEXT DEFAULT '',
  name        TEXT DEFAULT '',
  avatar      TEXT DEFAULT '',
  relay       TEXT DEFAULT '',
  source      TEXT DEFAULT '',
  updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_directory_name ON directory(name);
";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate_friends_avatar(&conn);
        migrate_friends_bio(&conn);
        migrate_posts_is_public(&conn);
        Ok(Store { conn })
    }

    /// Open an in-memory database (used by tests and embedded scenarios).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate_friends_avatar(&conn);
        migrate_friends_bio(&conn);
        migrate_posts_is_public(&conn);
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
            "INSERT INTO friends(toxid, nospam, name, avatar, bio, status, added_at, last_seen)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(toxid) DO UPDATE SET
               nospam = excluded.nospam,
               name   = excluded.name,
               avatar = excluded.avatar,
               bio    = excluded.bio,
               status = excluded.status,
               last_seen = excluded.last_seen",
            params![
                f.toxid,
                f.nospam,
                f.name,
                f.avatar,
                f.bio,
                f.status,
                f.added_at,
                f.last_seen
            ],
        )?;
        Ok(())
    }

    pub fn friend_list(&self) -> Result<Vec<FriendRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT toxid, nospam, name, avatar, bio, status, added_at, last_seen FROM friends",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(FriendRow {
                toxid: r.get(0)?,
                nospam: r.get(1)?,
                name: r.get(2)?,
                avatar: r.get(3)?,
                bio: r.get(4)?,
                status: r.get(5)?,
                added_at: r.get(6)?,
                last_seen: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn friend_remove(&self, toxid: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM friends WHERE toxid = ?1", params![toxid])?;
        Ok(())
    }

    // --- directory -------------------------------------------------------------

    pub fn dir_upsert(&self, entry: &DirectoryEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO directory(pubkey, toxid, name, avatar, relay, source, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(pubkey) DO UPDATE SET
               toxid = excluded.toxid,
               name = excluded.name,
               avatar = excluded.avatar,
               relay = excluded.relay,
               source = excluded.source,
               updated_at = excluded.updated_at",
            params![
                entry.pubkey,
                entry.toxid,
                entry.name,
                entry.avatar,
                entry.relay,
                entry.source,
                entry.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn dir_search(&self, query: &str, limit: u32) -> Result<Vec<DirectoryEntry>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, toxid, name, avatar, relay, source, updated_at
             FROM directory WHERE name LIKE ?1 OR pubkey LIKE ?1
             ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit], |r| {
            Ok(DirectoryEntry {
                pubkey: r.get(0)?,
                toxid: r.get(1)?,
                name: r.get(2)?,
                avatar: r.get(3)?,
                relay: r.get(4)?,
                source: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn dir_all(&self, limit: u32) -> Result<Vec<DirectoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT pubkey, toxid, name, avatar, relay, source, updated_at
             FROM directory ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(DirectoryEntry {
                pubkey: r.get(0)?,
                toxid: r.get(1)?,
                name: r.get(2)?,
                avatar: r.get(3)?,
                relay: r.get(4)?,
                source: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    // --- posts -----------------------------------------------------------------

    /// Insert a timeline entry. Returns `false` if the (id, author) pair
    /// already exists (idempotent de-duplication).
    pub fn post_upsert(&self, p: &PostRow) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO posts
               (id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                p.is_public,
                p.sig,
            ],
        )?;
        Ok(n > 0)
    }

    pub fn post_get(&self, id: &str) -> Result<Option<PostRow>> {
        self.conn
            .query_row(
                "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
                 FROM posts WHERE id = ?1",
                params![id],
                row_to_post,
            )
            .optional()
    }

    /// Timeline: newest-first posts by the given authors (following feed).
    pub fn timeline(&self, authors: &[String], limit: u32) -> Result<Vec<PostRow>> {
        let mut sql = String::from(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
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

    /// Update the Ed25519 signature for a post.
    pub fn post_update_sig(&self, id: &str, sig: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE posts SET sig = ?1 WHERE id = ?2",
            params![sig, id],
        )?;
        Ok(())
    }

    /// Remove all reactions by one author on one post (single-reaction rule).
    pub fn delete_reaction(&self, author: &str, parent_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM posts WHERE author = ?1 AND parent_id = ?2 AND kind = 2",
            params![author, parent_id],
        )?;
        Ok(())
    }

    /// All comments/reactions attached to a post, including nested comment
    /// replies, oldest first.
    pub fn thread_for(&self, post_id: &str) -> Result<Vec<PostRow>> {
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE descendants(id) AS (
               SELECT id FROM posts WHERE parent_id = ?1
               UNION ALL
               SELECT p.id FROM posts p JOIN descendants d ON p.parent_id = d.id
             )
             SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
             FROM posts WHERE id IN (SELECT id FROM descendants) ORDER BY ts ASC",
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
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
             FROM posts WHERE author = ?1 AND ts > ?2
             ORDER BY ts ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![author, since, limit], row_to_post)?;
        rows.collect()
    }

    /// Public posts with `ts > since`, oldest first. Used for public outbox sync.
    pub fn public_posts_since(&self, since: i64, limit: u32) -> Result<Vec<PostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
             FROM posts WHERE kind = 0 AND is_public = 1 AND ts > ?1
             ORDER BY ts ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![since, limit], row_to_post)?;
        rows.collect()
    }

    /// Posts authored by one user, newest first.
    pub fn posts_by_author(&self, author: &str, limit: u32) -> Result<Vec<PostRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
             FROM posts WHERE author = ?1 AND kind = 0
             ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![author, limit], row_to_post)?;
        rows.collect()
    }

    /// Search posts by text content (case-insensitive), newest first.
    pub fn search_posts(&self, query: &str, limit: u32) -> Result<Vec<PostRow>> {
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare(
            "SELECT id, author, kind, parent_id, text, emoji, ts, received_at, source, channel_id, is_public, sig
             FROM posts WHERE kind = 0 AND text LIKE ?1 ESCAPE '\\'
             ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit], row_to_post)?;
        rows.collect()
    }

    // --- long-post chunks ------------------------------------------------------

    /// Store one fragment of a long post. Returns false if it was a duplicate.
    pub fn chunk_upsert(
        &self,
        post_id: &str,
        author: &str,
        idx: u32,
        total: u32,
        ts: i64,
        part: &str,
    ) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO post_chunks(post_id, author, idx, total, ts, part, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![post_id, author, idx, total, ts, part, now_ms()],
        )?;
        Ok(n > 0)
    }

    pub fn chunk_count(&self, post_id: &str, author: &str) -> Result<usize> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM post_chunks WHERE post_id = ?1 AND author = ?2",
            params![post_id, author],
            |r| r.get(0),
        )
    }

    pub fn chunk_parts(&self, post_id: &str, author: &str) -> Result<Vec<(u32, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT idx, part FROM post_chunks WHERE post_id = ?1 AND author = ?2 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![post_id, author], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect()
    }

    pub fn chunk_delete(&self, post_id: &str, author: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM post_chunks WHERE post_id = ?1 AND author = ?2",
            params![post_id, author],
        )?;
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn migrate_friends_avatar(conn: &Connection) {
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN avatar TEXT DEFAULT ''", []);
}

fn migrate_friends_bio(conn: &Connection) {
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN bio TEXT DEFAULT ''", []);
}

fn migrate_posts_is_public(conn: &Connection) {
    let _ = conn.execute(
        "ALTER TABLE posts ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE posts ADD COLUMN sig TEXT DEFAULT ''", []);
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
        is_public: r.get(10)?,
        sig: r.get(11)?,
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
            is_public: false,
            sig: String::new(),
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
            is_public: false,
            sig: String::new(),
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
            is_public: false,
            sig: String::new(),
        };
        store.post_upsert(&c).unwrap();
        let thread = store.thread_for("post-1").unwrap();
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].id, "c1");
    }

    #[test]
    fn thread_includes_nested_comment_replies() {
        let store = Store::open_in_memory().unwrap();
        let mk = |id: &str, parent: Option<&str>, ts: i64| PostRow {
            id: id.into(),
            author: "z".into(),
            kind: PostKind::Comment,
            parent_id: parent.map(|s| s.to_string()),
            text: Some("reply".into()),
            emoji: None,
            ts,
            received_at: ts,
            source: PostSource::FriendDirect,
            channel_id: None,
            is_public: false,
            sig: String::new(),
        };
        store.post_upsert(&mk("c1", Some("post-1"), 1)).unwrap();
        store.post_upsert(&mk("c2", Some("c1"), 2)).unwrap();
        store.post_upsert(&mk("c3", Some("c2"), 3)).unwrap();
        let thread = store.thread_for("post-1").unwrap();
        let ids: Vec<_> = thread.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["c1", "c2", "c3"]);
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
    fn friend_upsert_roundtrip_with_bio() {
        let store = Store::open_in_memory().unwrap();
        let f = FriendRow {
            toxid: "aabbccdd".into(),
            nospam: String::new(),
            name: "Alice".into(),
            avatar: String::new(),
            bio: "去中心化爱好者".into(),
            status: 1,
            added_at: 1,
            last_seen: Some(1),
        };
        store.friend_upsert(&f).unwrap();
        let list = store.friend_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].bio, "去中心化爱好者");
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
            is_public: false,
            sig: String::new(),
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
            is_public: false,
            sig: String::new(),
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

    #[test]
    fn long_post_chunks_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.chunk_upsert("p1", "alice", 0, 2, 1, "hello ").unwrap());
        assert!(!store.chunk_upsert("p1", "alice", 0, 2, 1, "hello ").unwrap()); // duplicate
        store.chunk_upsert("p1", "alice", 1, 2, 1, "world").unwrap();
        assert_eq!(store.chunk_count("p1", "alice").unwrap(), 2);
        let parts = store.chunk_parts("p1", "alice").unwrap();
        assert_eq!(parts, vec![(0, "hello ".to_string()), (1, "world".to_string())]);
        store.chunk_delete("p1", "alice").unwrap();
        assert_eq!(store.chunk_count("p1", "alice").unwrap(), 0);
    }

    #[test]
    fn search_posts_matches_text() {
        let store = Store::open_in_memory().unwrap();
        let mk = |id: &str, author: &str, text: &str, ts: i64| PostRow {
            id: id.into(),
            author: author.into(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(text.into()),
            emoji: None,
            ts,
            received_at: ts,
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: false,
            sig: String::new(),
        };
        store.post_upsert(&mk("a", "x", "hello world", 3)).unwrap();
        store.post_upsert(&mk("b", "x", "ToxSocial is cool", 5)).unwrap();
        store.post_upsert(&mk("c", "x", "hello again", 1)).unwrap();
        let rows = store.search_posts("hello", 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
    }
}
