use portus_protocol::{EventObjectKind, TaskEvent};
use portus_task::TaskEventSink;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel},
    },
    time::Duration,
};

const DEFAULT_SUBSCRIBER_CAPACITY: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeEvent {
    pub sequence: u64,
    pub object_kind: EventObjectKind,
    pub object_ref: String,
    pub object_sequence: Option<u64>,
    pub kind: String,
    pub safe_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventFilter {
    pub object_kind: EventObjectKind,
    pub object_ref: String,
}

struct Subscriber {
    sender: SyncSender<RuntimeEvent>,
    filter: Option<EventFilter>,
    missed: Arc<AtomicU64>,
}

pub struct EventSubscription {
    receiver: Receiver<RuntimeEvent>,
    missed: Arc<AtomicU64>,
}

impl EventSubscription {
    pub fn recv(&self) -> Result<RuntimeEvent, std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<RuntimeEvent, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<RuntimeEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    #[must_use]
    pub fn take_missed(&self) -> u64 {
        self.missed.swap(0, Ordering::AcqRel)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishOutcome {
    pub delivered: usize,
    pub lagging: usize,
    pub disconnected: usize,
}

#[derive(Clone)]
pub struct EventHub {
    inner: Arc<EventHubInner>,
}

struct EventHubInner {
    next_sequence: AtomicU64,
    subscriber_capacity: usize,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new(DEFAULT_SUBSCRIBER_CAPACITY)
    }
}

impl EventHub {
    #[must_use]
    pub fn new(subscriber_capacity: usize) -> Self {
        assert!(
            subscriber_capacity > 0,
            "subscriber capacity must be nonzero"
        );
        Self {
            inner: Arc::new(EventHubInner {
                next_sequence: AtomicU64::new(1),
                subscriber_capacity,
                subscribers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> EventSubscription {
        self.subscribe_filtered(None)
    }

    pub fn subscribe_object(
        &self,
        object_kind: EventObjectKind,
        object_ref: impl Into<String>,
    ) -> EventSubscription {
        self.subscribe_filtered(Some(EventFilter {
            object_kind,
            object_ref: object_ref.into(),
        }))
    }

    fn subscribe_filtered(&self, filter: Option<EventFilter>) -> EventSubscription {
        let (sender, receiver) = sync_channel(self.inner.subscriber_capacity);
        let missed = Arc::new(AtomicU64::new(0));
        self.inner
            .subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Subscriber {
                sender,
                filter,
                missed: Arc::clone(&missed),
            });
        EventSubscription { receiver, missed }
    }

    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.inner
            .subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    pub fn publish(&self, kind: impl Into<String>, safe_summary: Option<String>) -> PublishOutcome {
        self.publish_object(
            EventObjectKind::Runtime,
            "portusd",
            None,
            kind,
            safe_summary,
        )
    }

    pub fn publish_task_event(&self, event: &TaskEvent) -> PublishOutcome {
        self.publish_object(
            EventObjectKind::Task,
            event.task_id.to_string(),
            Some(event.sequence),
            event.event_kind.clone(),
            event.safe_summary.clone(),
        )
    }

    pub fn publish_object(
        &self,
        object_kind: EventObjectKind,
        object_ref: impl Into<String>,
        object_sequence: Option<u64>,
        kind: impl Into<String>,
        safe_summary: Option<String>,
    ) -> PublishOutcome {
        let sequence = self.inner.next_sequence.fetch_add(1, Ordering::Relaxed);
        let event = RuntimeEvent {
            sequence,
            object_kind,
            object_ref: object_ref.into(),
            object_sequence,
            kind: kind.into(),
            safe_summary,
        };
        let mut subscribers = self
            .inner
            .subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut delivered = 0;
        let mut lagging = 0;
        let mut disconnected = 0;
        subscribers.retain(|subscriber| {
            if !filter_matches(subscriber.filter.as_ref(), &event) {
                return true;
            }
            match subscriber.sender.try_send(event.clone()) {
                Ok(()) => {
                    delivered += 1;
                    true
                }
                Err(TrySendError::Full(_)) => {
                    subscriber.missed.fetch_add(1, Ordering::Relaxed);
                    lagging += 1;
                    true
                }
                Err(TrySendError::Disconnected(_)) => {
                    disconnected += 1;
                    false
                }
            }
        });
        PublishOutcome {
            delivered,
            lagging,
            disconnected,
        }
    }
}

impl TaskEventSink for EventHub {
    fn task_event_committed(&self, event: &TaskEvent) {
        let _ = self.publish_task_event(event);
    }
}

fn filter_matches(filter: Option<&EventFilter>, event: &RuntimeEvent) -> bool {
    filter.is_none_or(|filter| {
        filter.object_kind == event.object_kind && filter.object_ref == event.object_ref
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::TaskId;
    use serde_json::json;

    #[test]
    fn subscribers_receive_monotonic_bounded_events() {
        let hub = EventHub::new(2);
        let subscription = hub.subscribe();
        assert_eq!(hub.publish("runtime.ready", None).delivered, 1);
        assert_eq!(
            hub.publish("runtime.degraded", Some("safe".into()))
                .delivered,
            1
        );
        let first = subscription.recv().unwrap();
        let second = subscription.recv().unwrap();
        assert!(second.sequence > first.sequence);
        assert_eq!(second.safe_summary.as_deref(), Some("safe"));
        assert_eq!(second.object_kind, EventObjectKind::Runtime);
    }

    #[test]
    fn object_subscription_ignores_unrelated_events() {
        let hub = EventHub::new(2);
        let task_id = TaskId::new();
        let subscription = hub.subscribe_object(EventObjectKind::Task, task_id.to_string());
        let _ = hub.publish("runtime.ready", None);
        let event = TaskEvent {
            task_id,
            sequence: 4,
            event_kind: "task.running".into(),
            source_ref: None,
            safe_summary: Some("running".into()),
            safe_data: json!({}),
            occurred_at_ms: 1,
        };
        assert_eq!(hub.publish_task_event(&event).delivered, 1);
        let received = subscription.recv().unwrap();
        assert_eq!(received.object_sequence, Some(4));
        assert!(subscription.try_recv().is_err());
    }

    #[test]
    fn lagging_subscriber_tracks_missed_wakeups_without_blocking_publisher() {
        let hub = EventHub::new(1);
        let subscription = hub.subscribe();
        assert_eq!(hub.publish("one", None).delivered, 1);
        let outcome = hub.publish("two", None);
        assert_eq!(outcome.lagging, 1);
        assert_eq!(outcome.delivered, 0);
        assert_eq!(subscription.take_missed(), 1);
        assert_eq!(subscription.take_missed(), 0);
    }

    #[test]
    fn disconnected_subscribers_are_removed() {
        let hub = EventHub::new(1);
        let subscription = hub.subscribe();
        drop(subscription);
        let outcome = hub.publish("cleanup", None);
        assert_eq!(outcome.disconnected, 1);
        assert_eq!(hub.subscriber_count(), 0);
    }
}
