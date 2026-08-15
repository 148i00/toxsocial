//! Application state: ToxSession + FeedEngine + persistence paths.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;
use tox_core::{Connection, ToxSession};
use tox_social::feed::FeedEngine;
use tox_store::{FriendRow, Store};

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
            let data = std::fs::read(&save_path)?;
            println!("[toxsocial] loading existing profile");
            ToxSession::from_savedata(Some(&data)).map_err(|e| e.to_string())?
        } else {
            println!("[toxsocial] creating new identity");
            ToxSession::new().map_err(|e| e.to_string())?
        };
        // Persist the (possibly new) save immediately.
        std::fs::write(&save_path, session.save())?;
        println!("[toxsocial] identity: {}", session.self_address());

        let store = Store::open(&db_path).map_err(|e| e.to_string())?;
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
        if let Err(e) = std::fs::write(&path, session.save()) {
            eprintln!("[toxsocial] failed to persist profile: {e}");
        }
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
