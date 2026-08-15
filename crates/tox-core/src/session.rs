//! [`ToxSession`]: owns a `Tox*` instance and its event loop.

use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use tox_ffi::*;

use crate::error::ToxError;
use crate::event::{Connection, Event, Status};

pub const MAX_NAME_LENGTH: usize = TOX_MAX_NAME_LENGTH;
pub const MAX_STATUS_MESSAGE_LENGTH: usize = TOX_MAX_STATUS_MESSAGE_LENGTH;

/// Callback context: handed to toxcore as `user_data`, forwards events out.
struct CallbackCtx {
    tx: Sender<Event>,
}

unsafe fn send(ctx: &mut CallbackCtx, ev: Event) {
    let _ = ctx.tx.send(ev); // receiver gone => just drop
}

// ---------------------------------------------------------------------------
// C callbacks (run on the tox iterate thread)
// ---------------------------------------------------------------------------

unsafe extern "C" fn on_friend_request(
    _tox: *mut Tox,
    public_key: *const u8,
    message: *const u8,
    length: usize,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    let pk = std::slice::from_raw_parts(public_key, TOX_PUBLIC_KEY_SIZE);
    let msg = std::slice::from_raw_parts(message, length).to_vec();
    send(
        ctx,
        Event::FriendRequest {
            public_key: hex::encode(pk),
            message: msg,
        },
    );
}

unsafe extern "C" fn on_friend_message(
    _tox: *mut Tox,
    friend_number: u32,
    message_type: u32,
    message: *const u8,
    length: usize,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    let raw = std::slice::from_raw_parts(message, length);
    let text = String::from_utf8_lossy(raw).into_owned();
    send(
        ctx,
        Event::FriendMessage {
            friend_number,
            message_type,
            text,
        },
    );
}

unsafe extern "C" fn on_friend_name(
    _tox: *mut Tox,
    friend_number: u32,
    name: *const u8,
    length: usize,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    let raw = std::slice::from_raw_parts(name, length);
    send(
        ctx,
        Event::FriendName {
            friend_number,
            name: String::from_utf8_lossy(raw).into_owned(),
        },
    );
}

unsafe extern "C" fn on_friend_status_message(
    _tox: *mut Tox,
    friend_number: u32,
    status: *const u8,
    length: usize,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    let raw = std::slice::from_raw_parts(status, length);
    send(
        ctx,
        Event::FriendStatusMessage {
            friend_number,
            status_message: String::from_utf8_lossy(raw).into_owned(),
        },
    );
}

unsafe extern "C" fn on_friend_status(
    _tox: *mut Tox,
    friend_number: u32,
    status: u32,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    send(
        ctx,
        Event::FriendStatus {
            friend_number,
            status: Status::from_raw(status),
        },
    );
}

unsafe extern "C" fn on_friend_connection_status(
    _tox: *mut Tox,
    friend_number: u32,
    connection: u32,
    user_data: *mut c_void,
) {
    let ctx = &mut *(user_data as *mut CallbackCtx);
    send(
        ctx,
        Event::FriendConnection {
            friend_number,
            connection: Connection::from_raw(connection),
        },
    );
}

// ---------------------------------------------------------------------------
// ToxSession
// ---------------------------------------------------------------------------

pub struct ToxSession {
    tox: *mut Tox,
    rx: Receiver<Event>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    /// Boxed CallbackCtx, leaked with `Box::into_raw`; reclaimed on drop.
    ctx: *mut CallbackCtx,
}

// The raw pointer is owned exclusively by us; channels are Send.
unsafe impl Send for ToxSession {}

impl ToxSession {
    /// Create a brand-new Tox instance (no savedata).
    pub fn new() -> Result<Self, ToxError> {
        Self::from_savedata(None)
    }

