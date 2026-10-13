# Lindaflor — Codebase Review (Claude Code)

**Date:** 2026-10-13
**Branch / commit:** `migration` @ `0a75006`
**Stack:** Rust 2024 + Topcoat 0.9, sqlx/Postgres, Valkey (redis-rs), S3/RustFS
**Size:** ~48k lines of Rust across 209 tracked files, 16 migrations, 14 integration test files plus in-module tests

> Complements `PRODUCTION_READINESS.md` (2026-10-07). That document's auth P0s were spot-checked and look landed: the dashboard scoping, OAuth `email_verified` gate + PKCE + state binding, `is_currently_banned` everywhere, hashed tokens (`0007`), encrypted TOTP (`0009`), and the order state machine (`store/order.rs`). This review covers **what is still wrong or new**, mostly in the commerce code (sets, cart, checkout, PIX) added since then.

**Severity:** `P0` blocks launch (money or security) · `P1` fix before or right after launch · `P2` hardening / quality

---

## 0. TL;DR

| # | Sev | Finding | Where |
|---|-----|---------|-------|
| 1 | P0 | Per-IP rate limits and the login lockout trust a client-supplied `X-Forwarded-For`, so they can be bypassed or turned into a site-wide DoS | `src/valkey.rs:76` |
| 2 | P0 | Set (conjunto) pricing can be exploited from the normal cart UI: extra units of one member get the cheaper set price | `src/app/store/cart.rs:1042`, `cart_sheet.rs:219-249` |
| 3 | P0 | PIX phone keys lose the `+55` prefix, so the generated BR Code is invalid for phone-key stores | `src/app/checkout.rs:1089` |
| 4 | P0 | Orders are still created with an empty PIX key when the store's PIX settings are unset | `src/app/checkout.rs:988` |
| 5 | P1 | A static PIX code paid after the 48h expiry leaves a `cancelled` order with no way back to `paid` | `src/app/store/order.rs:97` |
| 6 | P1 | Shipping price comes from the client-posted `state`, which is never validated | `src/app/checkout.rs:702,1040` |
| 7 | P1 | Coupon `per_user_limit` is bypassable (logged-out guest checkout and a fresh email each count as a new identity) | `src/app/store/coupons.rs` |
| 8 | P1 | Admin multi-step writes (set save, product create) are not transactional | `admin/sets/id.rs:530-580`, `admin/produtos/novo.rs:323-372` |
| 9 | P1 | Possible deadlocks: inventory rows are locked in different orders by different code paths, and every checkout runs the global expiry sweep | `inventory.rs:170`, `checkout.rs:815` |

Clippy is clean and the test suite passes; see §6.

---

## 1. P0 — Security / money

### 1.1 Client IP comes from an untrusted `X-Forwarded-For` header — `src/valkey.rs:76-92`
```rust
headers.get("x-forwarded-for")...split(',').next()   // leftmost, client-controlled
  .or_else(|| headers.get("x-real-ip"))
  .unwrap_or_else(|| "unknown".to_string())
```
This value feeds `RateLimitLayer` (`rate_limit.rs`), the `auth:fail:<ip>:<email>` lockout (`sign_in.rs:104-118`, `service.rs:368-382`), HTML login throttling (`login.rs:555,602`), the 2FA pending-token IP/UA binding (`two_factor.rs`) and the access logs (`logging/layer.rs`).

- **If the app is directly exposed:** an attacker sends a new random `X-Forwarded-For` on every request and gets a fresh bucket each time. That gives unlimited password guessing per account (the lockout key includes the IP, and there is no per-email counter), unlimited TOTP/backup-code guessing and unlimited guest-order token polling.
- **If there is no proxy header at all:** every client lands in the `"unknown"` bucket. One user who fails 5 sign-ins in a minute then rate-limits the whole site.
- **If it sits behind a proxy that appends:** the leftmost entry is still whatever the client sent.
- `src/app/newsletter.rs:32` already uses Topcoat's `topcoat::router::request::client_ip`, so the codebase uses two different IP sources.

**Fix:**
- Delete `valkey::client_ip` and use `topcoat::router::request::client_ip(cx)` everywhere.
- Configure the trusted proxies on the router in `app::router`, e.g. `builder.trusted_proxies(TrustedProxies::new().nearest(N))`, with `N` set per deploy target from env.
- Add a **per-email** failure counter (independent of IP) so distributed guessing against one account still locks out.
- Add a test: a spoofed `X-Forwarded-For` must not reset the bucket.

