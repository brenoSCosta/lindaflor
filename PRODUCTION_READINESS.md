# Lindaflor — Production Readiness Review

> **Historical document.** This review describes the pre-migration
> Rust/Topcoat codebase (`src/`, `migrations/`, `Cargo.toml`, …), which was
> replaced by the Go app and deleted on branch `migration-golang`. File paths
> and line numbers refer to that tree (see git history); the findings were
> carried into the Go port. Current layout: `docs/GO_MIGRATION.md`.

**Date:** 2026-10-07
**Stack:** Rust + Topcoat 0.9 (Axum), sqlx/Postgres, Valkey (redis-rs), RustFS/S3, `topcoat dev`
**Scope:** `src/auth/**`, `src/app/checkout|carrinho|store|pedido|conta|admin|dashboard`, `src/config|db|valkey|storage|logging|main|api|app`, planned `src/payments/` (§2.0), `migrations/0001–0006`, `Cargo.toml`, `devenv.*`
**PSP decision:** Mercado Pago (PIX via Payments API + webhook)
**Severity:** `P0` = do not launch without · `P1` = fix pre-launch / week-1 · `P2` = harden soon after

> How to use this file: work top-down. Each item has location, impact, and concrete fix. Checkboxes are intentional — check them off as you land fixes.
>
> Spot-checked against the tree on 2026-10-07 (second pass). Order status in DB is `pending_payment`, not `pending`.
>
> **Status 2026-10-22 (`migration` branch):** checkboxes below were re-verified against the code. Landed since the first pass:
> - Mercado Pago behind `PaymentProvider` (`src/payments/`, migration `0023`).
> - Transactional mail outbox (`0020`).
> - Docker/CI/backups.
> - Security review fixes A1–A6, B1, C1–C4 and D1–D2.
> - Tracking, saved addresses and guest-link expiry (`0024`).
> - `timestamptz` (`0025`).
> - The checkout service split + `thiserror` (`1003a65`).
> - `p2-polish` (`0026`): Google tokens no longer stored, prod CSP without CDNs, escaped `ILIKE`, per-recipient auth-mail cap, tracking in the shipped email.
>
> Still open: checkout create-order rate limit/CAPTCHA, PITR/S3 versioning/restore drill, S3/Google retry, Grafana stack and the remaining §5 items. See also `CODEBASE-REVIEW-CC.md` → Status.

---

## 0. TL;DR — why not launch today

1. **Auth has exploitable holes:** anonymous `/dashboard` leaks last-5 order IDs/status/totals, OAuth takeover via email linking (unverified claim defaults to true), token-only account-delete / email-change, plaintext + logged reset tokens, zero rate-limiting.
2. **Payments are manual/mock:** static locally-generated PIX EMV, no PSP, `paid` = admin button, settings page cannot save, WhatsApp reconciliation. Stock can be held hostage 48h by anonymous loop.
3. **Not deployable / not observable:** no `Dockerfile`, no CI, static `/api/health`, no request-id / JSON access logs / `/metrics`, no graceful shutdown, no backups, secrets derive `Debug`, `admin123` seed default.

Fix order: §1 Auth P0 → §2.0 `PaymentProvider` + §2 Mercado Pago → §3 Deploy/safety → §4 P1 commerce/correctness → §5 P2 hardening.

---

## 1. P0 — Auth & session security (blockers)

### 1.1 Anonymous recent-orders leak — `src/app/dashboard.rs:27-38`
```rust
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> { // no require_user
  let recent_orders = sqlx::query!(
    "SELECT id, guest_email, status::text ..., total_cents ... FROM orders ORDER BY ... LIMIT 5"
  )...
```
Any visitor (logged out) gets the last 5 orders. The query selects `guest_email` but the table **does not render it**. What is shown: truncated id, status, total, date; the href carries the **full order UUID**. Pedido pages still require `?t=` or owner (`pedido/id.rs:60-73,236-262`), so this is an ID/amount leak, not a guest-email dump. Still IDOR-ish + LGPD-relevant (order existence + totals of other people).
- [x] Fix: `let actor = require_user(cx).await?;` + `WHERE user_id = $1` (or restrict page to `admin_guard`). Drop unused `guest_email` from the SELECT. Add regression test: anonymous `GET /dashboard` → redirect/login, never order rows.

### 1.2 OAuth account takeover via email linking — `src/auth/routes/oauth.rs:230,472-505`
```rust
let email_verified = profile.email_verified.unwrap_or(true);
```
Two stacked bugs:
1. Missing claim defaults to `true`, which also `UPDATE`s the victim `email_verified = true`.
2. `find_or_create_google_user` **always** returns the existing `users.id` for that email, even when `email_verified` is `false` (`:472-505` — the flag only controls the UPDATE, not the link). Attacker with a Google account using the victim's email merges into the victim's row.
- [x] Fix: require `email_verified == Some(true)` before linking; if `false/None`, abort with “verify your Google email first” (do not create a second account that later collides). Never default to `true`. Test both branches.

### 1.3 Token-only destructive actions (plaintext + logged)
- `src/auth/routes/delete_user.rs:141-189` — `complete_delete(token)` needs only `token` (no session) to `DELETE FROM users`.
- `src/auth/routes/change_email.rs:140-219` — `confirm_change_email(token)` needs only `token` to change `users.email` + set `email_verified=true`.
- Tokens live plaintext in `verifications.value` (see §1.5) and are `tracing::info!`-logged (`delete_token`, `change_email_token`, `reset_token`, …). Logged delete URL is `GET /api/auth/delete-user?token=` but the route is **POST-only**.
- Email confirmation links are *meant* to work logged-out. Do **not** require an active session on confirm (that breaks the mail flow). The risk is stolen tokens from logs/DB.
- [x] Fix: hash at rest, single-use, short TTL (≤30 min for delete/change-email; current TTL is 60 min), stop logging raw tokens. Keep confirm token-only. Require password or TOTP on the *request* path (delete already has optional password). Add tests: hashed lookup, expired token, log-line must not contain the raw secret.

### 1.4 Immediate delete needs only email match — `src/app/settings.rs:1327-1330`, `src/auth/service.rs:1037-1056`
`delete_user_confirmed` checks only `confirm_email` match. Session cookies are `SameSite=Lax` (`session_config.rs`), so classic cross-site POST CSRF is largely blocked in current browsers. `OriginPolicy` is always on (§3.4); production also requires `APP_ORIGIN`.
- [x] Fix: require password (or TOTP if enrolled) + fresh session. Keep/add `Origin`/`Referer` check in prod. Consider 24h grace / soft-delete.