    /// Create a Tox instance from previously saved data.
    pub fn from_savedata(data: Option<&[u8]>) -> Result<Self, ToxError> {
        let mut err_opt: u32 = TOX_ERR_OPTIONS_NEW_OK;
        let options = unsafe { tox_options_new(&mut err_opt) };
        if options.is_null() {
            return Err(ToxError::OptionsNew(err_opt));
        }

        if let Some(data) = data {
            unsafe {
                tox_options_set_savedata_type(options, TOX_SAVEDATA_TYPE_TOX_SAVE);
                tox_options_set_savedata_data(options, data.as_ptr(), data.len());
                tox_options_set_savedata_length(options, data.len());
            }
        }

        let mut err_new: u32 = TOX_ERR_NEW_OK;
        let tox = unsafe { tox_new(options, &mut err_new) };
        unsafe { tox_options_free(options) };
        if tox.is_null() {
            return Err(ToxError::New(err_new));
        }

        // Register callbacks + spawn the event loop.
        let (tx, rx) = mpsc::channel();
        let ctx = Box::into_raw(Box::new(CallbackCtx { tx }));
        unsafe {
            tox_callback_friend_request(tox, Some(on_friend_request), ctx as *mut c_void);
            tox_callback_friend_message(tox, Some(on_friend_message), ctx as *mut c_void);
            tox_callback_friend_name(tox, Some(on_friend_name), ctx as *mut c_void);
            tox_callback_friend_status_message(tox, Some(on_friend_status_message), ctx as *mut c_void);
            tox_callback_friend_status(tox, Some(on_friend_status), ctx as *mut c_void);
            tox_callback_friend_connection_status(
                tox,
                Some(on_friend_connection_status),
                ctx as *mut c_void,
            );
        }

        let running = Arc::new(AtomicBool::new(true));
        let handle = spawn_iterate_loop(SendPtr(tox), SendPtr(ctx), running.clone());

        Ok(ToxSession {
            tox,
            rx,
            running,
            handle: Some(handle),
            ctx,
        })
    }

    // --- identity ---------------------------------------------------------

    /// Full ToxID: 76 hex chars (pubkey + nospam + checksum).
    pub fn self_address(&self) -> String {
        let mut buf = [0u8; TOX_ADDRESS_SIZE];
        unsafe { tox_self_get_address(self.tox, buf.as_mut_ptr()) };
        hex::encode(buf)
    }

    /// Public key only: 64 hex chars.
    pub fn self_public_key(&self) -> String {
        let mut buf = [0u8; TOX_PUBLIC_KEY_SIZE];
        unsafe { tox_self_get_public_key(self.tox, buf.as_mut_ptr()) };
        hex::encode(buf)
    }

    pub fn self_name(&self) -> String {
        let size = unsafe { tox_self_get_name_size(self.tox) };
        let mut buf = vec![0u8; size];
        unsafe { tox_self_get_name(self.tox, buf.as_mut_ptr()) };
        String::from_utf8_lossy(&buf).into_owned()
    }

    pub fn set_name(&mut self, name: &str) -> Result<(), ToxError> {
        let mut err: u32 = TOX_ERR_SET_INFO_OK;
        unsafe { tox_self_set_name(self.tox, name.as_ptr(), name.len(), &mut err) };
        if err != TOX_ERR_SET_INFO_OK {
            return Err(ToxError::SetInfo(err));
        }
        Ok(())
    }

    pub fn set_status_message(&mut self, msg: &str) -> Result<(), ToxError> {
        let mut err: u32 = TOX_ERR_SET_INFO_OK;
        unsafe { tox_self_set_status_message(self.tox, msg.as_ptr(), msg.len(), &mut err) };
        if err != TOX_ERR_SET_INFO_OK {
            return Err(ToxError::SetInfo(err));
        }
        Ok(())
    }

    pub fn set_status(&mut self, status: Status) {
        let raw = match status {
            Status::None => TOX_USER_STATUS_NONE,
            Status::Away => TOX_USER_STATUS_AWAY,
            Status::Busy => TOX_USER_STATUS_BUSY,
        };
        unsafe { tox_self_set_status(self.tox, raw) };
    }

    // --- persistence --------------------------------------------------------

    pub fn save(&self) -> Vec<u8> {
        let size = unsafe { tox_get_savedata_size(self.tox) };
        let mut buf = vec![0u8; size];
        unsafe { tox_get_savedata(self.tox, buf.as_mut_ptr()) };
        buf
    }

