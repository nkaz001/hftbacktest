//! End-to-end tests for LiveBot BBO (bookTicker) event handling.
//!
//! Uses a mock Channel to inject events without real IPC/exchange.
//! Verifies:
//! 1. BBO events update best bid/ask in the depth
//! 2. BBO backoff clears stale levels
//! 3. BBO never causes orderbook crossing
//! 4. Mixed BBO + L2 depth events work correctly
//! 5. Batch-wrapped BBO events process atomically

use std::{cell::RefCell, collections::VecDeque, time::Duration};

use hftbacktest::{
    depth::{HashMapMarketDepth, MarketDepth},
    live::{BotError, Instrument, LiveBotBuilder, ipc::Channel},
    types::{
        Bot, BuildError, ElapseResult, Event, LOCAL_ASK_DEPTH_BBO_EVENT, LOCAL_ASK_DEPTH_EVENT,
        LOCAL_BID_DEPTH_BBO_EVENT, LOCAL_BID_DEPTH_EVENT, LiveEvent, LiveRequest,
    },
};

// ---------------------------------------------------------------------------
// Mock Channel — uses thread-local event queue for injection
// ---------------------------------------------------------------------------

thread_local! {
    static MOCK_EVENTS: RefCell<VecDeque<(usize, LiveEvent)>> = RefCell::new(VecDeque::new());
}

struct MockChannel;

impl Channel for MockChannel {
    fn build<MD>(_instruments: &[Instrument<MD>]) -> Result<Self, BuildError> {
        MOCK_EVENTS.with(|e| e.borrow_mut().clear());
        Ok(MockChannel)
    }

    fn recv_timeout(
        &mut self,
        _id: u64,
        _timeout: Duration,
    ) -> Result<(usize, LiveEvent), BotError> {
        MOCK_EVENTS.with(|events| events.borrow_mut().pop_front().ok_or(BotError::Timeout))
    }

    fn send(&mut self, _id: u64, _inst_no: usize, _request: LiveRequest) -> Result<(), BotError> {
        Ok(())
    }
}

fn push_event(inst_no: usize, ev: LiveEvent) {
    MOCK_EVENTS.with(|events| {
        events.borrow_mut().push_back((inst_no, ev));
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_feed(ev_flag: u64, px: f64, qty: f64, exch_ts: i64) -> LiveEvent {
    LiveEvent::Feed {
        symbol: "TESTUSDT".to_string(),
        event: Event {
            ev: ev_flag,
            exch_ts,
            local_ts: exch_ts + 1_000_000, // 1ms simulated latency
            px,
            qty,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        },
    }
}

type TestBot = hftbacktest::live::LiveBot<MockChannel, HashMapMarketDepth>;

fn build_bot() -> TestBot {
    LiveBotBuilder::new()
        .register(Instrument::new(
            "test_connector",
            "TESTUSDT",
            0.1,   // tick_size
            0.001, // lot_size
            HashMapMarketDepth::new(0.1, 0.001),
            0,
        ))
        .build::<MockChannel>()
        .unwrap()
}

const TIMEOUT_NS: i64 = 1_000_000_000;

/// Drain all pending events from the mock channel.
fn drain_events(bot: &mut TestBot) {
    loop {
        match bot.wait_next_feed(false, TIMEOUT_NS) {
            Ok(ElapseResult::MarketFeed) => continue,
            _ => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// BBO events should set best bid and ask prices in the depth.
#[test]
fn bbo_updates_best_bid_ask() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 1.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_BBO_EVENT, 100.1, 2.0, 1000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(depth.best_bid_tick(), 1000, "best bid tick");
    assert_eq!(depth.best_ask_tick(), 1001, "best ask tick");
    assert_eq!(depth.best_bid_qty(), 1.0, "best bid qty");
    assert_eq!(depth.best_ask_qty(), 2.0, "best ask qty");
}

/// When BBO reports a lower best bid, stale levels above it must be cleared.
#[test]
fn bbo_backoff_clears_stale_bid_levels() {
    let mut bot = build_bot();

    // Set up initial depth via L2 events: bids at 100.0, 99.9, 99.8
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 100.0, 5.0, 1000));
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.9, 3.0, 1000));
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.8, 2.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.1, 1.0, 1000));
    drain_events(&mut bot);

    assert_eq!(bot.depth(0).best_bid_tick(), 1000);
    assert_eq!(bot.depth(0).bid_qty_at_tick(1000), 5.0);
    assert_eq!(bot.depth(0).bid_qty_at_tick(999), 3.0);

    // BBO says best bid backed off to 99.7 (tick 997)
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 99.7, 4.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(
        depth.best_bid_tick(),
        997,
        "best bid should be 997 after backoff"
    );
    assert_eq!(depth.bid_qty_at_tick(997), 4.0, "qty at new BBO");
    assert_eq!(
        depth.bid_qty_at_tick(1000),
        0.0,
        "stale bid at 1000 cleared"
    );
    assert_eq!(depth.bid_qty_at_tick(999), 0.0, "stale bid at 999 cleared");
    assert_eq!(depth.bid_qty_at_tick(998), 0.0, "stale bid at 998 cleared");
}

