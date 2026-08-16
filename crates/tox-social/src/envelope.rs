//! The `TSP/1` message envelope: versioned JSON messages exchanged between
//! friends over Tox friend messages.

use serde::{Deserialize, Serialize};

use crate::PROTOCOL_PREFIX;

pub const PROTOCOL_VERSION: u32 = 1;

/// A post broadcast to all friends (the unit of the timeline).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Post {
    pub v: u32,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "text")]
    pub text: String,
    #[serde(rename = "public", default)]
    pub public: bool,
    #[serde(rename = "sig", default)]
    pub sig: String,
}

/// A comment attached to a post.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Comment {
    pub v: u32,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "rt")]
    pub reply_to: String,
    #[serde(rename = "text")]
    pub text: String,
}

/// A reaction (like/emoji) attached to a post.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reaction {
    pub v: u32,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "rt")]
    pub reply_to: String,
    #[serde(rename = "e")]
    pub emoji: String,
}

/// Profile update (name / bio / avatar) broadcast to all friends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "bio")]
    pub bio: String,
    #[serde(rename = "avatar")]
    pub avatar: String,
    #[serde(rename = "avatar_len")]
    pub avatar_len: u64,
}

/// A fragment of a long post. The receiver assembles all fragments before
/// persisting the final `Post`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostChunk {
    pub v: u32,
    #[serde(rename = "pid")]
    pub post_id: String,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "n")]
    pub n: u32,
    #[serde(rename = "total")]
    pub total: u32,
    #[serde(rename = "part")]
    pub part: String,
}

/// One entry in a shared public directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirEntry {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "pubkey")]
    pub pubkey: String,
    #[serde(rename = "toxid", default)]
    pub toxid: String,
    #[serde(rename = "avatar", default)]
    pub avatar: String,
    #[serde(rename = "relay", default)]
    pub relay: String,
}

/// Ask friends (and optionally friends-of-friends) for directory entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirReq {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "q", default)]
    pub query: String,
    #[serde(rename = "depth", default)]
    pub depth: u32,
}

/// Directory entries returned by a friend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirResp {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "items", default)]
    pub items: Vec<DirEntry>,
}

/// Request public posts from a friend (and optionally friends-of-friends).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboxReq {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "since", default)]
    pub since: i64,
    #[serde(rename = "depth", default)]
    pub depth: u32,
}

/// Public posts returned by a friend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboxResp {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "items", default)]
    pub items: Vec<Envelope>,
}

/// Tell a friend that we are removing them (mutual unfollow).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Unfriend {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
}

/// Pull-based backfill request sent when a friend comes online (M4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncReq {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "since")]
    pub since: i64,
}

/// Backfill response: posts/comments the peer missed (M4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncPosts {
    pub v: u32,
    #[serde(rename = "a")]
    pub author: String,
    #[serde(rename = "ts")]
    pub ts: i64,
    #[serde(rename = "items")]
    pub items: Vec<Envelope>,
}

/// The full set of message kinds (tag = `t` field, snake_case).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Envelope {
    Post(Post),
    Comment(Comment),
    Reaction(Reaction),
    Profile(Profile),
    PostChunk(PostChunk),
    SyncReq(SyncReq),
    SyncPosts(SyncPosts),
    DirReq(DirReq),
    DirResp(DirResp),
    OutboxReq(OutboxReq),
    OutboxResp(OutboxResp),
    Unfriend(Unfriend),
}

impl Envelope {
    /// Serialize into the on-wire form: `"TSP/1 " + JSON`.
    pub fn encode(&self) -> String {
        format!(
            "{}{}",
            PROTOCOL_PREFIX,
            serde_json::to_string(self).expect("envelope serialization cannot fail")
        )
    }

    /// Parse a raw Tox friend message. Returns `None` for anything that is not
    /// a valid TSP/1 envelope (including ordinary chat messages).
    pub fn decode(raw: &str) -> Option<Self> {
        raw.strip_prefix(PROTOCOL_PREFIX)
            .and_then(|json| serde_json::from_str(json).ok())
    }

