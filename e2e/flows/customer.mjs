// Flow 2: customer journey — sign up + verification mail, browse/filter,
// cart with a product and a set, quantities, coupon, checkout with a saved
// address, Mercado Pago PIX approved by a signed webhook, status poll,
// payment mail, stock decrement.
import { env, expect, fakemp, goto, mail, mainText, newBrowser, shot, sql, step, uniq, waitFor } from '../lib.mjs';

export const customerPassword = 'Praia-Linda-2026!x';

export async function signUp(b, name, email, password) {
  await goto(b, '/login?mode=signup');
  await b.page.fill('form[action="/signup"] #name, #name', name);
  await b.page.fill('#email', email);
  await b.page.fill('#password', password);
  await Promise.all([b.page.waitForURL((u) => !u.pathname.startsWith('/login'), { timeout: 15000 }), b.page.locator('button[type=submit]').last().click()]);
  await b.page.waitForLoadState('networkidle').catch(() => {});
}

// addVariant opens the product page with a variant selected and adds it.
export async function addVariant(b, slug, sku) {
  const vid = sql(`select id from product_variants where sku='${sku}'`);
  await goto(b, `/produtos/${slug}?variant=${vid}`);
  const btn = b.page.locator('form[hx-post="/carrinho/adicionar"] button[type=submit]');
  await Promise.all([b.page.waitForResponse((r) => r.url().endsWith('/carrinho/adicionar')), btn.click()]);
  await b.page.waitForTimeout(400);
  return vid;
}

export async function cartCount(b) {
  return Number((await b.page.locator('#cart-count').first().innerText().catch(() => '0')).trim() || 0);
}

export async function fillAddress(p, { email, name = 'Maria Praia', street = 'Rua das Conchas' } = {}) {
  if (email) await p.fill('#email', email);
  await p.fill('#name', name);
  await p.fill('#phone', '79999816511');
  await p.fill('#street', street);
  await p.fill('#number', '42');
  await p.fill('#complement', 'Casa 2');
  await p.fill('#neighborhood', 'Atalaia');
  await p.fill('#city', 'Aracaju');
  await p.selectOption('#state', 'SE');
  await p.fill('#zip', '49037-000');
}

export async function placeOrder(p) {
  await Promise.all([p.waitForURL(/\/pedido\/[0-9a-f-]{36}/, { timeout: 20000 }), p.locator('button[value=pay]').click()]);
  await p.waitForLoadState('networkidle').catch(() => {});
  return p.url().match(/\/pedido\/([0-9a-f-]{36})/)[1];
}

function stockOf(sku) {
  return Number(sql(`select i.quantity from inventory i join product_variants v on v.id=i.variant_id where v.sku='${sku}'`));
}

