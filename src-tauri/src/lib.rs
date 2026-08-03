mod adapters;
mod commands;
mod config;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::add_project,
            commands::remove_project,
            commands::launch_project,
            commands::get_context,
            commands::set_context,
            commands::sync_context,
            commands::path_exists,
            commands::list_agents,
            commands::add_agent,
            commands::update_agent,
            commands::remove_agent,
            commands::list_groups,
            commands::add_group,
            commands::update_group,
            commands::remove_group,
            commands::launch_group,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
