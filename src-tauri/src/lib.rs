mod adapters;
mod commands;
mod config;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::add_project,
            commands::remove_project,
            commands::launch_project,
            commands::list_agents,
            commands::add_agent,
            commands::update_agent,
            commands::remove_agent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
