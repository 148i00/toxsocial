//! Application state: ToxSession + FeedEngine + persistence paths.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;
use tox_core::{Connection, ToxSession};
use tox_social::feed::FeedEngine;
use tox_store::{FriendRow, Store};

/// DPAPI-protected storage for the Tox save file (Windows). The profile key
/// is the user's most valuable secret; DPAPI binds it to this Windows account
/// so a copied `profile.tox` is useless on another machine. Legacy plaintext
/// files are read transparently and re-encrypted on the next save.
#[cfg(windows)]
mod profile_crypto {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let input = blob(data);
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
            if CryptProtectData(
                &input as *const _ as *mut _,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            ) == 0
            {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            Some(bytes)
        }
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let input = blob(data);
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
            if CryptUnprotectData(
                &input as *const _ as *mut _,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            ) == 0
            {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            Some(bytes)
        }
    }
}

pub struct AppState {
    pub session: Mutex<ToxSession>,
    pub engine: Mutex<FeedEngine>,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn load(app: &tauri::App) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        let save_path = data_dir.join("profile.tox");
        let db_path = data_dir.join("profile.db");
        println!("[toxsocial] data dir: {}", data_dir.display());

        let session = if save_path.exists() {
            let raw = std::fs::read(&save_path)?;
            // Try DPAPI first; fall back to plaintext for legacy profiles.
            let data = match profile_crypto::unprotect(&raw) {
                Some(plain) => {
                    println!("[toxsocial] profile loaded (DPAPI-encrypted)");
                    plain
                }
                None => {
                    println!("[toxsocial] profile loaded (legacy plaintext; will be encrypted on next save)");
                    raw
                }
            };
            match ToxSession::from_savedata(Some(&data)) {
                Ok(s) => {
                    println!("[toxsocial] loading existing profile");
                    s
                }
                Err(e) => {
                    // Corrupt main save: fall back to the backup copy before
                    // giving up — a fresh identity here looks exactly like
                    // "all my data is gone" to the user.
                    eprintln!("[toxsocial] profile.tox unreadable ({e}); trying backup");
                    load_saved_backup(&data_dir).map_err(|e2| {
                        format!("profile.tox: {e}; backup: {e2}")
                    })?
                }
            }
        } else {
            match load_saved_backup(&data_dir) {
                Ok(s) => {
                    println!("[toxsocial] profile.tox missing; recovered identity from backup");
                    s
                }
                Err(_) => {
                    println!("[toxsocial] creating new identity");
                    ToxSession::new().map_err(|e| e.to_string())?
                }
            }
        };
        // Persist the (possibly new) save immediately.
        write_profile(&save_path, &session.save());
        println!("[toxsocial] identity: {}", session.self_address());

        let store = Store::open(&db_path).map_err(|e| e.to_string())?;
        // Startup housekeeping: cap growth (posts / per-group chat / private
        // chat). Failures are non-fatal.
        match store.cleanup(5_000, 500, 1_000) {
            Ok((p, c, m)) if p + c + m > 0 => {
                println!("[toxsocial] startup cleanup removed {p} posts, {c} group msgs, {m} private msgs");
            }
            Ok(_) => {}
            Err(e) => eprintln!("[toxsocial] startup cleanup failed: {e}"),
        }
        sync_friends(&session, &store);
        let engine = FeedEngine::new(store);

        Ok(AppState {
            session: Mutex::new(session),
            engine: Mutex::new(engine),
            data_dir,
        })
    }

    /// Persist the Tox save data to disk.
    pub fn persist(&self) {
        let session = self.session.lock().unwrap();
        let path = self.data_dir.join("profile.tox");
        write_profile(&path, &session.save());
    }

    /// Resolve a display name for a public key: friend name, or short key.
    pub fn name_for(&self, pk: &str) -> String {
        let engine = self.engine.lock().unwrap();
        let store = engine.store();
        let friends = store.friend_list().unwrap_or_default();
        for f in &friends {
            if f.toxid == pk && !f.name.is_empty() {
                return f.name.clone();
            }
        }
        short_pk(pk)
    }
}

/// Write the Tox save data, DPAPI-encrypted on Windows (plain fallback if the
/// crypto call ever fails — availability over perfection). A second copy is
/// kept at `profile.tox.saved` so a lost/corrupt main save can be recovered.
fn write_profile(path: &std::path::Path, save: &[u8]) {
    let bytes = profile_crypto::protect(save).unwrap_or_else(|| save.to_vec());
    if let Err(e) = std::fs::write(path, &bytes) {
        eprintln!("[toxsocial] failed to persist profile: {e}");
    }
    if let Some(parent) = path.parent() {
        let backup = parent.join("profile.tox.saved");
        let _ = std::fs::write(&backup, &bytes);
    }
}

/// Load the identity from the `profile.tox.saved` backup copy.
fn load_saved_backup(
    data_dir: &std::path::Path,
) -> Result<ToxSession, Box<dyn std::error::Error>> {
    let backup = data_dir.join("profile.tox.saved");
    let raw = std::fs::read(&backup).map_err(|e| format!("no backup: {e}"))?;
    let data = profile_crypto::unprotect(&raw).unwrap_or(raw);
    ToxSession::from_savedata(Some(&data)).map_err(|e| format!("backup unreadable: {e}").into())
}

/// Sync the toxcore friend list into the store (names, status, toxid).
fn sync_friends(session: &ToxSession, store: &Store) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for n in session.friend_list() {
        let Ok(pk) = session.friend_public_key(n) else { continue };
        let name = session.friend_name(n).unwrap_or_default();
        let online = match session.friend_connection(n) {
            Connection::None => 0,
            _ => 1,
        };
        let row = FriendRow {
            toxid: pk,
            nospam: String::new(),
            name,
            avatar: String::new(),
            bio: String::new(),
            status: online,
            added_at: now,
            last_seen: Some(now),
        };
        let _ = store.friend_upsert(&row);
    }
}

fn short_pk(pk: &str) -> String {
    if pk.len() > 8 {
        format!("{}…", &pk[..8])
    } else {
        pk.to_string()
    }
}
