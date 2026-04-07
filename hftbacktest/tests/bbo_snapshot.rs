use std::collections::BTreeMap;

use hftbacktest::types::{
    BUY_EVENT, DEPTH_BBO_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event, SELL_EVENT,
};

fn make_event(ev: u64, px: f64, qty: f64, ts: i64) -> Event {
    Event {
        ev,
        exch_ts: ts,
        local_ts: ts + 1,
        px,
        qty,
        order_id: 0,
        ival: 0,
        fval: 0.0,
    }
}

fn price_key(px: f64) -> i64 {
    (px * 1e8) as i64
}

fn rebuild_bbo_snapshot(events: &[Event]) -> (BTreeMap<i64, f64>, BTreeMap<i64, f64>) {
    let mut bids = BTreeMap::new();
    let mut asks = BTreeMap::new();

    for event in events {
        let ev_type = event.ev & 0xff;
        if ev_type == DEPTH_EVENT || ev_type == DEPTH_SNAPSHOT_EVENT {
            let key = price_key(event.px);
            let is_bid = (event.ev & BUY_EVENT) != 0;

            if is_bid {
                if event.qty == 0.0 {
                    bids.remove(&key);
                } else {
                    bids.insert(key, event.qty);
                }
            } else if event.qty == 0.0 {
                asks.remove(&key);
            } else {
                asks.insert(key, event.qty);
            }
        } else if ev_type == DEPTH_BBO_EVENT {
            let key = price_key(event.px);
            let is_bid = (event.ev & BUY_EVENT) != 0;

            if is_bid {
                if event.qty > 0.0 {
                    bids.insert(key, event.qty);
                } else {
                    bids.remove(&key);
                }

                let stale: Vec<i64> = bids.range((key + 1)..).map(|(&k, _)| k).collect();
                for k in stale {
                    bids.remove(&k);
                }

                let crossed: Vec<i64> = asks.range(..=key).map(|(&k, _)| k).collect();
                for k in crossed {
                    asks.remove(&k);
                }
            } else {
                if event.qty > 0.0 {
                    asks.insert(key, event.qty);
                } else {
                    asks.remove(&key);
                }

                let stale: Vec<i64> = asks.range(..key).map(|(&k, _)| k).collect();
                for k in stale {
                    asks.remove(&k);
                }

                let crossed: Vec<i64> = bids.range(key..).map(|(&k, _)| k).collect();
                for k in crossed {
                    bids.remove(&k);
                }
            }
        }
    }

    (bids, asks)
}

#[test]
fn test_bbo_backoff_clears_stale_in_snapshot() {
    let events = vec![
        make_event(DEPTH_EVENT | BUY_EVENT, 1000.0, 3.0, 1),
        make_event(DEPTH_EVENT | BUY_EVENT, 999.0, 2.0, 2),
        make_event(DEPTH_EVENT | BUY_EVENT, 998.0, 1.0, 3),
        make_event(DEPTH_EVENT | SELL_EVENT, 1001.0, 1.0, 4),
        make_event(DEPTH_EVENT | SELL_EVENT, 1002.0, 1.0, 5),
        make_event(DEPTH_BBO_EVENT | BUY_EVENT, 997.0, 4.0, 6),
    ];

    let (bids, _) = rebuild_bbo_snapshot(&events);

    assert!(bids.contains_key(&price_key(997.0)));
    assert!(!bids.contains_key(&price_key(1000.0)));
    assert!(!bids.contains_key(&price_key(999.0)));
    assert!(!bids.contains_key(&price_key(998.0)));
}

#[test]
fn test_bbo_prevents_crossing_in_snapshot() {
    let events = vec![
        make_event(DEPTH_EVENT | SELL_EVENT, 1000.0, 1.0, 1),
        make_event(DEPTH_BBO_EVENT | BUY_EVENT, 1001.0, 2.0, 2),
    ];

    let (bids, asks) = rebuild_bbo_snapshot(&events);

    assert!(bids.contains_key(&price_key(1001.0)));
    assert!(!asks.contains_key(&price_key(1000.0)));
}

#[test]
fn test_bbo_ask_backoff_in_snapshot() {
    let events = vec![
        make_event(DEPTH_EVENT | SELL_EVENT, 1001.0, 1.0, 1),
        make_event(DEPTH_EVENT | SELL_EVENT, 1002.0, 1.0, 2),
        make_event(DEPTH_EVENT | SELL_EVENT, 1003.0, 1.0, 3),
        make_event(DEPTH_BBO_EVENT | SELL_EVENT, 1005.0, 4.0, 4),
    ];

    let (_, asks) = rebuild_bbo_snapshot(&events);

    assert!(asks.contains_key(&price_key(1005.0)));
    assert!(!asks.contains_key(&price_key(1001.0)));
    assert!(!asks.contains_key(&price_key(1002.0)));
    assert!(!asks.contains_key(&price_key(1003.0)));
}

#[test]
fn test_mixed_depth_and_bbo_snapshot() {
    let events = vec![
        make_event(DEPTH_EVENT | BUY_EVENT, 1000.0, 1.0, 1),
        make_event(DEPTH_BBO_EVENT | BUY_EVENT, 998.0, 2.0, 2),
        make_event(DEPTH_EVENT | BUY_EVENT, 999.0, 3.0, 3),
    ];

    let (bids, _) = rebuild_bbo_snapshot(&events);

    assert!(bids.contains_key(&price_key(998.0)));
    assert!(bids.contains_key(&price_key(999.0)));
    assert!(!bids.contains_key(&price_key(1000.0)));
}
