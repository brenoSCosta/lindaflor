use http::StatusCode;
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
  verification_token_like,
};
use uuid::Uuid;

#[tokio::test]
async fn update_user_change_password_list_accounts() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("account");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Account User", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let updated = json_post(
    &router,
    "/api/auth/update-user",
    cookie.as_deref(),
    serde_json::json!({ "name": "Updated Name" }),
  )
  .await;
  assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
  assert_eq!(updated.json["user"]["name"], "Updated Name");

  let changed = json_post(
    &router,
    "/api/auth/change-password",
    cookie.as_deref(),
    serde_json::json!({
        "currentPassword": password,
        "newPassword": "Quilt-Mango-Brave-Pixel-7395?",
    }),
  )
  .await;
  assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);

  let accounts =
    json_get(&router, "/api/auth/list-accounts", cookie.as_deref()).await;
  assert_eq!(accounts.status, StatusCode::OK);
  let list = accounts.json.as_array().expect("accounts array");
  assert_eq!(list.len(), 1);
  assert_eq!(list[0]["providerId"], "credential");

  // Insert a second login method so unlink of credential is allowed, then unlink the fake one.
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();
  let google_account_id = Uuid::now_v7();
  sqlx::query!(
        r#"
        INSERT INTO accounts (id, account_id, provider_id, user_id, created_at, updated_at)
        VALUES ($1, $2, 'google', $3, now(), now())
        "#,
        google_account_id,
        format!("google-{user_id}"),
        user_id,
    )
    .execute(&pool)
    .await
    .unwrap();

  let unlink = json_post(
    &router,
    "/api/auth/unlink-account",
    cookie.as_deref(),
    serde_json::json!({
        "providerId": "google",
        "accountId": format!("google-{user_id}"),
        "password": "Quilt-Mango-Brave-Pixel-7395?",
    }),
  )
  .await;
  assert_eq!(unlink.status, StatusCode::OK, "{:?}", unlink.json);
  assert_eq!(unlink.json["status"], true);

  let accounts2 =
    json_get(&router, "/api/auth/list-accounts", cookie.as_deref()).await;
  assert_eq!(accounts2.json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn delete_user_with_token() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("delete");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Delete Me", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id = signed_up.json["user"]["id"].as_str().unwrap().to_string();

  let request = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_eq!(request.status, StatusCode::OK, "{:?}", request.json);

  let token =
    verification_token_like(&pool, &format!("delete-user:{user_id}")).await;

  let deleted = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "token": token }),
  )
  .await;
  assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);

  let gone = sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE id = $1"#,
    Uuid::parse_str(&user_id).unwrap()
  )
  .fetch_optional(&pool)
  .await
  .unwrap();
  assert!(gone.is_none());

  let refuse = sign_in(&router, &email, password).await;
  assert_eq!(refuse.status, StatusCode::UNAUTHORIZED);
}

/// LGPD: deleting the account removes personal data (sessions, addresses,
/// wishlist, cart) but keeps orders as fiscal records, detached from the
/// user and still carrying a contact email (`0027_final_gaps.sql`).
#[tokio::test]
async fn delete_user_purges_personal_data_and_keeps_orders() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("lgpd");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let signed_up = sign_up(&router, "Lgpd User", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

  let product_id = Uuid::now_v7();
  let variant_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO products (id, name, slug, price_in_cents)
     VALUES ($1, 'Peça LGPD', $2, 1000)",
  )
  .bind(product_id)
  .bind(format!("lgpd-{product_id}"))
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO product_variants (id, product_id, sku, size, color)
     VALUES ($1, $2, $3, 'm', 'azul')",
  )
  .bind(variant_id)
  .bind(product_id)
  .bind(format!("lgpd-{variant_id}"))
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO user_addresses
       (id, user_id, recipient_name, zip_code, street, number, neighborhood,
        city, state, phone)
     VALUES ($1, $2, 'Lgpd User', '49000000', 'Rua A', '1', 'Centro',
             'Aracaju', 'SE', '79999999999')",
  )
  .bind(Uuid::now_v7())
  .bind(user_id)
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO wishlist_items (user_id, product_id) VALUES ($1, $2)",
  )
  .bind(user_id)
  .bind(product_id)
  .execute(&pool)
  .await
  .unwrap();
  let cart_id = Uuid::now_v7();
  sqlx::query("INSERT INTO carts (id, token, user_id) VALUES ($1, $2, $3)")
    .bind(cart_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO cart_items (id, cart_id, variant_id, quantity)
     VALUES ($1, $2, $3, 1)",
  )
  .bind(Uuid::now_v7())
  .bind(cart_id)
  .bind(variant_id)
  .execute(&pool)
  .await
  .unwrap();
  // One order with the checkout email, one legacy row without any.
  let typed_order = Uuid::now_v7();
  let bare_order = Uuid::now_v7();
  for (order_id, guest_email) in [
    (typed_order, Some("entrega@example.com")),
    (bare_order, None),
  ] {
    sqlx::query(
      "INSERT INTO orders (id, user_id, guest_email, status, total_cents,
                           shipping_address)
       VALUES ($1, $2, $3, 'delivered', 1000,
               '{\"name\": \"Lgpd User\", \"city\": \"Aracaju\"}')",
    )
    .bind(order_id)
    .bind(user_id)
    .bind(guest_email)
    .execute(&pool)
    .await
    .unwrap();
  }

  let request = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_eq!(request.status, StatusCode::OK, "{:?}", request.json);
  let token =
    verification_token_like(&pool, &format!("delete-user:{user_id}")).await;
  let deleted = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "token": token }),
  )
  .await;
  assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);

  for (table, sql) in [
    (
      "sessions",
      "SELECT count(*) FROM sessions WHERE user_id = $1",
    ),
    (
      "accounts",
      "SELECT count(*) FROM accounts WHERE user_id = $1",
    ),
    (
      "user_addresses",
      "SELECT count(*) FROM user_addresses WHERE user_id = $1",
    ),
    (
      "wishlist_items",
      "SELECT count(*) FROM wishlist_items WHERE user_id = $1",
    ),
    ("carts", "SELECT count(*) FROM carts WHERE user_id = $1"),
  ] {
    let left: i64 = sqlx::query_scalar(sql)
      .bind(user_id)
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(left, 0, "{table} rows must go with the user");
  }
  let cart_items: i64 =
    sqlx::query_scalar("SELECT count(*) FROM cart_items WHERE cart_id = $1")
      .bind(cart_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(cart_items, 0, "the cart is deleted, not orphaned");

  let orders: Vec<(Uuid, Option<Uuid>, Option<String>, serde_json::Value)> =
    sqlx::query_as(
      "SELECT id, user_id, guest_email, shipping_address FROM orders
       WHERE id = ANY($1)",
    )
    .bind(vec![typed_order, bare_order])
    .fetch_all(&pool)
    .await
    .unwrap();
  assert_eq!(orders.len(), 2, "orders are retained (fiscal records)");
  for (order_id, owner, guest_email, address) in orders {
    assert_eq!(owner, None, "orders are detached from the deleted user");
    assert_eq!(address["city"], "Aracaju");
    let expected = if order_id == typed_order {
      "entrega@example.com".to_string()
    } else {
      email.clone()
    };
    assert_eq!(guest_email.as_deref(), Some(expected.as_str()));
  }
}