### 1.5 Secret tokens stored plaintext + logged — `migrations/0003_auth_sessions.sql:17-24`, `src/auth/service.rs:355-475,499-523,820-848,1096-1121`
Reset / verify / 2FA-pending / OAuth-state / change-email / delete tokens in `verifications.value` with no hash, no unique index. `request_password_reset` does **not** delete old tokens (multiple concurrent resets). Change-email and delete **do** delete prior tokens for that identifier. Token values are logged (`reset_token`, `change_email_token`, `delete_token`, … — field names vary).
- [x] Fix:
  - Store `SHA256(token)` or HMAC-SHA256 with server pepper; compare hashed.
  - `UNIQUE(identifier, value_hash)` + `DELETE` old tokens on new request (including reset).
  - TTLs: reset 60 min, verify 24h, delete/change-email 30 min (today 60 min), OAuth state 10 min (already `OAUTH_STATE_TTL_SECS=600` — keep).
  - Remove all token values from logs; log only a hash prefix (e.g. first 8 chars of hash).
  - Rotate pepper via env `TOKEN_PEPPER`.

### 1.6 No rate-limiting / brute force / TOTP replay
- `Cargo.toml` has no `tower_governor`/`governor`; `src/app.rs:62-96`, `src/main.rs:53` add no limit layer.
- Targets: `POST /api/auth/sign-in/email`, `sign-up`, `password-reset`, `two-factor/verify-totp|verify-backup-code` (6-digit TOTP, `XXXX-XXXX` backup ~27 bits), `verify-email`, guest order poll `GET /pedido/:id?t=` / `/pedido/status-proc`.
- Order poll is already existence-oracle resistant: missing vs unauthorized both return `"unknown"` / “Pedido não encontrado” (`pedido/id.rs:190-261`). Rate-limit anyway (token brute-force). Dashboard §1.1 already leaks recent IDs without `t=`.
- `src/auth/totp.rs:27-43` accepts `skew=1` with no single-use cache → replay within window.
- `trust_device` checkbox accepted but ignored (`src/app/two_factor.rs:275` `let _ = body.trust_device`).
- [x] Fix:
  - `governor` + `RateLimitLayer` in `src/app.rs` (and API test router): 5/min/IP auth, 3/min TOTP/backup, 10/min order poll, 30/min pedido pages. Generic 429. Env override `RATE_LIMIT_<SCOPE>_PER_MIN`.
  - Valkey (fallback in-process) `auth:fail:<ip>:<email>`: 10 fails / 15 min lockout; HTML `/login` POST also `check_ip_rate`.
  - TOTP single-use `totp_reserve` 90s after a successful `verify_code`; 5 attempts per pending token then invalidate.
  - `trust_device` checkbox **removed** (no `trusted_devices` table this pass).
  - CAPTCHA remains a **stub** (`TURNSTILE_ENFORCE` config gate only; no siteverify). Wire Turnstile/hCaptcha on sign-up / password-reset / guest checkout if abuse is observed.

### 1.7 OAuth link CSRF — `src/auth/routes/oauth.rs:246-280`, `src/auth/routes/accounts.rs:156-166`
`store_oauth_state(..., Some(user_id))` embeds `link_user_id` (`store_oauth_state` at `:72`); `google_callback` / `link_google_account` never verifies current session == `link_user_id`; no PKCE/nonce bound to cookie.
- [x] Fix: bind `state` to session (`state = HMAC(session_id, nonce)`), verify on callback, store `link_user_id` server-side only, add PKCE (`code_challenge`) + `nonce`.

### 1.8 Pending-2FA bearer — `src/auth/routes/sign_in.rs:130-141`, `src/auth/routes/two_factor.rs:311-339,473-508`
`403 TWO_FACTOR_REQUIRED {token}` + `lindaflor_pending_2fa` cookie, not bound to session/IP.
- [x] Fix: bind pending token to IP + UA hash at mint (`pending_bind_eager`); verify with `pending_check_binding` (mismatch → 401). 10-min TTL, one-time DB row, 5 attempts then invalidate. Cookie: `HttpOnly + SameSite=Lax`; `Secure` + `__Host-lindaflor_pending_2fa` in production (same attrs on clear). Not bound to `session_id` (there is no session yet).

### 1.9 Banned-user check drift — `src/auth/user.rs:88-90` vs `src/auth/service.rs:54-68`
`current_user()` checks `if row.banned` (ignores `ban_expires`); `sign_in`/`impersonate` use `is_currently_banned()`. Expired temp bans never lift for active sessions; `complete_2fa_*` check `banned` only.
- [x] Fix: single `is_currently_banned(banned, ban_expires)` helper used everywhere + middleware re-check per request + test for expiry boundary.

---

## 2. P0 — Payments: replace manual PIX with Mercado Pago

Current state (verified):
- `src/app/checkout.rs:894-918,987-1031` builds a static EMV BR-Code locally, no PSP API, no webhook/signature. `paid` is a manual admin button (`src/app/admin/pedidos/id.rs:95-103`). Buyer “Atualizar status” (`src/app/pedido/id.rs:190-221`) is read-only poll. Flow depends on WhatsApp manual reconciliation.
- `src/app/admin/configuracoes.rs:1-117` defines only `#[page]` GET; form POSTs to `href!(page)` with no `#[route(POST)]` — settings are loaded into `_settings` and discarded (inputs not pre-filled). PIX/WhatsApp cannot be saved from UI; only DB/seed (`src/bin/seed/seeders/commerce.rs:148-173`).
- `src/app/checkout.rs:894-896` `pix_key.unwrap_or("")` still creates order + EMV with empty key when `store_settings` unset — no “PIX not configured” gate.
- Any anonymous client can `add + checkout` in a loop, holding all stock 48h per order (`src/app/store/inventory.rs:143-144`); expiry sweeper only runs opportunistically inside cart/checkout txs; no CAPTCHA/rate-limit on `create_order` (`src/app/checkout.rs:769`).
- Copy mismatch: buyer page says “24 horas” (`src/app/pedido/id.rs:370`) vs code `now()+48h` (`inventory.rs:143-144`).
- Client `pay_blocked` (`checkout.rs:308-331`) only checks form fields, not PIX/MP configuration.

### 2.0 Pattern playbook (do this *with* MP, not after)

Do **not** drop Mercado Pago types into `src/app/checkout.rs` (already ~1.7k lines: page + form + SQL + local EMV). Do **not** introduce hexagonal crates, DDD aggregates, Repository traits on every `sqlx::query!`, or CQRS. Use the five patterns below only where the tree already hurts.

**Already in the tree (keep the shape, do not rename):**
- **Strategy:** `ObjectStore` (`S3 | Memory | Unavailable`) in `src/storage.rs:456-511`; session `TokenStore` (`CookieTokenStore` vs `LocalHttpCookieTokenStore` in `session_config.rs`).
- **Adapter (informal):** `From` DTOs (`auth/routes/dto.rs`), SQL rows → `ProductSummary`. Google `GoogleUserInfo` → `User` is a mapping, not a type — leave it.
- **Not present:** order state table (status is a string + scattered `UPDATE`s), event bus, retry/circuit breaker, outbound HTTP timeouts on Google (`Client::new()` per call in `src/auth/google.rs:123,151`). Mail is `tracing::info!("… mail not wired yet")` (`auth/service.rs` reset/verify). WhatsApp is a `wa.me` link, not a server send.

