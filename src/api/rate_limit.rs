use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    body::Body,
};
use redis::AsyncCommands;
use std::sync::Arc;
use tracing::warn;

/// Rate limiter configuration
pub struct RateLimiter {
    pub redis: redis::aio::ConnectionManager,
    /// Max requests per window
    pub max_requests: u32,
    /// Window size in seconds
    pub window_seconds: u32,
}

impl RateLimiter {
    pub fn new(redis: redis::aio::ConnectionManager, max_requests: u32, window_seconds: u32) -> Self {
        Self {
            redis,
            max_requests,
            window_seconds,
        }
    }

    /// Check if request is allowed, return remaining requests
    pub async fn check(&self, key: &str) -> Result<u32, ()> {
        let mut conn = self.redis.clone();
        let rate_key = format!("rate:{}", key);

        // Get current count
        let count: u32 = conn.get(&rate_key).await.unwrap_or(0);

        if count >= self.max_requests {
            return Err(());
        }

        // Increment and set expiry
        let _: () = redis::pipe()
            .atomic()
            .incr(&rate_key, 1)
            .expire(&rate_key, self.window_seconds as i64)
            .query_async(&mut conn)
            .await
            .unwrap_or_default();

        Ok(self.max_requests - count - 1)
    }
}

/// Rate limiting middleware for auth endpoints
pub async fn rate_limit_auth(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Use IP address as rate limit key
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown");

    match limiter.check(&format!("auth:{}", ip)).await {
        Ok(remaining) => {
            let mut response = next.run(request).await;
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            Ok(response)
        }
        Err(_) => {
            warn!("Rate limit exceeded for IP: {}", ip);
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_rate_limit_config() {
        // Test that rate limit values are reasonable
        let max_requests: u32 = 10;
        let window_seconds: u32 = 60;
        
        assert!(max_requests > 0);
        assert!(window_seconds > 0);
        assert!(max_requests <= 1000); // Reasonable upper bound
        assert!(window_seconds <= 3600); // Max 1 hour window
    }
}
