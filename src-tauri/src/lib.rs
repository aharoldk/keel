//! Keel — a local-first, Git-native API client.
//!
//! The React UI talks to this core exclusively through the commands declared
//! in `commands` (see docs/IPC_CONTRACT.md).

// Public so the Keel Format parser/engines can be reused by external tooling.
pub mod ai;
pub mod auth;
mod commands;
pub mod codegen;
pub mod cookies;
pub mod datafile;
pub mod engine;
pub mod export_curl;
pub mod export_openapi;
pub mod gitutil;
pub mod graphql;
pub mod grpc;
pub mod history;
pub mod import_curl;
pub mod import_openapi;
pub mod import_postman;
pub mod import_source;
pub mod inherit;
pub mod model;
pub mod path_params;
pub mod runner;
pub mod secrets;
pub mod settings;
mod state;
pub mod workspace;
pub mod ws;

use state::AppState;
use tauri::image::Image;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("keel"));
            app.manage(AppState::new(config_dir));
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(icon) = Image::from_bytes(include_bytes!("../icons/32x32.png")) {
                    let _ = window.set_icon(icon);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace_open,
            commands::workspace_init,
            commands::workspace_close,
            commands::workspace_info,
            commands::workspace_load_tree,
            commands::folder_create,
            commands::request_create,
            commands::request_read,
            commands::request_save,
            commands::request_rename,
            commands::request_duplicate,
            commands::node_delete,
            commands::node_move,
            commands::node_reorder,
            commands::env_list,
            commands::env_read,
            commands::env_save,
            commands::env_delete,
            commands::env_values_read,
            commands::env_value_set,
            commands::env_value_delete,
            commands::secret_set,
            commands::secret_delete,
            commands::secret_list,
            commands::send_request,
            commands::history_list,
            commands::history_clear,
            commands::history_pins,
            commands::history_pin,
            commands::history_unpin,
            commands::git_status,
            commands::git_stage,
            commands::git_unstage,
            commands::git_commit,
            commands::git_log,
            commands::git_init,
            commands::git_diff_file,
            commands::git_branches,
            commands::git_checkout,
            commands::git_create_branch,
            commands::git_remotes,
            commands::git_set_remote,
            commands::git_pull,
            commands::git_push,
            commands::git_resolve,
            commands::import_curl,
            commands::import_openapi,
            commands::import_source,
            commands::import_zip,
            commands::import_git,
            commands::read_text_file,
            commands::fetch_url,
            commands::export_curl,
            commands::save_response,
            commands::settings_get,
            commands::settings_set,
            commands::get_app_version,
            commands::collection_read,
            commands::collection_save,
            commands::folder_read,
            commands::folder_save,
            commands::folder_delete_meta,
            commands::run_folder,
            commands::run_cancel,
            commands::cookie_list,
            commands::cookie_delete,
            commands::cookie_clear,
            commands::import_postman,
            commands::export_openapi,
            commands::generate_code,
            commands::graphql_introspect,
            commands::graphql_build_query,
            commands::ws_connect,
            commands::ws_send,
            commands::ws_close,
            commands::grpc_parse_proto,
            commands::grpc_call,
            commands::grpc_open,
            commands::grpc_close,
            commands::ai_status,
            commands::ai_key_set,
            commands::ai_key_clear,
            commands::ai_generate,
            commands::request_to_yaml,
            commands::request_from_yaml,
            commands::flow_to_yaml,
            commands::flow_list,
            commands::flow_read,
            commands::flow_save,
            commands::flow_delete,
            commands::flow_import,
            commands::export_request,
            commands::export_collection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keel");
}
