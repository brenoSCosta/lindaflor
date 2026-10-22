//! Product reviews: verified-buyer eligibility, moderation authz, public
//! visibility (approved only) and output escaping.

use http::{Method, Request, StatusCode, header};
use lindaflor::app::store::reviews::{
  NewReview, ReviewError, aggregate_rating, rating_summary, submit_review,
};
use sqlx::PgPool;
use test_support::{body_bytes, pool_and_router, sign_up, unique_email};
use topcoat::router::{Body, Router};
use uuid::Uuid;

const PASSWORD: &str = "Harbor-Violet-Tiger-4821-Lantern!";

struct Account {
  cookie: String,
  id: Uuid,
}

async fn account(router: &Router, prefix: &str) -> Account {
  let res =
    sign_up(router, "Cliente Teste", &unique_email(prefix), PASSWORD).await;
  assert_eq!(res.status, StatusCode::OK, "{:?}", res.json);
  Account {
    cookie: res.next_cookie.clone().expect("cookie"),
    id: Uuid::parse_str(res.json["user"]["id"].as_str().unwrap()).unwrap(),
  }
}

async fn make_admin(pool: &PgPool, id: Uuid) {
  sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// The storefront/admin pages are module-routed, which the shared test
/// router does not mount; build one from the `lindaflor::app` module tree
/// (same discovery as `src/app.rs`, without the production layers). Layouts
/// are skipped: the root layout's Tailwind stylesheet asset id embeds the
/// build's `OUT_DIR`, which differs between the bundled binary and test
/// builds. Layers (the `/admin` guard) are kept.
fn store_router(pool: PgPool) -> Router {
  use topcoat::cookie::RouterBuilderCookieExt;
  use topcoat::router::RouterBuilderDiscoverExt;
  use topcoat::runtime::RouterBuilderRuntimeExt;
  use topcoat::session::RouterBuilderSessionExt;

  let _ = lindaflor::app::produtos::slug::page;
  let _ = lindaflor::app::admin::avaliacoes::page;
  let _ = lindaflor::app::store::reviews::submit;
  let builder: topcoat::router::RouterBuilder =
    topcoat::router::ModuleRouterBuilder::new("lindaflor::app")
      .discover_segments()
      .discover_pages()
      .discover_routes()
      .discover_layers()
      .into();
  // Plus the explicit-path pages/routes (`/login`, `/avaliacoes/enviar`).
  let builder = builder
    .discover()
    .cookies()
    .sessions(lindaflor::auth::session_config())
    .app_context(pool);
  builder.runtime().build()
}

async fn setup() -> (PgPool, Router, Router) {
  let (pool, auth) = pool_and_router().await;
  let store = store_router(pool.clone());
  (pool, auth, store)
}

struct Product {
  id: Uuid,
  slug: String,
  variant_id: Uuid,
}

async fn product(pool: &PgPool) -> Product {
  let id = Uuid::now_v7();
  let slug = format!("biquini-review-{}", id.simple());
  sqlx::query(
    "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Biquíni Avaliado', $2, 15990)",
  )
  .bind(id)
  .bind(&slug)
  .execute(pool)
  .await
  .unwrap();
  let variant_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO product_variants (id, product_id, sku, size, color)
     VALUES ($1, $2, $3, 'm', 'Coral')",
  )
  .bind(variant_id)
  .bind(id)
  .bind(format!("SKU-{}", variant_id.simple()))
  .execute(pool)
  .await
  .unwrap();
  Product {
    id,
    slug,
    variant_id,
  }
}

/// An order of `variant_id` for `user_id`; `shipped_days_ago` sets
/// `shipped_at`.
async fn order(
  pool: &PgPool,
  user_id: Uuid,
  variant_id: Uuid,
  status: &str,
  shipped_days_ago: Option<i32>,
) -> Uuid {
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO orders (id, user_id, status, total_cents, shipped_at)
     VALUES ($1, $2, $3::order_status, 15990,
             now() - make_interval(days => $4))",
  )
  .bind(id)
  .bind(user_id)
  .bind(status)
  .bind(shipped_days_ago)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
     VALUES ($1, $2, $3, 'Biquíni Avaliado', 'M · Coral', 1, 15990)",
  )
  .bind(Uuid::now_v7())
  .bind(id)
  .bind(variant_id)
  .execute(pool)
  .await
  .unwrap();
  id
}