| Pattern | Adopt here | Skip |
|---|---|---|
| Strategy | `PaymentProvider` (MP / manual PIX / unavailable). Later: mail (`Log` vs SMTP) when send is real. | Coupon `fixed` vs `%` (one fn). Badge colors. Rate-limit scopes. |
| Adapter | `src/payments/mercadopago.rs`: MP JSON / `approved` → `Charge` / `Paid`. Webhook: verify signature, `GET /v1/payments/:id`, never trust body. | Wrapping sqlx or Topcoat. |
| State | `order.apply(event)` — see below. Display `match`es in `pedido/id.rs` / `conta/pedidos.rs` are labels only. | Cart qty. Pending-2FA cookie (TTL, not lifecycle). |
| Observer | Side effects **inside** `apply` (stock, coupon). Optional `pg_notify` / Valkey stream if webhook must return 200 before work. Mail listener when SMTP exists. | Pub/sub for logs (already `RequestLoggingLayer`). Every `sqlx` write. |
| Retry + circuit breaker | Outbound MP (create/fetch/refund) + later S3 5xx. Shared client + timeout on Google. | Inbound webhook (MP retries you — be idempotent). Postgres. `governor` 429s. Argon2. |

**Strategy + Adapter — `PaymentProvider`**

- [x] Add `src/payments/` with a public enum (or thin trait + enum), registered on the router like storage (`app_context`):
  - *Landed as `PaymentProvider::{StaticPix, MercadoPago}`. "Unavailable" is the open circuit breaker or an unconfigured static PIX (`payment_available` in checkout). Production refuses static PIX unless `ALLOW_STATIC_PIX=1`.*

```rust
pub enum PaymentProvider {
  MercadoPago(MercadoPagoClient),
  ManualPix,       // current EMV path; only when allow_manual_pix
  Unavailable,     // neither configured → checkout 400 pix_not_configured
}

impl PaymentProvider {
  pub async fn create_charge(&self, order: &NewCharge) -> Result<Charge, PaymentError>;
  pub async fn fetch_status(&self, id: &str) -> Result<ChargeStatus, PaymentError>;
  pub async fn refund(&self, id: &str) -> Result<(), PaymentError>;
}
```

  - Boot in `main.rs` the same way as `ObjectStore`: MP client if `MP_ACCESS_TOKEN` set (shared `reqwest::Client`, timeout 10s — see §2.1 / retry below); else `ManualPix` if `allow_manual_pix`; else `Unavailable`. Tests use a fake/in-memory backend (mirror `ObjectStore::memory()`).
  - Checkout, webhook, admin “Refresh from MP / Refund”, and the expiry worker talk **only** to `PaymentProvider` + `order.apply`. Pages must not import `mercadopago` HTTP types. Admin “mark paid” and local EMV stay `ManualPix`.
- [x] Extract an application service from the checkout page (same split auth already has: `routes/` vs `service.rs`). *Landed as `src/app/store/checkout_service.rs` (`1003a65`).*
  - `src/app/store/checkout.rs` (or `src/app/checkout/service.rs`): `create_order_in_tx`, quote, persist, reserve stock, then `provider.create_charge`.
  - `src/app/checkout.rs` stays GET/POST + view. `#[page]` must not own PIX/MP payloads.

**State — `order.apply` (highest value before a third writer)**

Today allowed moves live in **two** places and will be three with MP:
- Admin: `src/app/admin/pedidos/id.rs:93-122` — `pending_payment→paid/cancelled` (convert/release), `paid→delivered` (no restock). `processing`/`shipped` unreachable (§4.1).
- Expiry: `src/app/store/inventory.rs:321-346` — `pending_payment` past `reservation_expires_at` → `cancelled` + `release_reservation`.
- Next: webhook + refund.

- [x] One function, e.g. `order.apply(event)` inside `SELECT … FOR UPDATE`. Webhook, admin mark-paid, refund, and expiry worker all call it. Illegal transitions are `match` exhaustiveness / a typed error, not a scattered `UPDATE`.
  - Allowed: `pending_payment → paid | cancelled`; `paid → shipped → delivered`; `paid → refunded/cancelled` (restock + coupon restore, §4.3).
  - Map PSP events **in the adapter** (`approved → Paid`, `rejected|cancelled|expired → Cancelled`); never store raw MP strings as the source of truth for stock.

**Observer — side effects of `apply`, not a bus**

- [x] `Paid` → `convert_reservation` (+ `paid_at`). `Cancelled` / refund → `release_reservation` + restore coupon redemption (§4.3). Keep these **in the same tx** as the status write.
- [x] *(Not needed so far: the webhook fetches and applies inline, idempotent on `mp_payment_id`.)* If `GET /v1/payments/:id` + apply is slow, verify-then-queue (`pg_notify` or Valkey stream) and still `apply` once (idempotent on `mp_payment_id`). Do **not** add a crate-wide event bus this pass.
- [x] **Later (P2):** `Paid` / verify / reset mail as a listener (or a second Strategy: `Mail { Log, Smtp }`). *Landed: `Mailer::{Log, Smtp}` + Postgres outbox enqueued inside `order::apply` / checkout / auth token issuance (`src/mail/`).*

**Retry + circuit breaker — outbound HTTP only (P0 on MP)**

Today: Google `Client::new()` per `exchange_code` / `fetch_userinfo` (`google.rs:123,151`), no timeout; S3 one boot client, default reqwest timeouts (`storage.rs:22`); Valkey connect once, no PING (`valkey.rs:6`, §3.3).

- [x] Shared `reqwest::Client` (timeout 10s, pool) for MP **and** Google (stop per-request `Client::new()`). S3 already one client at boot — add explicit timeout.
- [x] MP `create_charge` / `fetch_status` / `refund`: retry **5xx + timeout** only, exponential backoff + jitter, cap 3 attempts. Send Mercado Pago `X-Idempotency-Key` (order id) on create so a retry cannot double-charge.
- [x] Circuit breaker around the MP client (open after consecutive failures, fail checkout fast with `psp_unavailable`, half-open probe). Same optional wrapper later for S3 put/delete (5xx only, never 404). *(The S3 wrapper is still open; see §5.)*
- [x] Do **not** retry inbound `POST /api/webhooks/mercadopago` (MP retries you). Idempotent `apply` on `mp_payment_id` is the defense. Do not wrap Postgres, Valkey fail-closed maps, or `governor`.

