//! Feed engine: validates incoming envelopes (author must be the sender),
//! persists them to the store, and produces outgoing posts.

use std::time::{SystemTime, UNIX_EPOCH};

use tox_store::{PostKind, PostRow, PostSource, Store};

use crate::envelope::{
    Comment, DirReq, DirResp, Envelope, OutboxReq, OutboxResp, Post, PostChunk, Profile, Reaction,
};
use crate::{MAX_ENVELOPE_BYTES, MAX_POST_CHARS};

/// Reason an incoming message was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    NotProtocol,
    Malformed,
    AuthorMismatch,
    TooLarge,
}

/// Outcome of processing one incoming friend message.
#[derive(Debug)]
pub enum Incoming {
    /// Accepted and persisted as a timeline entry.
    Persisted(Envelope),
    /// A profile update (stored into friends metadata by the caller).
    Profile(Profile),
    /// A long-post fragment was stored, but the post is not complete yet.
    Chunk,
    /// A directory request from a friend.
    DirReq(DirReq),
    /// A directory response from a friend.
    DirResp(DirResp),
    /// A public-outbox request from a friend.
    OutboxReq(OutboxReq),
    /// A public-outbox response from a friend.
    OutboxResp(OutboxResp),
    /// Rejected; the caller may fall back to treating it as plain chat.
    Rejected(Reject),
}

pub struct FeedEngine {
    store: Store,
}

impl FeedEngine {
    pub fn new(store: Store) -> Self {
        FeedEngine { store }
    }

    /// Access the underlying store (e.g. for thread queries).
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Process a raw friend message. `sender_pk` is the friend's public key
    /// (64 hex) as reported by the Tox layer — the only trusted identity.
    pub fn handle_incoming(&self, sender_pk: &str, raw: &str) -> Incoming {
        let Some(env) = Envelope::decode(raw) else {
            return Incoming::Rejected(Reject::NotProtocol);
        };
        if env.wire_len() > MAX_ENVELOPE_BYTES {
            return Incoming::Rejected(Reject::TooLarge);
        }
        // Author field must match the actual sender (no impersonation).
        if env.author() != sender_pk {
            return Incoming::Rejected(Reject::AuthorMismatch);
        }
        let received_at = now_ms();
        match env.clone() {
            Envelope::Profile(p) => Incoming::Profile(p),
            Envelope::PostChunk(c) => {
                if let Some(post) = self.handle_post_chunk(&c, sender_pk, received_at) {
                    Incoming::Persisted(Envelope::Post(post))
                } else {
                    Incoming::Chunk
                }
            }
            Envelope::DirReq(req) => Incoming::DirReq(req),
            Envelope::DirResp(resp) => Incoming::DirResp(resp),
            Envelope::OutboxReq(req) => Incoming::OutboxReq(req),
            Envelope::OutboxResp(resp) => Incoming::OutboxResp(resp),
            other => {
                self.persist(&other, sender_pk, received_at);
                Incoming::Persisted(other)
            }
        }
    }

    /// Write an envelope to the timeline store. Returns `false` on duplicate.
    pub fn persist(&self, env: &Envelope, author: &str, received_at: i64) -> bool {
        let row = match env {
            Envelope::Post(p) => Some(PostRow {
                id: p.id.clone(),
                author: author.to_string(),
                kind: PostKind::Post,
                parent_id: None,
                text: Some(p.text.clone()),
                emoji: None,
                ts: p.ts,
                received_at,
                source: PostSource::FriendDirect,
                channel_id: None,
                is_public: p.public,
            }),
            Envelope::Comment(c) => Some(PostRow {
                id: c.id.clone(),
                author: author.to_string(),
                kind: PostKind::Comment,
                parent_id: Some(c.reply_to.clone()),
                text: Some(c.text.clone()),
                emoji: None,
                ts: c.ts,
                received_at,
                source: PostSource::FriendDirect,
                channel_id: None,
                is_public: false,
            }),
            Envelope::Reaction(r) => {
                let _ = self.store.delete_reaction(author, &r.reply_to);
                Some(PostRow {
                    id: r.id.clone(),
                    author: author.to_string(),
                    kind: PostKind::Reaction,
                    parent_id: Some(r.reply_to.clone()),
                    text: None,
                    emoji: Some(r.emoji.clone()),
                    ts: r.ts,
                    received_at,
                    source: PostSource::FriendDirect,
                    channel_id: None,
                    is_public: false,
                })
            }
            Envelope::Profile(_)
            | Envelope::PostChunk(_)
            | Envelope::SyncReq(_)
            | Envelope::SyncPosts(_)
            | Envelope::DirReq(_)
            | Envelope::DirResp(_)
            | Envelope::OutboxReq(_)
            | Envelope::OutboxResp(_) => None,
        };
        match row {
            Some(row) => self.store.post_upsert(&row).unwrap_or(false),
            None => false,
        }
    }

