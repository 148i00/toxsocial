//! Background pump: tox events → store → frontend events.

use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use tox_core::event::{Connection, Event};
use tox_social::envelope::{
    DirEntry, DirReq, DirResp, Envelope, OutboxReq, OutboxResp, SyncPosts, SyncReq,
};
use tox_social::feed::Incoming;
use tox_social::MAX_ENVELOPE_BYTES;
use tox_store::{FriendRow, PostKind, PostRow, PostSource};

use crate::commands::{ReactionSummary, TimelineItem};
use crate::state::AppState;

pub fn spawn_event_pump(app: AppHandle) {
    std::thread::Builder::new()
        .name("tox-event-pump".to_string())
        .spawn(move || loop {
            let state = app.state::<AppState>();
            let ev = {
                let session = state.session.lock().unwrap();
                session.try_recv()
            };
            match ev {
                Some(ev) => handle_event(&app, &state, ev),
                None => {
                    heartbeat(&app, &state);
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        })
        .expect("failed to spawn event pump");
}

/// Print a liveness line once every ~10s (dev aid).
fn heartbeat(app: &AppHandle, state: &State<AppState>) {
    use std::sync::atomic::Ordering;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_HEARTBEAT.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) >= 10_000 {
        LAST_HEARTBEAT.store(now_ms, Ordering::Relaxed);
        let (friends, online) = {
            let session = state.session.lock().unwrap();
            let list = session.friend_list();
            let online = list
                .iter()
                .filter(|n| session.friend_connection(**n) != Connection::None)
                .count();
            (list.len(), online)
        };
        println!("[toxsocial] pump alive: friends={friends} online={online}");
        let _ = app.emit(
            "app:heartbeat",
            json!({ "friends": friends, "online": online }),
        );
    }
}

use std::sync::atomic::AtomicU64;
static LAST_HEARTBEAT: AtomicU64 = AtomicU64::new(0);

fn handle_event(app: &AppHandle, state: &State<AppState>, ev: Event) {
    match ev {
        Event::FriendRequest { public_key, message } => {
            let msg = String::from_utf8_lossy(&message).into_owned();
            println!("[toxsocial] friend request from {public_key}: {msg}");
            let accepted = {
                let mut session = state.session.lock().unwrap();
                session.add_friend_norequest(&public_key)
            };
            match accepted {
                Ok(n) => {
                    state.persist();
                    if let Some(channel_id) = msg.strip_prefix("join_channel ") {
                        let channel_id = channel_id.trim();
                        let session = state.session.lock().unwrap();
                        match session.conference_by_id(channel_id) {
                            Ok(conf) => match session.conference_invite(n, conf) {
                                Ok(()) => println!(
                                    "[toxsocial] invited new friend #{n} to channel {channel_id}"
                                ),
                                Err(e) => eprintln!("[toxsocial] auto-invite failed: {e}"),
                            },
                            Err(e) => eprintln!(
                                "[toxsocial] join_channel request for unknown channel {channel_id}: {e}"
                            ),
                        }
                    }
                    let _ = app.emit(
                        "friend:request",
                        json!({ "publicKey": public_key, "message": msg, "accepted": true, "friendNumber": n }),
                    );
                }
                Err(e) => eprintln!("[toxsocial] accept failed: {e}"),
            }
        }
        Event::FriendMessage {
            friend_number,
            text,
            ..
        } => {
            let pk = {
                let session = state.session.lock().unwrap();
                session
                    .friend_public_key(friend_number)
                    .unwrap_or_default()
            };
            if pk.is_empty() {
                return;
            }
            let outcome = {
                let engine = state.engine.lock().unwrap();
                engine.handle_incoming(&pk, &text)
            };
            let name = state.name_for(&pk);
            match outcome {
                Incoming::Persisted(env) => {
                    match env {
                        Envelope::Post(p) => {
                            println!("[toxsocial] post received from {name}: {}", p.text);
                            verify_post_via_relay(app, p.id.clone());
                            let _ = app.emit(
                                "feed:post",
                                json!({ "id": p.id, "author": p.author, "authorName": name, "text": p.text, "ts": p.ts }),
                            );
                        }
                        Envelope::Comment(c) => {
                            let _ = app.emit(
                                "feed:comment",
                                json!({ "id": c.id, "author": c.author, "authorName": name, "text": c.text, "ts": c.ts, "parentId": c.reply_to }),
                            );
                        }
                        Envelope::Reaction(r) => {
                            let _ = app.emit(
                                "feed:reaction",
                                json!({ "id": r.id, "author": r.author, "authorName": name, "emoji": r.emoji, "ts": r.ts, "parentId": r.reply_to }),
                            );
                        }
                        Envelope::SyncReq(req) => {
                            handle_sync_req(state, friend_number, &pk, &req);
                        }
                        Envelope::SyncPosts(sp) => {
                            handle_sync_posts(state, app, &pk, sp.items);
                        }
                        _ => {}
                    }
                }
                Incoming::Profile(p) => {
                    println!("[toxsocial] profile from {pk}: name={}", p.name);
                    update_friend_meta(state, &pk, Some(&p.name), None, Some(&p.avatar), Some(&p.bio));
                }
                Incoming::Chunk => {}
                Incoming::DirReq(req) => {
                    handle_dir_req(state, friend_number, &pk, &req);
                }
                Incoming::DirResp(resp) => {
                    handle_dir_resp(state, &pk, &resp);
                }
                Incoming::OutboxReq(req) => {
                    handle_outbox_req(state, friend_number, &req);
                }
                Incoming::OutboxResp(resp) => {
                    handle_outbox_resp(state, app, &pk, &resp);
                }
                Incoming::Unfriend(_) => {
                    handle_unfriend(state, friend_number, &pk);
                }
                Incoming::Rejected(_) => {
                    // Plain chat message — support "join_channel <id>" from friends.
                    if let Some(channel_id) = text.strip_prefix("join_channel ") {
                        let channel_id = channel_id.trim();
                        let invite_result = {
                            let session = state.session.lock().unwrap();
                            match session.conference_by_id(channel_id) {
                                Ok(conf) => session
                                    .conference_invite(friend_number, conf)
                                    .map(|_| conf)
                                    .map_err(|e| e.to_string()),
                                Err(e) => Err(e.to_string()),
                            }
                        };
                        match invite_result {
                            Ok(conf) => {
                                state.persist();
                                println!(
                                    "[toxsocial] auto-invited friend #{friend_number} to channel {channel_id}"
                                );
                                let _ = app.emit(
                                    "channel:joined",
                                    json!({ "conferenceNumber": conf, "friendNumber": friend_number }),
                                );
                            }
                            Err(e) => {
                                eprintln!("[toxsocial] join_channel request failed: {e}");
                                let _ = app.emit(
                                    "chat:message",
                                    json!({ "author": pk, "authorName": name, "text": text }),
                                );
                            }
                        }
                        return;
                    }
                    // Attachment request: "get_file <post_id>" — send the
                    // stored file to the requester automatically.
                    if let Some(post_id) = text.strip_prefix("get_file ") {
                        let post_id = post_id.trim();
                        handle_get_file(state, app, friend_number, post_id);
                        return;
                    }
                    // Plain chat message — not part of the social protocol yet.
                    let _ = app.emit(
                        "chat:message",
                        json!({ "author": pk, "authorName": name, "text": text }),
                    );
                }
            }
        }
        Event::FriendConnection {
            friend_number,
            connection,
        } => {
            let pk = {
                let session = state.session.lock().unwrap();
                session
                    .friend_public_key(friend_number)
                    .unwrap_or_default()
            };
            let online = connection != Connection::None;
            update_friend_meta(state, &pk, None, Some(online), None, None);
            let name = state.name_for(&pk);
            let _ = app.emit(
                "friend:connection",
                json!({ "publicKey": pk, "name": name, "online": online }),
            );
            if online {
                println!("[toxsocial] friend online: {name} ({})", short(pk.as_str()));
                send_sync_req(state, friend_number, &pk);
                // Also proactively push our posts to the friend, so they can see
                // our timeline/profile even if their sync_req never arrives.
                send_sync_posts_to_friend(state, friend_number, &pk);
            }
        }
        Event::FriendName {
            friend_number,
            name,
        } => {
            let pk = {
                let session = state.session.lock().unwrap();
                session
                    .friend_public_key(friend_number)
                    .unwrap_or_default()
            };
            update_friend_meta(state, &pk, Some(&name), None, None, None);
            let _ = app.emit(
                "friend:name",
                json!({ "publicKey": pk, "name": name }),
            );
        }
        Event::FriendStatusMessage {
            friend_number,
            status_message,
        } => {
            let pk = {
                let session = state.session.lock().unwrap();
                session
                    .friend_public_key(friend_number)
                    .unwrap_or_default()
            };
            if !pk.is_empty() {
                update_friend_meta(state, &pk, None, None, None, Some(&status_message));
                let _ = app.emit(
                    "friend:bio",
                    json!({ "publicKey": pk, "bio": status_message }),
                );
            }
        }
        Event::FriendStatus { .. } => {}
        Event::ConferenceInvite {
            friend_number,
            conference_type,
            cookie,
        } => {
            println!(
                "[toxsocial] conference invite from #{friend_number} type={conference_type}"
            );
            if conference_type == tox_core::ffi::TOX_CONFERENCE_TYPE_TEXT {
                let joined = {
                    let mut session = state.session.lock().unwrap();
                    session.conference_join(friend_number, &cookie)
                };
                match joined {
                    Ok(n) => {
                        state.persist();
                        let _ = app.emit(
                            "channel:joined",
                            json!({ "conferenceNumber": n, "friendNumber": friend_number }),
                        );
                    }
                    Err(e) => eprintln!("[toxsocial] conference join failed: {e}"),
                }
            }
        }
        Event::ConferenceConnected { conference_number } => {
            println!("[toxsocial] connected to conference #{conference_number}");
            flush_pending_channel_messages(&state, app, conference_number);
            let _ = app.emit(
                "channel:connected",
                json!({ "conferenceNumber": conference_number }),
            );
        }
        Event::ConferenceMessage {
            conference_number,
            peer_number,
            message_type: _,
            text,
        } => {
            // Persist the message so it survives restarts (see channel_messages
            // command). Grab session data first, then write to the store.
            let (channel_id, peer_name, peer_key, is_self, ts) = {
                let session = state.session.lock().unwrap();
                let channel_id = session
                    .conference_get_id(conference_number)
                    .unwrap_or_default();
                let peer_name = session
                    .conference_peer_name(conference_number, peer_number)
                    .unwrap_or_default();
                let peer_key = session
                    .conference_peer_public_key(conference_number, peer_number)
                    .unwrap_or_default();
                let is_self = !peer_key.is_empty() && peer_key == session.self_public_key();
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                (channel_id, peer_name, peer_key, is_self, ts)
            };
            // toxcore echoes our own conference messages back to us. The UI
            // already shows them optimistically (and `conference_send`
            // persists them), so skip persist + emit: otherwise our own
            // messages would appear twice, once as "someone else".
            if is_self {
                println!(
                    "[toxsocial] conference #{conference_number} self-message echo ignored"
                );
                return;
            }
            let display_name = if peer_name.is_empty() {
                format!("#{peer_number}")
            } else {
                peer_name
            };
            let msg_id = {
                let engine = state.engine.lock().unwrap();
                engine
                    .store()
                    .channel_message_insert(&tox_store::ChannelMessageRow {
                        id: 0,
                        conference_number,
                        channel_id: channel_id.clone(),
                        peer_name: display_name.clone(),
                        peer_key,
                        text: text.clone(),
                        ts,
                        direction: 0,
                        pending: false,
                    })
                    .unwrap_or(0)
            };
            println!(
                "[toxsocial] conference #{conference_number} peer {peer_number} ({display_name}): {text}"
            );
            let _ = app.emit(
                "channel:message",
                json!({
                    "conferenceNumber": conference_number,
                    "channelId": channel_id,
                    "peerNumber": peer_number,
                    "peerName": display_name,
                    "text": text,
                    "id": msg_id,
                    "ts": ts,
                }),
            );
        }
        Event::ConferencePeerName {
            conference_number,
            peer_number,
            name,
        } => {
            println!(
                "[toxsocial] conference #{conference_number} peer {peer_number} name={name}"
            );
        }
        Event::ConferencePeerListChanged { conference_number } => {
            println!("[toxsocial] conference #{conference_number} peer list changed");
            // Someone joined/left: flush any messages queued while the
            // channel had no other members.
            flush_pending_channel_messages(&state, app, conference_number);
            let _ = app.emit(
                "channel:peer_list_changed",
                json!({ "conferenceNumber": conference_number }),
            );
        }
        Event::FileChunkRequest { .. } => {}
        Event::FileRecvChunk { .. } => {}
        Event::FileRecv {
            friend_number,
            file_number,
            filename,
            file_size,
        } => {
            // Security: reject absurdly large transfers up front (a malicious
            // friend could otherwise exhaust our memory by sending a huge
            // file and having us buffer it all).
            const MAX_RECV_FILE: u64 = 100 * 1024 * 1024; // 100 MB
            if file_size > MAX_RECV_FILE {
                let session = state.session.lock().unwrap();
                let _ = session.reject_file(friend_number, file_number);
                drop(session);
                eprintln!(
                    "[toxsocial] rejected oversized incoming file from #{friend_number}: \
                     {filename} ({file_size} bytes > 100MB)"
                );
                return;
            }
            let friend_name = {
                let pk = state
                    .session
                    .lock()
                    .unwrap()
                    .friend_public_key(friend_number)
                    .unwrap_or_default();
                state.name_for(&pk)
            };
            println!(
                "[toxsocial] incoming file from #{friend_number} ({friend_name}): {filename} ({file_size} bytes)"
            );
            let _ = app.emit(
                "file:request",
                json!({
                    "friendNumber": friend_number,
                    "friendName": friend_name,
                    "fileNumber": file_number,
                    "filename": filename,
                    "fileSize": file_size,
                }),
            );
        }
        Event::FileReceived {
            friend_number,
            file_number,
            filename,
            data,
        } => {
            // Security: never trust a friend-supplied filename — strip
            // path components so `..\..\evil.exe` cannot escape the media
            // directory (path traversal write).
            let filename = sanitize_filename(&filename);
            let dir = state.data_dir.join("media");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(&filename);
            if std::fs::write(&path, &data).is_ok() {
                println!(
                    "[toxsocial] received file #{file_number} from #{friend_number}: {}",
                    path.display()
                );
                let _ = app.emit(
                    "file:received",
                    json!({ "friendNumber": friend_number, "filename": filename, "path": path.to_string_lossy() }),
                );
            }
        }
    }
}

/// Strip anything that could escape the media directory from a filename
/// received from a friend (path traversal defence).
fn sanitize_filename(name: &str) -> String {
    let base = name
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("file")
        .trim()
        .to_string();
    if base.is_empty() || base == "." || base == ".." {
        return "file".to_string();
    }
    let mut out: String = base.chars().take(180).collect();
    // Windows forbids these characters in filenames.
    for ch in ['<', '>', ':', '"', '|', '?', '*', '\0', '\u{1}'] {
        out = out.replace(ch, "_");
    }
    out.trim().to_string()
}

fn send_sync_req(state: &State<AppState>, friend_number: u32, pk: &str) {
    let me = state.session.lock().unwrap().self_public_key();
    let since = state
        .engine
        .lock()
        .unwrap()
        .latest_ts_for_author(pk)
        .unwrap_or(0);
    let req = Envelope::SyncReq(SyncReq {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        since,
    });
    let wire = req.encode();
    let session = state.session.lock().unwrap();
    match session.send_message(friend_number, &wire) {
        Ok(()) => println!("[toxsocial] sync_req sent to {pk} since={since}"),
        Err(e) => eprintln!("[toxsocial] failed to send sync_req: {e}"),
    }
}

fn send_sync_posts_to_friend(state: &State<AppState>, friend_number: u32, friend_pk: &str) {
    let me = state.session.lock().unwrap().self_public_key();
    let items = {
        let engine = state.engine.lock().unwrap();
        engine.self_posts_since(&me, 0, 200)
    };
    if items.is_empty() {
        return;
    }
    let count = items.len();
    for chunk in chunk_envelopes(&me, items) {
        let sync = Envelope::SyncPosts(SyncPosts {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me.clone(),
            ts: now_ms(),
            items: chunk,
        });
        let wire = sync.encode();
        let session = state.session.lock().unwrap();
        if let Err(e) = session.send_message(friend_number, &wire) {
            eprintln!("[toxsocial] failed to push sync_posts to {friend_pk}: {e}");
            return;
        }
    }
    println!("[toxsocial] pushed {count} posts to {friend_pk}");
}

fn handle_sync_req(
    state: &State<AppState>,
    friend_number: u32,
    sender_pk: &str,
    req: &SyncReq,
) {
    let me = state.session.lock().unwrap().self_public_key();
    let items = {
        let engine = state.engine.lock().unwrap();
        engine.self_posts_since(&me, req.since, 200)
    };
    if items.is_empty() {
        return;
    }
    for chunk in chunk_envelopes(&me, items) {
        let sync = Envelope::SyncPosts(SyncPosts {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me.clone(),
            ts: now_ms(),
            items: chunk,
        });
        let wire = sync.encode();
        let session = state.session.lock().unwrap();
        if let Err(e) = session.send_message(friend_number, &wire) {
            eprintln!("[toxsocial] failed to send sync_posts: {e}");
            return;
        }
    }
    println!("[toxsocial] sync_posts sent to {sender_pk}");
}

fn handle_sync_posts(
    state: &State<AppState>,
    app: &AppHandle,
    sender_pk: &str,
    items: Vec<Envelope>,
) {
    let persisted = {
        let engine = state.engine.lock().unwrap();
        engine.handle_sync_posts(sender_pk, items)
    };
    if persisted.is_empty() {
        return;
    }
    let name = state.name_for(sender_pk);
    for env in persisted {
        match env {
            Envelope::Post(p) => {
                println!("[toxsocial] post received from {name} (sync): {}", p.text);
                verify_post_via_relay(app, p.id.clone());
                let _ = app.emit(
                    "feed:post",
                    json!({ "id": p.id, "author": p.author, "authorName": name, "text": p.text, "ts": p.ts }),
                );
            }
            Envelope::Comment(c) => {
                let _ = app.emit(
                    "feed:comment",
                    json!({ "id": c.id, "author": c.author, "authorName": name, "text": c.text, "ts": c.ts, "parentId": c.reply_to }),
                );
            }
            Envelope::Reaction(r) => {
                let _ = app.emit(
                    "feed:reaction",
                    json!({ "id": r.id, "author": r.author, "authorName": name, "emoji": r.emoji, "ts": r.ts, "parentId": r.reply_to }),
                );
            }
            _ => {}
        }
    }
}

fn chunk_envelopes(author: &str, items: Vec<Envelope>) -> Vec<Vec<Envelope>> {
    let mut chunks = Vec::new();
    let mut current: Vec<Envelope> = Vec::new();
    for item in items {
        let mut candidate = current.clone();
        candidate.push(item.clone());
        let probe = Envelope::SyncPosts(SyncPosts {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: author.to_string(),
            ts: 0,
            items: candidate.clone(),
        });
        if probe.wire_len() <= MAX_ENVELOPE_BYTES || current.is_empty() {
            current = candidate;
        } else {
            chunks.push(std::mem::take(&mut current));
            current.push(item);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn handle_dir_req(state: &State<AppState>, friend_number: u32, _sender_pk: &str, req: &DirReq) {
    let items = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .dir_search(&req.query, 20)
            .unwrap_or_default()
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                pubkey: e.pubkey,
                toxid: e.toxid,
                avatar: e.avatar,
                relay: e.relay,
            })
            .collect::<Vec<_>>()
    };
    if !items.is_empty() {
        let me = state.session.lock().unwrap().self_public_key();
        let resp = Envelope::DirResp(DirResp {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me,
            ts: now_ms(),
            items,
        });
        let wire = resp.encode();
        let session = state.session.lock().unwrap();
        let _ = session.send_message(friend_number, &wire);
    }
    if req.depth > 0 {
        let me = state.session.lock().unwrap().self_public_key();
        let fwd = Envelope::DirReq(DirReq {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me,
            ts: now_ms(),
            query: req.query.clone(),
            depth: req.depth - 1,
        });
        let wire = fwd.encode();
        let session = state.session.lock().unwrap();
        for n in session.friend_list() {
            if n == friend_number {
                continue;
            }
            if session.friend_connection(n) != Connection::None {
                let _ = session.send_message(n, &wire);
            }
        }
    }
}

fn handle_dir_resp(state: &State<AppState>, sender_pk: &str, resp: &DirResp) {
    let now = now_ms();
    let engine = state.engine.lock().unwrap();
    for item in &resp.items {
        let entry = tox_store::DirectoryEntry {
            pubkey: item.pubkey.clone(),
            toxid: item.toxid.clone(),
            name: item.name.clone(),
            avatar: item.avatar.clone(),
            relay: item.relay.clone(),
            source: sender_pk.to_string(),
            updated_at: now,
        };
        let _ = engine.store().dir_upsert(&entry);
    }
    println!(
        "[toxsocial] directory updated from {sender_pk}: {} entries",
        resp.items.len()
    );
}

fn handle_outbox_req(state: &State<AppState>, friend_number: u32, req: &OutboxReq) {
    let items = {
        let engine = state.engine.lock().unwrap();
        engine.public_posts_since(req.since, 100)
    };
    if !items.is_empty() {
        let me = state.session.lock().unwrap().self_public_key();
        let resp = Envelope::OutboxResp(OutboxResp {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me,
            ts: now_ms(),
            items,
        });
        let wire = resp.encode();
        let session = state.session.lock().unwrap();
        let _ = session.send_message(friend_number, &wire);
    }
    if req.depth > 0 {
        let me = state.session.lock().unwrap().self_public_key();
        let fwd = Envelope::OutboxReq(OutboxReq {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me,
            ts: now_ms(),
            since: req.since,
            depth: req.depth - 1,
        });
        let wire = fwd.encode();
        let session = state.session.lock().unwrap();
        for n in session.friend_list() {
            if n == friend_number {
                continue;
            }
            if session.friend_connection(n) != Connection::None {
                let _ = session.send_message(n, &wire);
            }
        }
    }
}

/// Best-effort: verify a directly-received post against the Relay(s). If the
/// Relay has the same post (author + ts match), its timestamp was validated
/// by the Relay server clock; mark it verified so the UI stops warning, then
/// tell the frontend to refresh.
fn verify_post_via_relay(app: &AppHandle, post_id: String) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        match crate::commands::verify_post_on_relay(&state, &post_id).await {
            Ok(true) => {
                println!("[toxsocial] post {post_id} verified via Relay (ts validated)");
                let _ = handle.emit("post:ts_verified", json!({ "postId": post_id }));
            }
            _ => {}
        }
    });
}

