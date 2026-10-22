//! Product reviews: verified-buyer submission, moderation queries and the
//! storefront components (stars, rating summary, review list + form).
//!
//! Eligibility (who may review a product): the signed-in user owns an order
//! that contains a variant of the product and that order is either
//! `delivered`, or `shipped` at least [`SHIPPED_GRACE_DAYS`] days ago.
//! `delivered` is the honest signal, but it is set by hand in the admin and
//! is often skipped once the parcel leaves; the shipped fallback (longest
//! advertised delivery window, 15 business days, rounded up to three weeks)
//! keeps reviews possible without opening them to paid-but-unshipped orders
//! that may still be cancelled or refunded.
//!
//! Reviews start `pending` and only `approved` ones are public (list and
//! aggregates). Moderation lives in `src/app/admin/avaliacoes.rs`.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context, memoize},
  icon::{icon, iconify::iconify_icon},
  router::{content::Form, href, response::Response, route},
  runtime::{Event, shard, signal},
  view::{View, attributes, component, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::optional_user;
use crate::audit::AuditEvent;
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_header, card_title,
};
use crate::components::input::input;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};
use crate::components::select::select;
use crate::components::separator::separator;
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};

pub const TITLE_MAX: usize = 80;
pub const BODY_MAX: usize = 2000;
/// Days after `shipped_at` after which a shipped order counts as received.
pub const SHIPPED_GRACE_DAYS: i32 = 21;
/// Approved reviews per page on the product page.
pub const REVIEWS_PAGE_SIZE: i64 = 5;

/// Review submissions per user per [`REVIEW_SUBMIT_WINDOW`]. Keyed by user
/// id (submitting requires a session); env
/// `RATE_LIMIT_REVIEW_SUBMIT_PER_WINDOW`.
pub const SCOPE_REVIEW_SUBMIT: &str = "review:submit";
pub const DEFAULT_REVIEWS_PER_WINDOW: u32 = 5;
pub const REVIEW_SUBMIT_WINDOW: std::time::Duration =
  std::time::Duration::from_secs(60 * 60);

/// `orders o` predicate: owned by `$1` and received (see module docs).
const RECEIVED_ORDER_SQL: &str = "o.user_id = $1
  AND (o.status = 'delivered'
       OR (o.status = 'shipped'
           AND o.shipped_at <= now() - make_interval(days => 21)))";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
  Pequeno,
  Ideal,
  Grande,
}

