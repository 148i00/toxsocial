//! Tauri commands: the IPC surface exposed to the Vue frontend.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

use tox_core::{Connection, ToxError};
use tox_social::envelope::{Comment, Envelope, Post, Profile, Reaction, SyncReq};
use tox_store::{ChannelMessageRow, PostKind, PostRow};

use crate::events;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub connected: bool,
    pub connection: String,
    pub friends: usize,
    pub online_friends: usize,
    pub dht_nodes: usize,
    pub relay_ok: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OwnInfo {
    pub toxid: String,
    pub pubkey: String,
    pub name: String,
    pub status_message: String,
    pub avatar: String,
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
    pub author_avatar: String,
    pub kind: String,
    pub text: Option<String>,
    pub emoji: Option<String>,
    pub ts: i64,
    pub parent_id: Option<String>,
    pub comment_count: usize,
    pub reaction_count: usize,
    pub reactions: Vec<ReactionSummary>,
    pub is_own: bool,
    pub ts_verified: bool,
    pub source: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FriendInfo {
    pub toxid: String,
    pub pubkey: String,
    pub name: String,
    pub avatar: String,
    pub bio: String,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntryInfo {
    pub name: String,
    pub pubkey: String,
    pub toxid: String,
    pub avatar: String,
    pub relay: String,
    pub source: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublicChannelInfo {
    pub name: String,
    pub desc: String,
    pub host_toxid: String,
    pub channel_id: String,
    pub hosts: Vec<String>,
    pub members: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMessageInfo {
    pub id: i64,
    pub peer_name: String,
    pub text: String,
    pub ts: i64,
    pub direction: i64,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn get_own_info(state: State<AppState>) -> OwnInfo {
    let session = state.session.lock().unwrap();
    let avatar = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_get("avatar_url")
            .unwrap_or_default()
            .unwrap_or_default()
    };
    OwnInfo {
        toxid: session.self_address(),
        pubkey: session.self_public_key(),
        name: session.self_name(),
        status_message: session
            .self_status_message()
            .unwrap_or_default(),
        avatar,
        friend_count: session.friend_count(),
    }
}

#[tauri::command]
pub async fn get_network_status(state: State<'_, AppState>) -> Result<NetworkStatus, String> {
    let relays = relay_urls(&state);
    let (connection, friends, online, dht_nodes) = {
        let session = state.session.lock().unwrap();
        let connection = session.self_connection();
        let friends = session.friend_list();
        let online = friends
            .iter()
            .filter(|n| session.friend_connection(**n) != Connection::None)
            .count();
        let dht_nodes = session.dht_node_count() as usize;
        (connection, friends.len(), online, dht_nodes)
    };
    let mut relay_ok = false;
    for relay in &relays {
        if crate::relay::check_relay(relay).await.unwrap_or(false) {
            relay_ok = true;
            break;
        }
    }
    Ok(NetworkStatus {
        connected: connection != Connection::None,
        connection: match connection {
            Connection::None => "offline".to_string(),
            Connection::Tcp => "tcp".to_string(),
            Connection::Udp => "udp".to_string(),
        },
        friends,
        online_friends: online,
        dht_nodes,
        relay_ok,
    })
}

#[tauri::command]
pub async fn set_profile(
    state: State<'_, AppState>,
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
    let avatar = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_get("avatar_url")
            .unwrap_or_default()
            .unwrap_or_default()
    };
    let ts = now_ms();
    let profile = Profile {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts,
        name: name.trim().to_string(),
        bio: bio.trim().to_string(),
        avatar: avatar.clone(),
        avatar_len: if avatar.is_empty() { 0 } else { avatar.len() as u64 },
    };
    let wire = Envelope::Profile(profile).encode();
    {
        let session = state.session.lock().unwrap();
        for n in session.friend_list() {
            if session.friend_connection(n) != Connection::None {
                let _ = session.send_message(n, &wire);
            }
        }
    }
    // Also publish public profile to Relay(s) for discoverability.
    let relays = relay_urls(&state);
    let pubkey = state.session.lock().unwrap().self_public_key();
    let toxid = state.session.lock().unwrap().self_address();
    for relay in &relays {
        let _ = crate::relay::register_profile(
            relay,
            name.trim(),
            &pubkey,
            &toxid,
            &avatar,
        )
        .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_avatar(state: State<'_, AppState>, data_base64: String) -> Result<String, String> {
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
    let url = crate::media::upload_media(&data_base64, "avatar.png", &client_id).await?;
    {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_set("avatar_url", &url)
            .map_err(|e| e.to_string())?;
    }
    // Broadcast updated profile (with avatar) to online friends.
    let me = state.session.lock().unwrap().self_public_key();
    let name = state.session.lock().unwrap().self_name();
    let bio = state
        .session
        .lock()
        .unwrap()
        .self_status_message()
        .unwrap_or_default();
    let profile = Profile {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        name: name.clone(),
        bio,
        avatar: url.clone(),
        avatar_len: url.len() as u64,
    };
    let wire = Envelope::Profile(profile).encode();
    {
        let session = state.session.lock().unwrap();
        for n in session.friend_list() {
            if session.friend_connection(n) != Connection::None {
                let _ = session.send_message(n, &wire);
            }
        }
    }
    let relays = relay_urls(&state);
    let pubkey = state.session.lock().unwrap().self_public_key();
    let toxid = state.session.lock().unwrap().self_address();
    for relay in &relays {
        let _ = crate::relay::register_profile(
            relay,
            &name,
            &pubkey,
            &toxid,
            &url,
        )
        .await;
    }
    Ok(url)
}

#[tauri::command]
pub async fn set_avatar_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("头像 URL 必须以 http:// 或 https:// 开头".to_string());
    }
    {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_set("avatar_url", &url)
            .map_err(|e| e.to_string())?;
    }
    // Broadcast updated profile.
    let me = state.session.lock().unwrap().self_public_key();
    let name = state.session.lock().unwrap().self_name();
    let bio = state
        .session
        .lock()
        .unwrap()
        .self_status_message()
        .unwrap_or_default();
    let profile = Profile {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        name: name.clone(),
        bio,
        avatar: url.clone(),
        avatar_len: url.len() as u64,
    };
    let wire = Envelope::Profile(profile).encode();
    {
        let session = state.session.lock().unwrap();
        for n in session.friend_list() {
            if session.friend_connection(n) != Connection::None {
                let _ = session.send_message(n, &wire);
            }
        }
    }
    let relays = relay_urls(&state);
    let pubkey = state.session.lock().unwrap().self_public_key();
    let toxid = state.session.lock().unwrap().self_address();
    for relay in &relays {
        let _ = crate::relay::register_profile(
            relay,
            &name,
            &pubkey,
            &toxid,
            &url,
        )
        .await;
    }
    Ok(())
}

#[tauri::command]
pub fn add_friend(state: State<AppState>, toxid: String, message: String) -> Result<u32, String> {
    let n = {
        let mut session = state.session.lock().unwrap();
        session
            .add_friend(toxid.trim(), message.trim())
            .map_err(|e| match e {
                ToxError::FriendAdd(5) => {
                    "好友请求已发送，等待对方接受（不能重复发送）".to_string()
                }
                other => format!("add friend failed: {other}"),
            })?
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
        let session = state.session.lock().unwrap();
        let me = session.self_public_key();
        let unfriend = Envelope::Unfriend(tox_social::envelope::Unfriend {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me,
            ts: now_ms(),
        });
        let wire = unfriend.encode();
        if session.friend_connection(friend_number) != Connection::None {
            let _ = session.send_message(friend_number, &wire);
        }
    }
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
pub async fn publish_post(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    public: Option<bool>,
) -> Result<Post, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let text = text.trim().to_string();
    let is_public = public.unwrap_or(false);
    let (mut post, mut envelopes) = {
        let engine = state.engine.lock().unwrap();
        if text.chars().count() > tox_social::MAX_POST_CHARS {
            if is_public {
                engine
                    .publish_long_public_post(&me, &text)
                    .map_err(|e| e.to_string())?
            } else {
                engine
                    .publish_long_post(&me, &text)
                    .map_err(|e| e.to_string())?
            }
        } else if is_public {
            let post = engine
                .publish_public_post(&me, &text)
                .map_err(|e| e.to_string())?;
            (post.clone(), vec![Envelope::Post(post)])
        } else {
            let post = engine
                .publish_post(&me, &text)
                .map_err(|e| e.to_string())?;
            (post.clone(), vec![Envelope::Post(post)])
        }
    };
    // Sign public posts with our Ed25519 identity (short and long alike), so
    // the Relay and other clients can verify authenticity.
    let mut ed_pk = String::new();
    if is_public {
        let session = state.session.lock().unwrap();
        let sig = session
            .sign_data(post.signing_string().as_bytes())
            .map_err(|e| e.to_string())?;
        let sig_hex = hex::encode(sig);
        post.sig = sig_hex.clone();
        ed_pk = session.self_ed25519_public_key();
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .post_update_sig(&post.id, &sig_hex)
            .map_err(|e| e.to_string())?;
        if text.chars().count() <= tox_social::MAX_POST_CHARS {
            envelopes = vec![Envelope::Post(post.clone())];
        }
    }
    for env in envelopes {
        fan_out(&state, env)?;
    }
    if is_public {
        let relays = relay_urls(&state);
        let pubkey = post.author.clone();
        let id = post.id.clone();
        let ts = post.ts;
        let text = post.text.clone();
        for relay in &relays {
            if let Err(e) = crate::relay::publish_post(
                relay,
                &pubkey,
                &id,
                ts,
                &text,
                &post.sig,
                &ed_pk,
            )
            .await
            {
                eprintln!("[toxsocial] relay publish failed: {e}");
                // The Relay may have rejected the post (bad signature,
                // out-of-sync timestamp, etc.); tell the user instead of
                // silently succeeding.
                let _ = app.emit(
                    "relay:publish_failed",
                    serde_json::json!({ "relay": relay, "error": e }),
                );
            }
        }
    }
    state.persist();
    Ok(post)
}

#[tauri::command]
pub fn publish_comment(
    state: State<AppState>,
    post_id: String,
    text: String,
    reply_to: Option<String>,
) -> Result<Comment, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let target = reply_to
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| post_id.trim().to_string());
    let comment = {
        let engine = state.engine.lock().unwrap();
        engine
            .publish_comment(&me, &target, text.trim())
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

fn author_meta(
    state: &State<AppState>,
    engine: &tox_social::feed::FeedEngine,
) -> HashMap<String, (String, String)> {
    let me = state.session.lock().unwrap().self_public_key();
    let dir_avatars: HashMap<String, String> = engine
        .store()
        .dir_all(1000)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.pubkey, e.avatar))
        .collect();
    let mut map = HashMap::new();
    let friends = engine.store().friend_list().unwrap_or_default();
    for f in friends {
        let pubkey: String = f.toxid.chars().take(64).collect();
        let avatar = if !f.avatar.is_empty() {
            f.avatar.clone()
        } else {
            dir_avatars.get(&pubkey).cloned().unwrap_or_default()
        };
        map.insert(f.toxid.clone(), (f.name.clone(), avatar.clone()));
        map.insert(pubkey, (f.name, avatar));
    }
    let avatar = engine
        .store()
        .kv_get("avatar_url")
        .unwrap_or_default()
        .unwrap_or_default();
    map.insert(me, (state.session.lock().unwrap().self_name(), avatar));
    map
}

#[tauri::command]
pub async fn fetch_timeline(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<TimelineItem>, String> {
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
    let meta = author_meta(&state, &engine);
    let rows = engine
        .timeline(&authors, limit)
        .into_iter()
        .filter(|r| r.kind == PostKind::Post)
        .collect::<Vec<_>>();
    Ok(rows
        .iter()
        .map(|r| {
            let (name, avatar) = meta
                .get(&r.author)
                .cloned()
                .unwrap_or_else(|| (r.author.chars().take(8).collect(), String::new()));
            events::item_from_row_with_meta(&state, &engine, r, &name, &avatar)
        })
        .collect())
}

#[tauri::command]
pub async fn search_posts(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let engine = state.engine.lock().unwrap();
    let meta = author_meta(&state, &engine);
    let rows = engine.search_posts(&query, limit);
    Ok(rows
        .iter()
        .map(|r| {
            let (name, avatar) = meta
                .get(&r.author)
                .cloned()
                .unwrap_or_else(|| (r.author.chars().take(8).collect(), String::new()));
            events::item_from_row_with_meta(&state, &engine, r, &name, &avatar)
        })
        .collect())
}

#[tauri::command]
pub async fn fetch_posts_by_author(state: State<'_, AppState>, pubkey: String, limit: Option<u32>) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let engine = state.engine.lock().unwrap();
    let meta = author_meta(&state, &engine);
    let rows = engine.posts_by_author(pubkey.trim(), limit);
    Ok(rows
        .iter()
        .map(|r| {
            let (name, avatar) = meta
                .get(&r.author)
                .cloned()
                .unwrap_or_else(|| (r.author.chars().take(8).collect(), String::new()));
            events::item_from_row_with_meta(&state, &engine, r, &name, &avatar)
        })
        .collect())
}

#[tauri::command]
pub async fn fetch_thread(state: State<'_, AppState>, post_id: String) -> Result<Vec<TimelineItem>, String> {
    let engine = state.engine.lock().unwrap();
    let meta = author_meta(&state, &engine);
    let post = engine
        .store()
        .post_get(post_id.trim())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "post not found".to_string())?;
    let map_item = |row: &PostRow| {
        let (name, avatar) = meta
            .get(&row.author)
            .cloned()
            .unwrap_or_else(|| (row.author.chars().take(8).collect(), String::new()));
        events::item_from_row_with_meta(&state, &engine, row, &name, &avatar)
    };
    let mut items = vec![map_item(&post)];
    let thread = engine
        .store()
        .thread_for(post_id.trim())
        .map_err(|e| e.to_string())?;
    for row in thread {
        items.push(map_item(&row));
    }
    Ok(items)
}

#[tauri::command]
pub fn send_join_channel(state: State<AppState>, toxid: String, channel_id: String) -> Result<(), String> {
    let toxid = toxid.trim().to_string();
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| toxid == pk || toxid.starts_with(&pk))
                    .unwrap_or(false)
            })
            .ok_or_else(|| "not_friend".to_string())?
    };
    let session = state.session.lock().unwrap();
    session
        .send_message(friend_number, &format!("join_channel {channel_id}"))
        .map_err(|e| format!("send join request failed: {e}"))
}