async fn insert_review(
  pool: &PgPool,
  product_id: Uuid,
  user_id: Uuid,
  status: &str,
  rating: i16,
  title: &str,
  body: &str,
) -> Uuid {
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO product_reviews (id, product_id, user_id, rating, title, body, fit, status)
     VALUES ($1, $2, $3, $4, $5, $6, 'ideal', $7::review_status)",
  )
  .bind(id)
  .bind(product_id)
  .bind(user_id)
  .bind(rating)
  .bind(title)
  .bind(body)
  .bind(status)
  .execute(pool)
  .await
  .unwrap();
  id
}

async fn review_status(pool: &PgPool, id: Uuid) -> String {
  sqlx::query_scalar("SELECT status::text FROM product_reviews WHERE id = $1")
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn review_count(pool: &PgPool, product_id: Uuid) -> i64 {
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM product_reviews WHERE product_id = $1",
  )
  .bind(product_id)
  .fetch_one(pool)
  .await
  .unwrap()
}

async fn send(
  router: &Router,
  method: Method,
  path: &str,
  cookie: Option<&str>,
  form: Option<&str>,
) -> http::Response<Body> {
  let mut builder = Request::builder().method(method).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  let req = match form {
    Some(body) => builder
      .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
      .body(Body::from(body.to_owned())),
    None => builder.body(Body::empty()),
  }
  .expect("build request");
  router.handle(req).await
}

async fn get_html(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
) -> (StatusCode, String) {
  let response = send(router, Method::GET, path, cookie, None).await;
  let status = response.status();
  let body = body_bytes(response).await;
  (status, String::from_utf8_lossy(&body).into_owned())
}

fn review(title: &str) -> NewReview {
  NewReview::parse("5", title, "Caimento perfeito.", Some("ideal"))
    .expect("valid review")
}

