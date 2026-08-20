//! SQLite persistence layer.
//!
//! Schema follows docs/ARCHITECTURE.md §4: `kv`, `friends`, `posts`
//! (posts/comments/reactions in one table).

pub mod store;

pub use store::{
    ChannelMessageRow, DirectoryEntry, FriendRow, PostKind, PostRow, PostSource, Store,
};
