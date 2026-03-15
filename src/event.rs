use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;

use crate::app::Message;

/// Async event router: merges terminal key/resize events with SIGUSR1 signals.
/// Sends all events through the mpsc channel for the main loop to process.
pub async fn event_loop(tx: mpsc::Sender<Message>) {
    let mut reader = EventStream::new();
    let mut sigusr1 = signal(SignalKind::user_defined1())
        .expect("failed to register SIGUSR1 handler");

    loop {
        tokio::select! {
            Some(Ok(event)) = reader.next() => {
                match event {
                    CrosstermEvent::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                        if tx.send(Message::KeyPress(key)).await.is_err() {
                            break;
                        }
                    }
                    CrosstermEvent::Resize(w, h) => {
                        let _ = tx.send(Message::Resize(w, h)).await;
                    }
                    _ => {}
                }
            }
            _ = sigusr1.recv() => {
                let _ = tx.send(Message::RefreshSignal).await;
            }
        }
    }
}