/// A friend requested the attachment of one of our posts (`get_file
/// <post_id>`). Look up the locally stored file and send it over Tox's
/// file-transfer channel automatically.
fn handle_get_file(
    state: &State<AppState>,
    app: &AppHandle,
    friend_number: u32,
    post_id: &str,
) {
    let me = state.session.lock().unwrap().self_public_key();
    let (attachment, fname) = {
        let engine = state.engine.lock().unwrap();
        match engine.store().post_get(post_id) {
            Ok(Some(p)) if p.author == me => match p.attachment {
                Some(meta) => {
                    let fname = meta
                        .splitn(2, '|')
                        .next()
                        .unwrap_or("attachment")
                        .to_string();
                    (Some(meta), fname)
                }
                None => (None, String::new()),
            },
            _ => (None, String::new()),
        }
    };
    let Some(meta) = attachment else {
        eprintln!("[toxsocial] get_file for unknown post {post_id}");
        return;
    };
    // Files are stored under media/attachments/<post_id> (safe, no user
    // input in the path); only the display name comes from the metadata.
    let path = state
        .data_dir
        .join("media")
        .join("attachments")
        .join(post_id);
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[toxsocial] attachment file missing for {post_id}: {e}");
            return;
        }
    };
    let result = {
        let mut session = state.session.lock().unwrap();
        session.send_file_data(friend_number, &fname, &data)
    };
    match result {
        Ok(n) => {
            println!(
                "[toxsocial] sent attachment {post_id} ({meta}) to friend #{friend_number} as file #{n}"
            );
            let _ = app.emit(
                "file:auto_sent",
                json!({ "friendNumber": friend_number, "filename": fname, "postId": post_id }),
            );
        }
        Err(e) => eprintln!("[toxsocial] failed to send attachment {post_id}: {e}"),
    }
}

