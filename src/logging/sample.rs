use super::log_entry::SampleReason;

pub struct SampleDecisionInput {
  pub status: u16,
  pub duration_ms: u64,
  pub rate: f64,
  pub slow_threshold_ms: u64,
  pub random: f64,
}

pub fn decide_sample(input: SampleDecisionInput) -> SampleReason {
  if input.status >= 400 {
    return SampleReason::Error;
  }
  if input.duration_ms >= input.slow_threshold_ms {
    return SampleReason::Slow;
  }
  if input.random < input.rate {
    return SampleReason::Sampled;
  }
  SampleReason::Dropped
}

#[cfg(test)]
mod tests {
  use super::*;

  fn decide(
    status: u16,
    duration_ms: u64,
    rate: f64,
    slow_threshold_ms: u64,
    random: f64,
  ) -> SampleReason {
    decide_sample(SampleDecisionInput {
      status,
      duration_ms,
      rate,
      slow_threshold_ms,
      random,
    })
  }

  #[test]
  fn error_statuses_win() {
    assert_eq!(decide(400, 1, 0.0, 1000, 0.0), SampleReason::Error);
    assert_eq!(decide(500, 1, 0.0, 1000, 0.0), SampleReason::Error);
  }

  #[test]
  fn slow_boundary_is_inclusive() {
    assert_eq!(decide(200, 1000, 0.0, 1000, 0.0), SampleReason::Slow);
    assert_eq!(decide(200, 999, 0.0, 1000, 0.0), SampleReason::Dropped);
  }

  #[test]
  fn rate_zero_drops_unremarkable() {
    assert_eq!(decide(200, 10, 0.0, 1000, 0.0), SampleReason::Dropped);
  }

  #[test]
  fn rate_one_keeps_unremarkable() {
    assert_eq!(decide(200, 10, 1.0, 1000, 0.5), SampleReason::Sampled);
  }
}
