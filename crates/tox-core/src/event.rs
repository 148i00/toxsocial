//! Typed events produced by the tox event loop.

/// Online status of a peer (TOX_USER_STATUS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    None,
    Away,
    Busy,
}

impl Status {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Status::Away,
            2 => Status::Busy,
            _ => Status::None,
        }
    }
}

/// Connection status of a friend (TOX_CONNECTION).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    None,
    Tcp,
    Udp,
}

impl Connection {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Connection::Tcp,
            2 => Connection::Udp,
            _ => Connection::None,
        }
    }
}

/// Events delivered from the tox event-loop thread to the application.
#[derive(Debug, Clone)]
pub enum Event {
    /// A friend request was received. `public_key` is 64 hex chars.
    FriendRequest { public_key: String, message: Vec<u8> },
    /// A friend sent us a message.
    FriendMessage {
        friend_number: u32,
        message_type: u32,
        text: String,
    },
    /// A friend changed their name.
    FriendName {
        friend_number: u32,
        name: String,
    },
    /// A friend changed their status message.
    FriendStatusMessage {
        friend_number: u32,
        status_message: String,
    },
    /// A friend changed their online status (away/busy/none).
    FriendStatus {
        friend_number: u32,
        status: Status,
    },
    /// A friend came online / went offline (connection).
    FriendConnection {
        friend_number: u32,
        connection: Connection,
    },
}
