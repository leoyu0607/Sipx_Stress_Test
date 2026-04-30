#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tauri::Builder;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .init();

    Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::start_test,
            commands::stop_test,
            commands::get_snapshot,
            commands::get_report,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 啟動失敗");
}
