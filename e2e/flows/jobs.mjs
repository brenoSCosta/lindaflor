// Flow 5: background jobs (run the server with RESERVATION_SWEEP_SECS=5 and
// ABANDONED_CART_AFTER_MINUTES=1) — an unpaid order expires and releases
// its stock, the abandoned-cart reminder and its one-click unsubscribe,
// back-in-stock alert + restock in /admin/estoque + cancel link, newsletter
// welcome mail.
import { env, expect, goto, login, mail, mainText, newBrowser, sql, step, uniq, waitFor } from '../lib.mjs';
import { addVariant, fillAddress, placeOrder } from './customer.mjs';
import { liveAvailable } from './guest.mjs';

export async function backgroundJobs({ state }) {
  const g = await newBrowser('jobs-guest');
  const sku = sql(`select sku from product_variants where product_id='${state.productID}' and size='g'`);

  await step(g, 'unpaid order expires and releases its stock', async () => {
    const email = `expira-${uniq}@example.com`;
    const before = liveAvailable(sku);
    await addVariant(g, state.productSlug, sku);
    await goto(g, '/checkout');
    await fillAddress(g.page, { email });
    const orderID = await placeOrder(g.page);
    expect(liveAvailable(sku) === before - 1, 'not reserved');
    // Waiting out the real reservation window (and the 1 h MP grace) is not
    // practical: move the deadline into the past and let the sweeper run.
    sql(`update orders set reservation_expires_at = now() - interval '2 hours' where id='${orderID}'`);
    await waitFor(() => sql(`select status from orders where id='${orderID}'`) === 'cancelled', { timeout: 30000, what: 'order to expire (cancelled)' });
    expect(liveAvailable(sku) === before, `stock after expiry ${liveAvailable(sku)} (was ${before})`);
    await mail.wait(email, 'expirado');
  });

  const cartEmail = `carrinho-${uniq}@example.com`;
  await step(g, 'abandoned-cart reminder with one-click unsubscribe', async () => {
    const b = await newBrowser('abandoner');
    try {
      await addVariant(b, state.productSlug, sku);
      // The checkout remembers the typed e-mail on a coupon preview.
      await goto(b, '/checkout');
      await b.page.fill('#email', cartEmail);
      await b.page.locator('input[name=coupon_code]').fill('NAOEXISTE');
      b.allow(/\/checkout$/);
      await Promise.all([b.page.waitForLoadState('networkidle'), b.page.locator('button[value=apply]').click()]);
      await waitFor(() => sql(`select count(*) from carts where contact_email='${cartEmail}'`) === '1', { what: 'checkout e-mail remembered' });
    } finally {
      await b.ctx.close();
    }
    const m = await mail.wait(cartEmail, 'Suas peças estão esperando', { timeout: 150000 });
    const headers = await (await fetch(`${env.mailpit}/api/v1/message/${m.ID}/headers`)).json();
    const lu = (headers['List-Unsubscribe'] || [])[0] || '';
    expect(/One-Click/.test((headers['List-Unsubscribe-Post'] || [])[0] || ''), `List-Unsubscribe-Post: ${JSON.stringify(headers['List-Unsubscribe-Post'])}`);
    const url = lu.match(/<(https?:[^>]+)>/)?.[1];
    expect(url && url.startsWith(env.base), `List-Unsubscribe: ${lu}`);
    expect(mail.links(m).some((l) => l.includes('/newsletter/cancelar')), 'no unsubscribe link in the body');
    // RFC 8058 one-click: a cross-site, cookie-less POST.
    const res = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded', Origin: 'https://mail.example.net' }, body: 'List-Unsubscribe=One-Click' });
    expect(res.status === 200, `one-click unsubscribe → ${res.status}`);
    expect(sql(`select count(*) from newsletter_subscribers where lower(email)='${cartEmail}' and unsubscribed_at is not null`) === '1', 'not suppressed');
  });

  await step(g, 'back-in-stock: subscribe, restock in /admin/estoque, mail + cancel link', async () => {
    const email = `aviso-${uniq}@example.com`;
    const soldOut = sql(`select v.id from product_variants v join inventory_live l on l.variant_id=v.id where v.product_id='${state.productID}' and l.live_available <= 0 limit 1`);
    expect(soldOut, 'no sold-out variant');
    await goto(g, `/produtos/${state.productSlug}?variant=${soldOut}`);
    const f = g.page.locator('form[action="/avise-me"]');
    expect(await f.count(), 'no avise-me form on a sold-out variant');
    await f.locator('input[name=email]').fill(email);
    await Promise.all([g.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
    expect(sql(`select count(*) from stock_alerts where email_norm='${email}'`) === '1', 'alert not stored');

    const a = await newBrowser('admin-stock');
    try {
      await login(a, env.adminEmail, env.adminPassword);
      const invID = sql(`select id from inventory where variant_id='${soldOut}'`);
      const vsku = sql(`select sku from product_variants where id='${soldOut}'`);
      await goto(a, `/admin/estoque?q=${encodeURIComponent(vsku)}`);
      const row = a.page.locator(`form:has(input[name=inventory_id][value="${invID}"])`);
      expect(await row.count(), 'variant not in /admin/estoque');
      await row.locator('input[name=delta]').fill('4');
      await row.locator('input[name=reason]').fill('Reposição e2e');
      await Promise.all([a.page.waitForLoadState('networkidle'), row.locator('button[type=submit]').click()]);
      expect(liveAvailable(vsku) === 4, `stock after restock ${liveAvailable(vsku)}`);
    } finally {
      await a.ctx.close();
    }
    const m = await mail.wait(email, 'voltou ao estoque', { timeout: 30000 });
    const cancel = mail.link(m, /\/avise-me\/cancelar/);
    await goto(g, cancel);
    await Promise.all([g.page.waitForLoadState('networkidle'), g.page.locator('main form button[type=submit]').first().click()]);
    expect(/cancel/i.test(await mainText(g)), `cancel page: ${(await mainText(g)).slice(0, 200)}`);
  });

  await step(g, 'newsletter subscribe → welcome mail', async () => {
    const email = `news-${uniq}@example.com`;
    await goto(g, '/');
    const f = g.page.locator('#newsletter form[action="/newsletter"]');
    expect(await f.count(), 'no newsletter form on the home page');
    await f.locator('input[name=email]').fill(email);
    await f.locator('input[name=name]').fill('Leitora');
    await Promise.all([g.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
    const m = await mail.wait(email, 'Boas-vindas à newsletter');
    expect(mail.links(m).some((l) => l.includes('/newsletter/cancelar')), 'welcome mail has no unsubscribe link');
  });

  await g.ctx.close();
}
