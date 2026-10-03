#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            tauri::async_runtime::spawn(async move {
                let config = empire_daemon::DaemonConfig {
                    data_dir,
                    ..Default::default()
                };
                if let Err(error) = empire_daemon::serve(config).await {
                    eprintln!("embedded OpenAuto service stopped: {error:#}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run OpenAuto desktop app");
}
