//! The two ways a session reaches the game, behind one sink/stream pair so `run_inner` is the same code for both.
//!
//! * **Direct**: the daemon's own socket (`empire_socket::connect`), as before.
//! * **Attached** (injection mode): the socket of a browser playing through proxy_packet_monitor. The proxy
//!   forwards the server's frames here and sends what the session writes to the server. The browser owns the
//!   login, the heartbeat and the socket's life (`docs/architecture/injection-mode.md`).

use std::{fmt, pin::Pin, sync::Arc};

use futures_util::{Sink, SinkExt, Stream, StreamExt, sink, stream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub type GameSink = Pin<Box<dyn Sink<Message, Error = anyhow::Error> + Send>>;
pub type GameStream = Pin<Box<dyn Stream<Item = anyhow::Result<Message>> + Send>>;

/// The session's ends of a proxy link.
pub struct AttachedLink {
    /// Server-to-client frames the proxy saw.
    pub from_game: mpsc::Receiver<String>,
    /// Frames for the proxy to send to the server.
    pub to_game: mpsc::Sender<String>,
}

/// Carries an [`AttachedLink`] into the session through the (cloneable) connect request. Taken once.
#[derive(Clone, Default)]
pub struct AttachSlot(Arc<std::sync::Mutex<Option<AttachedLink>>>);

impl fmt::Debug for AttachSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AttachSlot")
    }
}

impl AttachSlot {
    pub fn new(link: AttachedLink) -> Self {
        Self(Arc::new(std::sync::Mutex::new(Some(link))))
    }

    pub fn take(&self) -> Option<AttachedLink> {
        self.0.lock().expect("attach slot").take()
    }
}

/// The daemon's own socket.
pub fn direct(socket: empire_socket::GameSocket) -> (GameSink, GameStream) {
    let (sink, stream) = socket.split();
    (
        Box::pin(sink.sink_map_err(anyhow::Error::from)),
        Box::pin(stream.map(|item| item.map_err(anyhow::Error::from))),
    )
}

/// A proxy link. The stream ends when the proxy goes away; text written to the sink goes to the server.
pub fn attached(link: AttachedLink) -> (GameSink, GameStream) {
    let AttachedLink { from_game, to_game } = link;
    let stream = stream::unfold(from_game, |mut receiver| async move {
        receiver.recv().await.map(|text| (Ok(Message::Text(text.into())), receiver))
    });
    let sink = sink::unfold(to_game, |sender, message: Message| async move {
        if let Message::Text(text) = message {
            sender.send(text.to_string()).await.map_err(|_| anyhow::anyhow!("the proxy link closed"))?;
        }
        Ok::<_, anyhow::Error>(sender)
    });
    (Box::pin(sink), Box::pin(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_attached_link_carries_text_both_ways_and_ends_with_the_proxy() {
        let (to_session, from_game) = mpsc::channel(4);
        let (to_game, mut at_proxy) = mpsc::channel(4);
        let (mut sink, mut stream) = attached(AttachedLink { from_game, to_game });
        to_session.send("%xt%gcu%1%0%{}%".to_owned()).await.unwrap();
        assert!(matches!(stream.next().await, Some(Ok(Message::Text(text))) if text.as_str() == "%xt%gcu%1%0%{}%"));
        sink.send(Message::Text("%xt%EmpireEx_21%crin%1%{}%".into())).await.unwrap();
        assert_eq!(at_proxy.recv().await.as_deref(), Some("%xt%EmpireEx_21%crin%1%{}%"));
        drop(to_session);
        assert!(stream.next().await.is_none());
    }
}
