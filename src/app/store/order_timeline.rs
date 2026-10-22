//! Customer-facing order progress: "Pedido recebido → Pagamento confirmado
//! → Enviado → Entregue", plus the carrier tracking link once shipped.

use topcoat::{
  Result,
  icon::{icon, iconify::iconify_icon},
  view::{View, ViewExt, attributes, component, view},
};

use super::order::tracking_url;

pub const STEP_LABELS: [&str; 4] = [
  "Pedido recebido",
  "Pagamento confirmado",
  "Enviado",
  "Entregue",
];

/// Index of the last reached step, or `None` for cancelled/refunded orders
/// (they left the happy path; the page shows the status instead).
pub fn reached_step(status: &str) -> Option<usize> {
  match status {
    "pending_payment" => Some(0),
    "paid" | "processing" => Some(1),
    "shipped" => Some(2),
    "delivered" => Some(3),
    _ => None,
  }
}

/// Timestamps are preformatted for display (`DD/MM/YYYY HH24:MI`).
#[derive(Clone, Debug, Default)]
pub struct OrderProgress {
  pub status: String,
  pub created_at: Option<String>,
  pub paid_at: Option<String>,
  pub shipped_at: Option<String>,
  pub delivered_at: Option<String>,
  pub carrier: String,
  pub tracking_code: Option<String>,
}

impl OrderProgress {
  fn step_dates(&self) -> [Option<String>; 4] {
    [
      self.created_at.clone(),
      self.paid_at.clone(),
      self.shipped_at.clone(),
      self.delivered_at.clone(),
    ]
  }

  pub fn tracking_link(&self) -> Option<String> {
    self
      .tracking_code
      .as_deref()
      .and_then(|code| tracking_url(&self.carrier, code))
  }
}

/// SQL select-list fragment matching [`OrderProgress`] for an `orders` row
/// aliased `o`. Kept here so every page formats dates the same way.
pub const PROGRESS_COLUMNS: &str = "o.status::text AS status,
  to_char(o.created_at, 'DD/MM/YYYY HH24:MI') AS created_at_fmt,
  to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS paid_at_fmt,
  to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS shipped_at_fmt,
  to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS delivered_at_fmt,
  o.carrier, o.tracking_code";

/// Row shape for [`PROGRESS_COLUMNS`].
#[derive(sqlx::FromRow)]
pub struct ProgressRow {
  pub status: String,
  pub created_at_fmt: Option<String>,
  pub paid_at_fmt: Option<String>,
  pub shipped_at_fmt: Option<String>,
  pub delivered_at_fmt: Option<String>,
  pub carrier: String,
  pub tracking_code: Option<String>,
}

impl From<ProgressRow> for OrderProgress {
  fn from(row: ProgressRow) -> Self {
    Self {
      status: row.status,
      created_at: row.created_at_fmt,
      paid_at: row.paid_at_fmt,
      shipped_at: row.shipped_at_fmt,
      delivered_at: row.delivered_at_fmt,
      carrier: row.carrier,
      tracking_code: row.tracking_code,
    }
  }
}

/// Tracking code with a link to the carrier's page when we know it.
#[component]
pub async fn tracking_line(progress: OrderProgress) -> Result<impl View> {
  let link = progress.tracking_link();
  let carrier = progress.carrier.clone();
  Ok(view! {
      if let Some(code) = progress.tracking_code {
          <p class="flex flex-wrap items-center gap-2 text-sm">
              icon(data: iconify_icon!("lucide:truck"), attrs: attributes! { class="size-4 text-muted-foreground" })
              <span class="text-muted-foreground">(carrier) ":"</span>
              <span class="font-mono">(code)</span>
              if let Some(url) = link {
                  <a href=(url) target="_blank" rel="noreferrer" class="text-primary hover:underline">
                      "Rastrear"
                  </a>
              }
          </p>
      }
  })
}

/// Vertical step list. Cancelled/refunded orders render nothing; callers
/// show the status label instead.
#[component]
pub async fn order_timeline(progress: OrderProgress) -> Result<impl View> {
  let Some(reached) = reached_step(&progress.status) else {
    return Ok(view! {}.boxed());
  };
  let dates = progress.step_dates();
  Ok(view! {
      <ol class="flex flex-col gap-3">
          for (index, label) in STEP_LABELS.iter().enumerate() {
              let done = index <= reached;
              let date = if done { dates[index].clone() } else { None };
              <li class="flex items-start gap-3">
                  if done {
                      <span class="mt-0.5 inline-flex size-5 shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground">
                          icon(data: iconify_icon!("lucide:check"), attrs: attributes! { class="size-3" })
                      </span>
                  } else {
                      <span class="mt-0.5 inline-flex size-5 shrink-0 rounded-full border border-border"></span>
                  }
                  <div class="flex flex-col">
                      <span class=(if done { "text-sm font-medium" } else { "text-sm text-muted-foreground" })>(*label)</span>
                      if let Some(at) = date {
                          <span class="text-xs text-muted-foreground">(at)</span>
                      }
                  </div>
              </li>
          }
      </ol>
      tracking_line(progress: progress.clone())
  }.boxed())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn steps_follow_status() {
    assert_eq!(reached_step("pending_payment"), Some(0));
    assert_eq!(reached_step("processing"), Some(1));
    assert_eq!(reached_step("shipped"), Some(2));
    assert_eq!(reached_step("delivered"), Some(3));
    assert_eq!(reached_step("cancelled"), None);
  }
}
