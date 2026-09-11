//! Tauri commands: the IPC surface exposed to the Vue frontend.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

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
    /// Whether the current user has reacted with this emoji.
    pub mine: bool,
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
    /// "friend" = mutual contact; "follow" = subscription (conference).
    pub kind: String,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrivateMessageInfo {
    pub id: i64,
    pub text: String,
    pub ts: i64,
    pub direction: i64,
}

/// Send a private (1:1) chat message to a friend. Plain Tox message, no TSP
/// envelope — this is the friend chat channel, not the social protocol.
#[tauri::command]
pub async fn send_private_message(
    state: State<'_, AppState>,
    peer: String,
    text: String,
) -> Result<i64, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("empty message".to_string());
    }
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| pk == peer || peer.starts_with(&pk))
                    .unwrap_or(false)
            })
            .ok_or_else(|| "对方不在你的关注列表里".to_string())?
    };
    {
        let session = state.session.lock().unwrap();
        session
            .send_message(friend_number, &text)
            .map_err(|e| format!("send failed: {e}"))?;
    }
    let ts = now_ms();
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .private_message_insert(&peer, &text, ts, 1)
        .map_err(|e| format!("persist failed: {e}"))
}

/// Persisted private chat history with a peer, chronological order.
#[tauri::command]
pub async fn private_messages(
    state: State<'_, AppState>,
    peer: String,
    limit: Option<u32>,
) -> Result<Vec<PrivateMessageInfo>, String> {
    let limit = limit.unwrap_or(200).min(1000);
    let engine = state.engine.lock().unwrap();
    let rows = engine
        .store()
        .private_messages_for_peer(&peer, limit)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, text, ts, direction)| PrivateMessageInfo {
            id,
            text,
            ts,
            direction,
        })
        .collect())
}

/// Check for an update via the Tauri updater (signed latest.json on GitHub
/// Releases). Returns the new version when one is available.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("updater unavailable: {e}"))?;
    let update = updater.check().await.map_err(|e| format!("{e}"))?;
    match update {
        Some(u) => Ok(UpdateInfo {
            current: current.clone(),
            latest: u.version.clone(),
            has_update: true,
        }),
        None => Ok(UpdateInfo {
            current: current.clone(),
            latest: current,
            has_update: false,
        }),
    }
}

/// Download and install the pending update, then relaunch the app.
///
/// The restart is done via a detached "wait 2s then start" jump script
/// instead of an immediate relaunch: the old process's WebView2 browser tree
/// may take a moment to release the user-data folder, and an immediate
/// relaunch into a locked EBWebView is exactly the "opens but hangs" bug.
#[tauri::command]
pub async fn perform_update(app: AppHandle) -> Result<(), String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("updater unavailable: {e}"))?;
    let Some(update) = updater.check().await.map_err(|e| format!("{e}"))? else {
        return Err("已是最新版本".to_string());
    };
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(
                    "update:progress",
                    serde_json::json!({ "downloaded": downloaded, "total": total }),
                );
            },
            || {
                println!("[toxsocial] update downloaded, installing…");
            },
        )
        .await
        .map_err(|e| format!("update failed: {e}"))?;
    // Relaunch via a detached jump script: give the old process (and its
    // WebView2 browser tree) two seconds to fully release the user-data
    // folder before the new binary starts.
    let exe = std::env::current_exe().map_err(|e| format!("{e}"))?;
    let jump = std::env::temp_dir().join("toxsocial-relaunch.cmd");
    std::fs::write(
        &jump,
        format!(
            "@echo off\r\ntimeout /t 2 /nobreak >nul\r\nstart \"\" \"{}\"\r\n",
            exe.display()
        ),
    )
    .map_err(|e| format!("{e}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", &jump.display().to_string()])
            .creation_flags(0x00000008) // DETACHED_PROCESS
            .spawn()
            .map_err(|e| format!("{e}"))?;
    }
    app.exit(0);
    #[allow(unreachable_code)]
    Ok(())
}

/// Numeric segment-by-segment comparison ("0.2.25" > "0.2.24").
fn compare_versions(a: &str, b: &str) -> i32 {
    let pa: Vec<i64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<i64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }
    0
}

