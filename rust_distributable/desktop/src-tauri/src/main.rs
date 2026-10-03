//! OpenAuto desktop.
//!
//! Two modes in one binary:
//!
//! * `empire-desktop --service` runs headless: it owns the database and the
//!   game connection, and keeps running when no window exists.
//! * any other invocation opens the window, first making sure a service is up.
//!
//! Splitting them is what makes the app behave like a background tool. The game
//! session is a live WebSocket and the server issues no reusable token — the
//! login reply `%xt%lli%1%0%` carries no credentials — so a session can only
//! survive a closed window by living in a process that outlives it. Signing in
//! again on every window open is the thing this avoids.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{Command, Stdio};
use std::time::Duration;

use empire_daemon::DEFAULT_BIND;

const SERVICE_FLAG: &str = "--service";

fn main() {
    if std::env::args().any(|argument| argument == SERVICE_FLAG) {
        run_service();
        return;
    }
    open_window();
}

/// Headless mode: the long-lived half of the app.
fn run_service() {
    if let Err(error) = tauri::async_runtime::block_on(empire_daemon::serve_from_env()) {
        eprintln!("OpenAuto service stopped: {error:#}");
        std::process::exit(1);
    }
}

fn open_window() {
    tauri::Builder::default()
        .setup(|_app| {
            // Started from `setup`, not before `run`: the window must be created
            // on the main thread with Tauri's runtime untouched, and doing async
            // work first left the app with no window at all. The UI polls, so a
            // service that is still starting up shows as "unavailable" for one
            // tick and then resolves itself.
            tauri::async_runtime::spawn(async {
                ensure_service().await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run the OpenAuto window");
}

/// Start the service if one is not already listening.
///
/// An existing service is reused rather than replaced, so reopening the window
/// never disturbs a signed-in session.
async fn ensure_service() {
    if empire_daemon::service_is_running(DEFAULT_BIND).await {
        return;
    }
    if let Err(error) = spawn_service() {
        eprintln!("could not start the OpenAuto service: {error}");
        return;
    }
    for _ in 0..40 {
        if empire_daemon::service_is_running(DEFAULT_BIND).await {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    eprintln!("the OpenAuto service did not come up in time");
}

/// Launch ourselves in service mode, detached from this window's lifetime.
fn spawn_service() -> std::io::Result<()> {
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg(SERVICE_FLAG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        // Its own process group, so quitting the window does not take the
        // service down with it.
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(|_| ())
}