    /// Create a new post (validated), persist it locally as self-published,
    /// and return the envelope to fan out to friends.
    pub fn publish_post(&self, author_pk: &str, text: &str) -> Result<Post, String> {
        if text.is_empty() {
            return Err("post text is empty".to_string());
        }
        if text.chars().count() > MAX_POST_CHARS {
            return Err(format!("post too long (max {MAX_POST_CHARS} chars)"));
        }
        let post = Post::new(author_pk, text);
        if Envelope::Post(post.clone()).wire_len() > MAX_ENVELOPE_BYTES {
            return Err("post exceeds 1300-byte envelope limit".to_string());
        }
        let row = PostRow {
            id: post.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(post.text.clone()),
            emoji: None,
            ts: post.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: post.public,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        Ok(post)
    }

    /// Create a public post and persist it locally.
    pub fn publish_public_post(&self, author_pk: &str, text: &str) -> Result<Post, String> {
        if text.is_empty() {
            return Err("post text is empty".to_string());
        }
        if text.chars().count() > MAX_POST_CHARS {
            return Err(format!("post too long (max {MAX_POST_CHARS} chars)"));
        }
        let mut post = Post::new(author_pk, text);
        post.public = true;
        if Envelope::Post(post.clone()).wire_len() > MAX_ENVELOPE_BYTES {
            return Err("post exceeds 1300-byte envelope limit".to_string());
        }
        let row = PostRow {
            id: post.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(post.text.clone()),
            emoji: None,
            ts: post.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: true,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        Ok(post)
    }

    /// Create a long post, persist it locally, and return the post plus the
    /// chunk envelopes to fan out to friends.
    pub fn publish_long_post(&self, author_pk: &str, text: &str) -> Result<(Post, Vec<Envelope>), String> {
        if text.is_empty() {
            return Err("post text is empty".to_string());
        }
        if text.chars().count() > 50_000 {
            return Err("post too long (max 50000 chars)".to_string());
        }
        let post = Post::new(author_pk, text);
        let row = PostRow {
            id: post.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(post.text.clone()),
            emoji: None,
            ts: post.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: post.public,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        let chunks = split_post_chunks(&post);
        Ok((post, chunks))
    }

    /// Create a long public post, persist it locally, and return chunk envelopes.
    pub fn publish_long_public_post(
        &self,
        author_pk: &str,
        text: &str,
    ) -> Result<(Post, Vec<Envelope>), String> {
        if text.is_empty() {
            return Err("post text is empty".to_string());
        }
        if text.chars().count() > 50_000 {
            return Err("post too long (max 50000 chars)".to_string());
        }
        let mut post = Post::new(author_pk, text);
        post.public = true;
        let row = PostRow {
            id: post.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(post.text.clone()),
            emoji: None,
            ts: post.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: true,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        let chunks = split_post_chunks(&post);
        Ok((post, chunks))
    }

    /// Store an incoming long-post fragment. Returns the assembled `Post` when
    /// all fragments have arrived.
    fn handle_post_chunk(&self, c: &PostChunk, sender_pk: &str, received_at: i64) -> Option<Post> {
        if c.author != sender_pk || c.total == 0 || c.n >= c.total {
            return None;
        }
        self.store
            .chunk_upsert(&c.post_id, sender_pk, c.n, c.total, c.ts, &c.part)
            .ok()?;
        let count = self.store.chunk_count(&c.post_id, sender_pk).ok()?;
        if count as u32 != c.total {
            return None;
        }
        let parts = self.store.chunk_parts(&c.post_id, sender_pk).ok()?;
        if parts.len() as u32 != c.total {
            return None;
        }
        let text = parts.into_iter().map(|(_, p)| p).collect::<String>();
        let post = Post {
            v: crate::envelope::PROTOCOL_VERSION,
            id: c.post_id.clone(),
            author: c.author.clone(),
            ts: c.ts,
            text,
            public: false,
        };
        let row = PostRow {
            id: post.id.clone(),
            author: sender_pk.to_string(),
            kind: PostKind::Post,
            parent_id: None,
            text: Some(post.text.clone()),
            emoji: None,
            ts: post.ts,
            received_at,
            source: PostSource::FriendDirect,
            channel_id: None,
            is_public: post.public,
        };
        let _ = self.store.post_upsert(&row);
        let _ = self.store.chunk_delete(&post.id, sender_pk);
        Some(post)
    }

    /// Create a comment on a post, persist locally, return envelope.
    pub fn publish_comment(
        &self,
        author_pk: &str,
        reply_to: &str,
        text: &str,
    ) -> Result<Comment, String> {
        if text.is_empty() {
            return Err("comment text is empty".to_string());
        }
        let comment = Comment::new(author_pk, reply_to, text);
        if Envelope::Comment(comment.clone()).wire_len() > MAX_ENVELOPE_BYTES {
            return Err("comment exceeds 1300-byte envelope limit".to_string());
        }
        let row = PostRow {
            id: comment.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Comment,
            parent_id: Some(comment.reply_to.clone()),
            text: Some(comment.text.clone()),
            emoji: None,
            ts: comment.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: false,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        Ok(comment)
    }

    /// Like/reaction on a post, persisted locally.
    pub fn publish_reaction(
        &self,
        author_pk: &str,
        reply_to: &str,
        emoji: &str,
    ) -> Result<Reaction, String> {
        let reaction = Reaction::new(author_pk, reply_to, emoji);
        self.store
            .delete_reaction(author_pk, reply_to)
            .map_err(|e| format!("store error: {e}"))?;
        let row = PostRow {
            id: reaction.id.clone(),
            author: author_pk.to_string(),
            kind: PostKind::Reaction,
            parent_id: Some(reaction.reply_to.clone()),
            text: None,
            emoji: Some(reaction.emoji.clone()),
            ts: reaction.ts,
            received_at: now_ms(),
            source: PostSource::SelfPublished,
            channel_id: None,
            is_public: false,
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        Ok(reaction)
    }

    /// Following feed: newest posts by the given authors.
    pub fn timeline(&self, authors: &[String], limit: u32) -> Vec<PostRow> {
        self.store.timeline(authors, limit).unwrap_or_default()
    }

    /// Search locally stored posts by text.
    pub fn search_posts(&self, query: &str, limit: u32) -> Vec<PostRow> {
        self.store.search_posts(query, limit).unwrap_or_default()
    }

    /// Posts authored by one user, newest first.
    pub fn posts_by_author(&self, author: &str, limit: u32) -> Vec<PostRow> {
        self.store.posts_by_author(author, limit).unwrap_or_default()
    }

    /// Latest timestamp we have seen from `author` (posts/comments/reactions).
    pub fn latest_ts_for_author(&self, author: &str) -> Option<i64> {
        self.store.latest_ts_for_author(author).unwrap_or(None)
    }

    /// Build `sync_posts` payload items: all locally stored envelopes authored
    /// by `author` with `ts > since`, oldest first.
    pub fn self_posts_since(&self, author: &str, since: i64, limit: u32) -> Vec<Envelope> {
        self.store
            .posts_by_author_since(author, since, limit)
            .unwrap_or_default()
            .into_iter()
            .filter_map(row_to_envelope)
            .collect()
    }

    /// Public posts known locally with `ts > since`, oldest first.
    pub fn public_posts_since(&self, since: i64, limit: u32) -> Vec<Envelope> {
        self.store
            .public_posts_since(since, limit)
            .unwrap_or_default()
            .into_iter()
            .filter_map(row_to_envelope)
            .collect()
    }

    /// Persist all items from a `sync_posts` message. Returns the envelopes
    /// that were actually new (not duplicates).
    pub fn handle_sync_posts(&self, sender_pk: &str, items: Vec<Envelope>) -> Vec<Envelope> {
        let received_at = now_ms();
        items
            .into_iter()
            .filter_map(|item| {
                // Re-validate each item: its author must be the friend who sent it.
                if item.author() != sender_pk {
                    return None;
                }
                if item.wire_len() > MAX_ENVELOPE_BYTES {
                    return None;
                }
                match item {
                    Envelope::Post(_) | Envelope::Comment(_) | Envelope::Reaction(_) => {
                        if self.persist(&item, sender_pk, received_at) {
                            Some(item)
                        } else {
                            None
                        }
                    }
                    Envelope::Profile(_)
                    | Envelope::PostChunk(_)
                    | Envelope::SyncReq(_)
                    | Envelope::SyncPosts(_)
                    | Envelope::DirReq(_)
                    | Envelope::DirResp(_)
                    | Envelope::OutboxReq(_)
                    | Envelope::OutboxResp(_) => None,
                }
            })
            .collect()
    }
}

fn row_to_envelope(row: PostRow) -> Option<Envelope> {
    match row.kind {
        PostKind::Post => Some(Envelope::Post(Post {
            v: 1,
            id: row.id,
            author: row.author,
            ts: row.ts,
            text: row.text.unwrap_or_default(),
            public: row.is_public,
        })),
        PostKind::Comment => Some(Envelope::Comment(Comment {
            v: 1,
            id: row.id,
            author: row.author,
            ts: row.ts,
            reply_to: row.parent_id.unwrap_or_default(),
            text: row.text.unwrap_or_default(),
        })),
        PostKind::Reaction => Some(Envelope::Reaction(Reaction {
            v: 1,
            id: row.id,
            author: row.author,
            ts: row.ts,
            reply_to: row.parent_id.unwrap_or_default(),
            emoji: row.emoji.unwrap_or_default(),
        })),
    }
}

impl Envelope {
    fn author(&self) -> &str {
        match self {
            Envelope::Post(p) => &p.author,
            Envelope::Comment(c) => &c.author,
            Envelope::Reaction(r) => &r.author,
            Envelope::Profile(p) => &p.author,
            Envelope::PostChunk(c) => &c.author,
            Envelope::SyncReq(s) => &s.author,
            Envelope::DirReq(r) => &r.author,
            Envelope::DirResp(r) => &r.author,
            Envelope::OutboxReq(r) => &r.author,
            Envelope::OutboxResp(r) => &r.author,
            Envelope::SyncPosts(s) => &s.author,
        }
    }
}

fn split_post_chunks(post: &Post) -> Vec<Envelope> {
    let chars: Vec<char> = post.text.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();

    for ch in chars {
        let mut candidate = current.clone();
        candidate.push(ch);
        let probe = Envelope::PostChunk(PostChunk {
            v: crate::envelope::PROTOCOL_VERSION,
            post_id: post.id.clone(),
            author: post.author.clone(),
            ts: post.ts,
            n: parts.len() as u32,
            total: 0,
            part: candidate.clone(),
        });
        if probe.wire_len() <= MAX_ENVELOPE_BYTES || current.is_empty() {
            current = candidate;
        } else {
            parts.push(std::mem::take(&mut current));
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    let total = parts.len() as u32;
    parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| {
            Envelope::PostChunk(PostChunk {
                v: crate::envelope::PROTOCOL_VERSION,
                post_id: post.id.clone(),
                author: post.author.clone(),
                ts: post.ts,
                n: i as u32,
                total,
                part,
            })
        })
        .collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn incoming_post_is_persisted() {
        let engine = FeedEngine::new(store());
        let author = "aabb";
        let post = Post::new(author, "第一条帖子");
        let raw = Envelope::Post(post.clone()).encode();

        match engine.handle_incoming(author, &raw) {
            Incoming::Persisted(env) => assert_eq!(env, Envelope::Post(post)),
            other => panic!("expected persisted, got {other:?}"),
        }
        assert_eq!(engine.timeline(&[author.to_string()], 10).len(), 1);
    }

    #[test]
    fn spoofed_author_is_rejected() {
        let engine = FeedEngine::new(store());
        let post = Post::new("attacker", "假装是别人发的");
        let raw = Envelope::Post(post.clone()).encode();
        assert!(matches!(
            engine.handle_incoming("victim", &raw),
            Incoming::Rejected(Reject::AuthorMismatch)
        ));
    }

    #[test]
    fn plain_chat_falls_through() {
        let engine = FeedEngine::new(store());
        assert!(matches!(
            engine.handle_incoming("aabb", "普通聊天消息"),
            Incoming::Rejected(Reject::NotProtocol)
        ));
    }

    #[test]
    fn publish_and_read_back() {
        let engine = FeedEngine::new(store());
        let author = "me".to_string();
        let post = engine.publish_post(&author, "你好，去中心化！").unwrap();
        let tl = engine.timeline(&[author.clone()], 10);
        assert_eq!(tl.len(), 1);
        assert_eq!(tl[0].id, post.id);
        assert_eq!(tl[0].source, PostSource::SelfPublished);
    }

    #[test]
    fn publish_comment_then_thread() {
        let engine = FeedEngine::new(store());
        let me = "me".to_string();
        let post = engine.publish_post(&me, "主贴").unwrap();
        let comment = engine.publish_comment(&me, &post.id, "评论").unwrap();
        let thread = engine.store.thread_for(&post.id).unwrap();
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].id, comment.id);
    }

    #[test]
    fn self_posts_since_returns_self_authored_envelopes() {
        let engine = FeedEngine::new(store());
        let me = "me".to_string();
        let post = engine.publish_post(&me, "离线补丁").unwrap();
        let comment = engine.publish_comment(&me, &post.id, "评论").unwrap();

        let items = engine.self_posts_since(&me, 0, 10);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|e| matches!(e, Envelope::Post(p) if p.id == post.id)));
        assert!(items.iter().any(|e| matches!(e, Envelope::Comment(c) if c.id == comment.id)));

        let after = engine.self_posts_since(&me, i64::MAX, 10);
        assert!(after.is_empty());
    }

    #[test]
    fn handle_sync_posts_persists_valid_and_ignores_spoofed() {
        let engine = FeedEngine::new(store());
        let friend = "friend-pk".to_string();
        let valid = Post::new(&friend, "好友离线时发的帖子");
        let spoofed = Post::new("attacker", "伪造帖子");

        let persisted = engine.handle_sync_posts(&friend, vec![Envelope::Post(valid.clone()), Envelope::Post(spoofed)]);
        assert_eq!(persisted.len(), 1);
        assert!(matches!(&persisted[0], Envelope::Post(p) if p.id == valid.id));

        let tl = engine.timeline(&[friend.clone()], 10);
        assert_eq!(tl.len(), 1);
        assert_eq!(tl[0].id, valid.id);
    }

    #[test]
    fn user_can_only_react_once_per_post() {
        let engine = FeedEngine::new(store());
        let me = "me".to_string();
        let post = engine.publish_post(&me, "帖子").unwrap();
        let _r1 = engine.publish_reaction(&me, &post.id, "👍").unwrap();
        let r2 = engine.publish_reaction(&me, &post.id, "❤️").unwrap();
        let thread = engine.store().thread_for(&post.id).unwrap();
        let reactions: Vec<_> = thread.iter().filter(|r| r.kind == PostKind::Reaction).collect();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].id, r2.id);
        assert_eq!(reactions[0].emoji.as_deref(), Some("❤️"));
    }

    #[test]
    fn long_post_is_split_and_reassembled() {
        let engine = FeedEngine::new(store());
        let me = "me".to_string();
        let long_text = "长文测试".repeat(500); // > 1000 chars
        let (post, chunks) = engine.publish_long_post(&me, &long_text).unwrap();
        assert!(chunks.len() > 1);
        assert_eq!(engine.timeline(&[me.clone()], 10).len(), 1);

        // Receiver engine: feed chunks one by one; only the last one completes.
        let rx = FeedEngine::new(store());
        for (i, env) in chunks.iter().enumerate() {
            let raw = env.encode();
            let outcome = rx.handle_incoming(&me, &raw);
            if i + 1 == chunks.len() {
                match outcome {
                    Incoming::Persisted(Envelope::Post(p)) => {
                        assert_eq!(p.id, post.id);
                        assert_eq!(p.text, long_text);
                    }
                    other => panic!("expected final post, got {other:?}"),
                }
            } else {
                assert!(matches!(outcome, Incoming::Chunk), "expected Chunk, got {outcome:?}");
            }
        }
        let tl = rx.timeline(&[me.clone()], 10);
        assert_eq!(tl.len(), 1);
        assert_eq!(tl[0].text.as_deref(), Some(long_text.as_str()));
    }
}
