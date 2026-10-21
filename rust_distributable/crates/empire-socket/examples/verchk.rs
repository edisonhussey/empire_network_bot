//! Transport check against the live game: open the socket the way the app does and send only `verChk` (no credentials, no login).
//! `apiOK` back means the upgrade, the deflate negotiation and the inflating wrapper all work.
//!
//!     cargo run -p empire-socket --example verchk -- [wss://ep-live-world2-game.goodgamestudios.com/]
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| "wss://ep-live-world2-game.goodgamestudios.com/".to_owned());
    let request = empire_socket::game_request(&url).expect("a valid ws(s) URL");
    let (mut socket, response) = match empire_socket::connect(request).await {
        Ok(ok) => ok,
        Err(error) => {
            println!("FAILED to open the socket: {error}");
            std::process::exit(1);
        }
    };
    println!("upgrade: HTTP {}, extensions: {:?}", response.status(), response.headers().get("sec-websocket-extensions"));
    socket
        .send(Message::Text("<msg t='sys'><body action='verChk' r='0'><ver v='166' /></body></msg>".into()))
        .await
        .expect("send verChk");
    match tokio::time::timeout(Duration::from_secs(8), socket.next()).await {
        Ok(Some(Ok(message))) => println!("reply: {message:?}"),
        Ok(Some(Err(error))) => println!("FAILED reading the reply: {error}"),
        Ok(None) => println!("FAILED: the server closed the socket"),
        Err(_) => println!("FAILED: no reply in 8 seconds"),
    }
}
