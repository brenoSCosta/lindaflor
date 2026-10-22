use super::*;

// --- Window ---

fn at(days_ago: i64) -> OffsetDateTime {
  OffsetDateTime::now_utc() - Duration::days(days_ago)
}

#[test]
fn window_counts_from_delivery() {
  let now = OffsetDateTime::now_utc();
  for kind in [ReturnKind::Exchange, ReturnKind::Return] {
    assert!(
      check_eligibility("delivered", kind, Some(at(6)), Some(at(10)), now)
        .is_ok()
    );
    assert_eq!(
      check_eligibility("delivered", kind, Some(at(8)), Some(at(10)), now),
      Err(Ineligible::Expired)
    );
  }
  // Exactly on the deadline is still inside.
  let delivered = now - Duration::days(RETURN_WINDOW_DAYS);
  assert_eq!(
    check_eligibility(
      "delivered",
      ReturnKind::Return,
      Some(delivered),
      None,
      now
    ),
    Ok(now)
  );
  // Defects: CDC art. 26 (90 days).
  assert!(
    check_eligibility("delivered", ReturnKind::Defect, Some(at(60)), None, now)
      .is_ok()
  );
  assert_eq!(
    check_eligibility("delivered", ReturnKind::Defect, Some(at(91)), None, now),
    Err(Ineligible::Expired)
  );
}

#[test]
fn window_without_delivery_uses_shipping_grace() {
  let now = OffsetDateTime::now_utc();
  let limit = UNCONFIRMED_DELIVERY_GRACE_DAYS + RETURN_WINDOW_DAYS;
  assert!(
    check_eligibility("shipped", ReturnKind::Exchange, None, Some(at(1)), now)
      .is_ok()
  );
  assert!(
    check_eligibility(
      "shipped",
      ReturnKind::Exchange,
      None,
      Some(at(limit - 1)),
      now
    )
    .is_ok()
  );
  assert_eq!(
    check_eligibility(
      "shipped",
      ReturnKind::Exchange,
      None,
      Some(at(limit + 1)),
      now
    ),
    Err(Ineligible::Expired)
  );
  assert_eq!(
    return_deadline(ReturnKind::Return, None, None),
    None,
    "no deadline before shipping"
  );
}

#[test]
fn window_needs_a_shipped_order() {
  let now = OffsetDateTime::now_utc();
  for status in ["pending_payment", "paid", "processing"] {
    assert_eq!(
      check_eligibility(status, ReturnKind::Return, None, None, now),
      Err(Ineligible::NotShipped)
    );
  }
  for status in ["cancelled", "refunded"] {
    assert_eq!(
      check_eligibility(status, ReturnKind::Return, Some(at(1)), None, now),
      Err(Ineligible::Expired)
    );
  }
}

#[test]
fn admin_transition_table() {
  use AdminReturnAction as A;
  use ReturnStatus as S;
  let approve = A::Approve {
    instructions: "x".into(),
  };
  let reject = A::Reject { reason: "x".into() };
  let refund = A::RefundOrder { restock: true };
  let record = A::RecordRefund {
    refund_cents: 1,
    restock: false,
  };
  let k = ReturnKind::Return;
  assert_eq!(
    admin_transition(S::Requested, k, &approve),
    Some(S::Approved)
  );
  assert_eq!(admin_transition(S::Approved, k, &approve), None);
  assert_eq!(admin_transition(S::Requested, k, &A::MarkReceived), None);
  assert_eq!(
    admin_transition(S::Approved, k, &A::MarkReceived),
    Some(S::Received)
  );
  for from in [S::Requested, S::Approved, S::Received] {
    assert_eq!(admin_transition(from, k, &reject), Some(S::Rejected));
  }
  for from in [S::Resolved, S::Rejected] {
    assert_eq!(admin_transition(from, k, &reject), None);
    assert_eq!(admin_transition(from, k, &refund), None);
  }
  assert_eq!(admin_transition(S::Approved, k, &refund), None);
  assert_eq!(admin_transition(S::Received, k, &refund), Some(S::Resolved));
  assert_eq!(admin_transition(S::Received, k, &record), Some(S::Resolved));
  // A size exchange is never refunded; a devolução is never exchanged.
  assert_eq!(
    admin_transition(S::Received, ReturnKind::Exchange, &refund),
    None
  );
  assert_eq!(admin_transition(S::Received, k, &A::ResolveExchanged), None);
  assert_eq!(
    admin_transition(S::Received, ReturnKind::Defect, &A::ResolveExchanged),
    Some(S::Resolved)
  );
  let note = A::Note { text: "x".into() };
  assert_eq!(admin_transition(S::Resolved, k, &note), Some(S::Resolved));
}

