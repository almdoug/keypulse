#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod ghost;
mod hotkey;
mod input;
mod storage;
mod winlist;

fn main() {
    tauri::Builder::default()
        .manage(engine::Engine::default())
        .setup(|app| {
            hotkey::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            winlist::list_windows,
            winlist::focus_window,
            winlist::is_admin,
            engine::start,
            engine::stop,
            engine::is_running,
            engine::test_sequence,
            storage::load_config,
            storage::save_config,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o KeyPulse");
}
