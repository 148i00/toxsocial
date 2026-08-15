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
use tox_social::envelope::Envelope;
use tox_social::feed::{FeedEngine, Incoming};
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

    let post = engine
        .publish_post(&me, text)
        .map_err(|e| anyhow!("{e}"))?;
    let wire = Envelope::Post(post.clone()).encode();
    let mut sent = 0;
    for n in session.friend_list() {
        if session.friend_connection(n) != Connection::None {
            session
                .send_message(n, &wire)
                .map_err(|e| anyhow!("send to #{n} failed: {e}"))?;
            sent += 1;
        }
    }
    persist(&session, save)?;
    println!(
        "post {} published, fanned out to {sent} online friend(s)",
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
            let post = engine
                .publish_post(me, rest)
                .map_err(|e| anyhow!("{e}"))?;
            let wire = Envelope::Post(post.clone()).encode();
            let mut sent = 0;
            for n in session.friend_list() {
                if session.friend_connection(n) != Connection::None {
                    session.send_message(n, &wire)?;
                    sent += 1;
                }
            }
            persist(session, save)?;
            println!(
                "[cmd    ] post {} published, fanned out to {sent} online friend(s)",
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
        "help" => println!("[cmd    ] commands: post <text>, comment <post_id> <text>, friends, help"),
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
                    _ => {}
                },
                Incoming::Profile(p) => println!(
                    "[profile {:.8}] name={} bio={}",
                    &p.author[..8.min(p.author.len())],
                    p.name,
                    p.bio
                ),
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
    }
    Ok(())
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