/// Deliver messages that were queued while the channel had no other members
/// (see `conference_send`). Called on connect/peer-list changes; only runs
/// when the channel now has at least two peers (self + at least one other).
fn flush_pending_channel_messages(
    state: &State<AppState>,
    app: &AppHandle,
    conference_number: u32,
) {
    let (peer_count, channel_id) = {
        let session = state.session.lock().unwrap();
        (
            session
                .conference_peer_count(conference_number)
                .unwrap_or(0),
            session
                .conference_get_id(conference_number)
                .unwrap_or_default(),
        )
    };
    if peer_count <= 1 {
        return;
    }
    let pending = {
        let engine = state.engine.lock().unwrap();
        engine
            .store()
            .channel_messages_pending(conference_number, &channel_id)
            .unwrap_or_default()
    };
    if pending.is_empty() {
        return;
    }
    let mut flushed = 0;
    for m in &pending {
        let ok = {
            let session = state.session.lock().unwrap();
            session
                .conference_send_message(conference_number, &m.text)
                .is_ok()
        };
        if !ok {
            break; // still offline; retry on the next peer-list change
        }
        let engine = state.engine.lock().unwrap();
        let _ = engine.store().channel_message_mark_delivered(m.id);
        flushed += 1;
    }
    if flushed > 0 {
        println!(
            "[toxsocial] flushed {flushed} queued message(s) to conference #{conference_number}"
        );
        let _ = app.emit(
            "channel:pending_flushed",
            json!({ "conferenceNumber": conference_number, "count": flushed }),
        );
    }
}

