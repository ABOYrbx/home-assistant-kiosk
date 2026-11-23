use std::time::{Duration, Instant};

/// Throttles slider updates so a drag sends at most one service call per
/// interval, with the final value sent on release.
#[derive(Debug, Clone)]
pub struct SliderDebounce {
    interval: Duration,
    last_sent: Option<Instant>,
    last_sent_value: Option<u32>,
    pending: Option<u32>,
}

impl SliderDebounce {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_sent: None,
            last_sent_value: None,
            pending: None,
        }
    }

    pub fn on_change(&mut self, value: u32, now: Instant) -> Option<u32> {
        if self.last_sent_value == Some(value) && self.pending.is_none() {
            return None;
        }
        match self.last_sent {
            Some(last) if now.saturating_duration_since(last) < self.interval => {
                self.pending = Some(value);
                None
            }
            _ => Some(self.emit(value, now)),
        }
    }

    pub fn on_release(&mut self, value: u32, now: Instant) -> Option<u32> {
        if self.last_sent_value == Some(value) && self.pending.is_none() {
            return None;
        }
        Some(self.emit(value, now))
    }

    pub fn tick(&mut self, now: Instant) -> Option<u32> {
        let pending = self.pending?;
        let due = self
            .last_sent
            .map(|last| now.saturating_duration_since(last) >= self.interval)
            .unwrap_or(true);
        if !due {
            return None;
        }
        Some(self.emit(pending, now))
    }

    pub fn reset(&mut self) {
        self.last_sent = None;
        self.last_sent_value = None;
        self.pending = None;
    }

    fn emit(&mut self, value: u32, now: Instant) -> u32 {
        self.last_sent = Some(now);
        self.last_sent_value = Some(value);
        self.pending = None;
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(100);

    #[test]
    fn sends_first_change_immediately() {
        let now = Instant::now();
        let mut debounce = SliderDebounce::new(INTERVAL);
        assert_eq!(debounce.on_change(40, now), Some(40));
    }

    #[test]
    fn throttles_rapid_changes_and_sends_the_latest_on_release() {
        let now = Instant::now();
        let mut debounce = SliderDebounce::new(INTERVAL);
        assert_eq!(debounce.on_change(40, now), Some(40));
        assert_eq!(
            debounce.on_change(50, now + Duration::from_millis(40)),
            None
        );
        assert_eq!(
            debounce.on_change(60, now + Duration::from_millis(70)),
            None
        );
        assert_eq!(
            debounce.on_release(70, now + Duration::from_millis(90)),
            Some(70)
        );
    }

    #[test]
    fn trailing_tick_does_not_exceed_the_interval() {
        let now = Instant::now();
        let mut debounce = SliderDebounce::new(INTERVAL);
        assert_eq!(debounce.on_change(10, now), Some(10));
        assert_eq!(
            debounce.on_change(20, now + Duration::from_millis(50)),
            None
        );
        assert_eq!(debounce.tick(now + Duration::from_millis(80)), None);
        assert_eq!(debounce.tick(now + Duration::from_millis(100)), Some(20));
        assert_eq!(debounce.tick(now + Duration::from_millis(150)), None);
    }

    #[test]
    fn release_does_not_duplicate_the_last_value() {
        let now = Instant::now();
        let mut debounce = SliderDebounce::new(INTERVAL);
        assert_eq!(debounce.on_change(80, now), Some(80));
        assert_eq!(
            debounce.on_release(80, now + Duration::from_millis(50)),
            None
        );
    }

    #[test]
    fn reset_allows_the_next_change_to_send() {
        let now = Instant::now();
        let mut debounce = SliderDebounce::new(INTERVAL);
        assert_eq!(debounce.on_change(30, now), Some(30));
        debounce.reset();
        assert_eq!(
            debounce.on_change(30, now + Duration::from_millis(10)),
            Some(30)
        );
    }
}
