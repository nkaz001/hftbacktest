//! Streaming computation primitives with O(1) update complexity.
//!
//! This module provides fundamental building blocks for incremental statistics:
//!
//! - `RingBuffer`: Fixed-size circular buffer
//! - `WelfordRolling`: O(1) rolling mean and standard deviation
//! - `EMA`: Exponential moving average
//! - `TimeWindowQueue`: Time-based sliding window aggregation

use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Fixed-size ring buffer for O(1) sliding window operations.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RingBuffer {
    buffer: Vec<f64>,
    capacity: usize,
    head: usize,
    count: usize,
}

impl RingBuffer {
    /// Creates a new ring buffer with the specified capacity.
    #[inline]
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            capacity,
            head: 0,
            count: 0,
        }
    }

    /// Pushes a value and returns the evicted value (if buffer was full).
    #[inline]
    pub fn push(&mut self, value: f64) -> Option<f64> {
        let evicted = if self.count == self.capacity {
            Some(self.buffer[self.head])
        } else {
            self.count += 1;
            None
        };

        self.buffer[self.head] = value;
        self.head = (self.head + 1) % self.capacity;
        evicted
    }

    /// Returns true if the buffer is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.count == self.capacity
    }

    /// Returns the number of elements in the buffer.
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns true if the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Clears the buffer.
    #[inline]
    pub fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
    }

    /// Returns the capacity of the buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Gets the value at the given index (0 = oldest).
    #[inline]
    pub fn get(&self, index: usize) -> Option<f64> {
        if index >= self.count {
            return None;
        }
        let actual_index = if self.count == self.capacity {
            (self.head + index) % self.capacity
        } else {
            index
        };
        Some(self.buffer[actual_index])
    }
}

/// Welford's algorithm for O(1) rolling mean and standard deviation.
///
/// This implementation maintains running statistics that can be updated
/// in constant time as new values arrive and old values expire.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct WelfordRolling {
    window: usize,
    ring: RingBuffer,
    n: usize,
    mean: f64,
    m2: f64,
}

impl WelfordRolling {
    /// Creates a new rolling statistics calculator with the specified window size.
    #[inline]
    pub fn new(window: usize) -> Self {
        Self {
            window,
            ring: RingBuffer::new(window),
            n: 0,
            mean: 0.0,
            m2: 0.0,
        }
    }

    /// Updates the statistics with a new value. O(1) time complexity.
    #[inline]
    pub fn update(&mut self, value: f64) {
        if self.ring.is_full() {
            // Remove oldest value and add new value
            if let Some(old_value) = self.ring.push(value) {
                let old_mean = self.mean;
                self.mean += (value - old_value) / self.window as f64;
                // Update M2 using the removal/addition formula
                self.m2 += (value - old_value) * ((value - self.mean) + (old_value - old_mean));
            }
        } else {
            // Welford's online algorithm for adding values
            self.ring.push(value);
            self.n += 1;
            let delta = value - self.mean;
            self.mean += delta / self.n as f64;
            let delta2 = value - self.mean;
            self.m2 += delta * delta2;
        }
    }

    /// Returns the current mean.
    #[inline]
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// Returns the current sample variance.
    #[inline]
    pub fn variance(&self) -> f64 {
        let n = self.ring.len();
        if n < 2 {
            return 0.0;
        }
        self.m2 / (n - 1) as f64
    }

    /// Returns the current sample standard deviation (ddof=1).
    #[inline]
    pub fn std(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Returns the current population variance (ddof=0, like numpy default).
    #[inline]
    pub fn population_variance(&self) -> f64 {
        let n = self.ring.len();
        if n < 1 {
            return 0.0;
        }
        self.m2 / n as f64
    }

    /// Returns the current population standard deviation (ddof=0, like numpy default).
    #[inline]
    pub fn population_std(&self) -> f64 {
        self.population_variance().sqrt()
    }

    /// Computes the z-score for the given value using current statistics (sample std, ddof=1).
    #[inline]
    pub fn zscore(&self, value: f64) -> f64 {
        let std = self.std();
        if std < 1e-10 {
            return 0.0;
        }
        (value - self.mean) / std
    }

    /// Computes the z-score using population std (ddof=0, like numpy default).
    #[inline]
    pub fn population_zscore(&self, value: f64) -> f64 {
        let std = self.population_std();
        if std < 1e-10 {
            return 0.0;
        }
        (value - self.mean) / std
    }

    /// Returns the number of samples in the window.
    #[inline]
    pub fn count(&self) -> usize {
        self.ring.len()
    }

    /// Returns true if the window is fully populated.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.ring.is_full()
    }