impl Fit {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw.trim() {
      "pequeno" => Some(Self::Pequeno),
      "ideal" => Some(Self::Ideal),
      "grande" => Some(Self::Grande),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Pequeno => "pequeno",
      Self::Ideal => "ideal",
      Self::Grande => "grande",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Pequeno => "Veste pequeno",
      Self::Ideal => "Tamanho ideal",
      Self::Grande => "Veste grande",
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
  #[error("{0}")]
  Invalid(&'static str),
  #[error("not a verified buyer of this product")]
  NotEligible,
  #[error("already reviewed")]
  AlreadyReviewed,
  #[error(transparent)]
  Db(#[from] sqlx::Error),
}

impl ReviewError {
  /// pt-BR message for the customer.
  pub fn message(&self) -> &'static str {
    match self {
      Self::Invalid(message) => message,
      Self::NotEligible => "Só quem recebeu este produto pode avaliá-lo.",
      Self::AlreadyReviewed => "Você já avaliou este produto.",
      Self::Db(_) => "Não foi possível salvar sua avaliação.",
    }
  }
}

/// A validated submission.
#[derive(Debug)]
pub struct NewReview {
  pub rating: i16,
  pub title: String,
  pub body: String,
  pub fit: Option<Fit>,
}

impl NewReview {
  /// Trim and validate raw form fields.
  pub fn parse(
    rating: &str,
    title: &str,
    body: &str,
    fit: Option<&str>,
  ) -> Result<Self, ReviewError> {
    let rating: i16 = rating
      .trim()
      .parse()
      .ok()
      .filter(|r| (1..=5).contains(r))
      .ok_or(ReviewError::Invalid("Escolha uma nota de 1 a 5."))?;
    let title = title.trim();
    if title.is_empty() || title.chars().count() > TITLE_MAX {
      return Err(ReviewError::Invalid(
        "O título deve ter entre 1 e 80 caracteres.",
      ));
    }
    let body = body.trim();
    if body.is_empty() || body.chars().count() > BODY_MAX {
      return Err(ReviewError::Invalid(
        "O comentário deve ter entre 1 e 2000 caracteres.",
      ));
    }
    let fit = match fit.map(str::trim).filter(|f| !f.is_empty()) {
      None => None,
      Some(raw) => {
        Some(Fit::parse(raw).ok_or(ReviewError::Invalid("Caimento inválido."))?)
      }
    };
    Ok(Self {
      rating,
      title: title.to_string(),
      body: body.to_string(),
      fit,
    })
  }
}

/// The user's most recent received order containing a variant of
/// `product_id`, if any.
pub async fn eligible_order(
  pool: &PgPool,
  user_id: Uuid,
  product_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
  sqlx::query_scalar(&format!(
    "SELECT o.id FROM orders o
     WHERE {RECEIVED_ORDER_SQL}
       AND EXISTS (SELECT 1 FROM order_items oi
                   JOIN product_variants v ON v.id = oi.variant_id
                   WHERE oi.order_id = o.id AND v.product_id = $2)
     ORDER BY o.created_at DESC LIMIT 1"
  ))
  .bind(user_id)
  .bind(product_id)
  .fetch_optional(pool)
  .await
}

/// Status of the user's review of the product (`pending`/`approved`/
/// `rejected`), if they wrote one.
pub async fn own_review_status(
  pool: &PgPool,
  user_id: Uuid,
  product_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT status::text FROM product_reviews
     WHERE user_id = $1 AND product_id = $2",
  )
  .bind(user_id)
  .bind(product_id)
  .fetch_optional(pool)
  .await
}

/// Store a verified buyer's review as `pending`. One per user and product
/// (the unique constraint settles concurrent submits).
pub async fn submit_review(
  pool: &PgPool,
  user_id: Uuid,
  product_id: Uuid,
  review: &NewReview,
) -> Result<Uuid, ReviewError> {
  if own_review_status(pool, user_id, product_id)
    .await?
    .is_some()
  {
    return Err(ReviewError::AlreadyReviewed);
  }
  let order_id = eligible_order(pool, user_id, product_id)
    .await?
    .ok_or(ReviewError::NotEligible)?;
  let id: Option<Uuid> = sqlx::query_scalar(
    "INSERT INTO product_reviews
       (id, product_id, user_id, order_id, rating, title, body, fit)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::review_fit)
     ON CONFLICT (user_id, product_id) DO NOTHING
     RETURNING id",
  )
  .bind(Uuid::now_v7())
  .bind(product_id)
  .bind(user_id)
  .bind(order_id)
  .bind(review.rating)
  .bind(&review.title)
  .bind(&review.body)
  .bind(review.fit.map(Fit::as_str))
  .fetch_optional(pool)
  .await?;
  id.ok_or(ReviewError::AlreadyReviewed)
}

/// Approved-review aggregates for one product.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RatingSummary {
  pub average: f64,
  pub count: i64,
  pub fit_pequeno: i64,
  pub fit_ideal: i64,
  pub fit_grande: i64,
}

impl RatingSummary {
  /// Most reported fit and its share of fit answers, e.g. `("Tamanho
  /// ideal", 80)`.
  pub fn fit_majority(&self) -> Option<(&'static str, i64)> {
    let total = self.fit_pequeno + self.fit_ideal + self.fit_grande;
    if total == 0 {
      return None;
    }
    let (fit, n) = [
      (Fit::Ideal, self.fit_ideal),
      (Fit::Pequeno, self.fit_pequeno),
      (Fit::Grande, self.fit_grande),
    ]
    .into_iter()
    .max_by_key(|(_, n)| *n)?;
    Some((fit.label(), (n * 100 + total / 2) / total))
  }
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
  average: Option<f64>,
  count: i64,
  fit_pequeno: i64,
  fit_ideal: i64,
  fit_grande: i64,
}

