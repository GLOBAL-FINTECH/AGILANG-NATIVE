use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub allowed: bool,
    pub remaining: u32,
    pub retry_after_seconds: u64,
}

pub struct RateLimiter {
    max_attempts: u32,
    window_duration: Duration,
    attempts: Mutex<HashMap<String, Vec<SystemTime>>>,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_duration: Duration) -> Self {
        Self {
            max_attempts,
            window_duration,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    pub fn check_and_increment(&self, key: &str) -> RateLimitStatus {
        let mut guard = self.attempts.lock().unwrap();
        let now = SystemTime::now();
        let cutoff = now - self.window_duration;

        let timestamps = guard.entry(key.to_string()).or_default();
        timestamps.retain(|&t| t > cutoff);

        if timestamps.len() >= self.max_attempts as usize {
            let oldest = timestamps.first().copied().unwrap_or(now);
            let elapsed = now.duration_since(oldest).unwrap_or_default();
            let retry_after = self
                .window_duration
                .checked_sub(elapsed)
                .unwrap_or(Duration::from_secs(1))
                .as_secs();

            RateLimitStatus {
                allowed: false,
                remaining: 0,
                retry_after_seconds: retry_after,
            }
        } else {
            timestamps.push(now);
            let remaining = self.max_attempts - (timestamps.len() as u32);
            RateLimitStatus {
                allowed: true,
                remaining,
                retry_after_seconds: 0,
            }
        }
    }

    pub fn reset(&self, key: &str) {
        let mut guard = self.attempts.lock().unwrap();
        guard.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_exceeded() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        let key = "user_ip_127_0_0_1";

        assert!(limiter.check_and_increment(key).allowed);
        assert!(limiter.check_and_increment(key).allowed);
        assert!(limiter.check_and_increment(key).allowed);

        let fourth = limiter.check_and_increment(key);
        assert!(!fourth.allowed);
        assert_eq!(fourth.remaining, 0);

        limiter.reset(key);
        assert!(limiter.check_and_increment(key).allowed);
    }
}
