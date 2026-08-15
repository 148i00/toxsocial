//! Background pump: tox events → store → frontend events.

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use tox_core::event::{Connection, Event};
use tox_social::envelope::{Envelope, SyncPosts, SyncReq};
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
                match session.recv_timeout(Duration::from_millis(250)) {
                    Ok(ev) => ev,
                    Err(RecvTimeoutError::Timeout) => {
                        drop(session);
                        heartbeat(&app, &state);
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            };
            handle_event(&app, &state, ev);
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
                    update_friend_meta(state, &pk, Some(&p.name), None, Some(&p.avatar));
                }
                Incoming::Chunk => {}
                Incoming::Rejected(_) => {
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
            update_friend_meta(state, &pk, None, Some(online), None);
            let name = state.name_for(&pk);
            let _ = app.emit(
                "friend:connection",
                json!({ "publicKey": pk, "name": name, "online": online }),
            );
            if online {
                println!("[toxsocial] friend online: {name} ({})", short(pk.as_str()));
                send_sync_req(state, friend_number, &pk);
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
            update_friend_meta(state, &pk, Some(&name), None, None);
            let _ = app.emit(
                "friend:name",
                json!({ "publicKey": pk, "name": name }),
            );
        }
        Event::FriendStatusMessage { .. } => {}
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
            println!("[toxsocial] conference #{conference_number} peer {peer_number}: {text}");
            let _ = app.emit(
                "channel:message",
                json!({ "conferenceNumber": conference_number, "peerNumber": peer_number, "text": text }),
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
        }
    }
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

fn update_friend_meta(
    state: &State<AppState>,
    pk: &str,
    name: Option<&str>,
    online: Option<bool>,
    avatar: Option<&str>,
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
    if let Some(online) = online {
        row.status = if online { 1 } else { 0 };
        if online {
            row.last_seen = Some(now);
        }
    }
    let _ = store.friend_upsert(&row);
}

/// Build a TimelineItem from a stored row (resolving display name).
pub(crate) fn item_from_row(
    state: &State<AppState>,
    engine: &tox_social::feed::FeedEngine,
    row: &PostRow,
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
        author_name: engine
            .store()
            .friend_list()
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.toxid == row.author && !f.name.is_empty())
            .map(|f| f.name)
            .unwrap_or_else(|| short(&row.author).to_string()),
        author_avatar: if row.author == me {
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
        },
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
        source: match row.source {
            PostSource::SelfPublished => "self",
            PostSource::FriendDirect => "friend",
            PostSource::Channel => "channel",
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
