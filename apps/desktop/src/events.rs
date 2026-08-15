//! Background pump: tox events → store → frontend events.

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use tox_core::event::{Connection, Event};
use tox_social::envelope::Envelope;
use tox_social::feed::Incoming;
use tox_store::{FriendRow, PostKind, PostRow, PostSource};

use crate::commands::TimelineItem;
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
            let mut session = state.session.lock().unwrap();
            match session.add_friend_norequest(&public_key) {
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
            let engine = state.engine.lock().unwrap();
            match engine.handle_incoming(&pk, &text) {
                Incoming::Persisted(env) => {
                    let name = state.name_for(&pk);
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
                        _ => {}
                    }
                }
                Incoming::Profile(p) => {
                    println!("[toxsocial] profile from {pk}: name={}", p.name);
                    update_friend_meta(state, &pk, Some(&p.name), None);
                }
                Incoming::Rejected(_) => {
                    // Plain chat message — not part of the social protocol yet.
                    let name = state.name_for(&pk);
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
            update_friend_meta(state, &pk, None, Some(online));
            let name = state.name_for(&pk);
            let _ = app.emit(
                "friend:connection",
                json!({ "publicKey": pk, "name": name, "online": online }),
            );
            if online {
                println!("[toxsocial] friend online: {name} ({})", short(pk.as_str()));
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
            update_friend_meta(state, &pk, Some(&name), None);
            let _ = app.emit(
                "friend:name",
                json!({ "publicKey": pk, "name": name }),
            );
        }
        Event::FriendStatusMessage { .. } => {}
        Event::FriendStatus { .. } => {}
    }
}

fn update_friend_meta(state: &State<AppState>, pk: &str, name: Option<&str>, online: Option<bool>) {
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
        status: 0,
        added_at: now,
        last_seen: None,
    });
    if let Some(name) = name {
        row.name = name.to_string();
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
    let (comment_count, reaction_count) = if row.kind == PostKind::Post {
        let thread = engine.store().thread_for(&row.id).unwrap_or_default();
        (
            thread
                .iter()
                .filter(|c| c.kind == PostKind::Comment)
                .count(),
            thread
                .iter()
                .filter(|c| c.kind == PostKind::Reaction)
                .count(),
        )
    } else {
        (0, 0)
    };
    TimelineItem {
        id: row.id.clone(),
        author: row.author.clone(),
        author_name: state.name_for(&row.author),
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
