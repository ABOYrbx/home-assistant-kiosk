use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ReconnectBackoff {
    base: Duration,
    cap: Duration,
    attempt: u32,
}

impl ReconnectBackoff {
    pub fn new() -> Self {
        Self {
            base: Duration::from_secs(1),
            cap: Duration::from_secs(60),
            attempt: 0,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let exp = self.base.saturating_mul(1u32 << self.attempt.min(6));
        let capped = exp.min(self.cap);
        self.attempt = self.attempt.saturating_add(1);
        // full jitter
        let millis = capped.as_millis() as u64;
        let half = millis / 2;
        let jitter = if millis == 0 {
            0
        } else {
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(0))
                % (half.max(1))
        };
        Duration::from_millis(half + jitter)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        Self::new()
    }
}