#[tauri::command]
pub fn send_file_to_friend(
    state: State<AppState>,
    friend_number: u32,
    filename: String,
    data_base64: String,
) -> Result<u32, String> {
    let data = decode_data_base64(&data_base64)?;
    let mut session = state.session.lock().unwrap();
    session
        .send_file_data(friend_number, &filename, &data)
        .map_err(|e| format!("send file failed: {e}"))
}

#[tauri::command]
pub fn send_file_to_friend_by_toxid(
    state: State<AppState>,
    toxid: String,
    filename: String,
    data_base64: String,
) -> Result<u32, String> {
    let toxid = toxid.trim().to_string();
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| toxid == pk || toxid.starts_with(&pk))
                    .unwrap_or(false)
            })
            .ok_or_else(|| "好友不存在或尚未添加".to_string())?
    };
    let data = decode_data_base64(&data_base64)?;
    let mut session = state.session.lock().unwrap();
    session
        .send_file_data(friend_number, &filename, &data)
        .map_err(|e| format!("send file failed: {e}"))
}

#[tauri::command]
pub fn accept_file(state: State<AppState>, friend_number: u32, file_number: u32) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .accept_file(friend_number, file_number)
        .map_err(|e| format!("accept file failed: {e}"))
}

#[tauri::command]
pub fn reject_file(state: State<AppState>, friend_number: u32, file_number: u32) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .reject_file(friend_number, file_number)
        .map_err(|e| format!("reject file failed: {e}"))
}