// --- DB fixtures ---

struct Fixture {
  order_id: Uuid,
  token: Uuid,
  owner: Uuid,
  /// `(order_item_id, variant_id)`: line 1 has 2 units, line 2 has 1.
  items: [(Uuid, Uuid); 2],
}

async fn insert_user(pool: &PgPool) -> Uuid {
  let id = Uuid::now_v7();
  sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, 'Cliente', $2)")
    .bind(id)
    .bind(format!("u-{}@example.com", id.simple()))
    .execute(pool)
    .await
    .unwrap();
  id
}

/// A delivered order (`delivered_sql` sets `delivered_at`) with two lines
/// and 5 units of stock per variant.
async fn seed(pool: &PgPool, provider: &str, delivered_sql: &str) -> Fixture {
  let owner = insert_user(pool).await;
  let order_id = Uuid::now_v7();
  let token = Uuid::new_v4();
  let warehouse_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO warehouses (id, code, name, is_default, active)
     VALUES ($1, $2, 'W', true, true)",
  )
  .bind(warehouse_id)
  .bind(format!("wh-{}", warehouse_id.simple()))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(&format!(
    "INSERT INTO orders (id, user_id, guest_email, status, access_token,
                         subtotal_cents, total_cents, payment_provider,
                         paid_at, shipped_at, delivered_at)
     VALUES ($1, $2, 'cliente@example.com', 'delivered', $3, 5000, 5000, $4,
             now() - interval '20 days', now() - interval '12 days', {delivered_sql})"
  ))
  .bind(order_id)
  .bind(owner)
  .bind(token)
  .bind(provider)
  .execute(pool)
  .await
  .unwrap();
  let mut items = [(Uuid::nil(), Uuid::nil()); 2];
  for (index, (quantity, price)) in
    [(2, 1500_i64), (1, 2000_i64)].into_iter().enumerate()
  {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let item_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Biquíni', $2, $3)",
    )
    .bind(product_id)
    .bind(format!("p-{product_id}"))
    .bind(price)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, 5, 0)",
    )
    .bind(Uuid::now_v7())
    .bind(variant_id)
    .bind(warehouse_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
       VALUES ($1, $2, $3, 'Biquíni', 'M / azul', $4, $5)",
    )
    .bind(item_id)
    .bind(order_id)
    .bind(variant_id)
    .bind(quantity)
    .bind(price)
    .execute(pool)
    .await
    .unwrap();
    items[index] = (item_id, variant_id);
  }
  Fixture {
    order_id,
    token,
    owner,
    items,
  }
}

fn request(kind: ReturnKind, lines: Vec<(Uuid, i32)>) -> NewReturn {
  NewReturn {
    kind,
    reason: "Ficou pequeno".to_string(),
    desired_size: Some("G".to_string()),
    lines,
  }
}