pub async fn rating_summary(
  pool: &PgPool,
  product_id: Uuid,
) -> Result<RatingSummary, sqlx::Error> {
  let row = sqlx::query_as::<_, SummaryRow>(
    "SELECT AVG(rating)::float8 AS average,
            COUNT(*) AS count,
            COUNT(*) FILTER (WHERE fit = 'pequeno') AS fit_pequeno,
            COUNT(*) FILTER (WHERE fit = 'ideal') AS fit_ideal,
            COUNT(*) FILTER (WHERE fit = 'grande') AS fit_grande
     FROM product_reviews
     WHERE product_id = $1 AND status = 'approved'",
  )
  .bind(product_id)
  .fetch_one(pool)
  .await?;
  Ok(RatingSummary {
    average: row.average.unwrap_or(0.0),
    count: row.count,
    fit_pequeno: row.fit_pequeno,
    fit_ideal: row.fit_ideal,
    fit_grande: row.fit_grande,
  })
}

/// schema.org `AggregateRating` inputs for a product's JSON-LD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AggregateRating {
  /// Mean of approved ratings, rounded to one decimal.
  pub rating_average: f64,
  pub review_count: i64,
}

/// `None` when the product has no approved review (search engines reject
/// an `AggregateRating` with zero reviews).
pub async fn aggregate_rating(
  pool: &PgPool,
  product_id: Uuid,
) -> Result<Option<AggregateRating>, sqlx::Error> {
  let summary = rating_summary(pool, product_id).await?;
  Ok((summary.count > 0).then(|| AggregateRating {
    rating_average: (summary.average * 10.0).round() / 10.0,
    review_count: summary.count,
  }))
}

/// `(average, count)` of approved reviews for every reviewed product.
pub async fn all_ratings(
  pool: &PgPool,
) -> Result<HashMap<Uuid, (f64, i64)>, sqlx::Error> {
  let rows: Vec<(Uuid, f64, i64)> = sqlx::query_as(
    "SELECT product_id, AVG(rating)::float8, COUNT(*)
     FROM product_reviews WHERE status = 'approved'
     GROUP BY product_id",
  )
  .fetch_all(pool)
  .await?;
  Ok(
    rows
      .into_iter()
      .map(|(id, avg, count)| (id, (avg, count)))
      .collect(),
  )
}

/// Approved ratings, loaded once per request so every product card in a
/// grid shares one aggregate query. Empty on error.
#[memoize]
pub async fn product_ratings(cx: &Cx) -> HashMap<Uuid, (f64, i64)> {
  all_ratings(app_context::<PgPool>(cx))
    .await
    .unwrap_or_else(|err| {
      tracing::warn!(error = %err, "product ratings unavailable");
      HashMap::new()
    })
}

#[derive(sqlx::FromRow)]
pub struct PublicReview {
  pub id: Uuid,
  pub rating: i16,
  pub title: String,
  pub body: String,
  pub fit: Option<String>,
  pub author_name: String,
  pub created_at: OffsetDateTime,
}

/// "Maria Souza" -> "Maria S." (first name + last initial).
pub fn public_author(name: &str) -> String {
  let mut parts = name.split_whitespace();
  let Some(first) = parts.next() else {
    return "Cliente".to_string();
  };
  match parts.last().and_then(|last| last.chars().next()) {
    Some(initial) => format!("{first} {}.", initial.to_uppercase()),
    None => first.to_string(),
  }
}

/// Approved reviews, newest first.
pub async fn list_approved(
  pool: &PgPool,
  product_id: Uuid,
  limit: i64,
  offset: i64,
) -> Result<Vec<PublicReview>, sqlx::Error> {
  sqlx::query_as::<_, PublicReview>(
    "SELECT r.id, r.rating, r.title, r.body, r.fit::text AS fit,
            u.name AS author_name, r.created_at
     FROM product_reviews r
     JOIN users u ON u.id = r.user_id
     WHERE r.product_id = $1 AND r.status = 'approved'
     ORDER BY r.created_at DESC
     LIMIT $2 OFFSET $3",
  )
  .bind(product_id)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
}

