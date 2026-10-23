# End-to-end suite

A Playwright walk through the whole store against **real services**:
Postgres 17, Valkey, an S3 server ([versitygw](https://github.com/versity/versitygw)),
[Mailpit](https://mailpit.axllent.org/) as the SMTP server, a fake Mercado
Pago API (`fakemp/`) and a Caddy TLS proxy for the production-mode pass. It
is **not** part of `go test`: `fakemp/` is its own Go module, and nothing
here is built by `go build ./...`.

```bash
make e2e                       # from the dev shell (nix develop / devenv shell)
make e2e FLOWS="admin customer"
E2E_KEEP=1 make e2e FLOWS=jobs # leave the stack running afterwards
```

`make e2e` runs `e2e/stack.sh` under `nix shell` with mailpit, versitygw,
caddy, node and playwright (with its Chromium) from nixpkgs; go, postgres
and valkey come from the dev shell. The script builds the binaries, boots
everything on ports `4741–4753` (`E2E_PORT_BASE` moves them) with state in
`e2e/.out/`, migrates on boot, seeds, creates an extra admin, runs the
flows and stops the stack. Logs: `e2e/.out/log/`, failure screenshots:
`e2e/.out/shots/`. To use an npm-installed Playwright instead, `npm ci` in
`e2e/` (the script prefers `e2e/node_modules/playwright`).

Every step fails on console errors, failed requests, unexpected 4xx/5xx
responses and `ERROR`/`WARN`/5xx lines in the server log during the step.
External hosts (the seed's Unsplash images) are stubbed in the browser.

## Flows

| Flow | What it proves |
| --- | --- |
| `admin` | Product with 3 uploaded images, 4 size/colour variants and stock; edit (delete an image, change the price); set with slots; home banner; images in the bucket and loading on the storefront through presigned URLs; avatar via the cropper in the sidebar |
| `customer` | Sign-up + verification mail link, catalog search/filter, product + set in the cart, quantities, coupon `BEMVINDA10`, checkout with a saved address (shown total = charged total), Mercado Pago PIX (QR + copia-e-cola), approval + signed webhook, status poll flips to paid without reload, payment mail, stock decremented |
| `fulfil` | processing → shipped (tracking mail) → delivered; review → approval → product page + `aggregateRating` JSON-LD; return → approve → receive → Mercado Pago refund (seen by the fake) + mails; reports page and CSV |
| `guest` | Guest checkout, emailed `?t=` link opens the order (404 without it), cancel releases the stock |
| `jobs` | Expired unpaid order is cancelled by the sweeper and releases stock (its deadline is moved into the past), abandoned-cart reminder with a working RFC 8058 one-click unsubscribe, back-in-stock alert after a restock in `/admin/estoque` with its cancel link, newsletter welcome mail |
| `security` | 2FA enabled from settings (secret read off the setup page, codes generated in the script), TOTP login, single-use backup code, revoke other sessions, change password, change e-mail via the mail link, delete from settings and via the mailed `/excluir-conta` link |
| `prod` | `APP_ENV=production` behind `https://localhost` (Caddy, internal CA): HSTS + CSP, `__Host-` cookies, login → cart → checkout (static PIX), no CSP/mixed-content violations |

The flows share state through `e2e/.out/state.json` (`fulfil` uses the
order from `customer`, which uses the product from `admin`), so a subset
should keep that order.

## Fake Mercado Pago

`fakemp/` implements `POST /v1/payments` (pending PIX with `qr_code` and a
PNG `qr_code_base64`, idempotent per `X-Idempotency-Key`),
`GET /v1/payments/{id}` and `POST /v1/payments/{id}/refunds`, plus control
endpoints for the driver:

- `GET /_fake/payments` — every payment (with a refund count)
- `POST /_fake/approve?external_reference=<order id>` (or `?id=`) — mark it
  approved and POST a webhook signed like Mercado Pago
  (`x-signature: ts=…,v1=HMAC-SHA256(secret, "id:…;request-id:…;ts:…;")`)
  to the payment's `notification_url` (`FAKEMP_WEBHOOK_URL` overrides)

```bash
cd e2e/fakemp && MP_ACCESS_TOKEN=TEST-x MP_WEBHOOK_SECRET=s go run . -addr 127.0.0.1:4290
```

Point the app at it with `PAYMENT_PROVIDER=mercadopago
MP_API_BASE_URL=http://127.0.0.1:4290 MP_ACCESS_TOKEN=TEST-x
MP_WEBHOOK_SECRET=s` and `APP_ENV=development` (production requires an https
base URL).

## Not covered

- Google sign-in (covered by Go tests with a fake OAuth server).
- Real mail delivery in production mode: production refuses `SMTP_TLS=none`,
  so the `prod` pass uses `MAIL_TRANSPORT=log`.
- Turnstile is not configured (the default).