### 2.1 Target design (Mercado Pago)
- [x] Create MP integration **behind** `PaymentProvider` (§2.0), e.g. `src/payments/mercadopago.rs` (not called from the page):
  - Shared `reqwest::Client` with `timeout(10s)` + pool + retry/CB from §2.0. Do not `Client::new()` per request (`src/auth/google.rs:123,151` does this today). `src/storage.rs:22` builds one client per `ObjectStore` at boot — keep that; add timeout.
  - `POST https://api.mercadopago.com/v1/payments` with `MP_ACCESS_TOKEN` (server-only env, never exposed to template). Body: `transaction_amount = total_cents/100`, `payment_method_id = "pix"`, `payer.email`, `external_reference = order.id`, `notification_url = https://<APP_ORIGIN>/api/webhooks/mercadopago`, `expiration_date` = order expiry.
  - Persist on `orders`: add columns `mp_payment_id BIGINT NULL, mp_status TEXT NULL, paid_at TIMESTAMPTZ NULL, payment_method TEXT DEFAULT 'manual_pix'`. Migration `0007_mercadopago.sql`.
  - Checkout: create `orders(status='pending_payment')` + reservations first, then create MP payment; store `mp_payment_id`; render MP `point_of_interaction.transaction_data.{qr_code,qr_code_base64,ticket_url}` instead of local EMV. Keep local EMV only behind `allow_manual_pix` flag.
  - Statuses: map MP `approved → paid` (convert reservations, `paid_at=now()`), `rejected/cancelled/expired → cancelled` (release reservations + restore coupon — see §4.3). Apply **only** via `order.apply` (§2.0) in one tx with `SELECT ... FOR UPDATE`; webhook handler idempotent on `mp_payment_id`.