/// A product the user received in an order and has not reviewed yet.
#[derive(sqlx::FromRow)]
pub struct ReviewableProduct {
  pub product_id: Uuid,
  pub name: String,
  pub slug: String,
}

/// Products of `order_id` the user may still review (order received,
/// product active, no review yet).
pub async fn reviewable_products_for_order(
  pool: &PgPool,
  user_id: Uuid,
  order_id: Uuid,
) -> Result<Vec<ReviewableProduct>, sqlx::Error> {
  sqlx::query_as::<_, ReviewableProduct>(&format!(
    "SELECT DISTINCT p.id AS product_id, p.name, p.slug
     FROM orders o
     JOIN order_items oi ON oi.order_id = o.id
     JOIN product_variants v ON v.id = oi.variant_id
     JOIN products p ON p.id = v.product_id
     WHERE o.id = $2 AND {RECEIVED_ORDER_SQL} AND p.active
       AND NOT EXISTS (SELECT 1 FROM product_reviews r
                       WHERE r.user_id = $1 AND r.product_id = p.id)
     ORDER BY p.name"
  ))
  .bind(user_id)
  .bind(order_id)
  .fetch_all(pool)
  .await
}

/// The user's received orders that still have something to review.
pub async fn reviewable_order_ids(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<HashSet<Uuid>, sqlx::Error> {
  let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
    "SELECT DISTINCT o.id
     FROM orders o
     JOIN order_items oi ON oi.order_id = o.id
     JOIN product_variants v ON v.id = oi.variant_id
     JOIN products p ON p.id = v.product_id
     WHERE {RECEIVED_ORDER_SQL} AND p.active
       AND NOT EXISTS (SELECT 1 FROM product_reviews r
                       WHERE r.user_id = $1 AND r.product_id = p.id)"
  ))
  .bind(user_id)
  .fetch_all(pool)
  .await?;
  Ok(ids.into_iter().collect())
}

// --- Moderation -----------------------------------------------------------

#[derive(sqlx::FromRow)]
pub struct AdminReview {
  pub id: Uuid,
  pub product_name: String,
  pub product_slug: String,
  pub author_name: String,
  pub author_email: String,
  pub rating: i16,
  pub title: String,
  pub body: String,
  pub fit: Option<String>,
  pub status: String,
  pub created_at: OffsetDateTime,
}

/// Moderation queue: pending first (oldest first, FIFO), then the most
/// recently moderated.
pub async fn list_for_admin(
  pool: &PgPool,
  limit: i64,
) -> Result<Vec<AdminReview>, sqlx::Error> {
  sqlx::query_as::<_, AdminReview>(
    "SELECT r.id, p.name AS product_name, p.slug AS product_slug,
            u.name AS author_name, u.email AS author_email,
            r.rating, r.title, r.body, r.fit::text AS fit,
            r.status::text AS status, r.created_at
     FROM product_reviews r
     JOIN products p ON p.id = r.product_id
     JOIN users u ON u.id = r.user_id
     ORDER BY (r.status = 'pending') DESC,
              CASE WHEN r.status = 'pending' THEN r.created_at END ASC,
              r.moderated_at DESC NULLS LAST, r.created_at DESC
     LIMIT $1",
  )
  .bind(limit)
  .fetch_all(pool)
  .await
}

pub async fn count_pending(pool: &PgPool) -> Result<i64, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM product_reviews WHERE status = 'pending'",
  )
  .fetch_one(pool)
  .await
}