/// When BBO reports a higher best ask, stale levels below it must be cleared.
#[test]
fn bbo_backoff_clears_stale_ask_levels() {
    let mut bot = build_bot();

    // Set up initial depth: asks at 100.1, 100.2, 100.3
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.1, 5.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.2, 3.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.3, 2.0, 1000));
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 100.0, 1.0, 1000));
    drain_events(&mut bot);

    assert_eq!(bot.depth(0).best_ask_tick(), 1001);

    // BBO says best ask moved up to 100.4 (tick 1004)
    push_event(0, make_feed(LOCAL_ASK_DEPTH_BBO_EVENT, 100.4, 4.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(
        depth.best_ask_tick(),
        1004,
        "best ask should be 1004 after backoff"
    );
    assert_eq!(depth.ask_qty_at_tick(1004), 4.0, "qty at new BBO");
    assert_eq!(
        depth.ask_qty_at_tick(1001),
        0.0,
        "stale ask at 1001 cleared"
    );
    assert_eq!(
        depth.ask_qty_at_tick(1002),
        0.0,
        "stale ask at 1002 cleared"
    );
    assert_eq!(
        depth.ask_qty_at_tick(1003),
        0.0,
        "stale ask at 1003 cleared"
    );
}

/// BBO bid arriving above the best ask must not cross the book.
/// The bid should be placed and the ask pushed above it.
#[test]
fn bbo_bid_prevents_crossing() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.9, 1.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.0, 1.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.5, 3.0, 1000));
    drain_events(&mut bot);

    // BBO bid at 100.1 would cross the ask at 100.0
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.1, 2.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    // Bid should actually be placed at 100.1
    assert_eq!(depth.best_bid_tick(), 1001, "bid should be at 1001");
    assert_eq!(depth.bid_qty_at_tick(1001), 2.0, "bid qty");
    // Ask at 100.0 should be removed (crossed), next ask at 100.5
    assert!(
        depth.best_bid_tick() < depth.best_ask_tick(),
        "book must not cross: bid_tick={} ask_tick={}",
        depth.best_bid_tick(),
        depth.best_ask_tick()
    );
}

/// BBO ask arriving below the best bid must not cross the book.
/// The ask should be placed and the bid pushed below it.
#[test]
fn bbo_ask_prevents_crossing() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 100.0, 1.0, 1000));
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.5, 3.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.1, 1.0, 1000));
    drain_events(&mut bot);

    // BBO ask at 99.9 would cross the bid at 100.0
    push_event(0, make_feed(LOCAL_ASK_DEPTH_BBO_EVENT, 99.9, 2.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    // Ask should actually be placed at 99.9
    assert_eq!(depth.best_ask_tick(), 999, "ask should be at 999");
    assert_eq!(depth.ask_qty_at_tick(999), 2.0, "ask qty");
    // Bid at 100.0 should be removed (crossed), next bid at 99.5
    assert!(
        depth.best_bid_tick() < depth.best_ask_tick(),
        "book must not cross: bid_tick={} ask_tick={}",
        depth.best_bid_tick(),
        depth.best_ask_tick()
    );
}

/// Mixed L2 depth + BBO events: BBO backoff should clear L2 levels,
/// while the opposite side remains intact.
#[test]
fn mixed_bbo_and_l2_depth() {
    let mut bot = build_bot();

    // L2 depth sets initial book
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 100.0, 5.0, 1000));
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.9, 3.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.1, 5.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.2, 3.0, 1000));
    drain_events(&mut bot);

    assert_eq!(bot.depth(0).best_bid_tick(), 1000);
    assert_eq!(bot.depth(0).best_ask_tick(), 1001);

    // BBO: bid backs off to 99.8
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 99.8, 2.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(depth.best_bid_tick(), 998, "bid backed off");
    assert_eq!(depth.bid_qty_at_tick(1000), 0.0, "stale bid cleared");
    assert_eq!(depth.bid_qty_at_tick(999), 0.0, "stale bid cleared");
    // Ask side should be unaffected by bid BBO
    assert_eq!(depth.ask_qty_at_tick(1001), 5.0, "ask unaffected");
    assert_eq!(depth.ask_qty_at_tick(1002), 3.0, "ask unaffected");
}

