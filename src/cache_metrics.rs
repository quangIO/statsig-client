//! Cache metrics and monitoring for the Statsig client
//!
//! This module provides metrics collection for cache performance monitoring.

use std::sync::atomic::{AtomicU64, Ordering};

/// Cache performance metrics
#[derive(Debug, Default)]
pub struct CacheMetrics {
    /// Number of cache hits
    hits: AtomicU64,
    /// Number of cache misses
    misses: AtomicU64,
    /// Number of items inserted into cache
    inserts: AtomicU64,
    /// Number of items evicted from cache
    evictions: AtomicU64,
}

impl CacheMetrics {
    /// Create new cache metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache insertion
    pub fn record_insert(&self) {
        self.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache eviction
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the total number of cache hits
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Get the total number of cache misses
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Get the total number of cache inserts
    pub fn inserts(&self) -> u64 {
        self.inserts.load(Ordering::Relaxed)
    }

    /// Get the total number of cache evictions
    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }

    /// Get the total number of cache requests (hits + misses)
    pub fn total_requests(&self) -> u64 {
        self.hits() + self.misses()
    }

    /// Get the cache hit ratio as a percentage (0.0 to 100.0)
    pub fn hit_ratio(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            (self.hits() as f64 / total as f64) * 100.0
        }
    }

    /// Reset all metrics to zero
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.inserts.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
    }

    /// Get a summary of cache metrics
    pub fn summary(&self) -> CacheMetricsSummary {
        CacheMetricsSummary {
            hits: self.hits(),
            misses: self.misses(),
            inserts: self.inserts(),
            evictions: self.evictions(),
            total_requests: self.total_requests(),
            hit_ratio: self.hit_ratio(),
        }
    }
}

/// A summary of cache metrics for reporting
#[derive(Debug, Clone)]
pub struct CacheMetricsSummary {
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
    pub evictions: u64,
    pub total_requests: u64,
    pub hit_ratio: f64,
}

impl std::fmt::Display for CacheMetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache Metrics: {} hits, {} misses, {:.2}% hit ratio, {} inserts, {} evictions",
            self.hits, self.misses, self.hit_ratio, self.inserts, self.evictions
        )
    }
}