/// Approve (`true`) or reject a review and audit it in one transaction.
/// Returns `false` when the review does not exist.
pub async fn moderate_review(
  pool: &PgPool,
  review_id: Uuid,
  approve: bool,
  actor_id: Uuid,
  actor_name: &str,
) -> Result<bool, sqlx::Error> {
  let next = if approve { "approved" } else { "rejected" };
  let mut tx = pool.begin().await?;
  let previous: Option<(String, Uuid)> = sqlx::query_as(
    "SELECT status::text, product_id FROM product_reviews
     WHERE id = $1 FOR UPDATE",
  )
  .bind(review_id)
  .fetch_optional(&mut *tx)
  .await?;
  let Some((from, product_id)) = previous else {
    return Ok(false);
  };
  sqlx::query(
    "UPDATE product_reviews
     SET status = $2::review_status, moderated_at = now(), moderated_by = $3
     WHERE id = $1",
  )
  .bind(review_id)
  .bind(next)
  .bind(actor_id)
  .execute(&mut *tx)
  .await?;
  crate::audit::record_audit_event(
    &mut *tx,
    AuditEvent {
      aggregate_type: "product_review",
      aggregate_id: &review_id.to_string(),
      entity_type: "product_review",
      entity_id: review_id,
      actor_id: Some(actor_id),
      actor_name: Some(actor_name),
      metadata: serde_json::json!({
        "action": if approve { "review.approve" } else { "review.reject" },
        "from": from,
        "to": next,
        "product_id": product_id,
      }),
    },
  )
  .await?;
  tx.commit().await?;
  Ok(true)
}

// --- Submission route -----------------------------------------------------

#[derive(Deserialize)]
pub struct ReviewForm {
  product_id: String,
  #[serde(default)]
  rating: String,
  #[serde(default)]
  title: String,
  #[serde(default)]
  body: String,
  fit: Option<String>,
}

/// Verified-buyer review submission (no-JS form POST).
#[route(POST "/avaliacoes/enviar")]
pub async fn submit(cx: &Cx, Form(form): Form<ReviewForm>) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let product = match Uuid::parse_str(form.product_id.trim()) {
    Ok(id) => {
      sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, slug FROM products WHERE id = $1 AND active",
      )
      .bind(id)
      .fetch_optional(pool)
      .await?
    }
    Err(_) => None,
  };
  let Some((product_id, slug)) = product else {
    set_toast(cx, Toast::error("Produto inválido."));
    return toast_redirect(cx, href!(crate::app::produtos::page).resolve(cx));
  };
  let product_url = href!(
    crate::app::produtos::slug::page,
    crate::app::produtos::slug::Slug(slug)
  )
  .resolve(cx);
  let back = format!("{product_url}#avaliacoes");

  let Some(user) = optional_user(cx).await? else {
    let login = href!(crate::app::login::page)
      .query([("next", product_url.as_str())])
      .resolve(cx);
    return toast_redirect(cx, login);
  };
  if !crate::rate_limit::check_ip_rate_window(
    SCOPE_REVIEW_SUBMIT,
    &user.user.id.to_string(),
    crate::rate_limit::window_limit_for(
      SCOPE_REVIEW_SUBMIT,
      DEFAULT_REVIEWS_PER_WINDOW,
    ),
    REVIEW_SUBMIT_WINDOW,
  ) {
    set_toast(
      cx,
      Toast::error("Muitas avaliações em pouco tempo. Tente mais tarde."),
    );
    return toast_redirect(cx, back);
  }

  let result = match NewReview::parse(
    &form.rating,
    &form.title,
    &form.body,
    form.fit.as_deref(),
  ) {
    Ok(review) => submit_review(pool, user.user.id, product_id, &review).await,
    Err(error) => Err(error),
  };
  match result {
    Ok(_) => set_toast(
      cx,
      Toast::success("Obrigada pela avaliação!")
        .description("Ela aparece na página depois da moderação."),
    ),
    Err(ReviewError::Db(error)) => return Err(error.into()),
    Err(error) => set_toast(cx, Toast::error(error.message())),
  }
  toast_redirect(cx, back)
}

// --- Components -------------------------------------------------------------

/// Five stars, `rating` rounded to the nearest whole star.
#[component]
pub async fn star_rating(
  rating: f64,
  #[default] small: bool,
) -> Result<impl View> {
  let filled = rating.round().clamp(0.0, 5.0) as usize;
  let label = format!("Nota {:.1} de 5", rating).replace('.', ",");
  let size = if small { "size-3.5" } else { "size-4" };
  Ok(view! {
      <span class="inline-flex items-center gap-0.5" role="img" aria-label=(label)>
          for index in 0..5 {
              if index < filled {
                  icon(data: iconify_icon!("lucide:star"), attrs: attributes! { class=(format!("{size} text-amber-500 fill-current")) })
              } else {
                  icon(data: iconify_icon!("lucide:star"), attrs: attributes! { class=(format!("{size} text-muted-foreground/40")) })
              }
          }
      </span>
  })
}

