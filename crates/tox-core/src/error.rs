//! Errors returned by the safe wrapper.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToxError {
    /// tox_options_new failed (error code).
    OptionsNew(u32),
    /// tox_new failed (error code).
    New(u32),
    /// tox_bootstrap / tox_add_tcp_relay failed (error code).
    Bootstrap(u32),
    /// tox_friend_add / tox_friend_add_norequest failed (error code).
    FriendAdd(u32),
    /// tox_friend_delete failed (error code).
    FriendDelete(u32),
    /// tox_friend_send_message failed (error code).
    SendMessage(u32),
    /// tox_self_set_name / status_message failed (error code).
    SetInfo(u32),
    /// A hex string or ToxID could not be parsed.
    Parse(String),
    /// The message exceeds TOX_MAX_MESSAGE_LENGTH.
    MessageTooLong(usize),
    /// A required pointer was null (allocation failure etc.).
    NullPointer,
    /// Conference operation failed (error code).
    Conference(u32),
    /// File transfer operation failed (error code).
    File(u32),
}

impl fmt::Display for ToxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToxError::OptionsNew(code) => write!(f, "tox_options_new failed: {code}"),
            ToxError::New(code) => write!(f, "tox_new failed: {code}"),
            ToxError::Bootstrap(code) => write!(f, "bootstrap failed: {code}"),
            ToxError::FriendAdd(code) => write!(f, "friend add failed: {code}"),
            ToxError::FriendDelete(code) => write!(f, "friend delete failed: {code}"),
            ToxError::SendMessage(code) => write!(f, "friend send message failed: {code}"),
            ToxError::SetInfo(code) => write!(f, "set info failed: {code}"),
            ToxError::Parse(s) => write!(f, "parse error: {s}"),
            ToxError::MessageTooLong(len) => {
                write!(f, "message too long: {len} bytes (max 1372)")
            }
            ToxError::NullPointer => write!(f, "null pointer returned from toxcore"),
            ToxError::Conference(code) => write!(f, "conference operation failed: {code}"),
            ToxError::File(code) => write!(f, "file transfer failed: {code}"),
        }
    }
}

impl std::error::Error for ToxError {}
