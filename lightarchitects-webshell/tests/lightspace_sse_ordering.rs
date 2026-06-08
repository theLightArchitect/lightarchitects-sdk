//! Lightspace SSE ordering invariant tests.
//!
//! Verifies the subscribe-before-dispatch invariant and in-order delivery for
//! `broadcast::Sender<WebEventV2>`. Three properties:
//!
//! 1. A subscriber attached BEFORE the producer sends all events receives them
//!    in sequence order (topics form a deterministic ordered list).
//! 2. Two concurrent subscribers both receive all events in the same order.
//! 3. A subscriber attached AFTER all events have been sent hits `Lagged` or
//!    sees a subset — confirming missed-event semantics.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::Utc;
use lightarchitects_webshell::events::{
    envelope::{Severity, WebEventV2},
    types::WebEvent,
};
use tokio::sync::broadcast;

/// Build a minimal `WebEventV2` whose topic encodes the sequence index.
fn make_event(seq: usize) -> WebEventV2 {
    WebEventV2 {
        topic: format!("v1.lightspace.test.{seq:04}"),
        timestamp: Utc::now(),
        agent_id: "test-agent".to_owned(),
        build_id: None,
        severity: Severity::Info,
        inner: WebEvent::GatewayNotify {
            payload: serde_json::json!({ "seq": seq }),
        },
    }
}

/// A subscriber attached before any sends receives every event in order.
#[tokio::test]
async fn subscribe_before_dispatch_receives_all_in_order() {
    const COUNT: usize = 20;
    let (tx, mut rx) = broadcast::channel::<WebEventV2>(64);

    // Subscribe BEFORE producer sends — subscribe-before-dispatch invariant.
    for i in 0..COUNT {
        tx.send(make_event(i)).expect("send failed");
    }

    let mut topics = Vec::with_capacity(COUNT);
    while topics.len() < COUNT {
        match rx.recv().await {
            Ok(ev) => topics.push(ev.topic),
            Err(broadcast::error::RecvError::Lagged(n)) => {
                panic!("subscriber lagged by {n} — buffer too small or subscribe was too late");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    assert_eq!(
        topics.len(),
        COUNT,
        "expected {COUNT} events, got {}",
        topics.len()
    );

    // Topics must be in strictly ascending sequence order.
    for (i, topic) in topics.iter().enumerate() {
        let expected = format!("v1.lightspace.test.{i:04}");
        assert_eq!(
            topic, &expected,
            "event at position {i} has topic '{topic}' — expected '{expected}'"
        );
    }
}

/// Two concurrent subscribers both receive all events in the same order.
#[tokio::test]
async fn dual_subscribers_both_receive_all_in_order() {
    const COUNT: usize = 20;
    let (tx, mut rx_a) = broadcast::channel::<WebEventV2>(64);
    let mut rx_b = tx.subscribe();

    for i in 0..COUNT {
        tx.send(make_event(i)).expect("send failed");
    }

    let (topics_a, topics_b) = tokio::join!(
        async {
            let mut out = Vec::with_capacity(COUNT);
            while out.len() < COUNT {
                match rx_a.recv().await {
                    Ok(ev) => out.push(ev.topic),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        panic!("rx_a lagged: {n}");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            out
        },
        async {
            let mut out = Vec::with_capacity(COUNT);
            while out.len() < COUNT {
                match rx_b.recv().await {
                    Ok(ev) => out.push(ev.topic),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        panic!("rx_b lagged: {n}");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            out
        },
    );

    assert_eq!(topics_a.len(), COUNT);
    assert_eq!(topics_b.len(), COUNT);
    assert_eq!(
        topics_a, topics_b,
        "subscribers received events in different order"
    );
}

/// A subscriber that joins AFTER events have been sent misses them (lagged or empty).
///
/// This is the negative test for the subscribe-before-dispatch invariant: it
/// documents that subscribing after the ring buffer has overwritten old entries
/// causes `RecvError::Lagged` or yields zero events — never silently receiving
/// stale data as if it were fresh.
#[tokio::test]
async fn late_subscriber_misses_events_with_tiny_buffer() {
    const COUNT: usize = 100;
    // Deliberately tiny buffer so overwrite happens before late subscriber drains.
    let (tx, _early) = broadcast::channel::<WebEventV2>(8);

    // Blast all events — ring buffer will overwrite itself many times.
    for i in 0..COUNT {
        // Ignore send errors from the early receiver being the only subscriber;
        // we drop `_early` here to allow the ring to fill then overwrite.
        let _ = tx.send(make_event(i));
    }

    // Subscribe AFTER all sends.
    let mut rx_late = tx.subscribe();

    // The late receiver should see RecvError::Lagged immediately, or the channel
    // is closed (tx still live) — either way the receiver does NOT see all events.
    let mut received_clean = 0usize;
    let mut got_lagged = false;

    // Drain whatever is left in the buffer — expect either lag or very few events.
    loop {
        match rx_late.try_recv() {
            Ok(_) => received_clean += 1,
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                got_lagged = true;
                break;
            }
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                break;
            }
        }
    }

    assert!(
        got_lagged || received_clean < COUNT,
        "late subscriber received all {COUNT} events — subscribe-before-dispatch invariant \
         may be violated (channel buffer too large for this test; increase COUNT)"
    );
}
