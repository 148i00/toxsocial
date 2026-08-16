//! Social protocol layer ("TSP/1") built on top of Tox friend messages.
//!
//! Every protocol message is a UTF-8 text message prefixed with
//! [`PROTOCOL_PREFIX`], followed by a JSON envelope (see docs/PROTOCOL.md).
//! Single message size is capped by c-toxcore at 1372 bytes.

pub mod envelope;
pub mod feed;

pub use envelope::{Comment, DirEntry, DirReq, DirResp, Envelope, OutboxReq, OutboxResp, Post, PostChunk, Profile, Reaction, SyncPosts, SyncReq, Unfriend};
pub use feed::FeedEngine;

/// Prefix marking a message as part of the social protocol.
pub const PROTOCOL_PREFIX: &str = "TSP/1 ";

/// Maximum safe payload size in bytes (leaves room for the prefix and UTF-8
/// multi-byte sequences below the 1372-byte hard limit).
pub const MAX_ENVELOPE_BYTES: usize = 1300;

/// Maximum recommended post length in characters (MVP).
pub const MAX_POST_CHARS: usize = 1000;