#[tokio::test]
async fn only_received_orders_make_a_verified_buyer() {
  let (pool, auth, _store) = setup().await;
  let product = product(&pool).await;

  // No order at all.
  let stranger = account(&auth, "review-stranger").await;
  let err = submit_review(&pool, stranger.id, product.id, &review("Oi"))
    .await
    .unwrap_err();
  assert!(matches!(err, ReviewError::NotEligible), "{err:?}");

  // Paid (not shipped) and recently shipped orders are not yet received.
  let early = account(&auth, "review-early").await;
  order(&pool, early.id, product.variant_id, "paid", None).await;
  order(&pool, early.id, product.variant_id, "shipped", Some(2)).await;
  order(&pool, early.id, product.variant_id, "cancelled", None).await;
  let err = submit_review(&pool, early.id, product.id, &review("Cedo"))
    .await
    .unwrap_err();
  assert!(matches!(err, ReviewError::NotEligible), "{err:?}");

  // Delivered: accepted once, as pending, linked to the order.
  let buyer = account(&auth, "review-buyer").await;
  let order_id =
    order(&pool, buyer.id, product.variant_id, "delivered", Some(5)).await;
  let id = submit_review(&pool, buyer.id, product.id, &review("Amei"))
    .await
    .expect("buyer may review");
  assert_eq!(review_status(&pool, id).await, "pending");
  let linked: Option<Uuid> =
    sqlx::query_scalar("SELECT order_id FROM product_reviews WHERE id = $1")
      .bind(id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(linked, Some(order_id));
  let err = submit_review(&pool, buyer.id, product.id, &review("De novo"))
    .await
    .unwrap_err();
  assert!(matches!(err, ReviewError::AlreadyReviewed), "{err:?}");

  // Shipped long enough ago counts as received (delivery rarely marked).
  let late = account(&auth, "review-late").await;
  order(&pool, late.id, product.variant_id, "shipped", Some(30)).await;
  submit_review(&pool, late.id, product.id, &review("Chegou"))
    .await
    .expect("shipped 30 days ago may review");

  assert_eq!(review_count(&pool, product.id).await, 2);
}

#[tokio::test]
async fn form_submission_requires_a_verified_buyer_and_is_rate_limited() {
  let (pool, auth, store) = setup().await;
  let product = product(&pool).await;
  let form = format!(
    "product_id={}&rating=4&title=Bom&body=Gostei+muito&fit=grande",
    product.id
  );
  let page = format!("/produtos/{}", product.slug);

  // Guest: sent to login, nothing stored.
  let res = send(
    &store,
    Method::POST,
    "/avaliacoes/enviar",
    None,
    Some(&form),
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  let location = res.headers()[header::LOCATION].to_str().unwrap();
  assert!(location.starts_with("/login"), "{location}");
  assert_eq!(review_count(&pool, product.id).await, 0);

  // Signed in, no order: no form on the page, POST refused.
  let stranger = account(&auth, "review-form-stranger").await;
  let (status, html) = get_html(&store, &page, Some(&stranger.cookie)).await;
  assert_eq!(status, StatusCode::OK);
  assert!(
    html.contains("id=\"avaliacoes\""),
    "review section rendered"
  );
  assert!(
    !html.contains("name=\"rating\""),
    "form hidden for non-buyers"
  );
  let res = send(
    &store,
    Method::POST,
    "/avaliacoes/enviar",
    Some(&stranger.cookie),
    Some(&form),
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  assert_eq!(review_count(&pool, product.id).await, 0);

  // Verified buyer: form shown, POST stored as pending.
  let buyer = account(&auth, "review-form-buyer").await;
  order(&pool, buyer.id, product.variant_id, "delivered", None).await;
  let (_, html) = get_html(&store, &page, Some(&buyer.cookie)).await;
  assert!(html.contains("name=\"rating\""), "form shown for buyers");
  let res = send(
    &store,
    Method::POST,
    "/avaliacoes/enviar",
    Some(&buyer.cookie),
    Some(&form),
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  let status: String = sqlx::query_scalar(
    "SELECT status::text FROM product_reviews WHERE user_id = $1 AND product_id = $2",
  )
  .bind(buyer.id)
  .bind(product.id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(status, "pending");
  let (_, html) = get_html(&store, &page, Some(&buyer.cookie)).await;
  assert!(!html.contains("name=\"rating\""), "one review per product");

  // Rate limit: a user who burns the budget on refused submits is blocked
  // even once they become eligible.
  let spammer = account(&auth, "review-form-spam").await;
  for _ in 0..5 {
    send(
      &store,
      Method::POST,
      "/avaliacoes/enviar",
      Some(&spammer.cookie),
      Some(&form),
    )
    .await;
  }
  order(&pool, spammer.id, product.variant_id, "delivered", None).await;
  send(
    &store,
    Method::POST,
    "/avaliacoes/enviar",
    Some(&spammer.cookie),
    Some(&form),
  )
  .await;
  let spam: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM product_reviews WHERE user_id = $1",
  )
  .bind(spammer.id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(spam, 0, "sixth submit within the window is refused");
}

#[tokio::test]
async fn only_approved_reviews_are_public() {
  let (pool, auth, store) = setup().await;
  let product = product(&pool).await;
  let a = account(&auth, "review-pub-a").await;
  let b = account(&auth, "review-pub-b").await;
  let c = account(&auth, "review-pub-c").await;
  let tag = product.id.simple().to_string();
  insert_review(
    &pool,
    product.id,
    a.id,
    "approved",
    4,
    &format!("Aprovada {tag}"),
    "ok",
  )
  .await;
  insert_review(
    &pool,
    product.id,
    b.id,
    "pending",
    1,
    &format!("Pendente {tag}"),
    "ok",
  )
  .await;
  insert_review(
    &pool,
    product.id,
    c.id,
    "rejected",
    1,
    &format!("Rejeitada {tag}"),
    "ok",
  )
  .await;

  let summary = rating_summary(&pool, product.id).await.unwrap();
  assert_eq!(summary.count, 1);
  assert!((summary.average - 4.0).abs() < f64::EPSILON);
  assert_eq!(summary.fit_ideal, 1);
  let aggregate = aggregate_rating(&pool, product.id).await.unwrap().unwrap();
  assert_eq!(aggregate.review_count, 1);
  assert!((aggregate.rating_average - 4.0).abs() < f64::EPSILON);

  let (status, html) =
    get_html(&store, &format!("/produtos/{}", product.slug), None).await;
  assert_eq!(status, StatusCode::OK);
  assert!(html.contains(&format!("Aprovada {tag}")));
  assert!(!html.contains(&format!("Pendente {tag}")));
  assert!(!html.contains(&format!("Rejeitada {tag}")));
  assert!(html.contains("1 avaliação"));

  // No approved review: no aggregate for JSON-LD.
  let empty = self::product(&pool).await;
  assert_eq!(aggregate_rating(&pool, empty.id).await.unwrap(), None);
}

#[tokio::test]
async fn moderation_is_admin_only_and_audited() {
  let (pool, auth, store) = setup().await;
  let product = product(&pool).await;
  let author = account(&auth, "review-mod-author").await;
  let user = account(&auth, "review-mod-user").await;
  let admin = account(&auth, "review-mod-admin").await;
  make_admin(&pool, admin.id).await;
  let id = insert_review(
    &pool, product.id, author.id, "pending", 5, "Moderar", "texto",
  )
  .await;
  let approve = format!("review_id={id}&action=approve");

  // Guests and customers can neither see nor act.
  let res = send(&store, Method::GET, "/admin/avaliacoes", None, None).await;
  assert!(res.status().is_redirection(), "{}", res.status());
  let res = send(
    &store,
    Method::GET,
    "/admin/avaliacoes",
    Some(&user.cookie),
    None,
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  for cookie in [None, Some(user.cookie.as_str())] {
    let res = send(
      &store,
      Method::POST,
      "/admin/avaliacoes",
      cookie,
      Some(&approve),
    )
    .await;
    assert!(res.status().is_redirection(), "{}", res.status());
  }
  assert_eq!(review_status(&pool, id).await, "pending");

  // Admin sees the queue and approves; the change is audited.
  let (status, html) =
    get_html(&store, "/admin/avaliacoes", Some(&admin.cookie)).await;
  assert_eq!(status, StatusCode::OK);
  assert!(html.contains("Moderar"));
  let res = send(
    &store,
    Method::POST,
    "/admin/avaliacoes",
    Some(&admin.cookie),
    Some(&approve),
  )
  .await;
  assert_eq!(res.status(), StatusCode::OK);
  assert_eq!(review_status(&pool, id).await, "approved");
  let (actor, action): (Option<Uuid>, String) = sqlx::query_as(
    "SELECT actor_user_id, metadata->>'action' FROM audit_events
     WHERE aggregate_type = 'product_review' AND aggregate_id = $1",
  )
  .bind(id.to_string())
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(actor, Some(admin.id));
  assert_eq!(action, "review.approve");

  let reject = format!("review_id={id}&action=reject");
  send(
    &store,
    Method::POST,
    "/admin/avaliacoes",
    Some(&admin.cookie),
    Some(&reject),
  )
  .await;
  assert_eq!(review_status(&pool, id).await, "rejected");
}

#[tokio::test]
async fn review_text_is_escaped() {
  let (pool, auth, store) = setup().await;
  let product = product(&pool).await;
  let author = account(&auth, "review-xss").await;
  let admin = account(&auth, "review-xss-admin").await;
  make_admin(&pool, admin.id).await;
  insert_review(
    &pool,
    product.id,
    author.id,
    "approved",
    5,
    "<script>alert('t')</script>",
    "<img src=x onerror=alert(1)>",
  )
  .await;

  let (status, html) =
    get_html(&store, &format!("/produtos/{}", product.slug), None).await;
  assert_eq!(status, StatusCode::OK);
  assert!(
    !html.contains("<script>alert('t')"),
    "title must be escaped"
  );
  assert!(!html.contains("<img src=x"), "body must be escaped");
  assert!(html.contains("&lt;script&gt;"));
  assert!(html.contains("&lt;img src=x"));

  let (_, html) =
    get_html(&store, "/admin/avaliacoes", Some(&admin.cookie)).await;
  assert!(!html.contains("<script>alert('t')"));
  assert!(!html.contains("<img src=x"));
}