/// "4,5" with a decimal comma.
fn format_average(average: f64) -> String {
  format!("{average:.1}").replace('.', ",")
}

fn reviews_count_label(count: i64) -> String {
  if count == 1 {
    "1 avaliação".to_string()
  } else {
    format!("{count} avaliações")
  }
}

/// Compact stars for product cards; renders nothing until the product has
/// an approved review. Reads [`product_ratings`] (one query per request).
#[component]
pub async fn card_rating(cx: &Cx, product_id: Uuid) -> Result<impl View> {
  let rating = product_ratings(cx).await.get(&product_id).copied();
  Ok(view! {
      if let Some((average, count)) = rating {
          <div class="flex items-center gap-1 text-xs text-muted-foreground">
              star_rating(rating: average, small: true)
              <span>"(" (count) ")"</span>
          </div>
      }
  })
}

/// Average, count and fit summary for the product header. Links to the
/// review section.
#[component]
pub async fn rating_summary_line(
  cx: &Cx,
  product_id: Uuid,
) -> Result<impl View> {
  let summary = rating_summary(app_context::<PgPool>(cx), product_id).await?;
  let fit = summary.fit_majority();
  Ok(view! {
      if summary.count > 0 {
          <a href="#avaliacoes" class="flex flex-wrap items-center gap-2 text-sm text-muted-foreground hover:text-foreground">
              star_rating(rating: summary.average)
              <span class="font-medium text-foreground">(format_average(summary.average))</span>
              <span>"(" (reviews_count_label(summary.count)) ")"</span>
              if let Some((label, pct)) = fit {
                  <span>"· " (label) " para " (pct) "%"</span>
              }
          </a>
      }
  })
}

/// Paginated approved reviews. `product_id` comes back from the client on
/// re-render, so it is parsed as input; only approved rows are ever read.
#[shard("/avaliacoes/lista")]
async fn review_list(
  cx: &Cx,
  product_id: String,
  total: i64,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let current_page = signal(cx, || 1_i64);
  let product_id = Uuid::parse_str(product_id.trim()).unwrap_or_default();
  let total_pages =
    ((total.max(0) + REVIEWS_PAGE_SIZE - 1) / REVIEWS_PAGE_SIZE).max(1);
  let page_num = current_page.get().clamp(1, total_pages);
  let reviews = list_approved(
    pool,
    product_id,
    REVIEWS_PAGE_SIZE,
    (page_num - 1) * REVIEWS_PAGE_SIZE,
  )
  .await?;
  let has_prev = page_num > 1;
  let has_next = page_num < total_pages;
  let prev_page = page_num - 1;
  let next_page = page_num + 1;
  let page_label = format!("Página {page_num} de {total_pages}");

  Ok(view! {
      <ul class="divide-y divide-border">
          #[key(review.id.to_string())]
          for review in reviews {
              let fit = review.fit.as_deref().and_then(Fit::parse);
              <li class="space-y-2 py-4">
                  <div class="flex flex-wrap items-center gap-2">
                      star_rating(rating: f64::from(review.rating), small: true)
                      <h3 class="font-medium">(review.title)</h3>
                  </div>
                  <p class="whitespace-pre-line text-sm text-muted-foreground">(review.body)</p>
                  <p class="text-xs text-muted-foreground">
                      (public_author(&review.author_name))
                      " · "
                      (crate::app::store::queries::format_datetime_br(review.created_at).chars().take(10).collect::<String>())
                      if let Some(fit) = fit {
                          " · " (fit.label())
                      }
                      " · Compra verificada"
                  </p>
              </li>
          }
      </ul>
      if total_pages > 1 {
          pagination(
              pagination_content(
                  if has_prev {
                      pagination_item(
                          <button
                              type="button"
                              class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                              @click=$(|_e: Event| current_page.set(prev_page))
                          >
                              "Anteriores"
                          </button>
                      )
                  }
                  pagination_item(
                      <span class="px-3 text-sm text-muted-foreground">(page_label)</span>
                  )
                  if has_next {
                      pagination_item(
                          <button
                              type="button"
                              class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                              @click=$(|_e: Event| current_page.set(next_page))
                          >
                              "Mais avaliações"
                          </button>
                      )
                  }
              )
          )
      }
  })
}