    /// Human-readable kind tag, e.g. `"post"`.
    pub fn kind(&self) -> &'static str {
        match self {
            Envelope::Post(_) => "post",
            Envelope::Comment(_) => "comment",
            Envelope::Reaction(_) => "reaction",
            Envelope::Profile(_) => "profile",
            Envelope::PostChunk(_) => "post_chunk",
            Envelope::SyncReq(_) => "sync_req",
            Envelope::SyncPosts(_) => "sync_posts",
            Envelope::DirReq(_) => "dir_req",
            Envelope::DirResp(_) => "dir_resp",
            Envelope::OutboxReq(_) => "outbox_req",
            Envelope::OutboxResp(_) => "outbox_resp",
            Envelope::Unfriend(_) => "unfriend",
        }
    }

    /// The wire size in bytes (UTF-8). Callers must reject > MAX_ENVELOPE_BYTES.
    pub fn wire_len(&self) -> usize {
        self.encode().len()
    }
}

impl Post {
    /// Canonical string that is signed for public posts.
    pub fn signing_string(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.id, self.author, self.ts, self.text, self.public
        )
    }

    pub fn new(author: &str, text: &str) -> Self {
        Post {
            v: PROTOCOL_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            author: author.to_string(),
            ts: now_ms(),
            text: text.to_string(),
            public: false,
            sig: String::new(),
        }
    }
}

impl Comment {
    pub fn new(author: &str, reply_to: &str, text: &str) -> Self {
        Comment {
            v: PROTOCOL_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            author: author.to_string(),
            ts: now_ms(),
            reply_to: reply_to.to_string(),
            text: text.to_string(),
        }
    }
}

impl Reaction {
    pub fn new(author: &str, reply_to: &str, emoji: &str) -> Self {
        Reaction {
            v: PROTOCOL_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            author: author.to_string(),
            ts: now_ms(),
            reply_to: reply_to.to_string(),
            emoji: emoji.to_string(),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_roundtrip_with_chinese_and_emoji() {
        let p = Post::new(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            "你好，去中心化世界！🎉",
        );
        let raw = Envelope::Post(p.clone()).encode();
        assert!(raw.starts_with(PROTOCOL_PREFIX));
        let decoded = Envelope::decode(&raw).unwrap();
        assert_eq!(decoded, Envelope::Post(p));
    }

    #[test]
    fn comment_roundtrip() {
        let c = Comment::new("aabb", "post-uuid-123", "+1 说得对");
        let raw = Envelope::Comment(c.clone()).encode();
        assert_eq!(Envelope::decode(&raw).unwrap(), Envelope::Comment(c));
    }

    #[test]
    fn reaction_roundtrip() {
        let r = Reaction::new("aabb", "post-uuid-123", "👍");
        let raw = Envelope::Reaction(r.clone()).encode();
        assert_eq!(Envelope::decode(&raw).unwrap(), Envelope::Reaction(r));
    }

    #[test]
    fn plain_chat_message_is_not_an_envelope() {
        assert!(Envelope::decode("你好，这是普通聊天").is_none());
        assert!(Envelope::decode("TSP/2 not-json").is_none());
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let bad = r#"{"t":"virus","v":1}"#;
        let raw = format!("{PROTOCOL_PREFIX}{bad}");
        assert!(Envelope::decode(&raw).is_none());
    }

    #[test]
    fn malformed_json_is_rejected() {
        let raw = format!("{PROTOCOL_PREFIX}{{not json");
        assert!(Envelope::decode(&raw).is_none());
    }

    #[test]
    fn wire_size_within_limit() {
        // 300 CJK chars = 900 bytes + JSON overhead: must fit in 1300B.
        let p = Post::new("aabb", &"中".repeat(300));
        assert!(Envelope::Post(p).wire_len() < crate::MAX_ENVELOPE_BYTES);
        // 1000 CJK chars exceeds the envelope: must NOT fit (byte gate).
        let long = Post::new("aabb", &"中".repeat(1000));
        assert!(Envelope::Post(long).wire_len() >= crate::MAX_ENVELOPE_BYTES);
    }

    #[test]
    fn profile_roundtrip() {
        let pr = Profile {
            v: 1,
            author: "aabb".into(),
            ts: 1,
            name: "Alice".into(),
            bio: "去中心化爱好者".into(),
            avatar: "".into(),
            avatar_len: 0,
        };
        let raw = Envelope::Profile(pr.clone()).encode();
        assert_eq!(Envelope::decode(&raw).unwrap(), Envelope::Profile(pr));
    }
}
