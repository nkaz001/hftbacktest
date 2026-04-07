//! Event order validation and correction utilities.

use crate::types::{EXCH_EVENT, Event, LOCAL_EVENT};

/// Adjusts the local timestamp if the feed latency is negative by offsetting it
/// by the maximum negative latency value.
///
/// ```text
/// feed_latency = local_timestamp - exch_timestamp
/// adjusted_local_timestamp = local_timestamp - min(feed_latency, 0) + base_latency
/// ```
///
/// # Arguments
/// * `data` - Mutable slice of events to be corrected in place
/// * `base_latency` - Additional latency to add (in nanoseconds)
///
/// # Returns
/// The minimum latency found (before correction)
pub fn correct_local_timestamp(data: &mut [Event], base_latency: i64) -> i64 {
    if data.is_empty() {
        return 0;
    }

    // Find minimum latency
    let mut min_latency = i64::MAX;
    for event in data.iter() {
        let latency = event.local_ts - event.exch_ts;
        min_latency = min_latency.min(latency);
    }

    // If minimum latency is negative, adjust all local timestamps
    if min_latency < 0 {
        let offset = -min_latency + base_latency;
        for event in data.iter_mut() {
            event.local_ts += offset;
        }
    }

    min_latency
}

/// Corrects exchange timestamps that are reversed by splitting each row into separate events.
/// These events are then ordered by both exchange and local timestamps through duplication.
///
/// # Arguments
/// * `data` - Input events (without EXCH_EVENT/LOCAL_EVENT flags)
/// * `sorted_exch_indices` - Indices that would sort data by exchange timestamp
/// * `sorted_local_indices` - Indices that would sort data by local timestamp
///
/// # Returns
/// A new vector with corrected event order, where each event has appropriate
/// EXCH_EVENT and/or LOCAL_EVENT flags set.
pub fn correct_event_order(
    data: &[Event],
    sorted_exch_indices: &[usize],
    sorted_local_indices: &[usize],
) -> Vec<Event> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(data.len() * 2);
    let mut exch_rn = 0;
    let mut local_rn = 0;
    let n = data.len();

    loop {
        if exch_rn >= n && local_rn >= n {
            break;
        }

        let sorted_exch = if exch_rn < n {
            Some(&data[sorted_exch_indices[exch_rn]])
        } else {
            None
        };

        let sorted_local = if local_rn < n {
            Some(&data[sorted_local_indices[local_rn]])
        } else {
            None
        };

        match (sorted_exch, sorted_local) {
            (Some(exch), Some(local)) => {
                if exch.exch_ts == local.exch_ts && exch.local_ts == local.local_ts {
                    // Same event - merge with both flags
                    debug_assert_eq!(exch.ev, local.ev);
                    debug_assert!(exch.px == local.px || (exch.px.is_nan() && local.px.is_nan()));
                    debug_assert_eq!(exch.qty, local.qty);

                    let mut event = exch.clone();
                    event.ev |= EXCH_EVENT | LOCAL_EVENT;
                    result.push(event);
                    exch_rn += 1;
                    local_rn += 1;
                } else if exch.exch_ts == local.exch_ts && exch.local_ts < local.local_ts {
                    // Exchange event comes first
                    let mut event = exch.clone();
                    event.ev |= EXCH_EVENT;
                    result.push(event);
                    exch_rn += 1;
                } else if exch.exch_ts < local.exch_ts {
                    // Exchange event comes first
                    let mut event = exch.clone();
                    event.ev |= EXCH_EVENT;
                    result.push(event);
                    exch_rn += 1;
                } else {
                    // Local event comes first
                    let mut event = local.clone();
                    event.ev |= LOCAL_EVENT;
                    result.push(event);
                    local_rn += 1;
                }
            }
            (Some(exch), None) => {
                let mut event = exch.clone();
                event.ev |= EXCH_EVENT;
                result.push(event);
                exch_rn += 1;
            }
            (None, Some(local)) => {
                let mut event = local.clone();
                event.ev |= LOCAL_EVENT;
                result.push(event);
                local_rn += 1;
            }
            (None, None) => break,
        }
    }

    result
}

/// Validates that the order of events is correct.
///
/// # Arguments
/// * `data` - Data to validate
///
/// # Returns
/// * `Ok(())` if validation passes
/// * `Err(String)` with error message if validation fails
pub fn validate_event_order(data: &[Event]) -> Result<(), String> {
    // Check exchange events are in order
    let mut prev_exch_ts = i64::MIN;
    for event in data.iter() {
        if (event.ev & EXCH_EVENT) == EXCH_EVENT {
            if event.exch_ts < prev_exch_ts {
                return Err("exchange events are out of order".to_string());
            }
            prev_exch_ts = event.exch_ts;
        }
    }

    // Check local events are in order
    let mut prev_local_ts = i64::MIN;
    for event in data.iter() {
        if (event.ev & LOCAL_EVENT) == LOCAL_EVENT {
            if event.local_ts < prev_local_ts {
                return Err("local events are out of order".to_string());
            }
            prev_local_ts = event.local_ts;
        }
    }

    Ok(())
}

