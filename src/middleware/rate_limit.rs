use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    config: RateLimitConfig,
}

#[derive(Clone)]
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst_size: u32,
    pub cleanup_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            requests_per_second: 100,
            burst_size: 200,
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        RateLimiter {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub fn check(&self, key: &str) -> RateLimitResult {
        let mut buckets = self.buckets.write().unwrap();

        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.requests_per_second, self.config.burst_size)
        });

        if bucket.try_consume() {
            RateLimitResult::Allowed
        } else {
            RateLimitResult::Denied {
                retry_after: bucket.time_until_available(),
            }
        }
    }

    pub fn check_ip(&self, ip: &str) -> RateLimitResult {
        self.check(&format!("ip:{}", ip))
    }

    pub fn check_user(&self, user_id: &str) -> RateLimitResult {
        self.check(&format!("user:{}", user_id))
    }

    pub fn cleanup(&self) {
        let mut buckets = self.buckets.write().unwrap();
        let now = Instant::now();

        buckets
            .retain(|_, bucket| now.duration_since(bucket.last_update) < Duration::from_secs(300));
    }
}

#[derive(Clone)]
pub struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_update: Instant,
}

impl TokenBucket {
    pub fn new(requests_per_second: u32, burst_size: u32) -> Self {
        TokenBucket {
            tokens: burst_size as f64,
            max_tokens: burst_size as f64,
            refill_rate: requests_per_second as f64,
            last_update: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_update = now;
    }

    fn time_until_available(&self) -> Duration {
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            let wait_time = (1.0 - self.tokens) / self.refill_rate;
            Duration::from_secs_f64(wait_time)
        }
    }
}

#[derive(Clone, Debug)]
pub enum RateLimitResult {
    Allowed,
    Denied { retry_after: Duration },
}

impl RateLimitResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            RateLimitResult::Denied { retry_after } => Some(*retry_after),
            _ => None,
        }
    }
}

pub struct RateLimitMiddleware {
    limiter: RateLimiter,
}

impl RateLimitMiddleware {
    pub fn new(requests_per_second: u32, burst_size: u32) -> Self {
        let config = RateLimitConfig {
            requests_per_second,
            burst_size,
            ..Default::default()
        };

        RateLimitMiddleware {
            limiter: RateLimiter::new(config),
        }
    }

    pub fn check(&self, ip: &str) -> RateLimitResult {
        self.limiter.check_ip(ip)
    }

    pub fn check_request(&self, ip: &str, user_id: Option<&str>) -> RateLimitResult {
        if let Some(user) = user_id {
            return self.limiter.check_user(user);
        }
        self.limiter.check_ip(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, 5);

        for _ in 0..5 {
            assert!(bucket.try_consume());
        }

        assert!(!bucket.try_consume());
    }

    #[test]
    fn test_rate_limiter_ip() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_second: 10,
            burst_size: 5,
            ..Default::default()
        });

        let result = limiter.check_ip("192.168.1.1");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_rate_limiter_user() {
        let limiter = RateLimiter::new(RateLimitConfig {
            requests_per_second: 10,
            burst_size: 5,
            ..Default::default()
        });

        let result = limiter.check_user("user123");
        assert!(result.is_allowed());
    }

    #[test]
    fn test_middleware() {
        let middleware = RateLimitMiddleware::new(10, 5);

        let result = middleware.check("192.168.1.1");
        assert!(result.is_allowed());
    }
}
