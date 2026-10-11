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

  let subscribers = sqlx::query_as::<_, NewsletterSubscriber>(
    "SELECT id, email, name, created_at FROM newsletter_subscribers ORDER BY created_at DESC"
  )
    .fetch_all(pool)
    .await?;

  let export_csv_url = href!(export_csv).resolve(cx);
  let empty = subscribers.is_empty();

  Ok(view! {
    container(
      <div class="space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h1 class="text-2xl font-bold">"Newsletter"</h1>
            <p class="text-sm text-muted-foreground">"Gerencie inscritos da newsletter"</p>
          </div>
          <a href=(export_csv_url) class="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90" download="newsletter.csv">
            "Exportar CSV"
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
              )
            )
            table_body(
              #[key(s.id.to_string())]
              for s in subscribers {
                table_row(
                  table_cell((s.email.clone()))
                  table_cell((s.name.clone().unwrap_or_else(|| "-".to_string())))
                  table_cell((format_datetime(s.created_at)))
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

  let subscribers = sqlx::query_as::<_, NewsletterSubscriber>(
    "SELECT id, email, name, created_at FROM newsletter_subscribers ORDER BY created_at DESC"
  )
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
  use super::csv_field;

  #[test]
  fn csv_field_quotes_and_neutralises_formulas() {
    assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
    assert_eq!(csv_field("=HYPERLINK()"), "\"'=HYPERLINK()\"");
  }
}
