//! Client-version check against the live game: run the handshake the app runs up to the server's `vck` reply and stop there.
//! Nothing after `vck` is sent, so no credentials leave the machine. Status `0` means the server accepts `CLIENT_VERSION`.
//!
//!     cargo run -p empire-socket --example vck -- [wss://ep-live-us1-game.goodgamestudios.com/] [EmpireEx_21] [client version]
use std::time::Duration;

use empire_core::protocol::parse_xt_packet;
use empire_core::session::{LoginCredentials, SessionMachine, SessionSettings};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().unwrap_or_else(|| "wss://ep-live-us1-game.goodgamestudios.com/".to_owned());
    let mut settings = SessionSettings::default();
    if let Some(header) = args.next() {
        settings.server_header = header;
    }
    if let Some(version) = args.next() {
        settings.client_version = version;
    }
    println!("checking client version {} on {url} ({})", settings.client_version, settings.server_header);
    let credentials = LoginCredentials {
        player_name: String::new(),
        portal_account_id: String::new(),
        password: None,
        login_token: None,
        registration_token: None,
    };
    let mut machine = SessionMachine::new(credentials, settings);

    let request = empire_socket::game_request(&url).expect("a valid ws(s) URL");
    let (mut socket, _) = match empire_socket::connect(request).await {
        Ok(ok) => ok,
        Err(error) => {
            println!("FAILED to open the socket: {error}");
            std::process::exit(1);
        }
    };
    for frame in machine.on_connected() {
        socket.send(Message::Text(frame.into())).await.expect("send");
    }
    loop {
        let text = match tokio::time::timeout(Duration::from_secs(10), socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
            Ok(Some(Ok(Message::Binary(bytes)))) => String::from_utf8_lossy(&bytes).into_owned(),
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(error))) => return println!("FAILED reading: {error}"),
            Ok(None) => return println!("FAILED: the server closed the socket"),
            Err(_) => return println!("FAILED: no reply in 10 seconds"),
        };
        let text = text.trim_end_matches('\0');
        if let Ok(packet) = parse_xt_packet(text)
            && packet.command == "vck"
        {
            let status = packet.status.as_deref().unwrap_or("?");
            let verdict = if status == "0" { "ACCEPTED" } else { "REFUSED" };
            return println!("{verdict}: vck status {status}, reply {text}");
        }
        for frame in machine.on_server_text(text).expect("handshake frame") {
            socket.send(Message::Text(frame.into())).await.expect("send");
        }
    }
}