#[tauri::command]
pub async fn get_own_info(state: State<'_, AppState>) -> Result<OwnInfo, String> {
    let session = state.session.lock().unwrap();
    let avatar = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .kv_get("avatar_url")
            .unwrap_or_default()
            .unwrap_or_default()
    };
    Ok(OwnInfo {
        toxid: session.self_address(),
        pubkey: session.self_public_key(),
        name: session.self_name(),
        status_message: session
            .self_status_message()
            .unwrap_or_default(),
        avatar,
        friend_count: session.friend_count(),
    })
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
pub async fn add_friend(
    state: State<'_, AppState>,
    toxid: String,
    message: String,
    kind: Option<String>,
) -> Result<u32, String> {
    let kind = match kind.as_deref() {
        Some("follow") => "follow",
        _ => "friend",
    };
    let toxid = toxid.trim().to_string();
    let n = {
        let mut session = state.session.lock().unwrap();
        session
            .add_friend(&toxid, message.trim())
            .map_err(|e| match e {
                ToxError::FriendAdd(5) => {
                    "好友请求已发送，等待对方接受（不能重复发送）".to_string()
                }
                other => format!("add friend failed: {other}"),
            })?
    };
    // Record how this contact entered the list. The toxcore friend already
    // exists at this point, so a duplicate-row error is not fatal.
    {
        let engine = state.engine.lock().unwrap();
        let store = engine.store();
        let pk: String = toxid.chars().take(64).collect();
        if let Err(e) = store.friend_set_kind(&pk, kind) {
            eprintln!("[toxsocial] set friend kind failed: {e}");
        }
    }
    state.persist();
    Ok(n)
}

