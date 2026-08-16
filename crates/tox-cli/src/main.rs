//! tox-cli: development / smoke-test client.
//!
//! Subcommands:
//!   init   — create a fresh identity and save it to a file
//!   show   — print identity + friends from a save file
//!   add    — send a friend request
//!   send   — send a plain text message to a friend
//!   post   — publish a social post (fan-out to all friends)
//!   run    — run the event loop (bootstrap + social feed processing)

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use tox_core::{Connection, Event, ToxSession, DEFAULT_BOOTSTRAP_NODES};
use tox_social::envelope::{
    DirEntry, DirReq, DirResp, Envelope, OutboxReq, OutboxResp, SyncPosts, SyncReq,
};
use tox_social::feed::{FeedEngine, Incoming};
use tox_social::MAX_ENVELOPE_BYTES;
use tox_store::{PostKind, Store};

/// Default DHT bootstrap nodes (shared, from tox-core).
const DEFAULT_NODES: &[(&str, u16, &str)] = DEFAULT_BOOTSTRAP_NODES;

#[derive(Parser)]
#[command(name = "tox-cli", version, about = "ToxSocial dev client")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a fresh identity and save it.
    Init {
        #[arg(long)]
        save: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    /// Print identity and friends from a save file.
    Show { save: PathBuf },
    /// Send a friend request (ToxID is 76 hex chars).
    Add {
        save: PathBuf,
        toxid: String,
        #[arg(long)]
        msg: Option<String>,
    },
    /// Send a plain text message to a friend.
    Send {
        save: PathBuf,
        #[arg(long)]
        friend: u32,
        text: String,
    },
    /// Publish a social post to all friends.
    Post {
        save: PathBuf,
        #[arg(long)]
        db: Option<PathBuf>,
        text: String,
    },
    /// Print the local timeline (posts + threads) from the store.
    Timeline {
        save: PathBuf,
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Run the event loop with the social feed engine.
    Run {
        save: PathBuf,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        no_bootstrap: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { save, name, status } => cmd_init(&save, name.as_deref(), status.as_deref()),
        Cmd::Show { save } => cmd_show(&save),
        Cmd::Add { save, toxid, msg } => cmd_add(&save, &toxid, msg.as_deref().unwrap_or("")),
        Cmd::Send { save, friend, text } => cmd_send(&save, friend, &text),
        Cmd::Post { save, db, text } => cmd_post(&save, db.as_deref(), &text),
        Cmd::Timeline { save, db } => cmd_timeline(&save, db.as_deref()),
        Cmd::Run { save, db, no_bootstrap } => cmd_run(&save, db.as_deref(), !no_bootstrap),
    }
}

fn cmd_init(save: &Path, name: Option<&str>, status: Option<&str>) -> Result<()> {
    let mut session = ToxSession::new().map_err(|e| anyhow!("tox_new failed: {e}"))?;
    if let Some(name) = name {
        session
            .set_name(name)
            .map_err(|e| anyhow!("set_name failed: {e}"))?;
    }
    if let Some(status) = status {
        session
            .set_status_message(status)
            .map_err(|e| anyhow!("set_status failed: {e}"))?;
    }
    persist(&session, save)?;
    println!("created: {}", save.display());
    println!("ToxID  : {}", session.self_address());
    println!("pubkey : {}", session.self_public_key());
    Ok(())
}

fn cmd_show(save: &Path) -> Result<()> {
    let session = load(save)?;
    println!("ToxID  : {}", session.self_address());
    println!("pubkey : {}", session.self_public_key());
    println!("name   : {}", session.self_name());
    println!("friends: {}", session.friend_count());
    for n in session.friend_list() {
        let pk = session
            .friend_public_key(n)
            .unwrap_or_else(|_| "?".to_string());
        let name = session.friend_name(n).unwrap_or_else(|_| "?".to_string());
        let conn = match session.friend_connection(n) {
            Connection::None => "offline",
            Connection::Tcp => "tcp",
            Connection::Udp => "udp",
        };
        println!("  [{n}] {name} <{pk}> ({conn})");
    }
    Ok(())
}

fn cmd_add(save: &Path, toxid: &str, msg: &str) -> Result<()> {
    let mut session = load(save)?;
    let n = session
        .add_friend(toxid, msg)
        .map_err(|e| anyhow!("add friend failed: {e}"))?;
    persist(&session, save)?;
    println!("friend added as #{n}");
    Ok(())
}

fn cmd_send(save: &Path, friend: u32, text: &str) -> Result<()> {
    let session = load(save)?;
    session
        .send_message(friend, text)
        .map_err(|e| anyhow!("send failed: {e}"))?;
    persist(&session, save)?;
    println!("sent to #{friend}");
    Ok(())
}

fn cmd_post(save: &Path, db: Option<&Path>, text: &str) -> Result<()> {
    let session = load(save)?;
    let store = Store::open(db.unwrap_or(&db_path(save)))
        .map_err(|e| anyhow!("cannot open store: {e}"))?;
    let engine = FeedEngine::new(store);
    let me = session.self_public_key();

    let text = text.trim().to_string();
    let (post, envelopes) = if text.chars().count() > tox_social::MAX_POST_CHARS {
        engine
            .publish_long_post(&me, &text)
            .map_err(|e| anyhow!("{e}"))?
    } else {
        let post = engine
            .publish_post(&me, &text)
            .map_err(|e| anyhow!("{e}"))?;
        (post.clone(), vec![Envelope::Post(post)])
    };
    let mut sent = 0;
    for env in envelopes {
        let wire = env.encode();
        for n in session.friend_list() {
            if session.friend_connection(n) != Connection::None {
                session
                    .send_message(n, &wire)
                    .map_err(|e| anyhow!("send to #{n} failed: {e}"))?;
                sent += 1;
            }
        }
    }
    persist(&session, save)?;
    println!(
        "post {} published, fanned out to {sent} message(s) to online friend(s)",
        post.id
    );
    Ok(())
}

/// Print the local timeline from the store (posts + threads).
fn cmd_timeline(save: &Path, db: Option<&Path>) -> Result<()> {
    let session = load(save)?;
    let store = Store::open(db.unwrap_or(&db_path(save)))
        .map_err(|e| anyhow!("cannot open store: {e}"))?;
    let engine = FeedEngine::new(store);

    // Authors: self + all friends (by public key).
    let mut authors = vec![session.self_public_key()];
    for n in session.friend_list() {
        if let Ok(pk) = session.friend_public_key(n) {
            authors.push(pk);
        }
    }

    let posts = engine.timeline(&authors, 50);
    if posts.is_empty() {
        println!("(empty timeline)");
        return Ok(());
    }
    for p in &posts {
        let kind = match p.kind {
            PostKind::Post => "post",
            PostKind::Comment => "comment",
            PostKind::Reaction => "reaction",
        };
        let text = p
            .text
            .clone()
            .unwrap_or_else(|| p.emoji.clone().unwrap_or_default());
        println!(
            "[{kind} {:.8} by {:.8}] {}",
            p.id,
            p.author,
            text
        );
        if p.kind == PostKind::Post {
            for c in engine.store().thread_for(&p.id).unwrap_or_default() {
                let ctext = c
                    .text
                    .clone()
                    .unwrap_or_else(|| c.emoji.clone().unwrap_or_default());
                println!("    └ [{:?} {:.8}] {}", c.kind, c.id, ctext);
            }
        }
    }
    Ok(())
}

fn cmd_run(save: &Path, db: Option<&Path>, bootstrap: bool) -> Result<()> {
    let mut session = load(save)?;
    let store = Store::open(db.unwrap_or(&db_path(save)))
        .map_err(|e| anyhow!("cannot open store: {e}"))?;
    let engine = FeedEngine::new(store);
    let me = session.self_public_key();

    if bootstrap {
        for (host, port, key) in DEFAULT_NODES {
            match session.bootstrap(host, *port, key) {
                Ok(()) => println!("bootstrap {host}:{port} ok"),
                Err(e) => println!("bootstrap {host}:{port} failed: {e}"),
            }
            // Most official nodes also run a TCP relay on the same port.
            let _ = session.add_tcp_relay(host, *port, key);
        }
    }

    println!("== {} ({}) running; Ctrl+C to quit ==", session.self_name(), me);
    println!(
        "   command file: {} (write 'post <text>' to publish)",
        cmd_path(save).display()
    );
    loop {
        // Process commands from the command file (single-session model).
        for line in read_commands(&cmd_path(save)) {
            if let Err(e) = handle_command(&line, &mut session, &engine, &me, save) {
                println!("[cmd    ] error: {e}");
            }
        }
        // Process events with a timeout so the command file gets polled.
        match session.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(ev) => {
                if let Err(e) = handle_event(ev, &mut session, &engine, save) {
                    println!("[event  ] error: {e}");
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Execute one line from the command file.
fn handle_command(
    line: &str,
    session: &mut ToxSession,
    engine: &FeedEngine,
    me: &str,
    save: &Path,
) -> Result<()> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }
    let (cmd, rest) = match line.split_once(' ') {
        Some((c, r)) => (c, r.trim()),
        None => (line, ""),
    };
    match cmd {
        "post" => {
            if rest.is_empty() {
                println!("[cmd    ] usage: post <text>");
                return Ok(());
            }
            let (post, envelopes) = if rest.chars().count() > tox_social::MAX_POST_CHARS {
                engine
                    .publish_long_post(me, rest)
                    .map_err(|e| anyhow!("{e}"))?
            } else {
                let post = engine
                    .publish_post(me, rest)
                    .map_err(|e| anyhow!("{e}"))?;
                (post.clone(), vec![Envelope::Post(post)])
            };
            let mut sent = 0;
            for env in envelopes {
                let wire = env.encode();
                for n in session.friend_list() {
                    if session.friend_connection(n) != Connection::None {
                        session.send_message(n, &wire)?;
                        sent += 1;
                    }
                }
            }
            persist(session, save)?;
            println!(
                "[cmd    ] post {} published, fanned out to {sent} message(s) to online friend(s)",
                post.id
            );
        }
        "comment" => {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() < 2 {
                println!("[cmd    ] usage: comment <post_id> <text>");
                return Ok(());
            }
            let comment = engine
                .publish_comment(me, parts[0], parts[1])
                .map_err(|e| anyhow!("{e}"))?;
            let wire = Envelope::Comment(comment.clone()).encode();
            for n in session.friend_list() {
                if session.friend_connection(n) != Connection::None {
                    session.send_message(n, &wire)?;
                }
            }
            persist(session, save)?;
            println!("[cmd    ] comment {} published", comment.id);
        }
        "friends" => {
            for n in session.friend_list() {
                let pk = session.friend_public_key(n).unwrap_or_else(|_| "?".into());
                let name = session.friend_name(n).unwrap_or_else(|_| "?".into());
                println!("[cmd    ] friend #{n} {name} <{pk}>");
            }
        }
        "conf_new" => {
            let n = session
                .conference_new()
                .map_err(|e| anyhow!("{e}"))?;
            println!("[cmd    ] conference #{n} created");
        }
        "conf_invite" => {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() < 2 {
                println!("[cmd    ] usage: conf_invite <friend#> <conf#>");
                return Ok(());
            }
            let friend: u32 = parts[0].parse().map_err(|e| anyhow!("bad friend#: {e}"))?;
            let conf: u32 = parts[1].parse().map_err(|e| anyhow!("bad conf#: {e}"))?;
            session
                .conference_invite(friend, conf)
                .map_err(|e| anyhow!("{e}"))?;
            println!("[cmd    ] invited friend #{friend} to conference #{conf}");
        }
        "conf_send" => {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() < 2 {
                println!("[cmd    ] usage: conf_send <conf#> <text>");
                return Ok(());
            }
            let conf: u32 = parts[0].parse().map_err(|e| anyhow!("bad conf#: {e}"))?;
            session
                .conference_send_message(conf, parts[1])
                .map_err(|e| anyhow!("{e}"))?;
            println!("[cmd    ] sent to conference #{conf}");
        }
        "conf_peers" => {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                println!("[cmd    ] usage: conf_peers <conf#>");
                return Ok(());
            }
            let conf: u32 = parts[0].parse().map_err(|e| anyhow!("bad conf#: {e}"))?;
            let count = session
                .conference_peer_count(conf)
                .map_err(|e| anyhow!("{e}"))?;
            println!("[cmd    ] conference #{conf} peers: {count}");
            for i in 0..count {
                if let Ok(name) = session.conference_peer_name(conf, i) {
                    let pk = session
                        .conference_peer_public_key(conf, i)
                        .unwrap_or_else(|_| "?".into());
                    println!("[cmd    ]   [{i}] {name} <{pk}>");
                }
            }
        }
        "help" => println!("[cmd    ] commands: post <text>, comment <post_id> <text>, friends, conf_new, conf_invite <friend#> <conf#>, conf_send <conf#> <text>, conf_peers <conf#>, help"),
        other => println!("[cmd    ] unknown command: {other}"),
    }
    Ok(())
}

/// Handle one tox event.
fn handle_event(
    ev: Event,
    session: &mut ToxSession,
    engine: &FeedEngine,
    save: &Path,
) -> Result<()> {
    match ev {
        Event::FriendRequest { public_key, message } => {
            let msg = String::from_utf8_lossy(&message).into_owned();
            println!("[request] from {public_key}: {msg}");
            match session.add_friend_norequest(&public_key) {
                Ok(n) => {
                    println!("  -> accepted as #{n}");
                    if let Some(channel_id) = msg.strip_prefix("join_channel ") {
                        let channel_id = channel_id.trim();
                        if let Ok(conf) = session.conference_by_id(channel_id) {
                            match session.conference_invite(n, conf) {
                                Ok(()) => println!(
                                    "  -> invited new friend #{n} to channel {channel_id}"
                                ),
                                Err(e) => eprintln!("  -> auto-invite failed: {e}"),
                            }
                        } else {
                            eprintln!("  -> unknown channel {channel_id}");
                        }
                    }
                    persist(session, save)?;
                }
                Err(e) => println!("  -> accept failed: {e}"),
            }
        }
        Event::FriendMessage {
            friend_number,
            text,
            ..
        } => {
            let pk = session.friend_public_key(friend_number).unwrap_or_default();
            match engine.handle_incoming(&pk, &text) {
                Incoming::Persisted(env) => match env {
                    Envelope::Post(p) => println!(
                        "[post   {:.8}] {}: {}",
                        &p.id,
                        &p.author[..8.min(p.author.len())],
                        p.text
                    ),
                    Envelope::Comment(c) => println!(
                        "[comment {:.8} -> {:.8}] {}: {}",
                        &c.id,
                        &c.reply_to,
                        &c.author[..8.min(c.author.len())],
                        c.text
                    ),
                    Envelope::Reaction(r) => println!(
                        "[reaction {:.8} -> {:.8}] {}: {}",
                        &r.id,
                        &r.reply_to,
                        &r.author[..8.min(r.author.len())],
                        r.emoji
                    ),
                    Envelope::SyncReq(req) => {
                        handle_sync_req(session, engine, friend_number, &pk, &req)?;
                    }
                    Envelope::SyncPosts(sp) => {
                        handle_sync_posts(session, engine, &pk, sp.items)?;
                    }
                    _ => {}
                },
                Incoming::Profile(p) => println!(
                    "[profile {:.8}] name={} bio={}",
                    &p.author[..8.min(p.author.len())],
                    p.name,
                    p.bio
                ),
                Incoming::Chunk => {}
                Incoming::DirReq(req) => {
                    handle_dir_req(session, engine, friend_number, &pk, &req)?;
                }
                Incoming::DirResp(resp) => {
                    handle_dir_resp(engine, &pk, &resp);
                }
                Incoming::OutboxReq(req) => {
                    handle_outbox_req(session, engine, friend_number, &req)?;
                }
                Incoming::OutboxResp(resp) => {
                    handle_outbox_resp(engine, &pk, &resp);
                }
                Incoming::Unfriend(_) => {
                    session.delete_friend(friend_number).ok();
                    engine.store().friend_remove(&pk).ok();
                    persist(session, save)?;
                    println!("[unfriend] removed {pk} (they unfriended us)");
                }
                Incoming::Rejected(_) => println!("[chat    #{friend_number}] {text}"),
            }
        }
        Event::FriendConnection {
            friend_number,
            connection,
        } => {
            let state = match connection {
                Connection::None => "offline",
                Connection::Tcp => "online (tcp)",
                Connection::Udp => "online (udp)",
            };
            println!("[conn    #{friend_number}] {state}");
            if connection != Connection::None {
                let pk = session.friend_public_key(friend_number).unwrap_or_default();
                send_sync_req(session, engine, friend_number, &pk);
            }
        }
        Event::FriendName {
            friend_number,
            name,
        } => println!("[name    #{friend_number}] {name}"),
        Event::FriendStatusMessage {
            friend_number,
            status_message,
        } => println!("[status  #{friend_number}] {status_message}"),
        Event::FriendStatus {
            friend_number,
            status,
        } => println!("[state   #{friend_number}] {status:?}"),
        Event::ConferenceInvite {
            friend_number,
            conference_type,
            cookie,
        } => {
            println!(
                "[invite  ] friend #{friend_number} invited us to conference type {conference_type}"
            );
            if conference_type == tox_core::ffi::TOX_CONFERENCE_TYPE_TEXT {
                match session.conference_join(friend_number, &cookie) {
                    Ok(n) => println!("  -> joined as conference #{n}"),
                    Err(e) => eprintln!("  -> join failed: {e}"),
                }
            } else {
                println!("  -> AV conference not supported yet");
            }
        }
        Event::ConferenceConnected { conference_number } => {
            println!("[conf    ] connected to conference #{conference_number}");
        }
        Event::ConferenceMessage {
            conference_number,
            peer_number,
            message_type: _,
            text,
        } => {
            println!("[conf#{conference_number} peer {peer_number}] {text}");
        }
        Event::ConferencePeerName {
            conference_number,
            peer_number,
            name,
        } => {
            println!("[conf    ] peer {peer_number} in #{conference_number} renamed to {name}");
        }
        Event::ConferencePeerListChanged { conference_number } => {
            println!("[conf    ] peer list changed in conference #{conference_number}");
        }
        Event::FileRecv {
            friend_number,
            file_number,
            filename,
            file_size,
        } => {
            println!("[file    ] incoming from #{friend_number}: {filename} ({file_size} bytes)");
        }
        Event::FileChunkRequest { .. } => {}
        Event::FileRecvChunk { .. } => {}
        Event::FileReceived {
            friend_number,
            file_number,
            filename,
            data,
        } => {
            println!(
                "[file    ] received #{file_number} from #{friend_number}: {filename} ({} bytes)",
                data.len()
            );
        }
    }
    Ok(())
}

fn send_sync_req(session: &ToxSession, engine: &FeedEngine, friend_number: u32, pk: &str) {
    let me = session.self_public_key();
    let since = engine.latest_ts_for_author(pk).unwrap_or(0);
    let req = Envelope::SyncReq(SyncReq {
        v: tox_social::envelope::PROTOCOL_VERSION,
        author: me,
        ts: now_ms(),
        since,
    });
    match session.send_message(friend_number, &req.encode()) {
        Ok(()) => println!("[sync    ] sync_req sent to {pk} since={since}"),
        Err(e) => eprintln!("[sync    ] failed to send sync_req: {e}"),
    }
}

fn handle_sync_req(
    session: &ToxSession,
    engine: &FeedEngine,
    friend_number: u32,
    sender_pk: &str,
    req: &SyncReq,
) -> Result<()> {
    let me = session.self_public_key();
    let items = engine.self_posts_since(&me, req.since, 200);
    if items.is_empty() {
        return Ok(());
    }
    for chunk in chunk_envelopes(&me, items) {
        let sync = Envelope::SyncPosts(SyncPosts {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: me.clone(),
            ts: now_ms(),
            items: chunk,
        });
        session.send_message(friend_number, &sync.encode())?;
    }
    println!("[sync    ] sync_posts sent to {sender_pk}");
    Ok(())
}

fn handle_sync_posts(
    _session: &ToxSession,
    engine: &FeedEngine,
    sender_pk: &str,
    items: Vec<Envelope>,
) -> Result<()> {
    let persisted = engine.handle_sync_posts(sender_pk, items);
    for env in persisted {
        match env {
            Envelope::Post(p) => println!(
                "[post   {:.8} (sync)] {}: {}",
                &p.id,
                &p.author[..8.min(p.author.len())],
                p.text
            ),
            Envelope::Comment(c) => println!(
                "[comment {:.8} -> {:.8} (sync)] {}: {}",
                &c.id,
                &c.reply_to,
                &c.author[..8.min(c.author.len())],
                c.text
            ),
            Envelope::Reaction(r) => println!(
                "[reaction {:.8} -> {:.8} (sync)] {}: {}",
                &r.id,
                &r.reply_to,
                &r.author[..8.min(r.author.len())],
                r.emoji
            ),
            _ => {}
        }
    }
    Ok(())
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

fn handle_dir_req(
    session: &ToxSession,
    engine: &FeedEngine,
    friend_number: u32,
    _sender_pk: &str,
    req: &DirReq,
) -> Result<()> {
    let items = engine
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
        .collect::<Vec<_>>();
    if !items.is_empty() {
        let resp = Envelope::DirResp(DirResp {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: session.self_public_key(),
            ts: now_ms(),
            items,
        });
        session.send_message(friend_number, &resp.encode())?;
    }
    if req.depth > 0 {
        let fwd = Envelope::DirReq(DirReq {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: session.self_public_key(),
            ts: now_ms(),
            query: req.query.clone(),
            depth: req.depth - 1,
        });
        let wire = fwd.encode();
        for n in session.friend_list() {
            if n == friend_number {
                continue;
            }
            if session.friend_connection(n) != Connection::None {
                session.send_message(n, &wire)?;
            }
        }
    }
    Ok(())
}

fn handle_dir_resp(engine: &FeedEngine, sender_pk: &str, resp: &DirResp) {
    let now = now_ms();
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
    println!("[dir     ] updated from {sender_pk}: {} entries", resp.items.len());
}

fn handle_outbox_req(
    session: &ToxSession,
    engine: &FeedEngine,
    friend_number: u32,
    req: &OutboxReq,
) -> Result<()> {
    let items = engine.public_posts_since(req.since, 100);
    if !items.is_empty() {
        let resp = Envelope::OutboxResp(OutboxResp {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: session.self_public_key(),
            ts: now_ms(),
            items,
        });
        session.send_message(friend_number, &resp.encode())?;
    }
    if req.depth > 0 {
        let fwd = Envelope::OutboxReq(OutboxReq {
            v: tox_social::envelope::PROTOCOL_VERSION,
            author: session.self_public_key(),
            ts: now_ms(),
            since: req.since,
            depth: req.depth - 1,
        });
        let wire = fwd.encode();
        for n in session.friend_list() {
            if n == friend_number {
                continue;
            }
            if session.friend_connection(n) != Connection::None {
                session.send_message(n, &wire)?;
            }
        }
    }
    Ok(())
}

fn handle_outbox_resp(engine: &FeedEngine, _sender_pk: &str, resp: &OutboxResp) {
    let received_at = now_ms();
    for item in &resp.items {
        if let Envelope::Post(p) = item {
            if engine.persist(item, &p.author, received_at) {
                println!(
                    "[outbox  ] public post from {}: {}",
                    &p.author[..8.min(p.author.len())],
                    p.text
                );
            }
        }
    }
}

/// Command file path: <save>.cmd
fn cmd_path(save: &Path) -> PathBuf {
    let mut p = save.as_os_str().to_os_string();
    p.push(".cmd");
    PathBuf::from(p)
}

/// Read and clear the command file (returns each non-empty line).
fn read_commands(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    // Clear the file so commands are executed exactly once.
    let _ = std::fs::write(path, "");
    // Strip a UTF-8 BOM that some editors / PowerShell add.
    let content = content.trim_start_matches('\u{FEFF}');
    content.lines().map(|l| l.to_string()).collect()
}

// ---------------------------------------------------------------------------

fn load(save: &Path) -> Result<ToxSession> {
    let data = std::fs::read(save).map_err(|e| anyhow!("cannot read {}: {e}", save.display()))?;
    ToxSession::from_savedata(Some(&data)).map_err(|e| anyhow!("tox load failed: {e}"))
}

fn persist(session: &ToxSession, save: &Path) -> Result<()> {
    let data = session.save();
    std::fs::write(save, data).map_err(|e| anyhow!("cannot write {}: {e}", save.display()))?;
    Ok(())
}

/// Default store path: <save-file>.db
fn db_path(save: &Path) -> PathBuf {
    let mut p = save.as_os_str().to_os_string();
    p.push(".db");
    PathBuf::from(p)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _unused() -> i64 {
    now_ms()
}