#[tauri::command]
pub fn get_friends(state: State<AppState>) -> Result<Vec<FriendInfo>, String> {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let friends = store.friend_list().map_err(|e| e.to_string())?;
    let dir_avatars: std::collections::HashMap<String, String> = store
        .dir_all(1000)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.pubkey, e.avatar))
        .collect();
    let session = state.session.lock().unwrap();
    let online_map: std::collections::HashMap<String, bool> = session
        .friend_list()
        .into_iter()
        .filter_map(|n| {
            let pk = session.friend_public_key(n).ok()?;
            Some((pk, session.friend_connection(n) != Connection::None))
        })
        .collect();
    Ok(friends
        .into_iter()
        .map(|f| {
            let pubkey: String = f.toxid.chars().take(64).collect();
            let online = online_map.get(&pubkey).copied().unwrap_or(false);
            let avatar = if !f.avatar.is_empty() {
                f.avatar.clone()
            } else {
                dir_avatars.get(&pubkey).cloned().unwrap_or_default()
            };
            FriendInfo {
                toxid: f.toxid,
                pubkey,
                name: f.name,
                avatar,
                bio: f.bio,
                online,
                last_seen: f.last_seen,
            }
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
pub fn get_relay_url(state: State<AppState>) -> Result<String, String> {
    Ok(current_relay(&state))
}

#[tauri::command]
pub fn set_relay_url(state: State<AppState>, url: String) -> Result<(), String> {
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Relay URL 必须以 http:// 或 https:// 开头".to_string());
    }
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .kv_set("relay_url", &url)
        .map_err(|e| format!("failed to save Relay URL: {e}"))
}

#[tauri::command]
pub fn get_relay_urls(state: State<AppState>) -> Result<Vec<String>, String> {
    Ok(relay_urls(&state))
}

#[tauri::command]
pub fn set_relay_urls(state: State<AppState>, urls: Vec<String>) -> Result<(), String> {
    let urls: Vec<String> = urls
        .into_iter()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .collect();
    if urls.is_empty() {
        return Err("至少需要一个有效的 Relay URL".to_string());
    }
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .kv_set("relay_urls", &serde_json::to_string(&urls).unwrap_or_default())
        .map_err(|e| format!("failed to save Relay URLs: {e}"))
}

#[tauri::command]
pub fn get_auto_start(app: tauri::AppHandle) -> Result<bool, String> {
    let autolaunch = app.autolaunch();
    autolaunch.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_auto_start(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn conference_new(state: State<AppState>) -> Result<u32, String> {
    let n = {
        let mut session = state.session.lock().unwrap();
        session
            .conference_new()
            .map_err(|e| format!("create conference failed: {e}"))?
    };
    let channel_id = {
        let session = state.session.lock().unwrap();
        session
            .conference_get_id(n)
            .map_err(|e| format!("get conference id failed: {e}"))?
    };
    mark_owned_channel(&state, &channel_id);
    state.persist();
    Ok(n)
}

#[tauri::command]
pub fn is_channel_owned(state: State<AppState>, conference_number: u32) -> Result<bool, String> {
    let channel_id = {
        let session = state.session.lock().unwrap();
        session
            .conference_get_id(conference_number)
            .map_err(|e| e.to_string())?
    };
    Ok(is_owned_channel(&state, &channel_id))
}

#[tauri::command]
pub fn conference_delete(state: State<AppState>, conference_number: u32) -> Result<(), String> {
    {
        let mut session = state.session.lock().unwrap();
        session
            .conference_delete(conference_number)
            .map_err(|e| format!("delete conference failed: {e}"))?;
    }
    // Drop persisted chat history for the deleted channel.
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .channel_messages_delete(conference_number)
        .map_err(|e| format!("delete channel messages failed: {e}"))?;
    drop(engine);
    state.persist();
    Ok(())
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
pub fn conference_invite_by_toxid(
    state: State<AppState>,
    conference_number: u32,
    toxid: String,
) -> Result<(), String> {
    let toxid = toxid.trim().to_string();
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| toxid == pk || toxid.starts_with(&pk))
                    .unwrap_or(false)
            })
            .ok_or_else(|| "好友不存在或尚未添加".to_string())?
    };
    let session = state.session.lock().unwrap();
    session
        .conference_invite(friend_number, conference_number)
        .map_err(|e| format!("invite failed: {e}"))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConferenceSendResult {
    pub id: i64,
    /// True when the message was queued offline because nobody else was in
    /// the channel; it will be flushed automatically when members join.
    pub queued: bool,
}

#[tauri::command]
pub fn conference_send(
    state: State<AppState>,
    conference_number: u32,
    text: String,
) -> Result<ConferenceSendResult, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("empty message".to_string());
    }
    // When the channel has no other members, toxcore cannot deliver the
    // message (nothing to broadcast to), so queue it as an offline message
    // and flush it later once someone joins (see
    // `flush_pending_channel_messages` in events.rs).
    let queued = {
        let session = state.session.lock().unwrap();
        let peers = session
            .conference_peer_count(conference_number)
            .unwrap_or(1);
        if peers > 1 {
            session
                .conference_send_message(conference_number, &text)
                .map_err(|e| format!("send to conference failed: {e}"))?;
            false
        } else {
            true
        }
    };
    // Persist the outbound message so history survives restarts.
    let (channel_id, me) = {
        let session = state.session.lock().unwrap();
        (
            session
                .conference_get_id(conference_number)
                .unwrap_or_default(),
            session.self_public_key(),
        )
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let engine = state.engine.lock().unwrap();
    let id = engine
        .store()
        .channel_message_insert(&ChannelMessageRow {
            id: 0,
            conference_number,
            channel_id,
            peer_name: String::new(),
            peer_key: me,
            text: text.clone(),
            ts,
            direction: 1,
            pending: queued,
        })
        .map_err(|e| format!("persist channel message failed: {e}"))?;
    Ok(ConferenceSendResult { id, queued })
}

/// Persisted chat history for a conference (newest-first capped, returned in
/// chronological order). Falls back to the stable channel id so history
/// survives restarts even if the conference number changed.
#[tauri::command]
pub fn channel_messages(
    state: State<AppState>,
    conference_number: u32,
    limit: Option<u32>,
) -> Result<Vec<ChannelMessageInfo>, String> {
    let limit = limit.unwrap_or(300).min(1000);
    let channel_id = {
        let session = state.session.lock().unwrap();
        session
            .conference_get_id(conference_number)
            .unwrap_or_default()
    };
    let engine = state.engine.lock().unwrap();
    let rows = engine
        .store()
        .channel_messages_for_conference(conference_number, &channel_id, limit)
        .map_err(|e| format!("load channel messages failed: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|r| ChannelMessageInfo {
            id: r.id,
            peer_name: r.peer_name,
            text: r.text,
            ts: r.ts,
            direction: r.direction,
        })
        .collect())
}

