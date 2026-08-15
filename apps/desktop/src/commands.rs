//! Tauri commands: the IPC surface exposed to the Vue frontend.

use serde::Serialize;
use tauri::State;

use tox_core::Connection;
use tox_social::envelope::{Comment, Envelope, Post, Profile, Reaction};
use tox_store::PostKind;

use crate::events;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OwnInfo {
    pub toxid: String,
    pub pubkey: String,
    pub name: String,
    pub status_message: String,
    pub friend_count: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub id: String,
    pub author: String,
    pub author_name: String,
    pub kind: String,
    pub text: Option<String>,
    pub emoji: Option<String>,
    pub ts: i64,
    pub parent_id: Option<String>,
    pub comment_count: usize,
    pub reaction_count: usize,
    pub is_own: bool,
    pub source: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FriendInfo {
    pub toxid: String,
    pub name: String,
    pub online: bool,
    pub last_seen: Option<i64>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_own_info(state: State<AppState>) -> OwnInfo {
    let session = state.session.lock().unwrap();
    OwnInfo {
        toxid: session.self_address(),
        pubkey: session.self_public_key(),
        name: session.self_name(),
        status_message: session
            .self_status_message()
            .unwrap_or_default(),
        friend_count: session.friend_count(),
    }
}

#[tauri::command]
pub fn set_profile(
    state: State<AppState>,
    name: String,
    bio: String,
) -> Result<(), String> {
    {
        let mut session = state.session.lock().unwrap();
        session
            .set_name(name.trim())
            .map_err(|e| format!("set name failed: {e}"))?;
        session
            .set_status_message(bio.trim())
            .map_err(|e| format!("set status failed: {e}"))?;
    }
    state.persist();

    // Broadcast the profile update to all online friends.
    let me = state.session.lock().unwrap().self_public_key();
    let ts = now_ms();
    let profile = Profile {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts,
        name: name.trim().to_string(),
        bio: bio.trim().to_string(),
        avatar: String::new(),
        avatar_len: 0,
    };
    let wire = Envelope::Profile(profile).encode();
    let session = state.session.lock().unwrap();
    for n in session.friend_list() {
        if session.friend_connection(n) != Connection::None {
            let _ = session.send_message(n, &wire);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn add_friend(state: State<AppState>, toxid: String, message: String) -> Result<u32, String> {
    let mut session = state.session.lock().unwrap();
    let n = session
        .add_friend(toxid.trim(), message.trim())
        .map_err(|e| format!("add friend failed: {e}"))?;
    state.persist();
    Ok(n)
}

#[tauri::command]
pub fn remove_friend(state: State<AppState>, friend_number: u32) -> Result<(), String> {
    let mut session = state.session.lock().unwrap();
    session
        .delete_friend(friend_number)
        .map_err(|e| format!("remove friend failed: {e}"))?;
    state.persist();
    Ok(())
}

#[tauri::command]
pub fn publish_post(state: State<AppState>, text: String) -> Result<Post, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let post = {
        let engine = state.engine.lock().unwrap();
        engine
            .publish_post(&me, text.trim())
            .map_err(|e| e.to_string())?
    };
    fan_out(&state, Envelope::Post(post.clone()))?;
    state.persist();
    Ok(post)
}

#[tauri::command]
pub fn publish_comment(
    state: State<AppState>,
    post_id: String,
    text: String,
) -> Result<Comment, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let comment = {
        let engine = state.engine.lock().unwrap();
        engine
            .publish_comment(&me, post_id.trim(), text.trim())
            .map_err(|e| e.to_string())?
    };
    fan_out(&state, Envelope::Comment(comment.clone()))?;
    state.persist();
    Ok(comment)
}

#[tauri::command]
pub fn publish_reaction(
    state: State<AppState>,
    post_id: String,
    emoji: String,
) -> Result<Reaction, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let reaction = {
        let engine = state.engine.lock().unwrap();
        engine
            .publish_reaction(&me, post_id.trim(), emoji.trim())
            .map_err(|e| e.to_string())?
    };
    fan_out(&state, Envelope::Reaction(reaction.clone()))?;
    state.persist();
    Ok(reaction)
}

#[tauri::command]
pub fn fetch_timeline(state: State<AppState>, limit: Option<u32>) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let authors = {
        let session = state.session.lock().unwrap();
        let mut a = vec![session.self_public_key()];
        for n in session.friend_list() {
            if let Ok(pk) = session.friend_public_key(n) {
                a.push(pk);
            }
        }
        a
    };
    let engine = state.engine.lock().unwrap();
    let rows = engine
        .timeline(&authors, limit)
        .into_iter()
        .filter(|r| r.kind == PostKind::Post)
        .collect::<Vec<_>>();
    Ok(rows
        .iter()
        .map(|r| events::item_from_row(&state, &engine, r))
        .collect())
}

#[tauri::command]
pub fn fetch_thread(state: State<AppState>, post_id: String) -> Result<Vec<TimelineItem>, String> {
    let engine = state.engine.lock().unwrap();
    let post = engine
        .store()
        .post_get(post_id.trim())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "post not found".to_string())?;
    let mut items = vec![events::item_from_row(&state, &engine, &post)];
    let thread = engine
        .store()
        .thread_for(post_id.trim())
        .map_err(|e| e.to_string())?;
    for row in thread {
        items.push(events::item_from_row(&state, &engine, &row));
    }
    Ok(items)
}

#[tauri::command]
pub fn get_friends(state: State<AppState>) -> Result<Vec<FriendInfo>, String> {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let friends = store.friend_list().map_err(|e| e.to_string())?;
    Ok(friends
        .into_iter()
        .map(|f| FriendInfo {
            toxid: f.toxid,
            name: f.name,
            online: f.status == 1,
            last_seen: f.last_seen,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Send an envelope to every currently-online friend.
fn fan_out(state: &State<AppState>, env: Envelope) -> Result<(), String> {
    let wire = env.encode();
    let session = state.session.lock().unwrap();
    for n in session.friend_list() {
        if session.friend_connection(n) != Connection::None {
            session
                .send_message(n, &wire)
                .map_err(|e| format!("send to friend #{n} failed: {e}"))?;
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