### 1.2 Set pricing can be exploited by changing one member's quantity — `src/app/store/cart.rs:1040-1057`
`quote_cart_for_checkout` checks that the **set** of variant IDs in a `set_id` group matches the composition. It then assigns each line `set_price / n` as its **unit** price, but it never checks that the member quantities match:
```rust
let base_share = set_price / n;          // per-unit share
items[i].unit_price_cents = base_share;  // multiplied later by each line's own quantity
```
The cart sheet shows `-`/`+` on every line, set members included (`cart_sheet.rs:219,249` call `set_cart_quantity_proc` with no `set_id` check). `set_quantity` (`cart.rs:479`) also ignores `set_id`.

**Example:** a set costs R$100 and the top alone costs R$80. With the set in the cart, the shopper clicks `+` on the top four times. They now pay `5 × 50 + 1 × 50 = R$300` for 5 tops and 1 bottom. Bought individually, the 5 tops alone cost R$400. Adding the top again through `add_to_cart_proc` does the same thing: the `ON CONFLICT (cart_id, variant_id)` at `cart.rs:356` keeps the existing `set_id`.

Related problems with the same root cause:
- Adding a set whose variant is already in the cart as a single line overwrites that line's `set_id` (`cart.rs:448`), so the earlier full-price unit gets the set price.
- Two sets that share a variant can't both be in the cart. The second one overwrites the first one's `set_id`, the first set becomes "partial", and checkout fails with "Estoque insuficiente" with no explanation.

**Fix:**
- Make the set a first-class cart line: either a `cart_sets(cart_id, set_id, quantity)` table, or a `bundle_qty` on the group with the rule "all members share one quantity".
- In the quote, reject any group whose member quantities differ.
- Make the `UNIQUE(cart_id, variant_id)` constraint include `set_id`, so loose units and set units are separate lines.
- In `cart_sheet`, hide per-member `+`/`-` on set lines and only offer set-level quantity and remove.

### 1.3 PIX phone keys are emitted without `+55` — `src/app/checkout.rs:1089-1091`
```rust
"cpf" | "cnpj" | "phone" => pix_key.chars().filter(|c| c.is_ascii_digit()).collect()
```
The Pix spec requires phone keys in E.164 form (`+5579999816511`). Stripping everything but digits drops the `+` (and any missing `55`), so banking apps reject the copy-paste code or resolve it to the wrong key. CPF/CNPJ stripping is correct.

The merchant name and city are also uppercased but keep accents (`sanitize_merchant_field` uses `is_alphanumeric`, which allows `Ã`). Some readers reject non-ASCII in tags 59/60.

**Fix:** normalize phone keys to `+55` + DDD + number, and validate that on save in `admin/configuracoes.rs`. Transliterate names to ASCII. There is **no unit test for `generate_pix_payload` at all**; add golden-vector tests (known payload + CRC) for each key type.

### 1.4 Orders can be created with an empty PIX key — `src/app/checkout.rs:987-996`
`settings.pix_key.as_deref().unwrap_or("")` still produces a BR Code, reserves stock for 48h and redirects to the order page. This was listed in `PRODUCTION_READINESS.md §2` and is still open.

**Fix:** in `create_order`, refuse with "Pagamento indisponível" when `pix_key` or `pix_key_type` is `None`. Hide the "Pagar" button in that state too.

---

## 2. P1 — Commerce correctness

### 2.1 Late PIX payments have no recovery path — `src/app/store/order.rs:97-120`
The BR Code is static (no PSP, no expiry inside the code), so a customer can pay after the reservation sweep has already moved the order to `cancelled`. `transition` has no `Cancelled → Paid` (or reopen) edge, so the admin can't record money that was actually received. Other gaps in the state machine:
- `Processing` exists in the enum and the DB but nothing transitions into or out of it. `mais_vendidos` counts it (`queries.rs:376`).
- `Delivered` can't be refunded, even though the storefront advertises "Troca fácil — até 7 dias" (`app.rs`, home features).
- `Shipped → Cancel/Refund` calls `restock_sold_items`, which puts goods that are physically in transit back into sellable stock.