    // --- network -------------------------------------------------------------

    /// Bootstrap to the DHT. `public_key` is 64 hex chars.
    pub fn bootstrap(&self, host: &str, port: u16, public_key: &str) -> Result<(), ToxError> {
        let pk = hex_to_bytes(public_key, TOX_PUBLIC_KEY_SIZE)?;
        let c_host = std::ffi::CString::new(host).map_err(|_| {
            ToxError::Parse("host contains interior NUL byte".to_string())
        })?;
        let mut err: u32 = TOX_ERR_BOOTSTRAP_OK;
        let ok = unsafe { tox_bootstrap(self.tox, c_host.as_ptr(), port, pk.as_ptr(), &mut err) };
        if !ok {
            return Err(ToxError::Bootstrap(err));
        }
        Ok(())
    }

    /// Add a TCP relay. `public_key` is 64 hex chars.
    pub fn add_tcp_relay(&self, host: &str, port: u16, public_key: &str) -> Result<(), ToxError> {
        let pk = hex_to_bytes(public_key, TOX_PUBLIC_KEY_SIZE)?;
        let c_host = std::ffi::CString::new(host).map_err(|_| {
            ToxError::Parse("host contains interior NUL byte".to_string())
        })?;
        let mut err: u32 = TOX_ERR_BOOTSTRAP_OK;
        let ok =
            unsafe { tox_add_tcp_relay(self.tox, c_host.as_ptr(), port, pk.as_ptr(), &mut err) };
        if !ok {
            return Err(ToxError::Bootstrap(err));
        }
        Ok(())
    }

    // --- friends ---------------------------------------------------------------

    /// Add a friend from a full ToxID (76 hex chars), with an optional request
    /// message. Returns the friend number.
    pub fn add_friend(&mut self, toxid: &str, message: &str) -> Result<u32, ToxError> {
        let addr = hex_to_bytes(toxid, TOX_ADDRESS_SIZE)?;
        let mut err: u32 = TOX_ERR_FRIEND_ADD_OK;
        let n = unsafe {
            tox_friend_add(
                self.tox,
                addr.as_ptr(),
                message.as_ptr(),
                message.len(),
                &mut err,
            )
        };
        if err != TOX_ERR_FRIEND_ADD_OK {
            return Err(ToxError::FriendAdd(err));
        }
        Ok(n)
    }

    /// Add a friend by public key only (64 hex), without a request message
    /// (e.g. accepting an incoming request).
    pub fn add_friend_norequest(&mut self, public_key: &str) -> Result<u32, ToxError> {
        let pk = hex_to_bytes(public_key, TOX_PUBLIC_KEY_SIZE)?;
        let mut err: u32 = TOX_ERR_FRIEND_ADD_OK;
        let n = unsafe { tox_friend_add_norequest(self.tox, pk.as_ptr(), &mut err) };
        if err != TOX_ERR_FRIEND_ADD_OK {
            return Err(ToxError::FriendAdd(err));
        }
        Ok(n)
    }

    pub fn delete_friend(&mut self, friend_number: u32) -> Result<(), ToxError> {
        let mut err: u32 = TOX_ERR_FRIEND_ADD_OK;
        let ok = unsafe { tox_friend_delete(self.tox, friend_number, &mut err) };
        if !ok {
            return Err(ToxError::FriendDelete(err));
        }
        Ok(())
    }

    pub fn friend_count(&self) -> usize {
        unsafe { tox_self_get_friend_list_size(self.tox) }
    }

    pub fn friend_list(&self) -> Vec<u32> {
        let n = self.friend_count();
        let mut list = vec![0u32; n];
        if n > 0 {
            unsafe { tox_self_get_friend_list(self.tox, list.as_mut_ptr()) };
        }
        list
    }

    pub fn friend_public_key(&self, friend_number: u32) -> Result<String, ToxError> {
        let mut buf = [0u8; TOX_PUBLIC_KEY_SIZE];
        let mut err: u32 = TOX_ERR_FRIEND_GET_PUBLIC_KEY_OK;
        let ok = unsafe { tox_friend_get_public_key(self.tox, friend_number, buf.as_mut_ptr(), &mut err) };
        if !ok {
            return Err(ToxError::FriendAdd(err));
        }
        Ok(hex::encode(buf))
    }

