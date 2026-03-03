use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::action::Action;

/// Events that feed into the main loop.
#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    #[allow(dead_code)]
    Resize(u16, u16),
    /// Async task results injected back into the event loop.
    Action(Action),
}

/// Spawns tasks that produce events. Returns a sender for injecting actions.
pub struct EventLoop {
    rx: mpsc::UnboundedReceiver<Event>,
    tx: mpsc::UnboundedSender<Event>,
}

impl EventLoop {
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Terminal events
        let tx_term = tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            while let Some(Ok(evt)) = reader.next().await {
                let event = match evt {
                    CrosstermEvent::Key(key) => Event::Key(key),
                    CrosstermEvent::Resize(w, h) => Event::Resize(w, h),
                    _ => continue,
                };
                if tx_term.send(event).is_err() {
                    break;
                }
            }
        });

        // Tick timer
        let tx_tick = tx.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_millis(tick_rate_ms));
            loop {
                interval.tick().await;
                if tx_tick.send(Event::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx, tx }
    }

    /// Get a sender for injecting events from async tasks.
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.tx.clone()
    }

    /// Receive the next event.
    pub async fn next(&mut self) -> Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Event channel closed"))
    }
}

/// Map a raw event to an action.
pub fn map_event(event: &Event) -> Option<Action> {
    match event {
        Event::Key(key) => map_key_event(key),
        Event::Tick => Some(Action::Tick),
        Event::Resize(_, _) => Some(Action::Render),
        Event::Action(action) => Some(action.clone()),
    }
}

fn map_key_event(key: &KeyEvent) -> Option<Action> {
    // Only Ctrl+C is truly global — everything else goes to the app for context-aware handling
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }
    Some(Action::Key(*key))
}
