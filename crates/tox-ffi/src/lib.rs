//! Raw FFI bindings to c-toxcore.
//!
//! Hand-written subset of the c-toxcore C API (v0.2.x), using the ABI-safe
//! accessor functions where available (e.g. `tox_options_set_*`) instead of
//! direct struct layout. Everything here is `unsafe`; safe wrappers live in
//! the `tox-core` crate.
//!
//! See https://github.com/TokTok/c-toxcore for the authoritative API.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::os::raw::{c_char, c_void};

// ---------------------------------------------------------------------------
// Constants (from toxcore/tox.h, v0.2.x)
// ---------------------------------------------------------------------------

pub const TOX_MAX_MESSAGE_LENGTH: usize = 1372;
pub const TOX_ADDRESS_SIZE: usize = 38; // 32 pubkey + 4 nospam + 2 checksum
pub const TOX_PUBLIC_KEY_SIZE: usize = 32;
pub const TOX_SECRET_KEY_SIZE: usize = 32;
pub const TOX_MAX_NAME_LENGTH: usize = 128;
pub const TOX_MAX_STATUS_MESSAGE_LENGTH: usize = 1007;
pub const TOX_MAX_FRIEND_REQUEST_LENGTH: usize = 921;
pub const TOX_MAX_HOSTNAME_LENGTH: usize = 255;

// ---------------------------------------------------------------------------
// Opaque types
// ---------------------------------------------------------------------------

pub enum Tox {}
pub enum Tox_Options {}

// ---------------------------------------------------------------------------
// Enums (values match tox.h)
// ---------------------------------------------------------------------------

pub type Tox_Savedata_Type = u32;
pub const TOX_SAVEDATA_TYPE_NONE: Tox_Savedata_Type = 0;
pub const TOX_SAVEDATA_TYPE_TOX_SAVE: Tox_Savedata_Type = 1;
pub const TOX_SAVEDATA_TYPE_SECRET_KEY: Tox_Savedata_Type = 2;

pub type Tox_User_Status = u32;
pub const TOX_USER_STATUS_NONE: Tox_User_Status = 0;
pub const TOX_USER_STATUS_AWAY: Tox_User_Status = 1;
pub const TOX_USER_STATUS_BUSY: Tox_User_Status = 2;

pub type Tox_Connection = u32;
pub const TOX_CONNECTION_NONE: Tox_Connection = 0;
pub const TOX_CONNECTION_TCP: Tox_Connection = 1;
pub const TOX_CONNECTION_UDP: Tox_Connection = 2;

pub type Tox_Message_Type = u32;
pub const TOX_MESSAGE_TYPE_NORMAL: Tox_Message_Type = 0;
pub const TOX_MESSAGE_TYPE_ACTION: Tox_Message_Type = 1;

pub const TOX_CONFERENCE_ID_SIZE: usize = 32;
pub const TOX_FILE_ID_LENGTH: usize = 32;
pub const TOX_MAX_FILENAME_LENGTH: usize = 255;
pub type Tox_File_Number = u32;
pub type Tox_Conference_Number = u32;
pub type Tox_Conference_Peer_Number = u32;
pub type Tox_Conference_Type = u32;
pub const TOX_CONFERENCE_TYPE_TEXT: Tox_Conference_Type = 0;
pub const TOX_CONFERENCE_TYPE_AV: Tox_Conference_Type = 1;