fn handle_outbox_resp(state: &State<AppState>, app: &AppHandle, sender_pk: &str, resp: &OutboxResp) {
    let received_at = now_ms();
    let mut new_posts = Vec::new();
    {
        let engine = state.engine.lock().unwrap();
        for item in &resp.items {
            if let Envelope::Post(p) = item {
                if engine.persist(item, &p.author, received_at) {
                    new_posts.push(p.clone());
                }
            }
        }
    }
    let name = state.name_for(sender_pk);
    for p in new_posts {
        println!("[toxsocial] public post received via outbox from {name}: {}", p.text);
        verify_post_via_relay(app, p.id.clone());
        let _ = app.emit(
            "feed:post",
            json!({ "id": p.id, "author": p.author, "authorName": name, "text": p.text, "ts": p.ts }),
        );
    }
}

fn handle_unfriend(state: &State<AppState>, friend_number: u32, pk: &str) {
    {
        let mut session = state.session.lock().unwrap();
        let _ = session.delete_friend(friend_number);
    }
    state.persist();
    {
        let engine = state.engine.lock().unwrap();
        let _ = engine.store().friend_remove(pk);
    }
    println!("[toxsocial] removed friend {pk} (they unfriended us)");
}

fn update_friend_meta(
    state: &State<AppState>,
    pk: &str,
    name: Option<&str>,
    online: Option<bool>,
    avatar: Option<&str>,
    bio: Option<&str>,
) {
    let engine = state.engine.lock().unwrap();
    let store = engine.store();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let existing = store
        .friend_list()
        .unwrap_or_default()
        .into_iter()
        .find(|f: &FriendRow| f.toxid == pk);
    let mut row = existing.unwrap_or(FriendRow {
        toxid: pk.to_string(),
        nospam: String::new(),
        name: String::new(),
        avatar: String::new(),
        bio: String::new(),
        status: 0,
        added_at: now,
        last_seen: None,
    });
    if let Some(name) = name {
        row.name = name.to_string();
    }
    if let Some(avatar) = avatar {
        row.avatar = avatar.to_string();
    }
    if let Some(bio) = bio {
        row.bio = bio.to_string();
    }
    if let Some(online) = online {
        row.status = if online { 1 } else { 0 };
        if online {
            row.last_seen = Some(now);
        }
    }
    let _ = store.friend_upsert(&row);
}

