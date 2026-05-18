//! `GitForest` SSE burst test — Phase 7 exit criterion (HIGH iter-4).
//!
//! Verifies that the broadcast channel can absorb and deliver 1000 events in
//! rapid succession without lag or drop, simulating concurrent gitforest +
//! ironclaw-spine SSE consumers.
//!
//! Design:
//!   1. Two subscribers attach before any events are sent.
//!   2. A producer blasts 1000 events in a tight loop.
//!   3. Both subscribers drain the channel and verify they received all 1000.
//!   4. Confirms the channel buffer + receiver semantics hold under burst load.
//!
//! Note: `broadcast::channel` uses a ring buffer — if a receiver lags the
//! producer by more than `capacity` items, it gets `RecvError::Lagged`.
//! This test uses capacity=2048 (> 1000) to guarantee zero-lag delivery.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lightarchitects_webshell::events::types::{TraceSpanSummary, WebEvent};
use tokio::sync::broadcast;

fn make_span_event(seq: usize) -> WebEvent {
    WebEvent::AyinSpan(TraceSpanSummary {
        id: format!("burst-span-{seq}"),
        parent_id: None,
        actor: format!("burst-actor-{}", seq % 7),
        action: "burst_write".to_owned(),
        timestamp: "2026-05-18T00:00:00Z".to_owned(),
        duration_ms: 1,
        outcome: serde_json::Value::String("success".to_owned()),
        metadata: serde_json::Value::Null,
        strand_activations: vec![],
    })
}

/// Ensures 1000 bursted events arrive on a single subscriber without lag.
#[tokio::test]
async fn sse_burst_1000_events_single_subscriber_no_drop() {
    const COUNT: usize = 1000;
    let (tx, mut rx) = broadcast::channel::<WebEvent>(2048);

    // Blast all 1000 events synchronously (no yield points — worst case for consumer lag)
    for i in 0..COUNT {
        tx.send(make_span_event(i)).expect("send failed");
    }

    // Drain the subscriber
    let mut received = 0usize;
    while received < COUNT {
        match rx.recv().await {
            Ok(_) => received += 1,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                panic!("subscriber lagged — dropped {n} events. Increase channel capacity.");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    assert_eq!(
        received, COUNT,
        "subscriber received {received}/{COUNT} events"
    );
}

/// Ensures 1000 bursted events arrive on TWO concurrent subscribers (gitforest + ironclaw sim).
#[tokio::test]
async fn sse_burst_1000_events_dual_subscriber_no_drop() {
    const COUNT: usize = 1000;
    let (tx, mut rx_a) = broadcast::channel::<WebEvent>(2048);
    let mut rx_b = tx.subscribe();

    // Blast all events
    for i in 0..COUNT {
        tx.send(make_span_event(i)).expect("send failed");
    }

    // Drain both subscribers concurrently
    let (result_a, result_b) = tokio::join!(
        async {
            let mut n = 0usize;
            while n < COUNT {
                match rx_a.recv().await {
                    Ok(_) => n += 1,
                    Err(broadcast::error::RecvError::Lagged(lag)) => panic!("rx_a lagged: {lag}"),
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            n
        },
        async {
            let mut n = 0usize;
            while n < COUNT {
                match rx_b.recv().await {
                    Ok(_) => n += 1,
                    Err(broadcast::error::RecvError::Lagged(lag)) => panic!("rx_b lagged: {lag}"),
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            n
        },
    );

    assert_eq!(result_a, COUNT, "subscriber A received {result_a}/{COUNT}");
    assert_eq!(result_b, COUNT, "subscriber B received {result_b}/{COUNT}");
}

/// Verifies that a slow subscriber with capacity=16 correctly reports Lagged
/// when burst exceeds buffer — confirms the lag-detection path works.
#[tokio::test]
async fn sse_burst_undersized_buffer_reports_lagged() {
    const COUNT: usize = 1000;
    let (tx, mut rx) = broadcast::channel::<WebEvent>(16); // deliberately tiny

    for i in 0..COUNT {
        let _ = tx.send(make_span_event(i)); // ignore SendError (no active receivers at start)
    }

    // The receiver should hit RecvError::Lagged because the ring buffer overflowed
    let mut got_lagged = false;
    loop {
        match rx.recv().await {
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => {
                got_lagged = true;
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    assert!(got_lagged, "expected Lagged error with undersized buffer");
}

/// Rate gauge: 1000 events in < 100ms is achievable in-process (channel is not the bottleneck).
#[tokio::test]
async fn sse_burst_1000_events_completes_within_100ms() {
    const COUNT: usize = 1000;
    let (tx, mut rx) = broadcast::channel::<WebEvent>(2048);

    let start = std::time::Instant::now();

    for i in 0..COUNT {
        tx.send(make_span_event(i)).expect("send failed");
    }

    let mut received = 0usize;
    while received < COUNT {
        match rx.recv().await {
            Ok(_) => received += 1,
            Err(_) => break,
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 100,
        "burst took {}ms — expected < 100ms (channel should not be the bottleneck)",
        elapsed.as_millis()
    );
}
