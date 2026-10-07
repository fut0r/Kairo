//! KairoDB desktop: a Tauri shell around `kairo-core`.
//!
//! Everything a screen can do is one of the commands in [`commands`]. They
//! hold no database logic of their own.

mod commands;
mod state;
mod store;

use state::AppState;
use store::Store;
use tauri::Manager;

/// The file that holds settings, recents and history.
const STORE_FILE: &str = "kairo.json";

/// Set this to keep the store somewhere other than the OS config directory,
/// for a portable install or an isolated test run.
const CONFIG_DIR_ENV: &str = "KAIRO_CONFIG_DIR";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = match std::env::var_os(CONFIG_DIR_ENV).filter(|dir| !dir.is_empty()) {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_config_dir()?,
            };
            app.manage(AppState::new(Store::load(dir.join(STORE_FILE))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::set_settings,
            commands::list_recents,
            commands::remove_recent,
            commands::clear_recents,
            commands::test_connection,
            commands::connect,
            commands::connect_project,
            commands::disconnect,
            commands::list_connections,
            commands::list_tables,
            commands::describe_table,
            commands::fetch_rows,
            commands::analyze_query,
            commands::run_query,
            commands::list_history,
            commands::clear_history,
            commands::validate_schema,
            commands::read_schema_file,
            commands::write_schema_file,
            commands::plan_schema,
            commands::apply_schema,
            commands::export_schema,
            commands::project_status,
            commands::init_project,
        ])
        .run(tauri::generate_context!())
        .expect("KairoDB failed to start");
}
