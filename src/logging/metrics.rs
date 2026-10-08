use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock, PoisonError};

const BUCKETS: &[u64] = &[5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];

#[derive(Clone, Debug, Default)]
struct Histogram {
  buckets: [u64; 10],
  inf: u64,
  sum: u64,
  count: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SeriesKey {
  method: String,
  route: String,
  status: u16,
}

struct Registry {
  counters: HashMap<SeriesKey, u64>,
  histograms: HashMap<SeriesKey, Histogram>,
}

fn registry() -> &'static Mutex<Registry> {
  static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
  REGISTRY.get_or_init(|| {
    Mutex::new(Registry {
      counters: HashMap::new(),
      histograms: HashMap::new(),
    })
  })
}

fn lock_registry() -> std::sync::MutexGuard<'static, Registry> {
  registry().lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn record_request(
  method: &str,
  route: &str,
  status: u16,
  duration_ms: u64,
) {
  let key = SeriesKey {
    method: method.to_string(),
    route: route.to_string(),
    status,
  };
  let mut reg = lock_registry();
  *reg.counters.entry(key.clone()).or_insert(0) += 1;
  let hist = reg.histograms.entry(key).or_default();
  for (i, &boundary) in BUCKETS.iter().enumerate() {
    if duration_ms <= boundary {
      hist.buckets[i] += 1;
    }
  }
  hist.inf += 1;
  hist.sum = hist.sum.saturating_add(duration_ms);
  hist.count += 1;
}

fn escape_label(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for ch in value.chars() {
    match ch {
      '\\' => out.push_str("\\\\"),
      '"' => out.push_str("\\\""),
      '\n' => out.push_str("\\n"),
      _ => out.push(ch),
    }
  }
  out
}

fn labels(key: &SeriesKey) -> String {
  format!(
    "method=\"{}\",route=\"{}\",status=\"{}\"",
    escape_label(&key.method),
    escape_label(&key.route),
    key.status
  )
}

pub fn format_prometheus() -> String {
  let reg = lock_registry();
  let mut out = String::new();
  let _ = writeln!(
    out,
    "# HELP http_requests_total Total HTTP requests processed"
  );
  let _ = writeln!(out, "# TYPE http_requests_total counter");
  let mut counter_keys: Vec<&SeriesKey> = reg.counters.keys().collect();
  counter_keys.sort_by(|a, b| {
    (&a.method, &a.route, a.status).cmp(&(&b.method, &b.route, b.status))
  });
  for key in counter_keys {
    let value = reg.counters[key];
    let _ = writeln!(out, "http_requests_total{{{}}} {value}", labels(key));
  }

  let _ = writeln!(
    out,
    "# HELP http_request_duration_ms HTTP request duration in milliseconds"
  );
  let _ = writeln!(out, "# TYPE http_request_duration_ms histogram");
  let mut hist_keys: Vec<&SeriesKey> = reg.histograms.keys().collect();
  hist_keys.sort_by(|a, b| {
    (&a.method, &a.route, a.status).cmp(&(&b.method, &b.route, b.status))
  });
  for key in hist_keys {
    let hist = &reg.histograms[key];
    let base = labels(key);
    for (i, &boundary) in BUCKETS.iter().enumerate() {
      let _ = writeln!(
        out,
        "http_request_duration_ms_bucket{{{base},le=\"{boundary}\"}} {}",
        hist.buckets[i]
      );
    }
    let _ = writeln!(
      out,
      "http_request_duration_ms_bucket{{{base},le=\"+Inf\"}} {}",
      hist.inf
    );
    let _ =
      writeln!(out, "http_request_duration_ms_sum{{{base}}} {}", hist.sum);
    let _ = writeln!(
      out,
      "http_request_duration_ms_count{{{base}}} {}",
      hist.count
    );
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use uuid::Uuid;

  #[test]
  fn format_includes_recorded_series_and_buckets() {
    let route = format!("/metrics-test-{}", Uuid::now_v7());
    record_request("GET", &route, 200, 12);
    record_request("GET", &route, 200, 3000);

    let output = format_prometheus();
    let series = format!(
      "http_requests_total{{method=\"GET\",route=\"{route}\",status=\"200\"}}"
    );
    assert!(output.contains(&series), "{output}");
    assert!(output.contains("le=\"25\""), "{output}");
    assert!(output.contains("le=\"5000\""), "{output}");
    assert!(
      output.contains(&format!(
        "http_request_duration_ms_bucket{{method=\"GET\",route=\"{route}\",status=\"200\",le=\"25\"}}"
      )),
      "{output}"
    );
    assert!(
      output.contains(&format!(
        "http_request_duration_ms_bucket{{method=\"GET\",route=\"{route}\",status=\"200\",le=\"5000\"}}"
      )),
      "{output}"
    );
  }
}