export async function customerJourney({ state }) {
  const c = await newBrowser('customer');
  const email = `cliente-${uniq}@example.com`;
  state.customerEmail = email;
  state.customerPassword = customerPassword;
  const skus = sql(`select string_agg(sku, ' ' order by sku) from product_variants where product_id='${state.productID}'`).split(' ');
  const coralP = skus.find((s) => s.endsWith('-P-CORAL'));
  const coralM = skus.find((s) => s.endsWith('-M-CORAL'));
  state.coralP = coralP;

  await step(c, 'sign up and verify email via Mailpit', async () => {
    const t0 = Date.now() - 1000;
    await signUp(c, 'Cliente Teste', email, customerPassword);
    const m = await mail.wait(email, 'Confirme seu e-mail', { after: t0 });
    const link = mail.link(m, /verify-email\?token=|verificar/);
    expect(link.startsWith(env.base), `verification link not on APP_ORIGIN: ${link}`);
    await goto(c, link);
    expect((await mainText(c)).includes('E-mail verificado') || c.page.url().includes('/dashboard'), `verify page: ${(await mainText(c)).slice(0, 200)}`);
    expect(sql(`select email_verified from users where email='${email}'`) === 't', 'email_verified false');
  });

  await step(c, 'browse, search and filter the catalog', async () => {
    await goto(c, '/produtos');
    const s = c.page.locator('#catalog-search');
    await s.fill('E2E');
    await c.page.waitForTimeout(1500);
    await waitFor(async () => (await c.page.locator(`a[href="/produtos/${state.productSlug}"]`).count()) > 0, { what: 'search result' });
    await goto(c, '/produtos?tamanho=m');
    expect((await c.page.locator('a[href^="/produtos/"]').count()) > 0, 'filtered catalog empty');
  });

  const before = {};
  await step(c, 'add product variants and a set to the cart', async () => {
    before.p = stockOf(coralP);
    before.m = stockOf(coralM);
    await addVariant(c, state.productSlug, coralP);
    await addVariant(c, state.productSlug, coralM);
    await goto(c, `/sets/${state.setSlug}`);
    const add = c.page.locator('form[hx-post="/conjuntos/adicionar"] button[type=submit]');
    expect(await add.isEnabled(), `set not addable: ${(await mainText(c)).slice(0, 300)}`);
    await Promise.all([c.page.waitForResponse((r) => r.url().endsWith('/conjuntos/adicionar')), add.click()]);
    await c.page.waitForTimeout(500);
    const n = await cartCount(c);
    expect(n >= 3, `cart badge ${n}`);
  });

  await step(c, 'change quantities in the cart sheet', async () => {
    await goto(c, '/carrinho/sheet');
    const plus = c.page.locator('button[aria-label="Aumentar"]').first();
    await Promise.all([c.page.waitForResponse((r) => r.url().endsWith('/carrinho/quantidade')), plus.click()]);
    await c.page.waitForTimeout(500);
    const q = sql(`select sum(ci.quantity) from cart_items ci join carts ca on ca.id=ci.cart_id join users u on u.id=ca.user_id where u.email='${email}'`);
    expect(Number(q) >= 4, `cart quantity ${q}`);
  });

  await step(c, 'apply coupon BEMVINDA10', async () => {
    await goto(c, '/carrinho/sheet');
    await c.page.fill('#cart-coupon', 'BEMVINDA10');
    await Promise.all([c.page.waitForResponse((r) => r.url().endsWith('/carrinho/cupom')), c.page.locator('form[action="/carrinho/cupom"] button[type=submit]').first().click()]);
    await c.page.waitForTimeout(500);
    const t = await c.page.locator('body').innerText();
    expect(/Cupom BEMVINDA10 aplicado/.test(t), `coupon not applied: ${t.slice(0, 400)}`);
  });

  await step(c, 'checkout with a new saved address (Mercado Pago)', async () => {
    await goto(c, '/checkout');
    const p = c.page;
    await fillAddress(p, {});
    const save = p.locator('#save_address');
    expect(await save.count(), 'no save_address checkbox');
    await save.check();
    await shot(c, 'customer-checkout');
    // The total the customer sees must be the total the order charges
    // (set bundle price included).
    const shown = await p.locator('aside').getByText(/^R\$/).last().innerText();
    state.orderID = await placeOrder(p);
    const charged = Number(sql(`select total_cents from orders where id='${state.orderID}'`));
    const shownCents = Number(shown.replace(/[^\d]/g, ''));
    expect(shownCents === charged, `checkout showed ${shown} but the order charges ${charged} cents`);
    const addr = sql(`select count(*) from user_addresses a join users u on u.id=a.user_id where u.email='${email}'`);
    expect(addr === '1', `saved addresses = ${addr}`);
    const o = sql(`select o.status||' '||coalesce(c.code,'')||' '||coalesce(o.mp_payment_id::text,'') from orders o left join coupons c on c.id=o.coupon_id where o.id='${state.orderID}'`);
    expect(/^pending_payment BEMVINDA10 \d+$/.test(o), `order row: ${o}`);
  });

  await step(c, 'order page shows the MP QR and copia-e-cola', async () => {
    const p = c.page;
    const qr = p.locator('img[alt="QR Code PIX"]');
    await qr.waitFor();
    expect(await qr.evaluate((i) => i.complete && i.naturalWidth > 0), 'QR image did not load');
    const code = await p.locator('code').first().innerText();
    expect(code.includes('fakemp'), `copia-e-cola not from MP: ${code.slice(0, 60)}`);
    await mail.wait(email, /Pedido #\w+ recebido/);
    await shot(c, 'customer-order-pending');
  });

  await step(c, 'approve via fake MP + signed webhook; poll flips to paid', async () => {
    const t0 = Date.now() - 1000;
    const res = await fakemp.approve(state.orderID);
    expect(res.webhook_status === 200 && /"ok"/.test(res.webhook_body), `webhook: ${JSON.stringify(res)}`);
    expect(sql(`select status from orders where id='${state.orderID}'`) === 'paid', 'order not paid in DB');
    // No reload: the 10 s htmx poll must pick it up.
    await waitFor(async () => /Pago|Pagamento confirmado/i.test(await c.page.locator('h1').first().innerText()), { timeout: 25000, what: 'order page to show paid' });
    await mail.wait(email, 'Pagamento confirmado', { after: t0 });
    expect(stockOf(coralP) === before.p - 2 || stockOf(coralP) === before.p - 1, `stock P ${before.p} → ${stockOf(coralP)}`);
    expect(stockOf(coralM) < before.m, `stock M ${before.m} → ${stockOf(coralM)}`);
    await shot(c, 'customer-order-paid');
  });

  await c.ctx.close();
}
