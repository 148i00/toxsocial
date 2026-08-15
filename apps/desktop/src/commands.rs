//! Tauri commands: the IPC surface exposed to the Vue frontend.

use serde::Serialize;
use tauri::State;

use tox_core::Connection;
use tox_social::envelope::{Comment, Envelope, Post, Profile, Reaction, SyncReq};
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
pub struct ReactionSummary {
    pub emoji: String,
    pub count: usize,
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
    pub reactions: Vec<ReactionSummary>,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaConfig {
    pub provider: String,
    pub has_client_id: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConferencePeerInfo {
    pub peer_number: u32,
    pub name: String,
    pub public_key: String,
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
    let n = {
        let mut session = state.session.lock().unwrap();
        session
            .add_friend(toxid.trim(), message.trim())
            .map_err(|e| format!("add friend failed: {e}"))?
    };
    state.persist();
    Ok(n)
}

#[tauri::command]
pub fn remove_friend(state: State<AppState>, friend_number: u32) -> Result<(), String> {
    {
        let mut session = state.session.lock().unwrap();
        session
            .delete_friend(friend_number)
            .map_err(|e| format!("remove friend failed: {e}"))?;
    }
    state.persist();
    Ok(())
}

#[tauri::command]
pub fn remove_friend_by_toxid(state: State<AppState>, toxid: String) -> Result<(), String> {
    let toxid = toxid.trim().to_string();
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| pk == toxid)
                    .unwrap_or(false)
            })
            .ok_or_else(|| "friend not found".to_string())?
    };
    {
        let mut session = state.session.lock().unwrap();
        session
            .delete_friend(friend_number)
            .map_err(|e| format!("remove friend failed: {e}"))?;
    }
    state.persist();
    {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .friend_remove(&toxid)
            .map_err(|e| format!("remove friend from store failed: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn publish_post(state: State<AppState>, text: String) -> Result<Post, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let text = text.trim().to_string();
    let (post, envelopes) = {
        let engine = state.engine.lock().unwrap();
        if text.chars().count() > tox_social::MAX_POST_CHARS {
            engine
                .publish_long_post(&me, &text)
                .map_err(|e| e.to_string())?
        } else {
            let post = engine
                .publish_post(&me, &text)
                .map_err(|e| e.to_string())?;
            (post.clone(), vec![Envelope::Post(post)])
        }
    };
    for env in envelopes {
        fan_out(&state, env)?;
    }
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
pub fn search_posts(
    state: State<AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let engine = state.engine.lock().unwrap();
    let rows = engine.search_posts(&query, limit);
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

#[tauri::command]
pub async fn upload_media(
    state: State<'_, AppState>,
    data_base64: String,
    filename: String,
) -> Result<String, String> {
    let client_id = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_get("imgur_client_id")
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    if client_id.is_empty() {
        return Err("请先在设置中填写 Imgur Client ID".to_string());
    }
    crate::media::upload_media(&data_base64, &filename, &client_id).await
}

#[tauri::command]
pub fn set_imgur_client_id(state: State<AppState>, client_id: String) -> Result<(), String> {
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .kv_set("imgur_client_id", client_id.trim())
        .map_err(|e| format!("failed to save Imgur Client ID: {e}"))
}

#[tauri::command]
pub fn get_media_config(state: State<AppState>) -> Result<MediaConfig, String> {
    let engine = state.engine.lock().unwrap();
    let client_id = engine
        .store()
        .kv_get("imgur_client_id")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    Ok(MediaConfig {
        provider: "imgur".to_string(),
        has_client_id: !client_id.is_empty(),
    })
}

#[tauri::command]
pub fn conference_new(state: State<AppState>) -> Result<u32, String> {
    let mut session = state.session.lock().unwrap();
    session
        .conference_new()
        .map_err(|e| format!("create conference failed: {e}"))
}

#[tauri::command]
pub fn conference_invite(
    state: State<AppState>,
    friend_number: u32,
    conference_number: u32,
) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .conference_invite(friend_number, conference_number)
        .map_err(|e| format!("invite failed: {e}"))
}

#[tauri::command]
pub fn conference_send(
    state: State<AppState>,
    conference_number: u32,
    text: String,
) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .conference_send_message(conference_number, text.trim())
        .map_err(|e| format!("send to conference failed: {e}"))
}

#[tauri::command]
pub fn conference_peers(
    state: State<AppState>,
    conference_number: u32,
) -> Result<Vec<ConferencePeerInfo>, String> {
    let session = state.session.lock().unwrap();
    let count = session
        .conference_peer_count(conference_number)
        .map_err(|e| e.to_string())?;
    let mut peers = Vec::new();
    for i in 0..count {
        peers.push(ConferencePeerInfo {
            peer_number: i,
            name: session
                .conference_peer_name(conference_number, i)
                .unwrap_or_default(),
            public_key: session
                .conference_peer_public_key(conference_number, i)
                .unwrap_or_default(),
        });
    }
    Ok(peers)
}

#[tauri::command]
pub fn request_sync_all(state: State<AppState>) -> Result<usize, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let targets: Vec<(u32, String)> = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .filter_map(|n| {
                if session.friend_connection(n) == Connection::None {
                    return None;
                }
                let pk = session.friend_public_key(n).ok()?;
                Some((n, pk))
            })
            .collect()
    };
    let mut sent = 0;
    for (friend_number, pk) in targets {
        let since = state
            .engine
            .lock()
            .unwrap()
            .latest_ts_for_author(&pk)
            .unwrap_or(0);
        let req = Envelope::SyncReq(SyncReq {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me.clone(),
            ts: now_ms(),
            since,
        });
        let wire = req.encode();
        let session = state.session.lock().unwrap();
        if session.send_message(friend_number, &wire).is_ok() {
            sent += 1;
        }
    }
    Ok(sent)
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
