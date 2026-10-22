use http::{
  HeaderValue,
  header::{CONTENT_DISPOSITION, CONTENT_TYPE},
};
use sqlx::PgPool;
use time::OffsetDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{href, page, response::IntoResponse, route},
  view::{View, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_admin;
use crate::components::container::container;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[derive(sqlx::FromRow)]
pub struct NewsletterSubscriber {
  pub id: Uuid,
  pub email: String,
  pub name: Option<String>,
  pub created_at: OffsetDateTime,
  pub unsubscribed_at: Option<OffsetDateTime>,
}

/// Every subscriber, newest first (the admin list shows the status).
const ALL_SUBSCRIBERS_SQL: &str =
  "SELECT id, email, name, created_at, unsubscribed_at
   FROM newsletter_subscribers ORDER BY created_at DESC";

/// Export: only addresses that still consent (LGPD: unsubscribed rows are
/// kept for the record but must never leave the system as a mailing list).
const ACTIVE_SUBSCRIBERS_SQL: &str =
  "SELECT id, email, name, created_at, unsubscribed_at
   FROM newsletter_subscribers WHERE unsubscribed_at IS NULL
   ORDER BY created_at DESC";

fn status_label(unsubscribed_at: Option<OffsetDateTime>) -> String {
  match unsubscribed_at {
    None => "Ativa".to_string(),
    Some(at) => format!("Cancelada em {}", format_datetime(at)),
  }
}

fn format_datetime(dt: OffsetDateTime) -> String {
  dt.format(&time::format_description::well_known::Iso8601::DEFAULT)
    .unwrap_or_else(|_| dt.to_string())
}

/// Admin newsletter subscribers list.
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let subscribers =
    sqlx::query_as::<_, NewsletterSubscriber>(ALL_SUBSCRIBERS_SQL)
      .fetch_all(pool)
      .await?;
  let active_count = subscribers
    .iter()
    .filter(|s| s.unsubscribed_at.is_none())
    .count();
  let cancelled_count = subscribers.len() - active_count;

  let export_csv_url = href!(export_csv).resolve(cx);
  let empty = subscribers.is_empty();

  Ok(view! {
    container(
      <div class="space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h1 class="text-2xl font-bold">"Newsletter"</h1>
            <p class="text-sm text-muted-foreground">"Gerencie inscritos da newsletter"</p>
            <p class="text-sm text-muted-foreground">
              (active_count) " ativas · " (cancelled_count) " canceladas (a exportação inclui só as ativas)"
            </p>
          </div>
          <a href=(export_csv_url) class="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90" download="newsletter.csv">
            "Exportar CSV (ativas)"
          </a>
        </div>

        if empty {
          <div class="text-center py-12 text-muted-foreground">
            "Nenhum inscritor ainda"
          </div>
        } else {
          table(
            table_header(
              table_row(
                table_head("E-mail")
                table_head("Nome")
                table_head("Data de Inscrição")
                table_head("Status")
              )
            )
            table_body(
              #[key(s.id.to_string())]
              for s in subscribers {
                table_row(
                  table_cell((s.email.clone()))
                  table_cell((s.name.clone().unwrap_or_else(|| "-".to_string())))
                  table_cell((format_datetime(s.created_at)))
                  table_cell((status_label(s.unsubscribed_at)))
                )
              }
            )
          )
        }
      </div>
    )
  })
}

/// Export newsletter subscribers as CSV.
#[route(GET "/admin/newsletter/export-csv")]
pub async fn export_csv(cx: &Cx) -> Result<impl IntoResponse> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let subscribers =
    sqlx::query_as::<_, NewsletterSubscriber>(ACTIVE_SUBSCRIBERS_SQL)
      .fetch_all(pool)
      .await?;

  let mut csv = String::from("email,name,created_at\n");
  for s in subscribers {
    csv.push_str(&format!(
      "{},{},{}\n",
      csv_field(&s.email),
      csv_field(s.name.as_deref().unwrap_or("")),
      format_datetime(s.created_at)
    ));
  }

  Ok((
    [
      (
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
      ),
      (
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"newsletter.csv\""),
      ),
    ],
    csv,
  ))
}

/// Quotes a CSV field and neutralises spreadsheet formula injection.
fn csv_field(raw: &str) -> String {
  let value = if raw.starts_with(['=', '+', '-', '@']) {
    format!("'{raw}")
  } else {
    raw.to_string()
  };
  format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn export_query_skips_unsubscribed() {
    let pool = crate::test_support::isolated_pool().await;
    crate::app::newsletter::subscribe_email(&pool, "stay@example.com", None)
      .await
      .unwrap();
    crate::app::newsletter::subscribe_email(&pool, "go@example.com", None)
      .await
      .unwrap();
    crate::app::newsletter::unsubscribe_email(&pool, "go@example.com")
      .await
      .unwrap();
    let active =
      sqlx::query_as::<_, NewsletterSubscriber>(ACTIVE_SUBSCRIBERS_SQL)
        .fetch_all(&pool)
        .await
        .unwrap();
    let emails: Vec<&str> = active.iter().map(|s| s.email.as_str()).collect();
    assert_eq!(emails, ["stay@example.com"]);
    let all = sqlx::query_as::<_, NewsletterSubscriber>(ALL_SUBSCRIBERS_SQL)
      .fetch_all(&pool)
      .await
      .unwrap();
    assert_eq!(all.len(), 2);
    let gone = all.iter().find(|s| s.email == "go@example.com").unwrap();
    assert!(status_label(gone.unsubscribed_at).starts_with("Cancelada em "));
    assert_eq!(status_label(None), "Ativa");
  }

  #[test]
  fn csv_field_quotes_and_neutralises_formulas() {
    assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
    assert_eq!(csv_field("=HYPERLINK()"), "\"'=HYPERLINK()\"");
  }
}
