//! Feed engine: validates incoming envelopes (author must be the sender),
//! persists them to the store, and produces outgoing posts.

use std::time::{SystemTime, UNIX_EPOCH};

use tox_store::{PostKind, PostRow, PostSource, Store};

use crate::envelope::{Comment, Envelope, Post, Profile, Reaction};
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
            }),
            Envelope::Reaction(r) => Some(PostRow {
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
            }),
            Envelope::Profile(_) | Envelope::SyncReq(_) | Envelope::SyncPosts(_) => None,
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
        };
        self.store
            .post_upsert(&row)
            .map_err(|e| format!("store error: {e}"))?;
        Ok(post)
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
}

impl Envelope {
    fn author(&self) -> &str {
        match self {
            Envelope::Post(p) => &p.author,
            Envelope::Comment(c) => &c.author,
            Envelope::Reaction(r) => &r.author,
            Envelope::Profile(p) => &p.author,
            Envelope::SyncReq(s) => &s.author,
            Envelope::SyncPosts(s) => &s.author,
        }
    }
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
}