#[tauri::command]
pub fn get_conference_peer_count(state: State<AppState>, conference_number: u32) -> Result<u32, String> {
    let session = state.session.lock().unwrap();
    session
        .conference_peer_count(conference_number)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conference_id(state: State<AppState>, conference_number: u32) -> Result<String, String> {
    let session = state.session.lock().unwrap();
    session
        .conference_get_id(conference_number)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_conferences(state: State<AppState>) -> Result<Vec<u32>, String> {
    let session = state.session.lock().unwrap();
    Ok(session.conference_chatlist())
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

#[tauri::command]
pub fn search_directory(state: State<AppState>, query: String, limit: Option<u32>) -> Result<Vec<DirectoryEntryInfo>, String> {
    let limit = limit.unwrap_or(50);
    let engine = state.engine.lock().unwrap();
    let rows = engine
        .store()
        .dir_search(query.trim(), limit)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|e| DirectoryEntryInfo {
            name: e.name,
            pubkey: e.pubkey,
            toxid: e.toxid,
            avatar: e.avatar,
            relay: e.relay,
            source: e.source,
        })
        .collect())
}

#[tauri::command]
pub fn request_directory_search(state: State<AppState>, query: String, depth: Option<u32>) -> Result<usize, String> {
    let depth = depth.unwrap_or(2);
    let me = state.session.lock().unwrap().self_public_key();
    let req = Envelope::DirReq(tox_social::envelope::DirReq {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        query: query.trim().to_string(),
        depth,
    });
    let wire = req.encode();
    let session = state.session.lock().unwrap();
    let mut sent = 0;
    for n in session.friend_list() {
        if session.friend_connection(n) != Connection::None {
            if session.send_message(n, &wire).is_ok() {
                sent += 1;
            }
        }
    }
    Ok(sent)
}

#[tauri::command]
pub async fn fetch_public_timeline(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let engine = state.engine.lock().unwrap();
    let meta = author_meta(&state, &engine);
    let rows = engine
        .store()
        .public_posts_since(0, limit)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let (name, avatar) = meta
                .get(&r.author)
                .cloned()
                .unwrap_or_else(|| (r.author.chars().take(8).collect(), String::new()));
            events::item_from_row_with_meta(&state, &engine, r, &name, &avatar)
        })
        .collect())
}

