use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub async fn allow(&self, key: &str, limit: usize, window: Duration) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().await;
        let entry = buckets.entry(key.to_string()).or_default();
        entry.retain(|&ts| now.duration_since(ts) <= window);

        if entry.len() >= limit {
            false
        } else {
            entry.push(now);
            true
        }
    }
}
