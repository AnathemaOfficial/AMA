use crate::errors::AmaError;
use dashmap::DashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_KEY_BYTES: usize = 128;

/// Validate Idempotency-Key header format.
pub fn validate_idempotency_key(key: &str) -> Result<Uuid, AmaError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(AmaError::BadRequest {
            message: "Idempotency-Key must be 1-128 bytes".into(),
        });
    }
    // Parse as UUID v4
    let uuid = Uuid::parse_str(key).map_err(|_| AmaError::BadRequest {
        message: "Idempotency-Key must be a valid UUID v4".into(),
    })?;
    // Verify it's actually version 4
    if uuid.get_version_num() != 4 {
        return Err(AmaError::BadRequest {
            message: "Idempotency-Key must be UUID v4".into(),
        });
    }
    Ok(uuid)
}

/// Status returned when checking the idempotency cache.
#[derive(Debug)]
pub enum IdempotencyStatus {
    /// Key not seen before — proceed with processing.
    New,
    /// Key is currently being processed by another request.
    InFlight,
    /// Key was processed before — return cached result.
    Cached(String),
    /// Cache is full and all entries within TTL — 503.
    Full,
}

#[derive(Debug)]
struct CacheEntry {
    result: Option<String>,
    created_at: Instant,
    in_flight: bool,
}

/// Idempotency cache with TTL and fail-closed overflow.
pub struct IdempotencyCache {
    entries: DashMap<Uuid, CacheEntry>,
    max_entries: usize,
    ttl: Duration,
}

impl IdempotencyCache {
    pub fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            max_entries,
            ttl,
        }
    }

    /// Check if key exists. If new, insert as in-flight.
    pub fn check_or_insert(&self, key: Uuid) -> IdempotencyStatus {
        // Check existing entry first
        if let Some(entry) = self.entries.get(&key) {
            if entry.created_at.elapsed() > self.ttl {
                // Expired — remove and treat as new
                drop(entry);
                self.entries.remove(&key);
            } else if entry.in_flight {
                return IdempotencyStatus::InFlight;
            } else if let Some(ref result) = entry.result {
                return IdempotencyStatus::Cached(result.clone());
            }
        }

        // Purge expired entries before checking capacity
        self.purge_expired();

        // Check capacity (fail-closed: 503 if full and all within TTL)
        if self.entries.len() >= self.max_entries {
            return IdempotencyStatus::Full;
        }

        // Insert as in-flight
        self.entries.insert(key, CacheEntry {
            result: None,
            created_at: Instant::now(),
            in_flight: true,
        });

        IdempotencyStatus::New
    }

    /// Mark a key as completed with its cached result.
    pub fn complete(&self, key: Uuid, result: String) {
        if let Some(mut entry) = self.entries.get_mut(&key) {
            entry.in_flight = false;
            entry.result = Some(result);
        }
    }

    /// Remove a key (e.g., on processing failure — allow retry).
    pub fn remove(&self, key: &Uuid) {
        self.entries.remove(key);
    }

    /// Purge entries beyond TTL.
    fn purge_expired(&self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            now.duration_since(entry.created_at) < self.ttl
        });
    }

    /// Current cache size (for /ama/status).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
