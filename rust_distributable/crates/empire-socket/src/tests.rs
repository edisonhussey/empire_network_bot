//! The whole path against an in-memory "server" that behaves like the game's: it only upgrades a request that offers
//! `permessage-deflate`, then sends its messages compressed.

use flate2::{Compress, Compression, FlushCompress};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};
use tokio_tungstenite::{
    client_async,
    tungstenite::{Message, handshake::derive_accept_key},
};

use crate::{game_request, stream::InflatingStream};

fn deflate(compressor: &mut Compress, message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(message.len() + 64);
    compressor.compress_vec(message, &mut out, FlushCompress::Sync).unwrap();
    out.truncate(out.len() - 4);
    out
}

fn server_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x80 | 0x40 | opcode, payload.len() as u8];
    assert!(payload.len() < 126);
    out.extend_from_slice(payload);
    out
}

#[test]
fn the_request_carries_the_browser_headers_and_the_deflate_offer() {
    let request = game_request("wss://example.test/").unwrap();
    let extensions = request.headers().get("sec-websocket-extensions").expect("the deflate offer").to_str().unwrap();
    assert!(extensions.starts_with("permessage-deflate"));
    assert_eq!(request.headers().get("origin").unwrap(), "https://empire-html5.goodgamestudios.com");
    assert!(request.headers().contains_key("user-agent"));
}

#[tokio::test]
async fn a_server_that_compresses_its_messages_is_read_as_plain_text() {
    let (client_end, mut server_end) = duplex(64 * 1024);
    let server = tokio::spawn(async move {
        // read the upgrade request, insist on the deflate offer, answer 101
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            server_end.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
        }
        let text = String::from_utf8(head).unwrap();
        assert!(text.to_ascii_lowercase().contains("sec-websocket-extensions: permessage-deflate"), "no deflate offer:\n{text}");
        let key = text.lines().find_map(|l| l.strip_prefix("sec-websocket-key: ").or_else(|| l.strip_prefix("Sec-WebSocket-Key: "))).unwrap().trim().to_owned();
        let reply = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n",
            derive_accept_key(key.as_bytes())
        );
        // send the headers and the first frame in ONE write: the wrapper must split them correctly
        let mut compressor = Compress::new(Compression::default(), false);
        let mut burst = reply.into_bytes();
        burst.extend(server_frame(0x2, &deflate(&mut compressor, b"<msg t='sys'><body action='apiOK' r='0'></body></msg>")));
        server_end.write_all(&burst).await.unwrap();
        // the client's verChk arrives masked and uncompressed
        let mut header = [0u8; 2];
        server_end.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], 0x81, "the client sends plain text frames");
        // then a second compressed message that leans on the first one's window
        server_end.write_all(&server_frame(0x2, &deflate(&mut compressor, b"<msg t='sys'><body action='apiOK' r='0'></body></msg>"))).await.unwrap();
        server_end
    });

    let request = game_request("wss://example.test/").unwrap();
    let (mut socket, response) = client_async(request, InflatingStream::new(client_end)).await.expect("the upgrade succeeds");
    assert_eq!(response.status(), 101);
    let first = socket.next().await.unwrap().unwrap();
    assert_eq!(first, Message::Binary(b"<msg t='sys'><body action='apiOK' r='0'></body></msg>".to_vec().into()));
    socket.send(Message::Text("<msg t='sys'><body action='verChk' r='0'><ver v='166' /></body></msg>".into())).await.unwrap();
    let second = socket.next().await.unwrap().unwrap();
    assert!(matches!(second, Message::Binary(_)));
    let _ = server.await;
}
