//! ToxSocial desktop entry point: wires the Tauri app to the shared crates.

mod commands;
mod events;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::load(app).map_err(|e| {
                eprintln!("[toxsocial] failed to init state: {e}");
                e
            })?;
            use tauri::Manager;
            app.manage(state);
            events::spawn_event_pump(app.handle().clone());

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
            commands::set_profile,
            commands::add_friend,
            commands::remove_friend,
            commands::publish_post,
            commands::publish_comment,
            commands::publish_reaction,
            commands::fetch_timeline,
            commands::fetch_thread,
            commands::get_friends,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            use tauri::RunEvent;
            match event {
                RunEvent::Exit => println!("[toxsocial] RunEvent::Exit"),
                RunEvent::ExitRequested { code, .. } => {
                    println!("[toxsocial] RunEvent::ExitRequested code={code:?}")
                }
                RunEvent::WindowEvent { label, event, .. } => {
                    use tauri::WindowEvent;
                    if let WindowEvent::Destroyed = event {
                        println!("[toxsocial] window destroyed: {label}");
                    }
                    if let WindowEvent::CloseRequested { .. } = event {
                        println!("[toxsocial] window close requested: {label}");
                    }
                }
                _ => {}
            }
        });
}
