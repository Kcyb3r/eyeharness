//! Central event bus ([plan §18]).
//!
//! A [`EventBus`] fans every [`Event`] out to a set of subscribers. The same
//! stream drives the logger, the HUD, replay, and metrics. Subscribers are
//! failure-isolated: a slow or crashing consumer must never stall the harness
//! core, and if every consumer drops, the bus keeps counting messages.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use harness_protocol::Event;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;

/// Capacity of the underlying broadcast channel.
const CHANNEL_CAPACITY: usize = 1024;

/// Error returned when publishing fails.
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    /// The channel was closed (all subscribers dropped / bus shut down).
    #[error("event bus closed: {0}")]
    Closed(String),
}

/// A handle to publish events into the bus.
#[derive(Clone)]
pub struct EventSender {
    tx: broadcast::Sender<Event>,
    published: std::sync::Arc<AtomicU64>,
    total_sent: std::sync::Arc<AtomicU64>,
    keepalive: Arc<std::sync::Mutex<broadcast::Receiver<Event>>>,
}

impl EventSender {
    /// Publish an event to all subscribers.
    pub fn publish(&self, event: Event) -> Result<(), BusError> {
        self.tx
            .send(event)
            .map_err(|SendError(_)| BusError::Closed("channel closed".into()))?;
        self.published.fetch_add(1, Ordering::Relaxed);
        self.total_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// The number of events published through this sender handle.
    pub fn published_count(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// The total number of events sent across all sender handles.
    pub fn total_sent(&self) -> u64 {
        self.total_sent.load(Ordering::Relaxed)
    }

    /// Access the underlying bus for subscribing additional consumers
    /// (HUD, replay, metrics).
    pub fn bus_ref(&self) -> EventBus {
        EventBus {
            tx: self.tx.clone(),
            total_sent: self.total_sent.clone(),
            keepalive: self.keepalive.clone(),
        }
    }
}

impl std::fmt::Debug for EventSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSender")
            .field("published", &self.published)
            .field("total_sent", &self.total_sent())
            .finish_non_exhaustive()
    }
}

/// A subscriber's view of the bus. Dropping it unsubscribes.
pub struct EventStream {
    rx: broadcast::Receiver<Event>,
}

impl EventStream {
    /// Receive the next event, if one is immediately or soon available.
    pub async fn recv(&mut self) -> Option<Event> {
        self.rx.recv().await.ok()
    }

    /// Try to receive without awaiting.
    pub fn try_recv(&mut self) -> Result<Event, TryRecvError> {
        self.rx.try_recv().map_err(|e| match e {
            broadcast::error::TryRecvError::Empty => TryRecvError::Empty,
            broadcast::error::TryRecvError::Closed => TryRecvError::Closed,
            broadcast::error::TryRecvError::Lagged(n) => TryRecvError::Lagged(n),
        })
    }
}

/// Errors from [`EventStream::try_recv`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TryRecvError {
    /// No events currently pending.
    Empty,
    /// The bus was shut down.
    Closed,
    /// The consumer fell too far behind and events were dropped.
    Lagged(u64),
}

/// The central async event bus.
#[derive(Clone)]
pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<Event>,
    total_sent: std::sync::Arc<AtomicU64>,
    keepalive: Arc<std::sync::Mutex<broadcast::Receiver<Event>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Create an empty event bus.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            tx,
            total_sent: std::sync::Arc::new(AtomicU64::new(0)),
            keepalive: Arc::new(std::sync::Mutex::new(_rx)),
        }
    }

    /// Get a sender handle for publishing.
    pub fn sender(&self) -> EventSender {
        EventSender {
            tx: self.tx.clone(),
            published: Arc::new(AtomicU64::new(0)),
            total_sent: self.total_sent.clone(),
            keepalive: self.keepalive.clone(),
        }
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> EventStream {
        EventStream {
            rx: self.tx.subscribe(),
        }
    }

    /// Number of total events sent across all sender handles.
    pub fn total_sent(&self) -> u64 {
        self.total_sent.load(Ordering::Relaxed)
    }

    /// Publish an event convenience wrapper.
    pub fn publish(&self, event: Event) -> Result<(), BusError> {
        self.sender().publish(event)
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("total_sent", &self.total_sent())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::EventKind;

    fn ev() -> Event {
        Event::new(EventKind::ObservationCreated, 0)
    }

    #[tokio::test]
    async fn publish_reaches_single_subscriber() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        bus.publish(ev()).unwrap();
        let got = sub.recv().await.unwrap();
        assert_eq!(got.kind, EventKind::ObservationCreated);
        assert_eq!(bus.total_sent(), 1);
    }

    #[tokio::test]
    async fn publish_reaches_multiple_subscribers() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish(ev()).unwrap();
        assert_eq!(a.recv().await.unwrap().kind, EventKind::ObservationCreated);
        assert_eq!(b.recv().await.unwrap().kind, EventKind::ObservationCreated);
    }

    #[tokio::test]
    async fn slow_subscriber_sees_lag_not_crash() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();

        // Publish far more than the channel capacity.
        for i in 0..(CHANNEL_CAPACITY + 50) {
            bus.publish(Event::new(EventKind::SystemHealth, i as u64))
                .unwrap();
        }
        // Channel is still open: a fresh subscriber sees a lag error first.
        let err = sub.try_recv();
        match err {
            Err(TryRecvError::Lagged(_)) => {}
            other => panic!("expected lagged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn no_subscribers_does_not_error() {
        let bus = EventBus::new();
        bus.publish(ev()).unwrap();
        assert_eq!(bus.total_sent(), 1);
    }
}