struct EnvRestore {
  key: &'static str,
  previous: Option<String>,
}

impl Drop for EnvRestore {
  fn drop(&mut self) {
    match self.previous.take() {
      Some(value) => unsafe { std::env::set_var(self.key, value) },
      None => unsafe { std::env::remove_var(self.key) },
    }
  }
}

fn set_env(key: &'static str, value: &str) -> EnvRestore {
  let previous = std::env::var(key).ok();
  unsafe { std::env::set_var(key, value) };
  EnvRestore { key, previous }
}

fn remove_env(key: &'static str) -> EnvRestore {
  let previous = std::env::var(key).ok();
  unsafe { std::env::remove_var(key) };
  EnvRestore { key, previous }
}

/// devenv (and other tests) may set Google OAuth env; serialize mutations.
static GOOGLE_ENV_LOCK: tokio::sync::Mutex<()> =
  tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn link_social_unavailable_without_google_env() {
  let _lock = GOOGLE_ENV_LOCK.lock().await;
  let _id = remove_env("GOOGLE_CLIENT_ID");
  let _secret = remove_env("GOOGLE_CLIENT_SECRET");
  let _origin = remove_env("APP_ORIGIN");

  let (_pool, router) = pool_and_router().await;
  let email = unique_email("link");
  let signed_up = sign_up(
    &router,
    "Link User",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let linked = json_post(
    &router,
    "/api/auth/link-social",
    cookie.as_deref(),
    serde_json::json!({ "provider": "google" }),
  )
  .await;
  assert_eq!(
    linked.status,
    StatusCode::NOT_IMPLEMENTED,
    "{:?}",
    linked.json
  );
}

#[tokio::test]
async fn google_callback_rejects_missing_pkce_verifier_before_network() {
  let _lock = GOOGLE_ENV_LOCK.lock().await;
  let _id =
    set_env("GOOGLE_CLIENT_ID", "test-client.apps.googleusercontent.com");
  let _secret = set_env("GOOGLE_CLIENT_SECRET", "test-secret");
  let _origin = set_env("APP_ORIGIN", "http://127.0.0.1:4200");

  let (pool, router) = pool_and_router().await;
  let state = format!("pkce-missing-{}", Uuid::now_v7().simple());
  let value = serde_json::json!({ "callback": "/dashboard" }).to_string();
  sqlx::query!(
    r#"
        INSERT INTO verifications (id, identifier, value, expires_at, created_at, updated_at)
        VALUES ($1, $2, $3, now() + interval '10 minutes', now(), now())
        "#,
    Uuid::now_v7(),
    format!("oauth-google:{state}"),
    value,
  )
  .execute(&pool)
  .await
  .expect("insert oauth state");

  let result = json_get(
    &router,
    &format!("/api/auth/callback/google?code=fake-code&state={state}"),
    None,
  )
  .await;
  assert_eq!(result.status, StatusCode::BAD_REQUEST, "{:?}", result.json);
}