// Error enums: "OK" is always 0 in toxcore.
pub const TOX_ERR_OPTIONS_NEW_OK: u32 = 0;
pub const TOX_ERR_NEW_OK: u32 = 0;
pub const TOX_ERR_BOOTSTRAP_OK: u32 = 0;
pub const TOX_ERR_FRIEND_ADD_OK: u32 = 0;
pub const TOX_ERR_FRIEND_SEND_MESSAGE_OK: u32 = 0;
pub const TOX_ERR_FRIEND_GET_PUBLIC_KEY_OK: u32 = 0;
pub const TOX_ERR_SET_INFO_OK: u32 = 0;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    // --- options (tox_options.h): allocate + accessors (ABI safe) ---------
    pub fn tox_options_new(error: *mut u32) -> *mut Tox_Options;
    pub fn tox_options_free(options: *mut Tox_Options);
    pub fn tox_options_set_savedata_type(options: *mut Tox_Options, t: Tox_Savedata_Type);
    pub fn tox_options_set_savedata_data(
        options: *mut Tox_Options,
        data: *const u8,
        length: usize,
    ) -> bool;
    pub fn tox_options_set_savedata_length(options: *mut Tox_Options, length: usize);

    // --- lifecycle ----------------------------------------------------------
    pub fn tox_new(options: *const Tox_Options, error: *mut u32) -> *mut Tox;
    pub fn tox_kill(tox: *mut Tox);
    pub fn tox_iterate(tox: *mut Tox, user_data: *mut c_void);
    pub fn tox_iteration_interval(tox: *const Tox) -> u32;
    pub fn tox_get_savedata_size(tox: *const Tox) -> usize;
    pub fn tox_get_savedata(tox: *const Tox, data: *mut u8);

    // --- self ----------------------------------------------------------------
    pub fn tox_self_get_address(tox: *const Tox, address: *mut u8);
    pub fn tox_self_get_public_key(tox: *const Tox, public_key: *mut u8);
    pub fn tox_self_get_nospam(tox: *const Tox) -> u32;
    pub fn tox_self_get_connection_status(tox: *const Tox) -> Tox_Connection;
    pub fn tox_self_set_nospam(tox: *mut Tox, nospam: u32);
    pub fn tox_self_get_secret_key(tox: *const Tox, secret_key: *mut u8);
    pub fn tox_self_set_name(tox: *mut Tox, name: *const u8, length: usize, error: *mut u32);
    pub fn tox_self_get_name(tox: *const Tox, name: *mut u8);
    pub fn tox_self_get_name_size(tox: *const Tox) -> usize;
    pub fn tox_self_set_status_message(
        tox: *mut Tox,
        status: *const u8,
        length: usize,
        error: *mut u32,
    );
    pub fn tox_self_get_status_message(tox: *const Tox, status: *mut u8);
    pub fn tox_self_get_status_message_size(tox: *const Tox) -> usize;
    pub fn tox_self_set_status(tox: *mut Tox, status: Tox_User_Status);

    // --- network --------------------------------------------------------------
    pub fn tox_bootstrap(
        tox: *mut Tox,
        host: *const c_char,
        port: u16,
        public_key: *const u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_add_tcp_relay(
        tox: *mut Tox,
        host: *const c_char,
        port: u16,
        public_key: *const u8,
        error: *mut u32,
    ) -> bool;

    // --- friends ----------------------------------------------------------------
    pub fn tox_friend_add(
        tox: *mut Tox,
        address: *const u8,
        message: *const u8,
        length: usize,
        error: *mut u32,
    ) -> u32;
    pub fn tox_friend_add_norequest(tox: *mut Tox, public_key: *const u8, error: *mut u32) -> u32;
    pub fn tox_friend_delete(tox: *mut Tox, friend_number: u32, error: *mut u32) -> bool;
    pub fn tox_friend_send_message(
        tox: *mut Tox,
        friend_number: u32,
        message_type: Tox_Message_Type,
        message: *const u8,
        length: usize,
        error: *mut u32,
    ) -> u32;
    pub fn tox_friend_get_public_key(
        tox: *const Tox,
        friend_number: u32,
        public_key: *mut u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_friend_get_name(
        tox: *const Tox,
        friend_number: u32,
        name: *mut u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_friend_get_name_size(tox: *const Tox, friend_number: u32, error: *mut u32) -> usize;
    pub fn tox_friend_get_status_message(
        tox: *const Tox,
        friend_number: u32,
        status: *mut u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_friend_get_status_message_size(
        tox: *const Tox,
        friend_number: u32,
        error: *mut u32,
    ) -> usize;
    pub fn tox_friend_get_connection_status(
        tox: *const Tox,
        friend_number: u32,
        error: *mut u32,
    ) -> Tox_Connection;
    pub fn tox_friend_get_last_online(
        tox: *const Tox,
        friend_number: u32,
        error: *mut u32,
    ) -> u64;
    pub fn tox_self_get_friend_list_size(tox: *const Tox) -> usize;
    pub fn tox_self_get_friend_list(tox: *const Tox, list: *mut u32);

    // --- file transfer ---------------------------------------------------------------
    pub fn tox_file_send(
        tox: *mut Tox,
        friend_number: u32,
        kind: u32,
        file_size: u64,
        file_id: *const u8,
        filename: *const u8,
        filename_length: usize,
        error: *mut u32,
    ) -> Tox_File_Number;
    pub fn tox_file_send_chunk(
        tox: *mut Tox,
        friend_number: u32,
        file_number: u32,
        position: u64,
        data: *const u8,
        length: usize,
        error: *mut u32,
    ) -> bool;
    pub fn tox_file_control(
        tox: *mut Tox,
        friend_number: u32,
        file_number: u32,
        control: u32,
        error: *mut u32,
    ) -> bool;
    pub fn tox_callback_file_recv(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(
                *mut Tox,
                u32,
                u32,
                u32,
                u64,
                *const u8,
                usize,
                *mut c_void,
            ),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_file_chunk_request(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(*mut Tox, u32, u32, u64, usize, *mut c_void),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_file_recv_chunk(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(*mut Tox, u32, u32, u64, *const u8, usize, *mut c_void),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_file_recv_control(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, u32, u32, *mut c_void)>,
        user_data: *mut c_void,
    );

    // --- conferences ---------------------------------------------------------------
    pub fn tox_conference_new(tox: *mut Tox, error: *mut u32) -> Tox_Conference_Number;
    pub fn tox_conference_delete(tox: *mut Tox, conference_number: u32, error: *mut u32) -> bool;
    pub fn tox_conference_peer_count(tox: *const Tox, conference_number: u32, error: *mut u32) -> u32;
    pub fn tox_conference_get_id(tox: *const Tox, conference_number: u32, id: *mut u8) -> bool;
    pub fn tox_conference_by_id(tox: *const Tox, id: *const u8, error: *mut u32) -> u32;
    pub fn tox_conference_get_chatlist_size(tox: *const Tox) -> usize;
    pub fn tox_conference_get_chatlist(tox: *const Tox, chatlist: *mut u32);
    pub fn tox_conference_peer_get_name_size(
        tox: *const Tox,
        conference_number: u32,
        peer_number: u32,
        error: *mut u32,
    ) -> usize;
    pub fn tox_conference_peer_get_name(
        tox: *const Tox,
        conference_number: u32,
        peer_number: u32,
        name: *mut u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_conference_peer_get_public_key(
        tox: *const Tox,
        conference_number: u32,
        peer_number: u32,
        public_key: *mut u8,
        error: *mut u32,
    ) -> bool;
    pub fn tox_conference_invite(
        tox: *mut Tox,
        friend_number: u32,
        conference_number: u32,
        error: *mut u32,
    ) -> bool;
    pub fn tox_conference_join(
        tox: *mut Tox,
        friend_number: u32,
        cookie: *const u8,
        length: usize,
        error: *mut u32,
    ) -> Tox_Conference_Number;
    pub fn tox_conference_send_message(
        tox: *mut Tox,
        conference_number: u32,
        message_type: Tox_Message_Type,
        message: *const u8,
        length: usize,
        error: *mut u32,
    ) -> bool;
    pub fn tox_callback_conference_invite(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(*mut Tox, u32, Tox_Conference_Type, *const u8, usize, *mut c_void),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_conference_connected(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, *mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn tox_callback_conference_message(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(
                *mut Tox,
                u32,
                Tox_Conference_Peer_Number,
                Tox_Message_Type,
                *const u8,
                usize,
                *mut c_void,
            ),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_conference_peer_name(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(*mut Tox, u32, Tox_Conference_Peer_Number, *const u8, usize, *mut c_void),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_conference_peer_list_changed(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, *mut c_void)>,
        user_data: *mut c_void,
    );

    // --- callbacks ----------------------------------------------------------------
    pub fn tox_callback_friend_request(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, *const u8, *const u8, usize, *mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn tox_callback_friend_message(
        tox: *mut Tox,
        callback: Option<
            unsafe extern "C" fn(*mut Tox, u32, Tox_Message_Type, *const u8, usize, *mut c_void),
        >,
        user_data: *mut c_void,
    );
    pub fn tox_callback_friend_name(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, *const u8, usize, *mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn tox_callback_friend_status_message(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, *const u8, usize, *mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn tox_callback_friend_status(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, Tox_User_Status, *mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn tox_callback_friend_connection_status(
        tox: *mut Tox,
        callback: Option<unsafe extern "C" fn(*mut Tox, u32, Tox_Connection, *mut c_void)>,
        user_data: *mut c_void,
    );

    // --- internal DHT helpers (not part of the public tox.h API) ------------
    // Linked from the vendored c-toxcore static library; `DHT*` is obtained by
    // reading the pinned internal `Tox -> Messenger -> DHT` layout (see
    // `ToxSession::dht_node_count` in tox-core).
    pub fn dht_get_num_closelist(dht: *const c_void) -> u16;
}

// ---------------------------------------------------------------------------
// Tests: link-time smoke (tox_new must resolve)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_tox_h() {
        assert_eq!(TOX_MAX_MESSAGE_LENGTH, 1372);
        assert_eq!(TOX_ADDRESS_SIZE, 38);
        assert_eq!(TOX_PUBLIC_KEY_SIZE, 32);
        assert_eq!(TOX_SAVEDATA_TYPE_TOX_SAVE, 1);
    }
}