/// Re-classify an existing contact between the friends and following lists.
#[tauri::command]
pub async fn set_contact_kind(
    state: State<'_, AppState>,
    toxid: String,
    kind: String,
) -> Result<(), String> {
    let kind = if kind == "follow" { "follow" } else { "friend" };
    let pk: String = toxid.trim().chars().take(64).collect();
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .friend_set_kind(&pk, kind)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_friend(state: State<'_, AppState>, friend_number: u32) -> Result<(), String> {
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
pub async fn remove_friend_by_toxid(state: State<'_, AppState>, toxid: String) -> Result<(), String> {
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
    community: Option<String>,
) -> Result<Post, String> {
    let me = state.session.lock().unwrap().self_public_key();
    let text = text.trim().to_string();
    let is_public = public.unwrap_or(false);
    // Community posts are inherently public.
    let community = community
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty());
    let is_public = is_public || community.is_some();
    let community_ref = community.as_deref();
    let (mut post, mut envelopes) = {
        let engine = state.engine.lock().unwrap();
        if text.chars().count() > tox_social::MAX_POST_CHARS {
            if is_public {
                engine
                    .publish_long_public_post(&me, &text, community_ref)
                    .map_err(|e| e.to_string())?
            } else {
                engine
                    .publish_long_post(&me, &text)
                    .map_err(|e| e.to_string())?
            }
        } else if is_public {
            let post = engine
                .publish_public_post(&me, &text, community_ref)
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
        let community = post.community.clone();
        for relay in &relays {
            if let Err(e) = crate::relay::publish_post(
                relay,
                &pubkey,
                &id,
                ts,
                &text,
                &post.sig,
                &ed_pk,
                community.as_deref(),
            )
            .await
            {
                // Retry a couple of times: transient network failures are
                // common, and a silently-missing post is why friends can
                // never see it in the public page.
                let mut last = e.clone();
                for attempt in 1..=2 {
                    std::thread::sleep(std::time::Duration::from_secs(attempt));
                    match crate::relay::publish_post(
                        relay,
                        &pubkey,
                        &id,
                        ts,
                        &text,
                        &post.sig,
                        &ed_pk,
                        community.as_deref(),
                    )
                    .await
                    {
                        Ok(()) => {
                            last = String::new();
                            break;
                        }
                        Err(_) if attempt == 2 => {
                            last = format!("{e} (after retries)");
                        }
                        Err(_) => {}
                    }
                }
                if !last.is_empty() {
                    eprintln!("[toxsocial] relay publish failed: {last}");
                    // The Relay may have rejected the post (bad signature,
                    // out-of-sync timestamp, etc.); tell the user instead of
                    // silently succeeding.
                    let _ = app.emit(
                        "relay:publish_failed",
                        serde_json::json!({ "relay": relay, "error": last }),
                    );
                }
            }
        }
    }
    state.persist();
    Ok(post)
}

#[tauri::command]
pub async fn publish_comment(
    state: State<'_, AppState>,
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
pub async fn publish_reaction(
    state: State<'_, AppState>,
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
pub async fn send_join_channel(state: State<'_, AppState>, toxid: String, channel_id: String) -> Result<(), String> {
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
pub async fn send_file_to_friend(
    state: State<'_, AppState>,
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
pub async fn send_file_to_friend_by_toxid(
    state: State<'_, AppState>,
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
pub async fn accept_file(state: State<'_, AppState>, friend_number: u32, file_number: u32) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .accept_file(friend_number, file_number)
        .map_err(|e| format!("accept file failed: {e}"))
}

#[tauri::command]
pub async fn reject_file(state: State<'_, AppState>, friend_number: u32, file_number: u32) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .reject_file(friend_number, file_number)
        .map_err(|e| format!("reject file failed: {e}"))
}

#[tauri::command]
pub async fn get_friends(state: State<'_, AppState>) -> Result<Vec<FriendInfo>, String> {
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
                kind: f.kind,
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
pub async fn set_imgur_client_id(state: State<'_, AppState>, client_id: String) -> Result<(), String> {
    let engine = state.engine.lock().unwrap();
    engine
        .store()
        .kv_set("imgur_client_id", client_id.trim())
        .map_err(|e| format!("failed to save Imgur Client ID: {e}"))
}

#[tauri::command]
pub async fn get_media_config(state: State<'_, AppState>) -> Result<MediaConfig, String> {
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
pub async fn get_relay_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(current_relay(&state))
}

#[tauri::command]
pub async fn set_relay_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
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
pub async fn get_relay_urls(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(relay_urls(&state))
}

#[tauri::command]
pub async fn set_relay_urls(state: State<'_, AppState>, urls: Vec<String>) -> Result<(), String> {
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

/// Windows registry "Run" key used for auto-start.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "ToxSocial";

fn run_reg(args: &[&str]) -> bool {
    std::process::Command::new("reg")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Auto-start is implemented directly on the Windows Run key so we can add a
/// `--minimized` flag: on boot the app starts hidden in the tray instead of
/// popping a window.
#[tauri::command]
pub fn get_auto_start() -> Result<bool, String> {
    Ok(run_reg(&["query", RUN_KEY, "/v", RUN_VALUE]))
}

#[tauri::command]
pub fn set_auto_start(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot locate exe: {e}"))?;
        let cmd = format!("\"{}\" --minimized", exe.display());
        if !run_reg(&[
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &cmd, "/f",
        ]) {
            return Err("failed to write auto-start registry entry".to_string());
        }
    } else if !run_reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]) {
        return Err("failed to remove auto-start registry entry".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn conference_new(state: State<'_, AppState>) -> Result<u32, String> {
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
pub async fn is_channel_owned(state: State<'_, AppState>, conference_number: u32) -> Result<bool, String> {
    let channel_id = {
        let session = state.session.lock().unwrap();
        session
            .conference_get_id(conference_number)
            .map_err(|e| e.to_string())?
    };
    Ok(is_owned_channel(&state, &channel_id))
}

#[tauri::command]
pub async fn conference_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    conference_number: u32,
) -> Result<(), String> {
    let channel_id = {
        let mut session = state.session.lock().unwrap();
        let id = session
            .conference_get_id(conference_number)
            .unwrap_or_default();
        session
            .conference_delete(conference_number)
            .map_err(|e| format!("delete conference failed: {e}"))?;
        id
    };
    // Drop persisted chat history keyed by the stable channel id. Deleting by
    // conference number is unsafe: toxcore reuses numbers after a channel is
    // deleted, which would wipe (or leak into) a newer channel's messages.
    {
        let engine = state.engine.lock().unwrap();
        if !channel_id.is_empty() {
            engine
                .store()
                .channel_messages_delete_by_channel(&channel_id)
                .map_err(|e| format!("delete channel messages failed: {e}"))?;
        }
    }
    state.persist();
    // Tell the Relay(s) we left this channel so its online-member count drops
    // immediately (instead of waiting for the 5-minute TTL). Signed, so only
    // the member themselves can trigger the removal.
    if !channel_id.is_empty() {
        let relays = relay_urls(&state);
        let (own_toxid, ed_pk) = {
            let session = state.session.lock().unwrap();
            (session.self_address(), session.self_ed25519_public_key())
        };
        let ts = now_ms();
        let sig = {
            let session = state.session.lock().unwrap();
            let s = session
                .sign_data(format!("members|{channel_id}|{own_toxid}|{ts}|leave").as_bytes())
                .map_err(|e| e.to_string())?;
            hex::encode(s)
        };
        for relay in relays {
            if let Err(e) = crate::relay::report_channel_membership(
                &relay,
                &channel_id,
                &own_toxid,
                true,
                ts,
                &sig,
                &ed_pk,
            )
            .await
            {
                eprintln!("[toxsocial] relay leave report failed: {e}");
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn conference_invite(
    state: State<'_, AppState>,
    friend_number: u32,
    conference_number: u32,
) -> Result<(), String> {
    let session = state.session.lock().unwrap();
    session
        .conference_invite(friend_number, conference_number)
        .map_err(|e| format!("invite failed: {e}"))
}

#[tauri::command]
pub async fn conference_invite_by_toxid(
    state: State<'_, AppState>,
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
pub async fn conference_send(
    state: State<'_, AppState>,
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
/// survives restarts even if the conference number changed. `before_id`
/// pages older history for the "load earlier" button.
#[tauri::command]
pub async fn channel_messages(
    state: State<'_, AppState>,
    conference_number: u32,
    limit: Option<u32>,
    before_id: Option<i64>,
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
        .channel_messages_for_conference(conference_number, &channel_id, limit, before_id)
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
pub async fn get_conference_peer_count(state: State<'_, AppState>, conference_number: u32) -> Result<u32, String> {
    let session = state.session.lock().unwrap();
    session
        .conference_peer_count(conference_number)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_conference_id(state: State<'_, AppState>, conference_number: u32) -> Result<String, String> {
    let session = state.session.lock().unwrap();
    session
        .conference_get_id(conference_number)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_conferences(state: State<'_, AppState>) -> Result<Vec<u32>, String> {
    let session = state.session.lock().unwrap();
    Ok(session.conference_chatlist())
}

#[tauri::command]
pub async fn conference_peers(
    state: State<'_, AppState>,
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
pub async fn request_sync_all(state: State<'_, AppState>) -> Result<usize, String> {
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
pub async fn search_directory(state: State<'_, AppState>, query: String, limit: Option<u32>) -> Result<Vec<DirectoryEntryInfo>, String> {
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
pub async fn request_directory_search(state: State<'_, AppState>, query: String, depth: Option<u32>) -> Result<usize, String> {
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

// ---------------------------------------------------------------------------
// Communities (Reddit-style topic feeds, distinct from chat groups)
// ---------------------------------------------------------------------------

/// One community in `my_communities`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommunityInfo {
    pub channel_id: String,
    pub conference_number: u32,
    pub name: String,
    pub desc: String,
    pub created_by_me: bool,
}

fn my_communities_load(state: &State<AppState>) -> Vec<CommunityInfo> {
    let engine = state.engine.lock().unwrap();
    let raw = engine
        .store()
        .kv_get("my_communities")
        .ok()
        .flatten()
        .unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn my_communities_save(state: &State<AppState>, list: &[CommunityInfo]) {
    let engine = state.engine.lock().unwrap();
    if let Ok(raw) = serde_json::to_string(list) {
        let _ = engine.store().kv_set("my_communities", &raw);
    }
}

/// Create a community: a dedicated conference (membership + realtime
/// distribution) registered in the Relay public directory under its stable
/// channel id.
#[tauri::command]
pub async fn create_community(
    state: State<'_, AppState>,
    name: String,
    desc: String,
) -> Result<CommunityInfo, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("社区名称不能为空".to_string());
    }
    let conference_number = {
        let mut session = state.session.lock().unwrap();
        session
            .conference_new()
            .map_err(|e| format!("create community failed: {e}"))?
    };
    let channel_id = {
        let session = state.session.lock().unwrap();
        session
            .conference_get_id(conference_number)
            .map_err(|e| format!("get community id failed: {e}"))?
    };
    let info = CommunityInfo {
        channel_id: channel_id.clone(),
        conference_number,
        name: name.clone(),
        desc: desc.trim().to_string(),
        created_by_me: true,
    };
    let mut list = my_communities_load(&state);
    list.push(info.clone());
    my_communities_save(&state, &list);
    state.persist();

    // Register in the public directory so others can discover and join.
    // Best-effort: retried on the next publish/open if the Relay is down.
    let relays = relay_urls(&state);
    let host_toxid = state.session.lock().unwrap().self_address();
    for relay in &relays {
        if let Err(e) =
            crate::relay::register_channel(relay, &name, desc.trim(), &host_toxid, &channel_id, "community").await
        {
            eprintln!("[toxsocial] community register failed: {e}");
        }
    }
    Ok(info)
}

/// Communities the user created or joined (local registry).
#[tauri::command]
pub async fn my_communities(state: State<'_, AppState>) -> Result<Vec<CommunityInfo>, String> {
    Ok(my_communities_load(&state))
}

/// Join a discovered community: record it locally and ask a known member
/// (host/co-host/members from the directory) to pull us into the conference.
#[tauri::command]
pub async fn join_community(
    state: State<'_, AppState>,
    channel_id: String,
    name: String,
    desc: String,
) -> Result<(), String> {
    let channel_id = channel_id.trim().to_lowercase();
    let mut list = my_communities_load(&state);
    if list.iter().any(|c| c.channel_id == channel_id) {
        return Ok(());
    }
    list.push(CommunityInfo {
        channel_id: channel_id.clone(),
        conference_number: u32::MAX, // resolved after the invite arrives
        name,
        desc,
        created_by_me: false,
    });
    my_communities_save(&state, &list);
    drop(list);
    // Ask known members for an invite (same flow as public groups).
    let relays = relay_urls(&state);
    for relay in &relays {
        let channels = crate::relay::list_channels(relay, "community").await.unwrap_or_default();
        let Some(ch) = channels.iter().find(|c| c.channel_id == channel_id) else {
            continue;
        };
        let mut contacts: Vec<String> = Vec::new();
        if !ch.host_toxid.is_empty() {
            contacts.push(ch.host_toxid.clone());
        }
        for h in &ch.hosts {
            contacts.push(h.clone());
        }
        for m in &ch.members {
            contacts.push(m.clone());
        }
        for contact in contacts {
            let _ = send_join_channel_inner(&state, &contact, &channel_id);
        }
        break;
    }
    Ok(())
}

/// Storage housekeeping: prune old rows and report the freed space / db size.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub removed_posts: usize,
    pub removed_channel_msgs: usize,
    pub removed_private_msgs: usize,
    pub db_size_bytes: i64,
}

#[tauri::command]
pub async fn cleanup_database(state: State<'_, AppState>) -> Result<CleanupReport, String> {
    let engine = state.engine.lock().unwrap();
    let (posts, channel, private) = engine
        .store()
        .cleanup(5_000, 500, 1_000)
        .map_err(|e| format!("cleanup failed: {e}"))?;
    let db_size_bytes = engine.store().db_size_bytes();
    Ok(CleanupReport {
        removed_posts: posts,
        removed_channel_msgs: channel,
        removed_private_msgs: private,
        db_size_bytes,
    })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DbStats {
    pub db_size_bytes: i64,
    pub post_count: i64,
    pub channel_msg_count: i64,
    pub private_msg_count: i64,
}

#[tauri::command]
pub async fn db_stats(state: State<'_, AppState>) -> Result<DbStats, String> {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let count = |sql: &str| -> i64 { store.query_count(sql) };
    Ok(DbStats {
        db_size_bytes: store.db_size_bytes(),
        post_count: count("SELECT COUNT(*) FROM posts"),
        channel_msg_count: count("SELECT COUNT(*) FROM channel_messages"),
        private_msg_count: count("SELECT COUNT(*) FROM private_messages"),
    })
}

/// Export the account (profile save data, base64) so the user can back it up
/// or move to another machine. The export is the raw save data; if the on-disk
/// profile is DPAPI-encrypted we re-export from the live session instead.
#[tauri::command]
pub async fn export_account(state: State<'_, AppState>) -> Result<String, String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    let session = state.session.lock().unwrap();
    let save = session.save();
    Ok(BASE64.encode(save))
}

/// Import an account from an `export_account` backup: replaces the on-disk
/// profile with the imported save (previous profile kept as .bak) and flags
/// the app to restart. The current session is unaffected until restart.
#[tauri::command]
pub async fn import_account(state: State<'_, AppState>, data_b64: String) -> Result<(), String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    let data = BASE64
        .decode(data_b64.trim())
        .map_err(|e| format!("invalid backup file: {e}"))?;
    // A tox save is at least a few hundred bytes; refuse obvious garbage.
    if data.len() < 400 {
        return Err("文件内容不像有效的账号备份".to_string());
    }
    let save_path = state.data_dir.join("profile.tox");
    let backup_path = state.data_dir.join("profile.tox.bak");
    if save_path.exists() {
        std::fs::copy(&save_path, &backup_path)
            .map_err(|e| format!("backup current profile failed: {e}"))?;
    }
    std::fs::write(&save_path, &data).map_err(|e| format!("write imported profile failed: {e}"))?;
    Ok(())
}

/// After an invite lands, remember which conference number belongs to a
/// joined community (join_community records `u32::MAX` until then).
#[tauri::command]
pub async fn update_community_conferences(
    state: State<'_, AppState>,
    entries: Vec<CommunityConfEntry>,
) -> Result<(), String> {
    let mut list = my_communities_load(&state);
    let mut changed = false;
    for e in &entries {
        if let Some(c) = list.iter_mut().find(|c| c.channel_id == e.channel_id) {
            if c.conference_number != e.conference_number {
                c.conference_number = e.conference_number;
                changed = true;
            }
        }
    }
    if changed {
        my_communities_save(&state, &list);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityConfEntry {
    pub channel_id: String,
    pub conference_number: u32,
}

/// Send a "join_channel <id>" request to a ToxID (adds as friend if needed).
fn send_join_channel_inner(
    state: &State<AppState>,
    contact: &str,
    channel_id: &str,
) -> Result<(), String> {
    let contact = contact.trim();
    let friend_number = {
        let session = state.session.lock().unwrap();
        session
            .friend_list()
            .into_iter()
            .find(|n| {
                session
                    .friend_public_key(*n)
                    .map(|pk| contact == pk || contact.starts_with(&pk))
                    .unwrap_or(false)
            })
    };
    match friend_number {
        Some(n) => {
            let session = state.session.lock().unwrap();
            session
                .send_message(n, &format!("join_channel {channel_id}"))
                .map_err(|e| format!("send join request failed: {e}"))?;
        }
        None => {
            // Add as friend with the join request as the message; the invite
            // is sent automatically once they accept (see events.rs).
            let mut session = state.session.lock().unwrap();
            session
                .add_friend(contact, &format!("join_channel {channel_id}"))
                .map_err(|e| format!("add friend failed: {e}"))?;
            drop(session);
            // The contact exists only to carry the conference invite: it is a
            // follow, and must not pollute the friends list.
            {
                let engine = state.engine.lock().unwrap();
                let pk: String = contact.chars().take(64).collect();
                let _ = engine.store().friend_set_kind(&pk, "follow");
            }
            let session = state.session.lock().unwrap();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn fetch_public_timeline(
    state: State<'_, AppState>,
    limit: Option<u32>,
    before: Option<i64>,
    community: Option<String>,
) -> Result<Vec<TimelineItem>, String> {
    let limit = limit.unwrap_or(50);
    let want = community.map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty());
    let engine = state.engine.lock().unwrap();
    let meta = author_meta(&state, &engine);
    let rows = match before {
        Some(b) => engine
            .store()
            .public_posts_before(b, limit)
            .map_err(|e| e.to_string())?,
        None => engine
            .store()
            .public_posts_latest(limit)
            .map_err(|e| e.to_string())?,
    };
    Ok(rows
        .iter()
        .filter(|r| {
            match &want {
                // Community view: only posts scoped to this community.
                Some(c) => r.channel_id.as_deref() == Some(c.as_str()),
                // Plain public page: exclude community-scoped posts.
                None => r.channel_id.is_none(),
            }
        })
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
pub async fn request_public_posts(state: State<'_, AppState>, since: Option<i64>, depth: Option<u32>) -> Result<usize, String> {
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
pub async fn fetch_relay_public_posts(
    state: State<'_, AppState>,
    since: Option<i64>,
    community: Option<String>,
) -> Result<usize, String> {
    let since = since.unwrap_or(0);
    let community = community.map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty());
    let relays = relay_urls(&state);
    let received_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let me = state.session.lock().unwrap().self_public_key();
    let mut count = 0;
    let mut relay_ids: Vec<String> = Vec::new();
    for relay in &relays {
        let items = crate::relay::fetch_outbox(relay, since, community.as_deref()).await?;
        let engine = state.engine.lock().unwrap();
        for item in &items {
            let id = item["id"].as_str().unwrap_or("").to_string();
            let pubkey = item["pubkey"].as_str().unwrap_or("").to_string();
            let text = item["text"].as_str().unwrap_or("").to_string();
            let ts = item["ts"].as_i64().unwrap_or(0);
            let sig = item["sig"].as_str().unwrap_or("").to_string();
            let community_field = item["community"].as_str().map(|c| c.to_lowercase());
            if id.is_empty() || pubkey.is_empty() {
                continue;
            }
            // Community-scoped fetch must not mix in unrelated public posts.
            if let Some(want) = &community {
                if community_field.as_deref() != Some(want.as_str()) {
                    continue;
                }
            }
            relay_ids.push(id.clone());
            let post = tox_social::envelope::Post {
                v: tox_social::envelope::PROTOCOL_VERSION,
                id: id.clone(),
                author: pubkey.clone(),
                ts,
                text,
                public: true,
                sig,
                attachment: None,
                community: community_field,
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
    // Keep the public page consistent with the Relay: drop cached public
    // posts by others that no longer exist there (deleted by their author).
    if !relay_ids.is_empty() {
        let engine = state.engine.lock().unwrap();
        let stale: Vec<String> = engine
            .store()
            .public_posts_since(0, 10_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.author != me)
            .filter(|p| !relay_ids.iter().any(|id| id == &p.id))
            .map(|p| p.id)
            .collect();
        if !stale.is_empty() {
            let n = stale.len();
            let _ = engine.store().delete_public_posts_not_in(&relay_ids);
            println!("[toxsocial] removed {n} stale public post(s) not on the relay");
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



#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferInfo {
    pub direction: String,
    pub friend_number: u32,
    pub file_number: u32,
    pub filename: String,
    pub sent: u64,
    pub total: u64,
}

/// In-flight file transfers (both directions) for the transfer-status UI.
#[tauri::command]
pub async fn file_transfers(state: State<'_, AppState>) -> Result<Vec<FileTransferInfo>, String> {
    let session = state.session.lock().unwrap();
    Ok(session
        .file_transfers()
        .into_iter()
        .map(|t| FileTransferInfo {
            direction: t.direction,
            friend_number: t.friend_number,
            file_number: t.file_number,
            filename: t.filename,
            sent: t.sent,
            total: t.total,
        })
        .collect())
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
        let channels = crate::relay::list_channels(relay, "group").await.unwrap_or_default();
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
    let (own_toxid, ed_pk) = {
        let session = state.session.lock().unwrap();
        (session.self_address(), session.self_ed25519_public_key())
    };
    let conferences = {
        let session = state.session.lock().unwrap();
        session.conference_chatlist()
    };
    let relays = relay_urls(&state);
    let mut public_ids = std::collections::HashSet::new();
    for relay in &relays {
        let channels = crate::relay::list_channels(relay, "group").await.unwrap_or_default();
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
            // Signed heartbeat: binds the membership report to our Tox
            // identity so nobody can fake our presence or kick us.
            let ts = now_ms();
            let sig = {
                let session = state.session.lock().unwrap();
                session
                    .sign_data(format!("members|{channel_id}|{own_toxid}|{ts}|report").as_bytes())
                    .map_err(|e| e.to_string())?
            };
            let sig_hex = hex::encode(sig);
            for relay in &relays {
                crate::relay::report_channel_membership(
                    relay,
                    &channel_id,
                    &own_toxid,
                    false,
                    ts,
                    &sig_hex,
                    &ed_pk,
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
        let existing = crate::relay::list_channels(relay, "group").await.unwrap_or_default();
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
        return Err("只有群组创建者或 host 才能发布为公共群组".to_string());
    }
    for relay in &relays {
        crate::relay::register_channel(
            relay,
            name.trim(),
            desc.trim(),
            &host_toxid,
            &channel_id,
            "group",
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