/// Build a TimelineItem from a stored row (resolving display name).
#[allow(dead_code)]
pub(crate) fn item_from_row(
    state: &State<AppState>,
    engine: &tox_social::feed::FeedEngine,
    row: &PostRow,
) -> TimelineItem {
    let me = state.session.lock().unwrap().self_public_key();
    let author_name = engine
        .store()
        .friend_list()
        .unwrap_or_default()
        .into_iter()
        .find(|f| f.toxid == row.author && !f.name.is_empty())
        .map(|f| f.name)
        .unwrap_or_else(|| short(&row.author).to_string());
    let author_avatar = if row.author == me {
        engine
            .store()
            .kv_get("avatar_url")
            .unwrap_or_default()
            .unwrap_or_default()
    } else {
        engine
            .store()
            .friend_list()
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.toxid == row.author)
            .map(|f| f.avatar)
            .unwrap_or_default()
    };
    item_from_row_with_meta(state, engine, row, &author_name, &author_avatar)
}

pub(crate) fn item_from_row_with_meta(
    state: &State<AppState>,
    engine: &tox_social::feed::FeedEngine,
    row: &PostRow,
    author_name: &str,
    author_avatar: &str,
) -> TimelineItem {
    let me = state.session.lock().unwrap().self_public_key();
    let (comment_count, reaction_count, reactions) = if row.kind == PostKind::Post {
        let thread = engine.store().thread_for(&row.id).unwrap_or_default();
        let mut counts: Vec<(String, usize)> = Vec::new();
        for c in thread.iter().filter(|c| c.kind == PostKind::Reaction) {
            let emoji = c.emoji.clone().unwrap_or_default();
            if let Some(entry) = counts.iter_mut().find(|(e, _)| *e == emoji) {
                entry.1 += 1;
            } else {
                counts.push((emoji, 1));
            }
        }
        (
            thread
                .iter()
                .filter(|c| c.kind == PostKind::Comment)
                .count(),
            thread
                .iter()
                .filter(|c| c.kind == PostKind::Reaction)
                .count(),
            counts
                .into_iter()
                .map(|(emoji, count)| ReactionSummary { emoji, count })
                .collect(),
        )
    } else {
        (0, 0, Vec::new())
    };
    TimelineItem {
        id: row.id.clone(),
        author: row.author.clone(),
        author_name: author_name.to_string(),
        author_avatar: author_avatar.to_string(),
        kind: match row.kind {
            PostKind::Post => "post",
            PostKind::Comment => "comment",
            PostKind::Reaction => "reaction",
        }
        .to_string(),
        text: row.text.clone(),
        emoji: row.emoji.clone(),
        ts: row.ts,
        parent_id: row.parent_id.clone(),
        comment_count,
        reaction_count,
        reactions,
        is_own: row.author == me,
        ts_verified: row.source == PostSource::RelayVerified || row.author == me,
        attachment: row.attachment.clone(),
        source: match row.source {
            PostSource::SelfPublished => "self",
            PostSource::FriendDirect => "friend",
            PostSource::Channel => "channel",
            PostSource::RelayVerified => "relay",
        }
        .to_string(),
    }
}

fn short(s: &str) -> &str {
    if s.len() > 8 {
        &s[..8]
    } else {
        s
    }
}