/// What the current visitor can do in the review section.
enum FormState {
  /// Guest, or signed in without a received order of this product.
  Hidden,
  Eligible,
  /// Already reviewed; carries the review status.
  Reviewed(String),
}

/// Review section for the product page: summary, approved list and, for
/// verified buyers, the submission form.
#[component]
pub async fn product_reviews(cx: &Cx, product_id: Uuid) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let summary = rating_summary(pool, product_id).await?;
  let state = match optional_user(cx).await? {
    None => FormState::Hidden,
    Some(user) => {
      match own_review_status(pool, user.user.id, product_id).await? {
        Some(status) => FormState::Reviewed(status),
        None
          if eligible_order(pool, user.user.id, product_id)
            .await?
            .is_some() =>
        {
          FormState::Eligible
        }
        None => FormState::Hidden,
      }
    }
  };
  let reviewed_note = match &state {
    FormState::Reviewed(status) => Some(match status.as_str() {
      "approved" => "Sua avaliação está publicada. Obrigada!",
      "rejected" => "Sua avaliação não foi aprovada pela moderação.",
      _ => "Recebemos sua avaliação. Ela aparece aqui depois da moderação.",
    }),
    _ => None,
  };
  let show_form = matches!(state, FormState::Eligible);
  let fit = summary.fit_majority();
  let total = summary.count;
  let product_id_text = product_id.to_string();

  Ok(view! {
      <section id="avaliacoes" class="scroll-mt-24">
          separator(attrs: attributes! { class="my-4" })
          <p class="text-sm font-medium tracking-tight text-primary uppercase">"Avaliações"</p>
          <h2 class="mt-2 mb-4 text-4xl font-bold tracking-tight">"O que dizem as clientes"</h2>
          <div class="grid gap-6 @3xl/page:grid-cols-[1fr_2fr]">
              <div class="space-y-4">
                  if total > 0 {
                      <div class="space-y-1">
                          <p class="text-4xl font-semibold">(format_average(summary.average))</p>
                          star_rating(rating: summary.average)
                          <p class="text-sm text-muted-foreground">(reviews_count_label(total))</p>
                          if let Some((label, pct)) = fit {
                              <p class="text-sm text-muted-foreground">"Caimento: " (label) " para " (pct) "% das clientes"</p>
                          }
                      </div>
                  } else {
                      <p class="text-sm text-muted-foreground">"Este produto ainda não tem avaliações."</p>
                  }
                  if let Some(note) = reviewed_note {
                      <p class="text-sm text-muted-foreground">(note)</p>
                  }
                  if show_form {
                      review_form(product_id: product_id_text.clone())
                  }
              </div>
              <div>
                  if total > 0 {
                      review_list(product_id: product_id_text, total: total)
                  }
              </div>
          </div>
      </section>
  })
}

