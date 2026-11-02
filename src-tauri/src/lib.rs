mod adapters;
mod commands;
mod config;
mod update;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // 自动更新最后一步 relaunch() 由该插件提供（评审 P1-2：未注册则安装
        // 成功后重启必失败，前端误报「自动更新失败」）。
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::add_project,
            commands::remove_project,
            commands::launch_project,
            commands::reorder_projects,
            commands::config_status,
            commands::read_context,
            commands::save_context,
            commands::context_status,
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
            update::check_for_updates,
            update::fetch_announcements,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// P1-2 回归：自动更新 relaunch() 链路三处必须一致存在（评审证据链正是
    /// "Cargo.toml / lib.rs / capabilities 三处一致缺失"，任一回退即复发）。
    #[test]
    fn process_plugin_is_registered_end_to_end() {
        let manifest = env!("CARGO_MANIFEST_DIR");

        let cargo_toml = std::fs::read_to_string(format!("{manifest}/Cargo.toml")).unwrap();
        assert!(
            cargo_toml.contains("tauri-plugin-process"),
            "Cargo.toml must depend on tauri-plugin-process"
        );

        let lib_rs = std::fs::read_to_string(format!("{manifest}/src/lib.rs")).unwrap();
        assert!(
            lib_rs.contains("tauri_plugin_process::init()"),
            "lib.rs must register the process plugin"
        );

        let caps: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{manifest}/capabilities/default.json")).unwrap(),
        )
        .unwrap();
        let permissions = caps["permissions"].as_array().unwrap();
        assert!(
            permissions
                .iter()
                .any(|p| p.as_str() == Some("process:default")),
            "capabilities/default.json must grant process:default"
        );
    }
}
