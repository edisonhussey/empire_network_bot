//! The game's WebSocket, the way the game's server insists on it.
//!
//! One place opens the socket for everything that talks to the game (the daemon's sessions and relay, the login proof, the hunter):
//!
//! * the upgrade carries the page's headers (`empire-core::ws_handshake`), including the **`permessage-deflate` offer**: without it the
//!   server answers `400 Bad Request` (see `docs/problems/19.md`);
//! * TLS is done here, so the byte stream can be wrapped;
//! * the wrapper ([`InflatingStream`]) inflates the server's compressed frames, so `tokio-tungstenite` above it is none the wiser.
//!
//! Callers keep using `tokio-tungstenite`'s `Message` and the `StreamExt` / `SinkExt` split exactly as before.

mod deflate;
mod stream;

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use empire_core::ws_handshake::UPGRADE_HEADERS;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{
        Error,
        client::IntoClientRequest,
        error::UrlError,
        handshake::client::{Request, Response},
        http::header::{HeaderName, HeaderValue},
    },
};

pub use deflate::{InflateError, Inflater};
pub use stream::InflatingStream;

/// The TCP connection, with TLS when the URL is `wss://`.
pub enum Wire {
    Plain(TcpStream),
    Tls(Box<tokio_native_tls::TlsStream<TcpStream>>),
}

impl AsyncRead for Wire {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Wire {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// What every game connection is: a WebSocket over the inflating wrapper.
pub type GameSocket = WebSocketStream<InflatingStream<Wire>>;

/// An upgrade request for `url` carrying every header the game page sends (including the deflate offer).
pub fn game_request(url: &str) -> Result<Request, Error> {
    let mut request = url.into_client_request()?;
    for (name, value) in UPGRADE_HEADERS {
        request.headers_mut().insert(HeaderName::from_static(name), HeaderValue::from_static(value));
    }
    Ok(request)
}

/// Open the game socket for `request` (build it with [`game_request`]).
pub async fn connect(request: Request) -> Result<(GameSocket, Response), Error> {
    let uri = request.uri().clone();
    let host = uri.host().ok_or(Error::Url(UrlError::NoHostName))?.to_owned();
    let tls = match uri.scheme_str() {
        Some("wss") => true,
        Some("ws") => false,
        _ => return Err(Error::Url(UrlError::UnsupportedUrlScheme)),
    };
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let tcp = TcpStream::connect((host.as_str(), port)).await.map_err(Error::Io)?;
    let wire = if tls {
        let connector = native_tls::TlsConnector::new().map_err(|error| Error::Io(io::Error::other(error)))?;
        let secured = tokio_native_tls::TlsConnector::from(connector)
            .connect(&host, tcp)
            .await
            .map_err(|error| Error::Io(io::Error::other(error)))?;
        Wire::Tls(Box::new(secured))
    } else {
        Wire::Plain(tcp)
    };
    client_async(request, InflatingStream::new(wire)).await
}

#[cfg(test)]
mod tests;