    /// Resets the statistics.
    #[inline]
    pub fn reset(&mut self) {
        self.ring.clear();
        self.n = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }
}

/// Exponential Moving Average with O(1) updates.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EMA {
    alpha: f64,
    value: f64,
    initialized: bool,
}

impl EMA {
    /// Creates a new EMA with the specified span (similar to pandas ewm).
    #[inline]
    pub fn new(span: usize) -> Self {
        Self {
            alpha: 2.0 / (span as f64 + 1.0),
            value: 0.0,
            initialized: false,
        }
    }

    /// Creates a new EMA with a specific alpha (decay factor).
    #[inline]
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            alpha,
            value: 0.0,
            initialized: false,
        }
    }

    /// Updates the EMA with a new value and returns the current EMA.
    #[inline]
    pub fn update(&mut self, new_value: f64) -> f64 {
        if !self.initialized {
            self.value = new_value;
            self.initialized = true;
        } else {
            self.value = self.alpha * new_value + (1.0 - self.alpha) * self.value;
        }
        self.value
    }

    /// Returns the current EMA value.
    #[inline]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Returns true if at least one value has been processed.
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Resets the EMA.
    #[inline]
    pub fn reset(&mut self) {
        self.value = 0.0;
        self.initialized = false;
    }
}

/// Time-windowed queue for aggregating values within a time window.
///
/// This structure maintains a sliding window based on timestamps,
/// providing amortized O(1) sum/count operations.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TimeWindowQueue {
    window_ns: i64,
    entries: VecDeque<(i64, f64)>,
    sum: f64,
}

impl TimeWindowQueue {
    /// Creates a new time window queue with the specified window in nanoseconds.
    #[inline]
    pub fn new(window_ns: i64) -> Self {
        Self {
            window_ns,
            entries: VecDeque::new(),
            sum: 0.0,
        }
    }

    /// Adds a new value at the given timestamp and returns the current sum.
    #[inline]
    pub fn update(&mut self, timestamp: i64, value: f64) -> f64 {
        // Add new entry
        self.entries.push_back((timestamp, value));
        self.sum += value;

        // Remove expired entries
        let cutoff = timestamp - self.window_ns;
        while let Some(&(ts, val)) = self.entries.front() {
            if ts < cutoff {
                self.entries.pop_front();
                self.sum -= val;
            } else {
                break;
            }
        }

        self.sum
    }

    /// Returns the current sum of values in the window.
    #[inline]
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// Returns the number of entries in the window.
    #[inline]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Returns the mean of values in the window.
    #[inline]
    pub fn mean(&self) -> f64 {
        if self.entries.is_empty() {
            0.0
        } else {
            self.sum / self.entries.len() as f64
        }
    }

    /// Clears all entries.
    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.sum = 0.0;
    }

    /// Returns true if the queue is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer() {
        let mut buf = RingBuffer::new(3);

        assert!(buf.push(1.0).is_none());
        assert!(buf.push(2.0).is_none());
        assert!(buf.push(3.0).is_none());
        assert!(buf.is_full());

        assert_eq!(buf.push(4.0), Some(1.0));
        assert_eq!(buf.push(5.0), Some(2.0));
    }

    #[test]
    fn test_welford_rolling() {
        let mut welford = WelfordRolling::new(5);

        // Add values 1, 2, 3, 4, 5
        for i in 1..=5 {
            welford.update(i as f64);
        }

        // Mean should be 3.0
        assert!((welford.mean() - 3.0).abs() < 1e-10);

        // After adding 6, window is [2, 3, 4, 5, 6], mean = 4.0
        welford.update(6.0);
        assert!((welford.mean() - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_ema() {
        let mut ema = EMA::new(3);

        ema.update(1.0);
        assert_eq!(ema.value(), 1.0);

        ema.update(2.0);
        // EMA = 0.5 * 2 + 0.5 * 1 = 1.5
        assert!((ema.value() - 1.5).abs() < 1e-10);
    }

    #[test]
    fn test_time_window_queue() {
        let mut queue = TimeWindowQueue::new(100); // 100ns window

        queue.update(0, 1.0);
        queue.update(50, 2.0);
        assert_eq!(queue.sum(), 3.0);

        // At t=150, the first entry should expire
        queue.update(150, 3.0);
        assert_eq!(queue.sum(), 5.0); // 2.0 + 3.0
    }
}
