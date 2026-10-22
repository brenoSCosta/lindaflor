use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Minimal consecutive-failure circuit breaker for outbound PSP calls.
///
/// Closed: every call passes; `threshold` consecutive transient failures
/// open it. Open: calls fail fast until `cooldown` elapses. Half-open: one
/// probe call is let through — success closes the breaker, failure re-opens
/// it for another `cooldown`. Only transient failures (5xx, timeouts,
/// connection errors) count; a 4xx is the caller's fault, not an outage.
#[derive(Debug)]
pub struct CircuitBreaker {
  threshold: u32,
  cooldown: Duration,
  state: Mutex<State>,
}

#[derive(Debug, Clone, Copy)]
enum State {
  Closed { failures: u32 },
  Open { until: Instant },
  HalfOpen { probing: bool },
}

impl CircuitBreaker {
  pub fn new(threshold: u32, cooldown: Duration) -> Self {
    Self {
      threshold: threshold.max(1),
      cooldown,
      state: Mutex::new(State::Closed { failures: 0 }),
    }
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(|e| e.into_inner())
  }

  /// Whether a call may proceed now. In half-open state only the first
  /// caller gets the probe slot.
  pub fn allow(&self) -> bool {
    let mut state = self.lock();
    match *state {
      State::Closed { .. } => true,
      State::Open { until } => {
        if Instant::now() >= until {
          *state = State::HalfOpen { probing: true };
          true
        } else {
          false
        }
      }
      State::HalfOpen { probing } => {
        if probing {
          false
        } else {
          *state = State::HalfOpen { probing: true };
          true
        }
      }
    }
  }

  /// Read-only check (no probe slot consumed): used to fail checkout fast
  /// before creating an order.
  pub fn is_open(&self) -> bool {
    match *self.lock() {
      State::Closed { .. } => false,
      State::Open { until } => Instant::now() < until,
      State::HalfOpen { probing } => probing,
    }
  }

  pub fn on_success(&self) {
    *self.lock() = State::Closed { failures: 0 };
  }

  pub fn on_failure(&self) {
    let mut state = self.lock();
    let open = State::Open {
      until: Instant::now() + self.cooldown,
    };
    *state = match *state {
      State::Closed { failures } if failures + 1 >= self.threshold => open,
      State::Closed { failures } => State::Closed {
        failures: failures + 1,
      },
      State::Open { .. } | State::HalfOpen { .. } => open,
    };
  }

  /// A call that finished without a verdict on PSP health (e.g. a 4xx):
  /// release a half-open probe slot without opening or closing.
  pub fn on_neutral(&self) {
    let mut state = self.lock();
    if let State::HalfOpen { .. } = *state {
      *state = State::HalfOpen { probing: false };
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn opens_after_threshold_and_half_opens_after_cooldown() {
    let breaker = CircuitBreaker::new(2, Duration::from_millis(30));
    assert!(breaker.allow());
    breaker.on_failure();
    assert!(breaker.allow(), "one failure stays closed");
    breaker.on_failure();
    assert!(breaker.is_open());
    assert!(!breaker.allow(), "open fails fast");
    std::thread::sleep(Duration::from_millis(40));
    assert!(breaker.allow(), "cooldown elapsed: probe allowed");
    assert!(!breaker.allow(), "only one probe at a time");
    breaker.on_failure();
    assert!(!breaker.allow(), "failed probe re-opens");
    std::thread::sleep(Duration::from_millis(40));
    assert!(breaker.allow());
    breaker.on_success();
    assert!(!breaker.is_open());
    assert!(breaker.allow());
  }

  #[test]
  fn success_resets_failure_count() {
    let breaker = CircuitBreaker::new(2, Duration::from_secs(60));
    breaker.on_failure();
    breaker.on_success();
    breaker.on_failure();
    assert!(breaker.allow());
  }
}
