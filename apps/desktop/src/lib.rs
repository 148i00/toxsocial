//! ToxSocial desktop entry point: wires the Tauri app to the shared crates.

mod commands;
mod events;
mod media;
mod relay;
mod state;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;

use state::AppState;

static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            use tauri::Manager;

            let state = AppState::load(app).map_err(|e| {
                eprintln!("[toxsocial] failed to init state: {e}");
                e
            })?;
            app.manage(state);
            events::spawn_event_pump(app.handle().clone());

            // System tray: keep the app alive in the background.
            let show_i = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            let _tray = TrayIconBuilder::with_id("toxsocial-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        ALLOW_EXIT.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Best-effort bootstrap to the public DHT.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<AppState>();
                let session = state.session.lock().unwrap();
                for (host, port, key) in tox_core::DEFAULT_BOOTSTRAP_NODES {
                    if let Err(e) = session.bootstrap(host, *port, key) {
                        eprintln!("[toxsocial] bootstrap {host}:{port}: {e}");
                    }
                    let _ = session.add_tcp_relay(host, *port, key);
                }
                println!("[toxsocial] bootstrapped to DHT");
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_own_info,
            commands::get_app_version,
            commands::get_network_status,
            commands::set_profile,
            commands::set_avatar,
            commands::set_avatar_url,
            commands::add_friend,
            commands::remove_friend,
            commands::remove_friend_by_toxid,
            commands::publish_post,
            commands::publish_comment,
            commands::publish_reaction,
            commands::fetch_timeline,
            commands::fetch_thread,
            commands::fetch_posts_by_author,
            commands::get_friends,
            commands::send_file_to_friend,
            commands::send_join_channel,
            commands::upload_media,
            commands::set_imgur_client_id,
            commands::get_media_config,
            commands::conference_new,
            commands::conference_invite,
            commands::conference_send,
            commands::conference_invite_by_toxid,
            commands::conference_peers,
            commands::get_conference_id,
            commands::get_conference_peer_count,
            commands::list_conferences,
            commands::request_sync_all,
            commands::search_posts,
            commands::search_directory,
            commands::request_directory_search,
            commands::fetch_public_timeline,
            commands::request_public_posts,
            commands::search_relay_directory,
            commands::fetch_relay_public_posts,
            commands::list_public_channels,
            commands::add_channel_host,
            commands::remove_channel_host,
            commands::delete_public_channel,
            commands::register_public_channel,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            use tauri::{RunEvent, WindowEvent};
            match event {
                RunEvent::Exit => println!("[toxsocial] RunEvent::Exit"),
                RunEvent::ExitRequested { api, code, .. } => {
                    if ALLOW_EXIT.load(Ordering::SeqCst) {
                        println!("[toxsocial] RunEvent::ExitRequested code={code:?}");
                    } else {
                        api.prevent_exit();
                        println!("[toxsocial] exit prevented (running in background)");
                    }
                }
                RunEvent::WindowEvent { label, event, .. } => {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        println!("[toxsocial] window close requested: {label}; hiding to tray");
                        api.prevent_close();
                        if let Some(window) = app.get_webview_window(&label) {
                            let _ = window.hide();
                        }
                    }
                }
                _ => {}
            }
        });
}
