use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::warn;

use super::super::helpers::unix_now_secs;
use super::super::types::{GoogleOAuthState, RateLimitRecord};

impl GoogleOAuthState {
    pub(crate) async fn check_rate_limit(
        &self,
        bucket: &str,
        limit: u64,
        window_secs: u64,
    ) -> Result<(), Response> {
        let now = unix_now_secs();
        let key = self.key(&format!("ratelimit:{bucket}"));

        if let Some(mut conn) = self.redis_conn().await {
            let script = redis::Script::new(
                r"
                local c = redis.call('INCR', KEYS[1])
                if c == 1 then
                    redis.call('EXPIRE', KEYS[1], ARGV[1])
                end
                return c
                ",
            );
            let count: u64 = script
                .key(&key)
                .arg(window_secs as i64)
                .invoke_async(&mut conn)
                .await
                .unwrap_or(0);
            if count > limit {
                warn!(target: "axon.mcp.oauth", bucket, count, limit, "rate limit exceeded (redis)");
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "rate_limited",
                        "error_description": "too many requests",
                        "retry_after_seconds": window_secs
                    })),
                )
                    .into_response());
            }
            return Ok(());
        }

        let mut rl = self.inner.rate_limits.lock().await;
        let entry = rl.entry(bucket.to_string()).or_insert(RateLimitRecord {
            count: 0,
            reset_at_unix: now + window_secs,
        });
        if now >= entry.reset_at_unix {
            entry.count = 0;
            entry.reset_at_unix = now + window_secs;
        }
        entry.count += 1;
        if entry.count > limit {
            warn!(target: "axon.mcp.oauth", bucket, count = entry.count, limit, "rate limit exceeded (memory)");
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "rate_limited",
                    "error_description": "too many requests",
                    "retry_after_seconds": entry.reset_at_unix.saturating_sub(now)
                })),
            )
                .into_response());
        }
        Ok(())
    }
}
