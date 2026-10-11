#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

struct Backend(Mutex<Option<Child>>);

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            let repo = root.parent().unwrap();
            let python = repo.join("venv/bin/python");
            let child = Command::new(&python)
                .arg("-m")
                .arg("backend.server")
                .current_dir(&root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok();
            app.manage(Backend(Mutex::new(child)));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.app_handle().try_state::<Backend>() {
                    if let Ok(mut child) = state.0.lock() {
                        if let Some(mut process) = child.take() {
                            if process.try_wait().ok().flatten().is_some() { return; }
                            if let Ok(mut stream) = TcpStream::connect_timeout(
                                &"127.0.0.1:8798".parse().unwrap(), Duration::from_millis(400)
                            ) {
                                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                                let _ = stream.write_all(b"POST /api/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
                                let mut response = [0u8; 128];
                                let _ = stream.read(&mut response);
                            }
                            let deadline = Instant::now() + Duration::from_secs(6);
                            while Instant::now() < deadline {
                                if process.try_wait().ok().flatten().is_some() { return; }
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            let _ = process.kill();
                            let _ = process.wait();
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run packet monitor");
}