- [x] Webhook `POST /api/webhooks/mercadopago`:
  - Validate `x-signature` (`ts=...,v1=...`) per [MP webhook docs](https://www.mercadopago.com.br/developers/en/docs/your-integrations/notifications/webhooks): parse `ts`/`v1`, build manifest `id:{data.id};request-id:{x-request-id};ts:{ts};` (`data.id` from **query** params, lowercase if alphanumeric; omit missing parts), `v1 == hex(HMAC_SHA256(secret, manifest))`. Reject missing/invalid with 401. Optionally also allowlist `notification_url` secret query param (`?secret=<WEBHOOK_SECRET>`).
  - On `type=payment` + `action=payment.updated`, fetch `GET /v1/payments/:id` (never trust body alone), then `order.apply` (§2.0). Return 200 fast; verify-then-queue if slow (Valkey stream or `pg_notify` + worker).
  - Log `mp_payment_id + order_id + new_status` only; never log full MP body (contains payer PII).
- [x] Admin UI. *Notes on what landed:*
  - *The settings POST exists. MP credentials are env/`*_FILE` only (`MP_ACCESS_TOKEN`, `MP_WEBHOOK_SECRET`), not admin fields.*
  - *MP orders show MP status + "Sincronizar" + "Estornar no Mercado Pago". The manual "Marcar como pago" / "Recebido após expirar" / local "Estornar" are hidden for MP orders and refused server-side (`p2-polish`).*
  - *Every admin transition is audited.*
  - *Revenue counts paid/shipped/delivered only.*
  - Implement `#[route(POST "/admin/configuracoes")]` handler: validate + save `pix_key/type, merchant_name/city, whatsapp_*` AND new `mp_access_token_set (bool, write-only), mp_public_key, mp_webhook_secret_set, allow_manual_pix`. Pre-fill inputs from `_settings` (currently unpopulated). Never render secrets back.
  - Orders admin: replace bare “mark paid” with `MP status + Refresh from MP + Refund` (`POST /v1/payments/:id/refunds`). Keep manual override behind `require_admin` + audit log (`admin_audit(user_id, order_id, action)`).
  - Dashboard revenue (`src/app/admin.rs:52-57`): sum only `status='paid'` (currently `NOT IN ('cancelled')` inflates with unpaid `pending_payment`).
- [~] Abuse / correctness gates. *Landed:*
  - *The checkout gate.*
  - *The background expiry sweeper (MP-aware).*
  - *24h vs 48h copy aligned to the 48h hold (`p2-polish`).*

  *Still open: rate limit / CAPTCHA on order creation, and documenting the shipping-on-discount rule in `termos`.*
  - Gate checkout when neither MP nor manual PIX configured: `400 pix_not_configured`.
  - Rate-limit + CAPTCHA on `create_order`; background worker (tokio-cron or `pg_cron`) to expire `pending_payment` past `reservation_expires_at` and `release_expired_reservations` — do not rely on per-request sweeper.
  - Align copy: decide 24h vs 48h, update both code and template.
  - Decide shipping-on-discount rule explicitly (currently free-shipping threshold `29_900` on pre-discount subtotal — `checkout.rs:782,190`): document in `termos` + keep server-side recompute (already correct — `create_order_in_tx` re-quotes via `quote_cart_for_checkout`, `checkout.rs:770-805`).
- [x] Tests (embedded PG + MP mock server): *(`src/payments/tests.rs`, `store/order.rs::apply_rejects_illegal_transition`, `create_order_fails_when_pix_not_configured`)*
  - `webhook_approved_marks_paid_once` (double-delivery idempotent), `webhook_bad_signature_rejected`, `expired_mp_releases_stock_and_coupon`, `empty_pix_key_blocks_checkout`, `manual_paid_requires_admin_audit`.
  - `apply_rejects_illegal_transition` (e.g. `delivered → paid`); `create_charge_retries_5xx_once_with_same_idempotency_key`; `circuit_open_returns_psp_unavailable` (no extra MP HTTP).

---

## 3. P0 — Deploy, safety nets, secrets

### 3.1 No Dockerfile / compose / CI — repo root, `.github/` missing
Only `devenv.nix` dev stack exists. Not reproducibly deployable.
- [x] Add multi-stage `Dockerfile` (pinned `rust:1.98.1-slim` per `rust-toolchain.toml`): `cargo chef`-style cache, `sqlx migrate run` as separate migrate-job (not in web boot — see §3.5), `topcoat asset bundle`, non-root user, `HEALTHCHECK`.
- [x] Add `.dockerignore`, `docker-compose.prod.yml` (web + migrate job + postgres + valkey + rustfs, secrets via `*_FILE`). `fly.toml` / `render.yaml` still open (pick a target).
- [x] Add `.github/workflows/ci.yml`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo audit`, `sqlx migrate info`, `cargo sqlx prepare --check`, asset-bundle smoke.
- [x] Pin prod toolchain outside devenv (landed: `flake.nix` uses `rust-overlay` `stable."1.98.1"` with `rust-src/clippy/rustfmt`).
### 3.2 Secrets leak via `Debug` + weak defaults — `src/config.rs:3,13`, `devenv.nix:34,42-48`, `README.md:53-54`
```rust
#[derive(Clone, Debug)] pub struct S3Config { ... secret_access_key ... }
#[derive(Clone, Debug)] pub struct Config { ... database_url ... }
```
Any `{:?}` log/panic leaks DB password + S3 secret. Dev creds hardcoded (`DATABASE_URL` `postgres:postgres` at `devenv.nix:34`, `rustfsadmin`/`rustfsadmin` at `:42-43`, `GOCSPX-fake` at `:48`); seed default `SEED_ADMIN_PASSWORD=admin123` (`README.md:54`, `src/bin/seed/seeders/admin.rs:11-12`).
- [x] Fix: manual `Debug` impls redacting secrets (`"***"`); forbid `dotenvy` in prod image; support `*_FILE` (Docker secrets); fail boot if `SEED_*` weak defaults present when `APP_ENV=production`; never run `cargo run --bin seed` in prod (one-shot admin-create with random password instead).

### 3.3 Health is static, no readiness, no shutdown — `src/api.rs:21-23`, `src/main.rs:53`, `src/db.rs:4`, `src/valkey.rs:6`
`GET /api/health` returns `{"status":"ok"}` even during DB/Valkey/S3 outage. No `livez/readyz` split; `tests/health.rs:7` only asserts static `ok`. `topcoat::serve` with no `shutdown(signal)` truncates in-flight on SIGTERM. No `pool.close()`, no Valkey teardown. `get_multiplexed_async_connection` with no PING/timeout/retry.
- [x] Fix: `/api/health` (liveness, cheap) + `/api/readyz` (`SELECT 1`, `PING`, `HeadBucket`, 2s timeouts, 503 when degraded). `tokio::signal::ctrl_c + SIGTERM` graceful drain (30s), then `pool.close().await`. Extend `tests/health.rs` to cover degraded-backend 503.

### 3.7 Request logs + Prometheus primitives — `src/logging/**`, `src/app.rs`, `src/main.rs`
No request correlation, no structured access log, no scrapeable metrics. `tracing_subscriber::fmt` is human app logs only (and must stay that way — do not mix JSON access lines into it). Grafana / Loki / Alloy / Prometheus scrape stack is **out of scope** until after launch (P2); ship the same primitives sepro uses in `@sepro/logging` first.

- [x] Pathless `RequestLoggingLayer` registered **after** rate-limit layers (outermost pathless → sees 429s). Skip `OPTIONS` and `GET /metrics` entirely (no id, no metric, no log).
- [x] UUID **v7** `request_id` on `Cx` (`Arc<Mutex<RequestStore>>`) + `x-request-id` via `response_headers` (must land on error responses too). Finalize **once** (`store.logged`) on both `Ok` and `Err` from `next.run` (Topcoat errors leave the chain).
- [x] One compact JSON line per kept request (stdout always; optional `LOG_FILE_PATH` tee + rotate 10MiB / 5 files). Shape: `request_id`, `timestamp` (request **start**, RFC3339 UTC), `method`, `path` (pathname only — no query), `duration_ms`, `client_ip`, `user_agent` (max 256), optional `user_id`/`session_id`/`role` (no org fields), `status`, optional `error: {code,message}`, `sampled`, `sample_reason`. Never log bodies, cookies, or `Authorization`. Unknown 500s: `code: "INTERNAL"`, message `internal error` (no sqlx/connection text).
- [x] Sampling (first match): status ≥ 400 → `error`; duration ≥ `LOG_SLOW_THRESHOLD_MS` (default 1000) → `slow`; else `random < LOG_SAMPLE_RATE` (default 1) → `sampled`; else drop the **log line** only. Metrics still recorded.
- [x] In-process Prometheus text at `GET /metrics` (`text/plain; version=0.0.4`): `http_requests_total{method,route,status}` + `http_request_duration_ms` histogram buckets `[5,10,25,50,100,250,500,1000,2500,5000]`. `METRICS_TOKEN` unset = open in dev, closed (401) in production, where boot also requires it; set = `Authorization: Bearer` timing-safe, else 401. Skip `/metrics` on the global rate-limit layer.
- [x] Bounded `route` labels: `/` keep; `/api/auth/*` collapse; known pages keep templates (`/pedido/{id}`, `/produtos/{slug}`, …); UUID/numeric → `{id}`; assets + scanner junk → `other`.
- [x] Enrich `set_request_user` from `current_user` when a session is already resolved (no extra lookup). Env: `LOG_SAMPLE_RATE`, `LOG_SLOW_THRESHOLD_MS`, `LOG_FILE_PATH`, `LOG_FILE_MAX_BYTES`, `LOG_FILE_MAX_FILES`, `METRICS_TOKEN`.
- [x] Tests: sample table, route-label cardinality, metrics text, metrics-auth; integration `tests/logging.rs` (`x-request-id` == log `request_id`, one line, OPTIONS/`/metrics` silent, 404 `error`, `sample_rate=0` drops 200s, signed-in user fields).
- [ ] **Later (P2, not this pass):** Grafana + Loki + Alloy + scrape job. Do not add `infra/observability/` or compose services now.

### 3.4 CSRF / CORS / headers — `src/app.rs`, `src/security_headers.rs`, `Cargo.toml`
Landed: prod boot requires valid `APP_ORIGIN`; `OriginPolicy` always Verify (trust that origin when set); same-origin only (no `CorsLayer` / no `Access-Control-*`); enforcing CSP + nosniff + Referrer-Policy via `tower-http` `SetResponseHeader` + native `response_headers` fallback; HSTS prod-only. OpenAPI Scalar/Swagger stay dev-only CDN (no SRI/self-host this pass).
- [x] Require `APP_ORIGIN` in prod; always install `OriginPolicy`; document same-origin CORS; security headers + enforcing CSP; HSTS outside dev/test.
- [ ] Later (explicitly deferred): SRI or self-host Scalar/Swagger; assert `openapi_docs_enabled()==false` in prod tests. *(Partial, `p2-polish`: the prod CSP no longer allows `unpkg`/`jsdelivr`. The CDN docs are dev-only and covered by the dev CSP. `'unsafe-eval'` is still required by the Topcoat runtime.)*

### 3.5 Migrations on every boot, tiny pool — `src/db.rs:5-8`
`sqlx::migrate!().run()` on boot: OK single-instance, unsafe multi-replica race; blocks serve on DB down. `max_connections(5)` hardcoded, no timeouts (`test_support.rs:34` uses 16 — divergence).
- [x] Fix (landed): migrate-job pattern (`MIGRATE_ON_BOOT`, default on outside prod / off in prod, abort on pending/dirty/checksum mismatch); pool from env (`PG_MAX_CONN`, default 20 prod) + `acquire_timeout(5s)`, `idle_timeout(10m)`; no down-migrations in prod (forward-fix + PITR, enforced by `embedded_migrations_are_forward_only` test).
- [x] Pre-migrate backup step + `scripts/migrate.sh` (Dockerfile `migrate` target, compose `migrate` job gating `web`); RPO/RTO in README "Deploy & backups".

### 3.6 No backups / DR — `migrations/`, `devenv.nix:87`, `dump.rdb` in root
No `pg_dump`/`pg_basebackup`/PITR, no Valkey `BGSAVE` schedule, no S3 versioning/lifecycle. `dump.rdb` (89B) is local persistence, not a backup.
- [~] Landed: `scripts/backup.sh` (custom-format dump, `pg_restore --list` verify, rotation, optional S3 upload), compose `backup` job for host cron, Valkey AOF+RDB in compose, restore + drill + RPO/RTO in README. Still open: WAL archiving/PITR (managed PG or pgBackRest/wal-g), S3 versioning/replication setup, first restore drill.
- [ ] Original: nightly `pg_dump --format=custom` + WAL archiving / PITR (7d min), Valkey RDB+AOF + snapshot schedule, S3 versioning + cross-region copy, quarterly restore drill. Document RPO/RTO in README. Remove `dump.rdb` from repo working tree noise (already gitignored — keep ignored).

---

## 4. P1 — Commerce correctness & hardening (week-1)

### 4.1 Order state machine dead ends — `src/app/admin/pedidos/id.rs:93-122`, `migrations/0002_commerce.sql:7`
Enum is `pending_payment | paid | processing | shipped | delivered | cancelled`. Admin only implements `pending_payment→paid/cancelled` and `paid→delivered`. `processing`/`shipped` are unreachable; no `paid→cancelled/refund` restock path. Double-POST safe (`FOR UPDATE` + re-check — good, keep).
- [x] Fix: implement `pending_payment→paid→shipped→delivered` + `paid→refunded/cancelled` (restock via `convert/release` inverse + coupon restore) as the `order.apply` table in §2.0. Needed for MP refunds (§2). Manual DB work until then — document runbook.

### 4.2 Catalog unbounded — `src/app/store/queries.rs:132-200`, `src/app.rs:221-227`, `src/app/produtos.rs:107-115`, `src/app/sets/slug.rs:49`, `src/app/produtos/slug.rs:112-120`
`list_products` has no `LIMIT/OFFSET`. Home runs **two** unbounded queries (`featured` + `all`, not the same catalog twice). Related products load the full category then `.take(4)` in memory. (Admin `pedidos/produtos/conta/pedidos` correctly use `COUNT+LIMIT/OFFSET` — keep as pattern.)
- [x] Fix: paginate storefront (`LIMIT 24 + cursor/offset`, clamp `1..10_000` like admin); related via `WHERE id != $1 ... LIMIT 4` in SQL; add index on `(active, featured, created_at)`.

### 4.3 Coupon edge cases — `src/app/store/coupons.rs:109-210,176-203`, `src/app/checkout.rs:784-792`, `migrations/0006_inventory_reservations.sql:15-16`
- `per_user_limit` skipped when `user_id=None` — guests reuse unlimited coupons infinitely (only global `max_uses` stops it). Email collected but unused for limiting. → Key redemptions by `COALESCE(user_id::text, guest_email)` when email present; else require login for per-user coupons.
- Cancelled/expired orders keep `coupon_redemptions` rows, still counted toward caps — single-use coupon burned by abandoned PIX. → On cancel/expire, delete or mark `refunded` redemption in same tx (needed for §2 refund flow).
- Posted `coupon_code` wins over `carts.coupon_code`; stale stored code fails order instead of auto-clearing; `carts.coupon_code` has no FK, survives coupon delete. → On `CouponReject`, clear cart code + surface “cupom removido, revise o total” instead of hard fail.
- `fixed` only checks `>0` (`admin/cupons.rs:836-841`) — misconfigured `fixed=999999` clamped to subtotal by `calc_coupon_discount` (no negative total — OK) but yields free-items+shipping-only. → Add admin cap (e.g. ≤ subtotal typical or ≤ 50_000) + warning.
- [x] Guest/email redemption key, `refunded_at` restore, stale-code clear, admin fixed cap 50_000.

### 4.4 Inventory accounting gaps — `src/app/store/inventory.rs:251-302`, `migrations/0002_commerce.sql:66-73`, `src/app/store/cart.rs:315-541`
- Normal convert/release uses `GREATEST(reserved - $2, 0)` (`inventory.rs:178-190`). `fallback_from_order_items` does raw `reserved - $2` for **both** convert and release (`:279-296`) → double-release (expiry race + admin cancel) can drive `reserved` negative. No `CHECK(reserved>=0)` to catch it; reads mask with `GREATEST(quantity-reserved,0)`.
- `hydrate_cart_in_tx` locks only `ci`, not inventory rows; cart qty can transiently exceed stock (mitigated at checkout re-quote — checkout locking `ORDER BY i.id FOR UPDATE` + `reserve_for_order` re-check is correct, keep + keep `last_unit_race_only_one_order_succeeds` test).
- `hydrate_cart` fire-and-forget `release_expired_reservations` on every cart view — extra write tx per read, failures ignored.
- `i32` cents summed without overflow check (`cart.rs:533-535`, `checkout.rs:781,805`); huge qty panics debug / wraps release. `emv_field` uses byte `len()` (`checkout.rs:958-960`) — OK ASCII today, wrong for multibyte per Bacen.
- [x] Fix: `GREATEST` in the fallback path too + `CHECK(quantity>=0, reserved>=0, reserved<=quantity)`; move expiry sweep to background worker; `i64` cents + `checked_add`; `chars().count()` for EMV lengths; `i32 → i64` migration for money columns if needed.

### 4.5 Admin data honesty — `src/app/admin.rs:52-63`, `src/app/admin/produtos.rs:117-128`, `src/app/admin/estoque.rs:150-162`, `src/app/admin/produtos/id.rs:486-561`
- “Receita Total” sums `NOT IN ('cancelled')` (includes unpaid `pending_payment`). → `WHERE status='paid'`.
- Product list `SUM(quantity)` labeled “Disponível” ignores `reserved` (overstates vs storefront `quantity-reserved`). `low_stock_count` ignores `reserved`/warehouse `active`. → Use `quantity-reserved`. Estoque table itself already shows reserved + available (`estoque.rs:103-107`).
- Estoque page copy promises “entradas, transferências e CSV” (`:159`) but the page is read-only; `sync_variants` never updates `inventory.quantity` — new variants get `0`, existing qty uneditable, no replenishment UI path. → Minimal `adjust_stock(variant_id, delta, reason)` + audit table before launch, else document manual-SQL runbook and restrict who can run it.
- [x] Paid-through-fulfillment revenue, `quantity-reserved` Disponível/low-stock, `adjust_stock` + `audit_events`.

### 4.6 Input validation / body limits / error leakage
- `sign_up.rs:57-63` only `name.is_empty()`, `email.contains('@')`, `len>=8`; `normalize_email` is `trim+lowercase` (`dto.rs:120`); no max-len, no RFC email, no complexity. `sign_in.rs:84` only empty-check.
- `BodyLimit` only on `/settings/avatar` (3MB) + `/admin/produtos/create|update` (20MB) (`src/app.rs:90-93`). All `/api/auth/*` JSON unbounded → large-body DoS. Avatar limit mismatch: route 3MB vs `validate_image` 2MB (`src/app/utils.rs:16,40-47` — magic-byte classify is good, keep).
- `map_err(|e| Error::from(io::Error::other(e.to_string())))?` pattern (`delete_user.rs:65,92`, `session.rs:23`, `sessions.rs:60,121,165`, `admin.rs:104,416`, `accounts.rs:67,117,193`) turns full `sqlx`/`argon2` strings into 500 bodies — leaks constraint/table/connection detail. Same for S3 500-char body (`storage.rs:401→utils.rs:106`) and Google `Token(status: body)` (`google.rs:122,145`).
- `sign_up.rs:82` / `change_email.rs:72-82` distinguish “already exists/in use” → user enumeration (sign-in correctly generic — keep).
- [x] Fix: shared `validator` rules (name 1..100, email RFC + 254 max, password 8..128 + breach-list via `hibp` or zxcvbn floor); global `BodyLimit(1MB)` default + per-route overrides; map internal errors to generic `500 internal` + `error_id` (log full with `tracing::error!(error_id, ?err)`); return generic “email sent if account exists” on reset/signup-enumeration paths; shared `reqwest` client timeout (Google/S3/MP — §2.0 retry/CB).

---

## 5. P2 — Medium/Low hardening (post-launch)

- [x] **Password policy:** `MIN_PASSWORD_LEN=8` only (`service.rs:29`, `password.rs:18-23`). Argon2id + random salt is good — add `max_len=128` (CPU-DoS), optional pepper, breach-list check. No change needed to hasher.
  - Landed (P2-1): `src/auth/password.rs::validate_password` enforces `min..=max` chars-counted length (defaults 8/128, env-overridable `PASSWORD_MIN_LEN`/`PASSWORD_MAX_LEN`) + `zxcvbn ≥ 3` (env `PASSWORD_MIN_ZXCVBN_SCORE`) with account email/name as inputs; wired into sign-up / reset-confirm / change-password (API + HTML settings). HIBP k-anonymity **deferred** (needs outbound HTTP at signup/reset; revisit with mail/async infra).
- [x] **Backup codes:** `XXXX-XXXX` + fast `SHA256(salt||code)` (`totp.rs:46-74`) — offline brute-force trivial if `two_factor.backup_codes` leaked. Move to Argon2 or 128-bit entropy (`8×5-bit` → `16 chars`), hash individually.
  - Landed (P2-2): codes hashed with Argon2id PHC-only (`hash_backup_codes`/`consume_backup_code`, newline-joined, `spawn_blocking` + timeout so verify is not a DoS amplifier). Direct cutover, no legacy SHA256 support. Entropy still `XXXX-XXXX` — 128-bit upgrade deferred.
- [x] **TOTP secret:** plaintext + indexed (`migrations/0001:4`, `0003:40`, `two_factor_secret_idx`). Encrypt at rest (XChaCha20-Poly1305, key from KMS/env) or at minimum drop index + one-time display only. Ensure `enable` response secret shown once.
  - Landed (P2-3): `migrations/0009_totp_secret_encrypted.sql` drops `two_factor_secret_idx` and the plaintext `secret` column, adds `secret_nonce`/`secret_ciphertext`/`secret_key_version` NOT NULL. `src/auth/totp_crypto.rs` owns XChaCha20-Poly1305 (random 24B nonce, `Zeroizing` plaintexts); key from `TOTP_ENCRYPTION_KEY[_FILE]` via `src/config.rs:totp_encryption_key` (base64-32B or hex-64, required in prod boot, ephemeral+warn in dev/test; KMS envelope documented there). Direct cutover, no legacy/plaintext path — decrypt failures fail hard 401. `pending_two_factor_setup` deleted — sole exposure is the enable response (utoipa notes "shown ONCE"); `GET /settings?setup=1` renders no secret.
- [x] **Sessions:** `change_password` `revoke_other_sessions` opt-in default false (`change_password.rs:112-115`); `change-email` never revokes (`change_email.rs:199-208`). Default revoke-others on password/email change + “sign out everywhere” button.
  - Landed (P2-4): `POST /api/auth/change-password` omits → revokes (`unwrap_or(true)`); explicit `false` opts out. `POST /api/auth/change-email` revokes others immediately (keeps authorizing session); confirm revokes all, still token-only so logged-out mail links work. `POST /settings` threads a `revoke_other_sessions=false` opt-out checkbox through `change_password`, revokes others on `change_email`, and adds a `revoke_other_sessions` "Sair de todos os outros dispositivos" action (+ `sessions_tab` button, current session keeps no button). Tests: API default/explicit-false, HTML parity, change-email request/confirm revocation, `current` flag.
- [x] **`generate-backup-codes`:** `if let Some(pw) && ...` skips verification when `password=None` (`two_factor.rs:546-562`) — authenticated session alone rotates codes. Require password or TOTP always.
  - Landed (P2-5): route requires password re-check **or** valid TOTP code (OAuth/passwordless users); neither → 401. Covered by `tests/auth_two_factor.rs` P2-5 step-up tests.
- [x] **`set_user_role`:** allows self-demotion + targeting other admins (`service.rs:1475-1492`); `ban/remove` allow targeting admins; `revoke_admin_session:1611-1625` deletes arbitrary `sessions.id` no ownership/audit. Add `actor != target` + `cannot target equal-or-higher role` + audit log.
  - Landed (P2-6): `actor != target` self-guard + `ensure_not_admin_target` on set-role/ban/unban/remove + `revoke_admin_session` requires `user_id` ownership check; all privileged mutations append to `audit_events` (`migrations/0010_audit_events.sql`: id uuid v7, aggregate_type/id, entity_type/id, actor_user_id FK users SET NULL, actor_name, metadata jsonb, occurred_at, created_at).
- [x] **Cookies:** pending-2FA cookie no `Secure`/`__Host` (`service.rs:134-149`), clear drops attributes (removal may fail); toast/cart `Secure` conditional. Verify prod `Secure=true`; test prod cookie path (tests force `APP_ENV=development` — `tests/common/mod.rs:35,91`).
  - Landed (P2-7): session/pending-2FA/toast/cart cookies hardened prod-only (`__Host-` + `Secure`, clear mirrors attributes); dedicated prod-only `tests/prod_cookies.rs` binary (`pool_and_prod_router`, `APP_ENV=production`) asserts flags on set + clear. Never mixed with dev-router tests in one binary (env is process-global).
- [x] *(Landed: `access_token_expires_at`, 90 days, extended on ship/deliver; expired ≡ not found. No rotate-on-resend.)* **Guest order token:** `?t=<Uuid>` long-lived, no expiry, URL-logged (`pedido/id.rs:60-73`). Add `expires_at` (e.g. 90d) + rotate on email-link re-send; keep collapsing missing vs unauthorized to the same message.
- [ ] **Images:** `collect_multipart` buffers `8×2MB=16MB` in memory before validation (`admin/produtos.rs:297-321`); `remaining` trusts client `delete_image_id[]` length (`produtos/id.rs:392-407`); seed uses external `picsum.photos` URLs passing `is_storage_key` (`seeders/commerce.rs:299-308`, `utils.rs:50-52`). Add total-request cap + streaming size-first check; count after ownership check; allowlist external URLs or self-host seeds.
- [ ] **Storefront N-serial:** sequential `await resolve_storage_url` in loops (`checkout.rs:100-104`, `carrinho.rs:105-109`) — harmless today (pure presign, `storage.rs:247-283`) but N-serial if presign ever does I/O. `join_all` when adding I/O.
- [ ] **Admin search:** `ILIKE '%q%'` leading-wildcard unindexed (`admin/pedidos.rs:82`, `admin/produtos.rs:101`) — fine now; add `pg_trgm` if catalog >10k. *(Wildcards in user input are now escaped everywhere via `db::escape_like` + `ESCAPE '\'`.)*
- [ ] *(`HOST` is now documented in the README.)* **Ops nits:** `HOST` defaults `127.0.0.1` (`main.rs:49`, undocumented — prod needs `0.0.0.0`); invalid `PORT` exits 1 no context (`config.rs:23`); external `://` URLs pass through `resolve_storage_url` unvalidated (`utils.rs:63` — allowlist if user-supplied); `open-redirect` defense good (`safe_callback_path`, `google.rs:154`, `app.rs:206`) but `redirect_to/callback_url` params only logged/ignored — remove dead params or wire them.
- [ ] **Grafana stack:** after §3.7 primitives, optional scrape + Loki ship (Prometheus / Loki / Alloy / Grafana compose). Not required to launch.
- [ ] *(Sends are now throttled: at most 5 verify/reset/change-email mails per recipient per hour, with extras dropped silently. The gating decision is still open.)* **Email verification not enforced:** sign-up auto-sign-in with `email_verified=false`, sign-in never gates (`service.rs:160-328`, `sign_up.rs:89-147`). Decide: gate checkout / account-actions on verified, or accept unverified (document + throttle sends).
- [x] *(Landed: `src/mail/` outbox + `Mailer::{Log, Smtp}`.)* **Mail Strategy / Observer:** replace `tracing::info!("mail not wired yet")` with `Mail { Log, Smtp }` (or SES) as a listener of `order.apply(Paid)` + reset/verify. Not a `Notifier` enum until send is real. WhatsApp stays a `wa.me` link unless we add a server send.
- [ ] **S3 / Google retry:** after MP CB lands, wrap S3 put/delete 5xx and Google token/userinfo with one retry + fail-closed breaker (user-facing OAuth can fail fast).

---

## 6. Suggested execution order

| Sprint | Focus | Exit criteria |
|---|---|---|
| 1 (3–5d) | §1 Auth P0 + rate-limit + token hashing | `cargo test` auth suite green incl. new takeover/replay/enumeration tests; no token in logs (`rg "reset_token|change_email_token|delete_token|verification_token"` only in tests) |
| 2 (1–2w) | §2.0 `PaymentProvider` + adapter + `order.apply` + MP retry/CB, then §2 MP + configuracoes POST + expiry worker | Checkout/webhook/admin talk only to `PaymentProvider`; `apply` is the only status writer; sandbox `approved → paid`, `expired → cancelled+release`, bad-signature 401, double-webhook idempotent; create-charge retry does not double-charge (`X-Idempotency-Key`); `allow_manual_pix=false` in staging |
| 3 (3–5d) | §3.7 logging/metrics primitives, then §3 Dockerfile/CI/health/shutdown/backups + Debug redaction | `x-request-id` on responses; JSON access line (no secrets); `GET /metrics` prometheus text; OPTIONS/`/metrics` excluded; image builds cold, `ci.yml` green, `/readyz` 503 on killed DB, SIGTERM drains, restore drill recorded |
| 4 (1w) | §4 P1 commerce + validation + error mapping | Paginated storefront, revenue = paid-only, coupon-restore covered, 500 bodies generic + `error_id` logged |

---

## 7. Verification checklist (run before each release)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
sqlx migrate info            # from repo root with DATABASE_URL set
cargo sqlx prepare --check   # offline query cache in sync
APP_ENV=production cargo test --test health  # assert docs 404 + Secure cookies
# staging smoke:
curl -s $BASE/api/health | jq .
curl -sI $BASE/api/health | grep -i x-request-id
curl -s $BASE/metrics | head  # prometheus text; 401 if METRICS_TOKEN set without Bearer
curl -s -o /dev/null -w "%{http_code}" $BASE/api/readyz  # 200 healthy, 503 degraded
curl -s $BASE/api/openapi.json -o /dev/null -w "%{http_code}\n"  # expect 404 in prod
k6 run tests/load/checkout_smoke.js  # add: 50 VUs cart→checkout→webhook
```

---

## Appendix — file:line index (spot-verified 2026-10-07)

- `src/logging/**` request JSON + `/metrics` primitives (no Grafana) · `src/app/dashboard.rs:27-38` anon last-5 orders (id/status/total; `guest_email` unused in view) · `src/config.rs:3,13` Debug secrets · `src/main.rs:53` no shutdown · `src/api.rs:21` static health · `src/app.rs` BodyLimit gaps + dual unbounded home queries · `src/auth/session_config.rs:22-35,54-89` cookie hardening gap · `src/auth/routes/oauth.rs:25,72,230,246-280,472-505` state TTL good / default-true + always-link-by-email / link CSRF · `src/auth/routes/delete_user.rs:65,92,141-189` log-leak + token-only delete · `src/auth/routes/change_email.rs:72-82,140-219` oracle + token-only change · `src/auth/service.rs:29,134-149,355-523,645,691,785-851,1037-1121,1475-1625` policy/tokens/ban/role gaps · `src/auth/totp.rs:27-74` replay + weak backup hash · `src/auth/routes/sign_in.rs:78-174,226` brute-force + reflect · `src/auth/routes/sign_up.rs:57-83` weak validation + oracle · `src/app/settings.rs:1327-1330` email-only delete · `src/app/two_factor.rs:263-295` pending bearer + ignored trust_device · `src/app/checkout.rs:100-104,135-143,190,308-331,667-805,849-918,958-1031` quote-OK / shipping rule / pay_blocked ignores PIX config / manual PIX (extract service, §2.0) · `src/app/store/cart.rs:315-599,690-750` clamp-OK / lock gaps / i32 sums · `src/app/store/inventory.rs:67-302,336-343` lock-OK / fallback negative-reserved / 48h hold · `src/app/store/coupons.rs:97-210` validation-OK / guest bypass / burn-on-cancel · `src/app/pedido/id.rs:60-73,190-261,370` token-or-owner OK / missing≡unauthorized / 24v48h · `src/app/admin.rs:30-63` guard-OK / revenue inflated · `src/app/admin/configuracoes.rs:1-117` no POST · `src/app/admin/pedidos/id.rs:93-122` `pending_payment` dead-ends · `src/app/admin/cupons.rs:836-877` fixed-cap gap · `src/app/admin/produtos.rs:100-135,214,297-321,492-531` pagination-OK / display-reserved gap / buffered upload · `src/app/store/queries.rs:132-200` unbounded list · `migrations/0002_commerce.sql:7,66-89` enum/invariant gaps · `migrations/0003_auth_sessions.sql:17-40` plaintext tokens + secret index · `migrations/0006_inventory_reservations.sql:15-16` coupon FK gap · `src/db.rs:5-8` boot-migrate + pool=5 · `src/valkey.rs:6` no PING · `src/storage.rs:22,41,247-401` boot-time client (OK) / warn-only bucket / body leak / `ObjectStore` backends (copy for `PaymentProvider`, §2.0) · `src/auth/google.rs:105-161` per-req `Client::new()` (shared client + retry/CB, §2.0) + safe_callback good · `src/openapi.rs:33,187,196` docs gate + CDN SRI gap · `tests/common/mod.rs:35,90-91` dev-forced + router parity gap