/// Creates sorted indices by exchange timestamp (stable sort)
pub fn argsort_by_exch_ts(data: &[Event]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..data.len()).collect();
    indices.sort_by(|&a, &b| data[a].exch_ts.cmp(&data[b].exch_ts));
    indices
}

/// Creates sorted indices by local timestamp (stable sort)
pub fn argsort_by_local_ts(data: &[Event]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..data.len()).collect();
    indices.sort_by(|&a, &b| data[a].local_ts.cmp(&data[b].local_ts));
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BUY_EVENT, DEPTH_EVENT, SELL_EVENT, TRADE_EVENT};

    #[test]
    fn test_correct_local_timestamp_positive_latency() {
        let mut data = vec![
            Event {
                ev: DEPTH_EVENT | BUY_EVENT,
                exch_ts: 1000,
                local_ts: 1100,
                px: 100.0,
                qty: 1.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
            Event {
                ev: DEPTH_EVENT | SELL_EVENT,
                exch_ts: 1050,
                local_ts: 1150,
                px: 101.0,
                qty: 2.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
        ];

        let min_latency = correct_local_timestamp(&mut data, 0);
        assert_eq!(min_latency, 100);
        // No change since latency is positive
        assert_eq!(data[0].local_ts, 1100);
        assert_eq!(data[1].local_ts, 1150);
    }

    #[test]
    fn test_correct_local_timestamp_negative_latency() {
        let mut data = vec![
            Event {
                ev: DEPTH_EVENT | BUY_EVENT,
                exch_ts: 1000,
                local_ts: 900, // Negative latency: 900 - 1000 = -100
                px: 100.0,
                qty: 1.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
            Event {
                ev: DEPTH_EVENT | SELL_EVENT,
                exch_ts: 1050,
                local_ts: 1000, // Latency: 1000 - 1050 = -50
                px: 101.0,
                qty: 2.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
        ];

        let min_latency = correct_local_timestamp(&mut data, 10);
        assert_eq!(min_latency, -100);
        // Offset = -(-100) + 10 = 110
        assert_eq!(data[0].local_ts, 900 + 110);
        assert_eq!(data[1].local_ts, 1000 + 110);
    }

    #[test]
    fn test_correct_event_order() {
        // Create events that need reordering
        let data = vec![
            Event {
                ev: DEPTH_EVENT | BUY_EVENT,
                exch_ts: 1000,
                local_ts: 1100,
                px: 100.0,
                qty: 1.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
            Event {
                ev: TRADE_EVENT | SELL_EVENT,
                exch_ts: 1050,
                local_ts: 1080, // local_ts < first event's local_ts in local order
                px: 101.0,
                qty: 2.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
        ];

        let exch_indices = argsort_by_exch_ts(&data);
        let local_indices = argsort_by_local_ts(&data);

        let result = correct_event_order(&data, &exch_indices, &local_indices);

        // Should have 3 events: exch[0], local[1], local[0] (since local[1] has earlier local_ts)
        assert_eq!(result.len(), 3);

        // First: exchange event (exch_ts=1000)
        assert_eq!(result[0].exch_ts, 1000);
        assert!((result[0].ev & EXCH_EVENT) == EXCH_EVENT);

        // Second: local event (local_ts=1080)
        assert_eq!(result[1].local_ts, 1080);
        assert!((result[1].ev & LOCAL_EVENT) == LOCAL_EVENT);

        // Third: local event (local_ts=1100)
        assert_eq!(result[2].local_ts, 1100);
        assert!((result[2].ev & LOCAL_EVENT) == LOCAL_EVENT);
    }

    #[test]
    fn test_validate_event_order_valid() {
        let data = vec![
            Event {
                ev: DEPTH_EVENT | BUY_EVENT | EXCH_EVENT,
                exch_ts: 1000,
                local_ts: 1100,
                px: 100.0,
                qty: 1.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
            Event {
                ev: DEPTH_EVENT | SELL_EVENT | LOCAL_EVENT,
                exch_ts: 1050,
                local_ts: 1150,
                px: 101.0,
                qty: 2.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
        ];

        assert!(validate_event_order(&data).is_ok());
    }

    #[test]
    fn test_validate_event_order_invalid_exch() {
        let data = vec![
            Event {
                ev: DEPTH_EVENT | BUY_EVENT | EXCH_EVENT,
                exch_ts: 1050, // Later
                local_ts: 1100,
                px: 100.0,
                qty: 1.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
            Event {
                ev: DEPTH_EVENT | SELL_EVENT | EXCH_EVENT,
                exch_ts: 1000, // Earlier - out of order!
                local_ts: 1150,
                px: 101.0,
                qty: 2.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            },
        ];

        assert!(validate_event_order(&data).is_err());
    }
}