**Fix:**
- Add an admin-only `reopen_and_pay` (re-reserve; fail if stock is gone).
- Allow `Delivered → Refunded`, with restocking as an explicit admin choice ("returned to stock?").
- Drop `Processing` or wire it up.
- Write every `apply` to `audit_events`. Today only stock adjustments are audited (`admin/estoque.rs:210`); order status changes at `admin/pedidos/id.rs:88` are not.

### 2.2 Shipping trusts the client-posted `state` — `src/app/checkout.rs:702-734,1018-1047`
`validate_pay_fields` never checks `state ∈ BRAZILIAN_STATES` and never checks it against the CEP. Posting `state=SE` with any CEP gets the R$19,90 rate instead of R$39,90 or R$59,90, and the order stores whatever `state` was posted. The remote-region heuristic is also rough: `!starts_with("49")` is redundant once you check prefixes `66/68/69/78/79`, and `78`/`79` are MT/MS, not the North.

**Fix:** derive the UF from the CEP range on the server (it's a static table) and ignore or cross-check the posted state. Validate that the CEP is exactly 8 digits.

### 2.3 Coupon `per_user_limit` is easy to bypass — `src/app/store/coupons.rs` (identity block)
The identity is `COALESCE(user_id::text, lower(guest_email))`:
- A logged-in user's redemptions are keyed by `user_id`. The same person can log out and check out as a guest with the same email; the email key won't match their earlier user-keyed rows.
- Guest emails are unverified, so `a+1@…`, `a+2@…` and so on each count as a new identity.

**Fix:**
- Always store `lower(email)` on the redemption: the account email for users, the checkout email for guests.
- Count `WHERE user_id = $uid OR lower(guest_email) = $email`.
- Normalize `+tag` and Gmail dots if first-purchase or one-per-person coupons matter.
- Unrelated but in the same area: `coupons.code` is `UNIQUE` case-sensitively while lookup uses `UPPER(code)`. Admin create uppercases (`cupons.rs:167`), but a DB-inserted `abc` and `ABC` would make `fetch_optional` pick one arbitrarily. Replace it with `UNIQUE (upper(code))`.

### 2.4 Admin writes are not atomic
- `admin/sets/id.rs:530-580` runs `UPDATE sets`, `DELETE set_members`, `INSERT …`, `DELETE set_slots` and then a slot/option insert loop, all on `pool`. A failure halfway through leaves a set with no slots. A concurrent checkout can also see the set mid-rewrite, and `quote_cart_for_checkout` then reports "Estoque insuficiente".
- `admin/produtos/novo.rs:323-372` inserts the product, then variants, then inventory with manual compensating `DELETE`. An inventory insert failure (`?` at line ~372) leaves a product and variants with no inventory rows.
- `configuracoes.rs` has a SELECT-then-INSERT race on `store_settings` (two admins → two rows; `get_store_settings` reads `ORDER BY created_at LIMIT 1`).

**Fix:** wrap each of these in `pool.begin()` / `tx.commit()`, with image uploads after the commit (already the case for images). Make `store_settings` a singleton (`id boolean PRIMARY KEY DEFAULT true CHECK (id)`) and upsert.

### 2.5 Lock ordering and sweep contention
- `reserve_for_order` locks inventory per variant in cart order, which is mostly covered by `lock_cart_inventory`'s id-ordered lock. But `apply_reservation_rows` (`inventory.rs:170`), `fallback_from_order_items` and `restock_sold_items` lock in reservation or item order. Pay, cancel, sweep and checkout running at the same time on overlapping inventory rows can deadlock. Postgres aborts one of them, which surfaces as a 500.
- `create_order_in_tx` calls `expire_pending_reservations_in_tx` (`checkout.rs:815`). That cancels **every** expired order inside each shopper's checkout transaction, even though `main.rs` already runs a background sweeper every 60s. A slow or failed sweep then breaks unrelated checkouts.

**Fix:** sort the inventory IDs before locking everywhere (or take one `SELECT … WHERE id = ANY($1) ORDER BY id FOR UPDATE` up front). Remove the sweep from checkout; the `available` computation already ignores expired holds only after the sweep runs, so if you want, also treat expired holds as available in `variant_availability`. Also, the sweeper runs in every app instance; fine with `SKIP LOCKED`, but worth knowing.

### 2.6 Silent stock loss
`restock_sold_items` `continue`s when a variant has no active-warehouse rows, so refunded units vanish without a log line. `convert` uses `GREATEST(quantity - n, 0)`, which hides oversell instead of failing. Log a `tracing::warn!` with the order and variant in both cases.

---

## 3. P1/P2 — Security hardening

| Sev | Finding | Where | Fix |
|-----|---------|-------|-----|
| P1 | Per-IP limits are hostile to Brazilian mobile CGNAT: 5 sign-ins per minute per IP, and a **300/min global** budget that also counts assets and procedures | `rate_limit.rs`, `app.rs:116` | After §1.1, raise the global budget or exempt `/assets`; rely on the per-account lockout for auth |
| P2 | Governor `DashMapStateStore` keys are never pruned, so memory grows with every distinct IP (worse with the §1.1 spoofing) | `rate_limit.rs:100-105` | Call `retain_recent()` periodically (e.g. from the existing sweeper task) |
| P2 | Cart token is `Uuid::now_v7()` (time-ordered, ~74 random bits) and acts as a bearer secret | `cart.rs:247` | Use `Uuid::new_v4()` like `orders.access_token` |
| P2 | The OIDC `nonce` is generated and stored but never checked (`_stored_nonce`); the profile comes from `userinfo` | `oauth.rs:398` | Acceptable with PKCE + server-side code exchange; either verify the `id_token` or stop generating the nonce, so the code doesn't imply a check that isn't there |
| P2 | Google `access_token`/`refresh_token`/`id_token` are stored in plaintext in `accounts` | `oauth.rs:555-590` | Don't store them (the app only needs identity), or encrypt them like the TOTP secret |
| P2 | CSP allows `'unsafe-inline' 'unsafe-eval'` plus two public CDNs in prod | `security_headers.rs:33` | Self-host Scalar/Swagger (docs are dev-only anyway) and drop the `unpkg`/`jsdelivr` script sources in prod |
| P2 | Product/catalog search doesn't escape `%`/`_` in `ILIKE` | `queries.rs:419,469,534,567` | Escape them, or use `pg_trgm` / full-text search |
| P2 | Newsletter: weak email check (`contains('@') && contains('.')`), and any DB error is reported as "already subscribed". The `ON CONFLICT DO NOTHING` path also returns "Inscrição realizada! Verifique seu e-mail." even though no email is ever sent | `newsletter.rs:52,64-79` | Use `auth::validation::validate_email`, map only real duplicates, fix the copy |
| P2 | Wishlist toggle with a well-formed but nonexistent product UUID → FK error → 500 | `wishlist.rs:152` | Check that the product exists, or map the FK violation to `"error"` |

---

## 4. P2 — Correctness / UX nits

- **Checkout phone validation differs between client and server.** The client checks `phone.get().len()` (bytes of the raw input, `checkout.rs:325`) while the server counts digits (`phone_digit_count`). A user who types `(79) 99981-6511` (15 chars, 11 digits) has the Pay button blocked client-side even though the server would accept it. Count digits in the `expr!` too.
- **The checkout email check** on the client is just `contains("@")`. That's fine, since the server uses `validate_email`.
- **Dead settings:** `first_purchase_coupon_code` is saved (`configuracoes.rs:247-272`) but nothing on the storefront reads it.
- **The low-stock count is hard-coded to `<= 5`** (`admin.rs:65`) and ignores `product_variants.low_stock_threshold`.
- **Admin order page:** POST renders directly with no redirect afterwards, so a refresh re-submits the form. It also returns 400 for an unknown order instead of 404 (`admin/pedidos/id.rs:73-104`).
- **CheckoutError → CouponRemoved** is chosen by comparing the message string `"Cupom inválido."` (`checkout.rs:~860`). Return a typed variant from `resolve_coupon` instead.
- **Timestamp types are mixed:** the commerce tables from `0002` use `timestamp` (no tz) while everything after uses `timestamptz`. `created_at::text` rendering will be off when the DB timezone isn't UTC. Plan a forward migration to `timestamptz`.
- **`0002_commerce.sql` starts with `DROP TABLE … CASCADE`.** It's harmless for sqlx, which never re-runs a migration, but it's a trap if anyone ever runs it by hand. Leave it alone (changing it breaks the checksum) and add a comment in the README.
- **Images are presigned on every render** (`utils.rs:66`, 24h expiry). The signature includes the request time, so every page view produces new image URLs and browsers/CDNs never get a cache hit. Product and banner images are public; serve them from a public bucket path or CDN, or cache the presigned URL per key for most of its lifetime.
- **Hot-path N+1 lookups:** `quote_cart_for_checkout` runs 2 queries per set group, and `add_set_to_cart` runs one `variant_availability` per slot. That's fine at this size; batch them with `ANY($1)` if sets grow.
- **`valkey.rs:354-364`** computes `ttl_secs` and then discards it (`let _ = ttl_secs;`). That's dead code.
- **`HOST` defaults to `127.0.0.1`** (`main.rs:41`), which is undocumented in the README; containers need `0.0.0.0`.

---

## 5. Code quality & structure

**What's good**
- Clear separation in auth (`routes/` vs `service.rs`), with tests at both levels; the auth suite is the strongest part of the repo.
- Money is `i64` cents end to end (`0011`) with `checked_*` at the order boundary, and there are DB `CHECK`s on inventory (`reserved <= quantity`) and coupons (percent 1..50).
- Order transitions are centralized in one `apply()` under `FOR UPDATE`, with side effects in the same transaction.
- Secrets are handled carefully: `*_FILE` support, redacted `Debug`, hashed one-time tokens, an AEAD-encrypted TOTP secret, and `__Host-` cookies in prod.
- Every admin `route`/`procedure` calls `require_admin` explicitly, in addition to the `admin_guard` layer. I verified this for all 40+ handlers.

**What to improve**
- **Large files:** `checkout.rs` (1.9k lines) mixes the page view, form validation, the order service, PIX/EMV encoding and tests; `settings.rs` (1.9k), `cart.rs` (1.6k) and `auth/service.rs` (2.2k) are similar. Extract `store/checkout_service.rs` (create_order + validation) and `store/pix.rs` (EMV + CRC, unit-tested), as `PRODUCTION_READINESS.md §2.0` already proposes.
- **Error enums:** `CheckoutError`, `CartError`, `QuoteError`, `InventoryError`, `CouponReject` and `ApplyError` each repeat the same `Db(sqlx::Error)` plus a hand-written `Display`/`From`. `thiserror` is already a dependency; use `#[derive(thiserror::Error)]` with `#[from]`.
- **Repeated SQL:** the product summary projection (two image subqueries plus the `json_agg` color subquery) is copy-pasted 4 times in `queries.rs` (lines ~289, 337, 500, 580). Pull it into one `const` fragment or a SQL view.
- **Long argument lists:** `list_products_with_filters` takes 9 positional arguments (`false, true, …` at the call sites in `app.rs` aren't readable). Use a `CatalogQuery` struct; clippy flags this along with 3 others.
- **Hand-maintained duplicates:** `GLOBAL_OVERRIDE_PREFIXES` must be kept in sync with the scoped layers in `app.rs` (the comment says so). Have the scoped constructors register their prefix so the two can't drift.
- **Stale README:** it still describes "Rust web API built with axum", the project is called `topcoat`, and the tree lists only `tests/health.rs`. Update it for Topcoat pages, the storefront and the actual test layout.

---

## 6. Tooling status (this pass)

| Check | Result |
|-------|--------|
| `cargo-with-sqlx cargo clippy --all-targets` | Clean, 0 warnings. The 6 original style warnings were fixed without `#[allow]` (follow-up on 2026-10-13) |
| `cargo test` | Passes (run after the style fixes) |
| CI / Dockerfile | Still absent (`PRODUCTION_READINESS.md §3.1` is open) |

**Test gaps:**
- No HTTP-level checkout test: POST `/checkout`, then the redirect to `/pedido/:id?t=`.
- No tests for `generate_pix_payload` or the CRC.
- No test that set member quantities stay equal (§1.2).
- No test for spoofed `X-Forwarded-For` (§1.1).
- No test for state/CEP shipping (§2.2).
- No test for coupon reuse across the user and guest identities (§2.3).

---

## 7. Suggested order of work

1. **§1.1** IP source plus per-email lockout. Small change, closes several bypasses at once.
2. **§1.3 + §1.4** PIX key normalization and the not-configured gate, with golden tests. Without them, phone-key stores can't take payment at all.
3. **§1.2** Set quantity model, plus a cart UI change.
4. **§2.1** Reopen/late-payment path and auditing order transitions. This is needed before real money flows, since the PIX code is static.
5. **§2.2–2.5** Shipping derived from CEP, coupon identity, admin transactions, lock ordering.
6. Then the remaining open items in `PRODUCTION_READINESS.md` (Mercado Pago, Dockerfile/CI, backups) and the P2 list above.