    pub fn friend_name(&self, friend_number: u32) -> Result<String, ToxError> {
        let mut err: u32 = TOX_ERR_FRIEND_GET_PUBLIC_KEY_OK;
        let size = unsafe { tox_friend_get_name_size(self.tox, friend_number, &mut err) };
        let mut buf = vec![0u8; size];
        unsafe { tox_friend_get_name(self.tox, friend_number, buf.as_mut_ptr(), &mut err) };
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn friend_connection(&self, friend_number: u32) -> Connection {
        Connection::from_raw(unsafe { tox_friend_get_connection_status(self.tox, friend_number) })
    }

    /// Send a plain-text message to a friend. Must be <= 1372 bytes.
    pub fn send_message(&self, friend_number: u32, text: &str) -> Result<(), ToxError> {
        if text.len() > TOX_MAX_MESSAGE_LENGTH {
            return Err(ToxError::MessageTooLong(text.len()));
        }
        let mut err: u32 = TOX_ERR_FRIEND_SEND_MESSAGE_OK;
        let _msg_id = unsafe {
            tox_friend_send_message(
                self.tox,
                friend_number,
                TOX_MESSAGE_TYPE_NORMAL,
                text.as_ptr(),
                text.len(),
                &mut err,
            )
        };
        if err != TOX_ERR_FRIEND_SEND_MESSAGE_OK {
            return Err(ToxError::SendMessage(err));
        }
        Ok(())
    }

    // --- events ---------------------------------------------------------------

    /// Non-blocking read of the next event.
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Blocking read of the next event.
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Stop the event loop and release the Tox instance.
    pub fn shutdown(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.running.store(false, Ordering::SeqCst);
            let _ = handle.join();
        }
        // Callbacks are no longer running; reclaim ctx and tox safely.
        if !self.ctx.is_null() {
            unsafe {
                drop(Box::from_raw(self.ctx));
            }
            self.ctx = std::ptr::null_mut();
        }
        if !self.tox.is_null() {
            unsafe { tox_kill(self.tox) };
            self.tox = std::ptr::null_mut();
        }
    }
}

impl Drop for ToxSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Raw-pointer wrapper that is `Send`: we own the pointee exclusively and
/// guarantee the pointee outlives the event-loop thread (join on shutdown).
#[derive(Clone, Copy)]
struct SendPtr<T>(*mut T);
unsafe impl<T> Send for SendPtr<T> {}

fn spawn_iterate_loop(
    tox: SendPtr<Tox>,
    ctx: SendPtr<CallbackCtx>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("tox-iterate".to_string())
        .spawn(move || {
            // Rebind to capture the whole SendPtr wrappers: edition-2021
            // disjoint capture would otherwise capture the raw pointer fields
            // themselves and fail the Send bound.
            let tox = tox;
            let ctx = ctx;
            while running.load(Ordering::SeqCst) {
                let interval = unsafe { tox_iteration_interval(tox.0) };
                std::thread::sleep(std::time::Duration::from_millis(interval as u64));
                unsafe {
                    tox_iterate(tox.0, ctx.0 as *mut c_void);
                }
            }
        })
        .expect("failed to spawn tox iterate thread")
}

fn hex_to_bytes(s: &str, expected: usize) -> Result<Vec<u8>, ToxError> {
    if s.len() != expected * 2 {
        return Err(ToxError::Parse(format!(
            "expected {expected} bytes ({} hex chars), got {}",
            expected * 2,
            s.len()
        )));
    }
    hex::decode(s).map_err(|e| ToxError::Parse(format!("invalid hex: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_bytes_roundtrip() {
        let h = "abcd".repeat(16); // 64 hex chars = 32 bytes
        let b = hex_to_bytes(&h, 32).unwrap();
        assert_eq!(b.len(), 32);
        assert_eq!(hex::encode(b), h);
    }

    #[test]
    fn hex_to_bytes_rejects_bad_len() {
        assert!(hex_to_bytes("abcd", 32).is_err());
    }
}