#[tauri::command]
pub fn request_public_posts(state: State<AppState>, since: Option<i64>, depth: Option<u32>) -> Result<usize, String> {
    let since = since.unwrap_or(0);
    let depth = depth.unwrap_or(2);
    let me = state.session.lock().unwrap().self_public_key();
    let req = Envelope::OutboxReq(tox_social::envelope::OutboxReq {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        since,
        depth,
    });
    let wire = req.encode();
    let session = state.session.lock().unwrap();
    let mut sent = 0;
    for n in session.friend_list() {
        if session.friend_connection(n) != Connection::None {
            if session.send_message(n, &wire).is_ok() {
                sent += 1;
            }
        }
    }
    Ok(sent)
}

#[tauri::command]
pub async fn search_relay_directory(state: State<'_, AppState>, query: String) -> Result<Vec<DirectoryEntryInfo>, String> {
    let relays = relay_urls(&state);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for relay in &relays {
        let entries = crate::relay::search_directory(relay, query.trim())
            .await
            .unwrap_or_default();
        for e in entries {
            if !seen.insert(e.pubkey.clone()) {
                continue;
            }
            out.push(DirectoryEntryInfo {
                name: e.name,
                pubkey: e.pubkey,
                toxid: e.toxid,
                avatar: e.avatar,
                relay: e.relay,
                source: "relay".to_string(),
            });
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn fetch_relay_public_posts(state: State<'_, AppState>, since: Option<i64>) -> Result<usize, String> {
    let since = since.unwrap_or(0);
    let relays = relay_urls(&state);
    let received_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut count = 0;
    for relay in &relays {
        let items = crate::relay::fetch_outbox(relay, since).await?;
        let engine = state.engine.lock().unwrap();
        for item in &items {
            let id = item["id"].as_str().unwrap_or("").to_string();
            let pubkey = item["pubkey"].as_str().unwrap_or("").to_string();
            let text = item["text"].as_str().unwrap_or("").to_string();
            let ts = item["ts"].as_i64().unwrap_or(0);
            let sig = item["sig"].as_str().unwrap_or("").to_string();
            if id.is_empty() || pubkey.is_empty() {
                continue;
            }
            let post = tox_social::envelope::Post {
                v: tox_social::envelope::PROTOCOL_VERSION,
                id: id.clone(),
                author: pubkey.clone(),
                ts,
                text,
                public: true,
                sig,
            };
            let env = Envelope::Post(post);
            if engine.persist(&env, &pubkey, received_at) {
                count += 1;
                // Relay validated the timestamp (within ±15s), so it can be
                // trusted for display; mark it so the UI stops warning.
                let _ = engine.store().post_mark_relay_verified(&id);
            }
        }
    }
    Ok(count)
}

/// Check the Relay(s) for a post that arrived directly from a friend. If the
/// Relay has it (same author + timestamp), its timestamp passed the Relay's
/// ±15s server-clock check, so we can mark it verified and drop the UI
/// warning. Best-effort: network/relay failures just leave it unverified.
pub async fn verify_post_on_relay(
    state: &State<'_, AppState>,
    post_id: &str,
) -> Result<bool, String> {
    let relays = relay_urls(state);
    let (author, ts) = {
        let engine = state.engine.lock().unwrap();
        match engine.store().post_get(post_id) {
            Ok(Some(p)) => (p.author, p.ts),
            _ => return Ok(false),
        }
    };
    for relay in &relays {
        let Ok(Some(item)) = crate::relay::fetch_post_by_id(relay, post_id).await else {
            continue;
        };
        let item_pubkey = item["pubkey"].as_str().unwrap_or("");
        let item_ts = item["ts"].as_i64().unwrap_or(0);
        if item_pubkey.eq_ignore_ascii_case(&author) && item_ts == ts {
            let engine = state.engine.lock().unwrap();
            let _ = engine.store().post_mark_relay_verified(post_id);
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
pub async fn add_channel_host(
    state: State<'_, AppState>,
    channel_id: String,
    new_host_toxid: String,
) -> Result<(), String> {
    let requester = {
        let session = state.session.lock().unwrap();
        session.self_address()
    };
    let relays = relay_urls(&state);
    for relay in &relays {
        crate::relay::add_channel_host(
            relay,
            &channel_id,
            &requester,
            new_host_toxid.trim(),
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_channel_host(
    state: State<'_, AppState>,
    channel_id: String,
    remove_host_toxid: String,
) -> Result<(), String> {
    let requester = {
        let session = state.session.lock().unwrap();
        session.self_address()
    };
    let relays = relay_urls(&state);
    for relay in &relays {
        crate::relay::remove_channel_host(
            relay,
            &channel_id,
            &requester,
            remove_host_toxid.trim(),
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_public_channel(state: State<'_, AppState>, channel_id: String) -> Result<(), String> {
    let host_toxid = {
        let session = state.session.lock().unwrap();
        session.self_address()
    };
    let relays = relay_urls(&state);
    for relay in &relays {
        crate::relay::delete_channel(relay, &channel_id, &host_toxid).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_public_channels(state: State<'_, AppState>) -> Result<Vec<PublicChannelInfo>, String> {
    let relays = relay_urls(&state);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for relay in &relays {
        let channels = crate::relay::list_channels(relay).await.unwrap_or_default();
        for c in channels {
            if !seen.insert(c.channel_id.clone()) {
                continue;
            }
            out.push(PublicChannelInfo {
                name: c.name,
                desc: c.desc,
                host_toxid: c.host_toxid,
                channel_id: c.channel_id,
                hosts: c.hosts,
                members: c.members,
            });
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn report_channel_memberships(state: State<'_, AppState>) -> Result<usize, String> {
    let own_toxid = {
        let session = state.session.lock().unwrap();
        session.self_address()
    };
    let conferences = {
        let session = state.session.lock().unwrap();
        session.conference_chatlist()
    };
    let relays = relay_urls(&state);
    let mut public_ids = std::collections::HashSet::new();
    for relay in &relays {
        let channels = crate::relay::list_channels(relay).await.unwrap_or_default();
        for c in channels {
            public_ids.insert(c.channel_id);
        }
    }
    let mut reported = 0;
    for n in conferences {
        let channel_id = {
            let session = state.session.lock().unwrap();
            session.conference_get_id(n).map_err(|e| e.to_string())?
        };
        if public_ids.contains(&channel_id) {
            for relay in &relays {
                crate::relay::report_channel_membership(
                    relay,
                    &channel_id,
                    &own_toxid,
                )
                .await?;
            }
            reported += 1;
        }
    }
    Ok(reported)
}

#[tauri::command]
pub async fn register_public_channel(
    state: State<'_, AppState>,
    conference_number: u32,
    name: String,
    desc: String,
) -> Result<(), String> {
    let (channel_id, host_toxid, pubkey) = {
        let session = state.session.lock().unwrap();
        let channel_id = session
            .conference_get_id(conference_number)
            .map_err(|e| e.to_string())?;
        let host_toxid = session.self_address();
        let pubkey = session.self_public_key();
        (channel_id, host_toxid, pubkey)
    };
    let relays = relay_urls(&state);
    let mut is_host = false;
    for relay in &relays {
        let existing = crate::relay::list_channels(relay).await.unwrap_or_default();
        if let Some(c) = existing.iter().find(|c| c.channel_id == channel_id) {
            if c.hosts.iter().any(|h| {
                h == &host_toxid
                    || h == &pubkey
                    || host_toxid.starts_with(h)
                    || h.starts_with(&pubkey)
            }) {
                is_host = true;
                break;
            }
        }
    }
    if !is_owned_channel(&state, &channel_id) && !is_host {
        return Err("只有频道创建者或 host 才能发布为公共频道".to_string());
    }
    for relay in &relays {
        crate::relay::register_channel(
            relay,
            name.trim(),
            desc.trim(),
            &host_toxid,
            &channel_id,
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Return all configured Relay URLs, defaulting to the built-in public relay.
fn relay_urls(state: &State<AppState>) -> Vec<String> {
    let engine = state.engine.lock().unwrap();
    let raw = engine
        .store()
        .kv_get("relay_urls")
        .unwrap_or_default()
        .unwrap_or_default();
    let mut urls: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    if urls.is_empty() {
        let single = engine
            .store()
            .kv_get("relay_url")
            .unwrap_or_default()
            .unwrap_or_default();
        if !single.is_empty() {
            urls.push(single);
        }
    }
    if urls.is_empty() {
        urls.push(crate::relay::DEFAULT_RELAY.to_string());
    }
    urls
}

/// Return the first configured Relay URL, defaulting to the built-in public relay.
fn current_relay(state: &State<AppState>) -> String {
    relay_urls(state).into_iter().next().unwrap_or_else(|| crate::relay::DEFAULT_RELAY.to_string())
}

/// Whether the current user created this channel locally.
fn is_owned_channel(state: &State<AppState>, channel_id: &str) -> bool {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let raw = store
        .kv_get("owned_channel_ids")
        .unwrap_or_default()
        .unwrap_or_default();
    serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .iter()
        .any(|id| id == channel_id)
}

/// Record a channel as created by the current user, so only the creator can
/// publish it as a public channel unless they are also a host.
fn mark_owned_channel(state: &State<AppState>, channel_id: &str) {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let raw = store
        .kv_get("owned_channel_ids")
        .unwrap_or_default()
        .unwrap_or_default();
    let mut ids: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    if !ids.iter().any(|id| id == channel_id) {
        ids.push(channel_id.to_string());
        let _ = store.kv_set("owned_channel_ids", &serde_json::to_string(&ids).unwrap_or_default());
    }
}

/// Decode base64, accepting either raw base64 or a `data:` URL (as produced by
/// `FileReader.readAsDataURL` in the frontend).
fn decode_data_base64(data_base64: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let b64 = data_base64
        .trim()
        .strip_prefix("data:")
        .and_then(|s| s.split_once(',').map(|(_, b)| b))
        .unwrap_or(data_base64.trim());
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("invalid base64: {e}"))
}

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