#[component]
async fn review_form(product_id: String) -> Result<impl View> {
  Ok(view! {
      card(
          attrs: attributes! { id="avaliar" },
          card_header(
              card_title("Avalie sua compra")
              card_description("Sua avaliação é publicada após moderação.")
          )
          card_content(
              <form method="post" action=(href!(submit)) class="space-y-4">
                  <input type="hidden" name="product_id" value=(product_id)>
                  <fieldset class="space-y-2">
                      <legend class="text-sm font-medium">"Nota"</legend>
                      <div class="flex flex-wrap gap-3">
                          for value in (1..=5).rev() {
                              <label class="flex items-center gap-1 text-sm">
                                  <input type="radio" name="rating" value=(value.to_string()) required=(true)>
                                  (value) " ★"
                              </label>
                          }
                      </div>
                  </fieldset>
                  <label class="block space-y-1 text-sm font-medium">
                      <span>"Título"</span>
                      input(attrs: attributes! { name="title" required=(true) maxlength="80" })
                  </label>
                  <label class="block space-y-1 text-sm font-medium">
                      <span>"Comentário"</span>
                      textarea(attrs: attributes! { name="body" required=(true) maxlength="2000" rows="4" })
                  </label>
                  <label class="block space-y-1 text-sm font-medium">
                      <span>"Caimento (opcional)"</span>
                      select(
                          attrs: attributes! { name="fit" },
                          <option value="">"Não informar"</option>
                          <option value="pequeno">"Veste pequeno"</option>
                          <option value="ideal">"Tamanho ideal"</option>
                          <option value="grande">"Veste grande"</option>
                      )
                  </label>
                  button(
                      variant: ButtonVariant::Primary,
                      attrs: attributes! { type="submit" class="w-full" },
                      "Enviar avaliação"
                  )
              </form>
          )
      )
  })
}

/// "Avaliar" links for the products of a received order the signed-in owner
/// has not reviewed yet. Renders nothing otherwise (guests, other users,
/// orders not yet received).
#[component]
pub async fn order_review_links(cx: &Cx, order_id: Uuid) -> Result<impl View> {
  let products = match optional_user(cx).await? {
    Some(user) => {
      reviewable_products_for_order(
        app_context::<PgPool>(cx),
        user.user.id,
        order_id,
      )
      .await?
    }
    None => Vec::new(),
  };
  Ok(view! {
      if !products.is_empty() {
          card(
              card_header(
                  card_title("Avalie suas compras")
                  card_description("Conte para outras clientes o que achou.")
              )
              card_content(
                  <ul class="divide-y divide-border">
                      #[key(product.product_id.to_string())]
                      for product in products {
                          <li class="flex items-center justify-between gap-4 py-2">
                              <span class="text-sm">(product.name)</span>
                              <a
                                  href=(format!("{}#avaliar", href!(crate::app::produtos::slug::page, crate::app::produtos::slug::Slug(product.slug)).resolve(cx)))
                                  class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))
                              >
                                  "Avaliar"
                              </a>
                          </li>
                      }
                  </ul>
              )
          )
      }
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_validates_fields() {
    let ok = NewReview::parse("5", "  Ótimo ", " Amei ", Some("ideal"))
      .expect("valid");
    assert_eq!(ok.rating, 5);
    assert_eq!(ok.title, "Ótimo");
    assert_eq!(ok.body, "Amei");
    assert_eq!(ok.fit, Some(Fit::Ideal));
    assert!(
      NewReview::parse("5", "t", "b", Some(""))
        .unwrap()
        .fit
        .is_none()
    );
    for rating in ["0", "6", "", "x", "4.5"] {
      assert!(
        NewReview::parse(rating, "t", "b", None).is_err(),
        "{rating}"
      );
    }
    assert!(NewReview::parse("3", " ", "b", None).is_err());
    assert!(NewReview::parse("3", &"a".repeat(81), "b", None).is_err());
    assert!(NewReview::parse("3", &"á".repeat(80), "b", None).is_ok());
    assert!(NewReview::parse("3", "t", &"b".repeat(2001), None).is_err());
    assert!(NewReview::parse("3", "t", "b", Some("enorme")).is_err());
  }

  #[test]
  fn public_author_keeps_first_name_and_initial() {
    assert_eq!(public_author("Maria da Silva"), "Maria S.");
    assert_eq!(public_author("ana"), "ana");
    assert_eq!(public_author("  "), "Cliente");
  }

  #[test]
  fn fit_majority_reports_share() {
    let summary = RatingSummary {
      fit_pequeno: 1,
      fit_ideal: 3,
      ..RatingSummary::default()
    };
    assert_eq!(summary.fit_majority(), Some(("Tamanho ideal", 75)));
    assert_eq!(RatingSummary::default().fit_majority(), None);
  }

  #[test]
  fn received_predicate_matches_grace_days() {
    assert!(
      RECEIVED_ORDER_SQL.contains(&format!("days => {SHIPPED_GRACE_DAYS}"))
    );
  }
}