/// BBO events inside a batch should be processed atomically.
#[test]
fn bbo_batch_processing() {
    let mut bot = build_bot();

    push_event(0, LiveEvent::BatchStart);
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 1.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_BBO_EVENT, 100.1, 2.0, 1000));
    push_event(0, LiveEvent::BatchEnd);

    // Batch returns Ok after BatchEnd (all events processed atomically)
    let result = bot.wait_next_feed(false, TIMEOUT_NS);
    assert!(
        matches!(result, Ok(ElapseResult::Ok)),
        "batch should return Ok after BatchEnd, got {:?}",
        result
    );

    let depth = bot.depth(0);
    assert_eq!(depth.best_bid_tick(), 1000);
    assert_eq!(depth.best_ask_tick(), 1001);
}

/// Rapid sequential BBO updates (simulating bookTicker at 1ms interval).
/// Each update backs off the bid; all intermediate levels must be cleared.
#[test]
fn bbo_rapid_sequential_backoff() {
    let mut bot = build_bot();

    // Start with L2 depth at 100.0 bid, 100.1 ask
    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 100.0, 5.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.1, 5.0, 1000));
    drain_events(&mut bot);

    // 10 rapid BBO updates, bid moves down 0.1 each time
    for i in 1..=10i64 {
        let ts = 2_000_000 + i * 1_000_000; // 1ms apart
        let bid_px = 100.0 - (i as f64) * 0.1;
        push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, bid_px, 1.0, ts));
    }
    drain_events(&mut bot);

    let depth = bot.depth(0);
    // After 10 updates: bid at 99.0 (tick 990)
    assert_eq!(depth.best_bid_tick(), 990, "final bid after 10 backoffs");
    assert_eq!(depth.bid_qty_at_tick(990), 1.0);

    // All levels 991..=1000 should be cleared
    for tick in 991..=1000 {
        assert_eq!(
            depth.bid_qty_at_tick(tick),
            0.0,
            "stale bid at tick {} should be cleared",
            tick
        );
    }
}

/// BBO qty update at the same price level (no backoff) should update qty in place.
#[test]
fn bbo_qty_update_same_level() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 5.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_BBO_EVENT, 100.1, 5.0, 1000));
    drain_events(&mut bot);

    assert_eq!(bot.depth(0).best_bid_qty(), 5.0);

    // Same price, different qty
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 8.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(depth.best_bid_tick(), 1000, "price unchanged");
    assert_eq!(depth.best_bid_qty(), 8.0, "qty updated in place");
}

/// BBO bid moving UP (tightening spread) should update best_bid correctly.
#[test]
fn bbo_bid_tightens_spread() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_EVENT, 99.8, 2.0, 1000));
    push_event(0, make_feed(LOCAL_ASK_DEPTH_EVENT, 100.2, 2.0, 1000));
    drain_events(&mut bot);

    assert_eq!(bot.depth(0).best_bid_tick(), 998);
    assert_eq!(bot.depth(0).best_ask_tick(), 1002);

    // BBO: bid improves to 100.0
    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 3.0, 2000));
    drain_events(&mut bot);

    let depth = bot.depth(0);
    assert_eq!(depth.best_bid_tick(), 1000, "bid tightened to 1000");
    assert_eq!(depth.bid_qty_at_tick(1000), 3.0);
    // The old level 998 should NOT be cleared (BBO moved up, not backoff)
    assert_eq!(depth.bid_qty_at_tick(998), 2.0, "lower bid level preserved");
}

/// Feed latency should be updated after processing BBO events.
#[test]
fn bbo_updates_feed_latency() {
    let mut bot = build_bot();

    push_event(0, make_feed(LOCAL_BID_DEPTH_BBO_EVENT, 100.0, 1.0, 5000));
    drain_events(&mut bot);

    let latency = bot.feed_latency(0);
    assert!(
        latency.is_some(),
        "feed latency should be set after BBO event"
    );
    let (exch_ts, local_ts) = latency.unwrap();
    assert_eq!(exch_ts, 5000);
    assert_eq!(local_ts, 5000 + 1_000_000);
}