async fn stock(pool: &PgPool, variant_id: Uuid) -> i32 {
  sqlx::query_scalar("SELECT quantity FROM inventory WHERE variant_id = $1")
    .bind(variant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn return_status(
  pool: &PgPool,
  id: Uuid,
) -> (String, Option<String>, Option<i64>) {
  sqlx::query_as(
    "SELECT status, resolution, refund_cents FROM return_requests WHERE id = $1",
  )
  .bind(id)
  .fetch_one(pool)
  .await
  .unwrap()
}

async fn mail_kinds(pool: &PgPool, return_id: Uuid) -> Vec<String> {
  sqlx::query_scalar(
    "SELECT kind FROM email_outbox WHERE dedupe_key LIKE $1 ORDER BY created_at, id",
  )
  .bind(format!("return:{return_id}:%"))
  .fetch_all(pool)
  .await
  .unwrap()
}

// --- Customer: access + eligibility ---

#[tokio::test]
async fn only_owner_or_token_holder_can_open() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '2 days'").await;
  let line = vec![(f.items[0].0, 1)];
  let stranger = insert_user(&pool).await;

  // Another signed-in customer, a wrong token, nobody: all "not found".
  for (token, user) in [
    (None, Some(stranger)),
    (Some(Uuid::new_v4().to_string()), None),
    (Some(Uuid::new_v4().to_string()), Some(stranger)),
    (None, None),
  ] {
    let outcome = create_request(
      &pool,
      f.order_id,
      token.as_deref(),
      user,
      request(ReturnKind::Exchange, line.clone()),
    )
    .await;
    assert!(matches!(outcome, Err(ReturnError::NotFound)), "{outcome:?}");
  }
  let count: i64 = sqlx::query_scalar(
    "SELECT count(*) FROM return_requests WHERE order_id = $1",
  )
  .bind(f.order_id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(count, 0);

  // An expired guest link is refused like a wrong one.
  sqlx::query(
    "UPDATE orders SET access_token_expires_at = now() - interval '1 minute' WHERE id = $1",
  )
  .bind(f.order_id)
  .execute(&pool)
  .await
  .unwrap();
  assert!(matches!(
    create_request(
      &pool,
      f.order_id,
      Some(&f.token.to_string()),
      None,
      request(ReturnKind::Exchange, line.clone())
    )
    .await,
    Err(ReturnError::NotFound)
  ));

  // The owner's session works without the token.
  let id = create_request(
    &pool,
    f.order_id,
    None,
    Some(f.owner),
    request(ReturnKind::Exchange, line),
  )
  .await
  .expect("owner opens");
  assert_eq!(return_status(&pool, id).await.0, "requested");
  assert_eq!(mail_kinds(&pool, id).await, vec!["return_requested"]);
  let audit: String = sqlx::query_scalar(
    "SELECT metadata->>'action' FROM audit_events
     WHERE aggregate_type = 'return' AND aggregate_id = $1",
  )
  .bind(id.to_string())
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(audit, "create");
}

#[tokio::test]
async fn guest_token_opens_and_caps_apply() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '2 days'").await;
  let token = f.token.to_string();

  // More units than ordered, a foreign item, nothing selected: invalid.
  for lines in [
    vec![(f.items[0].0, 3)],
    vec![(Uuid::now_v7(), 1)],
    vec![(f.items[0].0, 0)],
    vec![(f.items[0].0, -1)],
  ] {
    let outcome = create_request(
      &pool,
      f.order_id,
      Some(&token),
      None,
      request(ReturnKind::Return, lines),
    )
    .await;
    assert!(
      matches!(outcome, Err(ReturnError::Invalid(_))),
      "{outcome:?}"
    );
  }
  let mut blank = request(ReturnKind::Return, vec![(f.items[0].0, 1)]);
  blank.reason = "   ".to_string();
  assert!(matches!(
    create_request(&pool, f.order_id, Some(&token), None, blank).await,
    Err(ReturnError::Invalid(_))
  ));

  let first = create_request(
    &pool,
    f.order_id,
    Some(&token),
    None,
    request(ReturnKind::Return, vec![(f.items[0].0, 1)]),
  )
  .await
  .expect("guest opens");
  // One open request per order.
  assert!(matches!(
    create_request(
      &pool,
      f.order_id,
      Some(&token),
      None,
      request(ReturnKind::Return, vec![(f.items[1].0, 1)])
    )
    .await,
    Err(ReturnError::AlreadyOpen)
  ));
  let page = order_returns(&pool, f.order_id).await.unwrap();
  assert!(page.visible && !page.can_open());

  // Once rejected, a new one may claim the remaining units (the rejected
  // unit is free again).
  admin_apply(
    &pool,
    first,
    AdminReturnAction::Reject {
      reason: "Peça usada".into(),
    },
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  let items = returnable_items(&pool, f.order_id).await.unwrap();
  assert_eq!(items[0].returnable, 2);
  assert!(order_returns(&pool, f.order_id).await.unwrap().can_open());
}

#[tokio::test]
async fn window_is_enforced_on_create() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '8 days'").await;
  let token = f.token.to_string();
  assert!(matches!(
    create_request(
      &pool,
      f.order_id,
      Some(&token),
      None,
      request(ReturnKind::Exchange, vec![(f.items[0].0, 1)])
    )
    .await,
    Err(ReturnError::Ineligible(Ineligible::Expired))
  ));
  // A defect claim is still inside its 90 days.
  create_request(
    &pool,
    f.order_id,
    Some(&token),
    None,
    request(ReturnKind::Defect, vec![(f.items[0].0, 1)]),
  )
  .await
  .expect("defect within 90 days");

  // Not shipped yet: refused.
  let g = seed(&pool, "static_pix", "NULL").await;
  sqlx::query(
    "UPDATE orders SET status = 'paid', shipped_at = NULL WHERE id = $1",
  )
  .bind(g.order_id)
  .execute(&pool)
  .await
  .unwrap();
  assert!(matches!(
    create_request(
      &pool,
      g.order_id,
      Some(&g.token.to_string()),
      None,
      request(ReturnKind::Return, vec![(g.items[0].0, 1)])
    )
    .await,
    Err(ReturnError::Ineligible(Ineligible::NotShipped))
  ));
  assert!(!order_returns(&pool, g.order_id).await.unwrap().visible);
}

// --- Admin ---

async fn open_received(
  pool: &PgPool,
  f: &Fixture,
  kind: ReturnKind,
  lines: Vec<(Uuid, i32)>,
) -> Uuid {
  let id = create_request(
    pool,
    f.order_id,
    Some(&f.token.to_string()),
    None,
    request(kind, lines),
  )
  .await
  .unwrap();
  admin_apply(
    pool,
    id,
    AdminReturnAction::Approve {
      instructions: "Código de postagem 123".into(),
    },
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  admin_apply(
    pool,
    id,
    AdminReturnAction::MarkReceived,
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  id
}

#[tokio::test]
async fn admin_transitions_are_audited_and_mailed() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '1 day'").await;
  let id = create_request(
    &pool,
    f.order_id,
    None,
    Some(f.owner),
    request(ReturnKind::Exchange, vec![(f.items[0].0, 1)]),
  )
  .await
  .unwrap();

  // Illegal: receive before approval; approve without instructions.
  assert!(matches!(
    admin_apply(&pool, id, AdminReturnAction::MarkReceived, None, None).await,
    Err(ReturnError::IllegalTransition)
  ));
  assert!(matches!(
    admin_apply(
      &pool,
      id,
      AdminReturnAction::Approve {
        instructions: " ".into()
      },
      None,
      None
    )
    .await,
    Err(ReturnError::Invalid(_))
  ));
  assert_eq!(return_status(&pool, id).await.0, "requested");

  let admin = insert_user(&pool).await;
  for action in [
    AdminReturnAction::Note {
      text: "Cliente ligou".into(),
    },
    AdminReturnAction::Approve {
      instructions: "Código de postagem AB123".into(),
    },
    AdminReturnAction::MarkReceived,
    AdminReturnAction::ResolveExchanged,
  ] {
    admin_apply(&pool, id, action, Some(admin), Some("Admin"))
      .await
      .unwrap();
  }
  assert_eq!(
    return_status(&pool, id).await,
    ("resolved".to_string(), Some("exchanged".to_string()), None)
  );
  // Terminal: no more transitions.
  assert!(matches!(
    admin_apply(
      &pool,
      id,
      AdminReturnAction::Reject { reason: "x".into() },
      None,
      None
    )
    .await,
    Err(ReturnError::IllegalTransition)
  ));

  let trail: Vec<(String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
    "SELECT metadata->>'action', actor_user_id, metadata FROM audit_events
     WHERE aggregate_type = 'return' AND aggregate_id = $1
     ORDER BY occurred_at, id",
  )
  .bind(id.to_string())
  .fetch_all(&pool)
  .await
  .unwrap();
  let actions: Vec<&str> = trail.iter().map(|(a, _, _)| a.as_str()).collect();
  assert_eq!(
    actions,
    ["create", "note", "approve", "receive", "resolve_exchanged"]
  );
  assert_eq!(trail[1].1, Some(admin));
  assert_eq!(trail[1].2["note"], "Cliente ligou");
  assert_eq!(trail[2].2["from"], "requested");
  assert_eq!(trail[2].2["to"], "approved");

  assert_eq!(
    mail_kinds(&pool, id).await,
    ["return_requested", "return_approved", "return_resolved"]
  );
  let approved: String = sqlx::query_scalar(
    "SELECT text_body FROM email_outbox WHERE dedupe_key = $1",
  )
  .bind(format!("return:{id}:approved"))
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(approved.contains("AB123"), "{approved}");
  assert!(approved.contains(&format!("/pedido/{}?t=", f.order_id)));
}

#[tokio::test]
async fn reject_mails_the_reason() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '1 day'").await;
  let id =
    open_received(&pool, &f, ReturnKind::Return, vec![(f.items[1].0, 1)]).await;
  admin_apply(
    &pool,
    id,
    AdminReturnAction::Reject {
      reason: "Peça com sinais de uso".into(),
    },
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  assert_eq!(return_status(&pool, id).await.0, "rejected");
  let body: String = sqlx::query_scalar(
    "SELECT text_body FROM email_outbox WHERE dedupe_key = $1",
  )
  .bind(format!("return:{id}:rejected"))
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(body.contains("Peça com sinais de uso"), "{body}");
}

#[tokio::test]
async fn full_return_refunds_the_order_with_restock() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '1 day'").await;
  let id = open_received(
    &pool,
    &f,
    ReturnKind::Return,
    vec![(f.items[0].0, 2), (f.items[1].0, 1)],
  )
  .await;
  let options = refund_options(&pool, id).await.unwrap();
  assert!(options.full_order && !options.mercado_pago);
  // The whole order is covered: a manual partial record is refused.
  assert!(matches!(
    admin_apply(
      &pool,
      id,
      AdminReturnAction::RecordRefund {
        refund_cents: 100,
        restock: false
      },
      None,
      None
    )
    .await,
    Err(ReturnError::Invalid(_))
  ));
  admin_apply(
    &pool,
    id,
    AdminReturnAction::RefundOrder { restock: true },
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  assert_eq!(
    return_status(&pool, id).await,
    (
      "resolved".to_string(),
      Some("refunded".to_string()),
      Some(5000)
    )
  );
  let order_status: String =
    sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
      .bind(f.order_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(order_status, "refunded");
  assert_eq!(stock(&pool, f.items[0].1).await, 7);
  assert_eq!(stock(&pool, f.items[1].1).await, 6);
  // The order's own audit row records the refund with the restock choice.
  let order_audit: serde_json::Value = sqlx::query_scalar(
    "SELECT metadata FROM audit_events
     WHERE aggregate_type = 'order' AND aggregate_id = $1",
  )
  .bind(f.order_id.to_string())
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(order_audit["action"], "refund");
  assert_eq!(order_audit["restock"], true);
}

#[tokio::test]
async fn partial_return_records_refund_and_restocks_only_returned_units() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "static_pix", "now() - interval '1 day'").await;
  let id =
    open_received(&pool, &f, ReturnKind::Defect, vec![(f.items[0].0, 1)]).await;
  let options = refund_options(&pool, id).await.unwrap();
  assert!(!options.full_order);
  assert_eq!(options.max_manual_cents, 1500);
  // Order-level refund is all-or-nothing: refused for a partial return.
  assert!(matches!(
    admin_apply(
      &pool,
      id,
      AdminReturnAction::RefundOrder { restock: true },
      None,
      None
    )
    .await,
    Err(ReturnError::Invalid(_))
  ));
  for cents in [0, 1501] {
    assert!(matches!(
      admin_apply(
        &pool,
        id,
        AdminReturnAction::RecordRefund {
          refund_cents: cents,
          restock: true
        },
        None,
        None
      )
      .await,
      Err(ReturnError::Invalid(_))
    ));
  }
  admin_apply(
    &pool,
    id,
    AdminReturnAction::RecordRefund {
      refund_cents: 1500,
      restock: true,
    },
    None,
    Some("Admin"),
  )
  .await
  .unwrap();
  assert_eq!(
    return_status(&pool, id).await,
    (
      "resolved".to_string(),
      Some("refunded".to_string()),
      Some(1500)
    )
  );
  assert_eq!(stock(&pool, f.items[0].1).await, 6);
  assert_eq!(stock(&pool, f.items[1].1).await, 5);
  let order_status: String =
    sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
      .bind(f.order_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(order_status, "delivered");
}

#[tokio::test]
async fn mercado_pago_orders_need_the_psp_refund() {
  let pool = crate::test_support::fresh_pool().await;
  let f = seed(&pool, "mercadopago", "now() - interval '1 day'").await;
  let id = open_received(
    &pool,
    &f,
    ReturnKind::Return,
    vec![(f.items[0].0, 2), (f.items[1].0, 1)],
  )
  .await;
  assert!(matches!(
    admin_apply(
      &pool,
      id,
      AdminReturnAction::RefundOrder { restock: false },
      None,
      None
    )
    .await,
    Err(ReturnError::Invalid(_))
  ));
  assert_eq!(check_mp_refund(&pool, id).await.unwrap(), f.order_id);
  // Until MP confirms the refund, the request can't be closed as refunded.
  assert!(matches!(
    finish_mp_refund(&pool, id, true, None, Some("Admin")).await,
    Err(ReturnError::Invalid(_))
  ));
  // What the payments service does after a successful PSP refund.
  let mut tx = pool.begin().await.unwrap();
  crate::app::store::order::apply(
    &mut tx,
    f.order_id,
    OrderEvent::Refund,
    ApplyOptions::default(),
  )
  .await
  .unwrap();
  tx.commit().await.unwrap();
  finish_mp_refund(&pool, id, true, None, Some("Admin"))
    .await
    .unwrap();
  assert_eq!(
    return_status(&pool, id).await.1.as_deref(),
    Some("refunded")
  );
  assert_eq!(stock(&pool, f.items[0].1).await, 7);
}
