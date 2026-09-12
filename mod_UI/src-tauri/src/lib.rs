mod commands_browser;
mod commands_cache;
mod commands_diagnostics;
mod commands_history;
mod commands_search;
mod commands_settings;
mod commands_settings_runtime;
mod dependency;
#[cfg(test)]
#[path = "../../../报告/ai_codes/integration_baselines.rs"]
mod integration_baselines;
mod logic;
mod models;
mod search;
mod search_multi;
mod search_sanitize;
mod search_urls;
mod secrets;
mod state;
mod storage;
pub(crate) use state::SearchState;

#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod state_tests;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(state::SearchState::default())
        .manage(state::BrowserOpenLimiter::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            storage::ensure_data_directory(app.handle()).map_err(std::io::Error::other)?;
            storage::save_default_input_rules(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands_settings::generate_script,
            commands_settings::generate_result_commands,
            commands_settings::clean_script,
            commands_settings::build_history_records,
            commands_settings_runtime::load_settings,
            commands_settings_runtime::save_settings,
            commands_settings_runtime::clear_github_token,
            commands_history::load_history,
            commands_history::save_history,
            commands_cache::clear_package_cache,
            commands_cache::clear_invalidated_cache,
            commands_cache::export_package_cache,
            commands_cache::import_package_cache,
            commands_search::search_multi_ecosystem,
            commands_cache::load_package_cache,
            commands_cache::delete_package_cache_entry,
            commands_browser::open_package_search,
            commands_browser::open_package_page,
            commands_search::start_search,
            commands_search::start_binary_search,
            commands_search::stop_search,
            commands_search::pause_search,
            commands_search::resume_search,
            commands_search::cancel_search_package,
            commands_diagnostics::export_diagnostics,
            commands_settings::load_input_rules,
            commands_settings::save_input_rules,
            commands_diagnostics::test_mirror_speed,
            commands_diagnostics::test_network_connection,
            commands_settings::check_system_toolchain,
            commands_settings::execute_r_script,
            commands_diagnostics::fetch_reverse_dependencies,
            commands_cache::load_cached_results,
            commands_cache::rate_cache_result
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
