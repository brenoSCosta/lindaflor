// Flow 7: production-mode smoke behind TLS (APP_ENV=production, __Host-
// cookies, production CSP/HSTS): login, add to cart, checkout. Run with
// E2E_BASE_URL=https://localhost:<port> E2E_IGNORE_HTTPS_ERRORS=1.
import { env, expect, goto, login, newBrowser, sql, step } from '../lib.mjs';
import { addVariant, fillAddress, placeOrder } from './customer.mjs';

export async function prodSmoke({ state }) {
  const b = await newBrowser('prod');
  const violations = [];
  b.page.on('console', (m) => {
    if (/Content Security Policy|Mixed Content/i.test(m.text())) violations.push(m.text());
  });

  await step(b, 'https home: HSTS + CSP headers', async () => {
    const res = await goto(b, '/');
    expect(env.base.startsWith('https://'), 'E2E_BASE_URL must be https for the production smoke');
    const h = res.headers();
    expect(/max-age=\d+/.test(h['strict-transport-security'] || ''), `HSTS: ${h['strict-transport-security']}`);
    expect(/default-src 'self'/.test(h['content-security-policy'] || ''), `CSP: ${h['content-security-policy']}`);
  });

  await step(b, 'login sets a __Host- session cookie', async () => {
    await login(b, env.adminEmail, env.adminPassword);
    const cookies = await b.ctx.cookies();
    const sess = cookies.find((c) => c.name === '__Host-session');
    expect(sess && sess.secure && sess.httpOnly && sess.path === '/' && !sess.domain.startsWith('.'), `cookies: ${cookies.map((c) => c.name).join(',')}`);
  });

  await step(b, 'add to cart and check out (static PIX)', async () => {
    const sku = sql(`select v.sku from product_variants v join inventory_live l on l.variant_id=v.id join products p on p.id=v.product_id where p.active and l.live_available > 2 order by v.sku limit 1`);
    const slug = sql(`select p.slug from products p join product_variants v on v.product_id=p.id where v.sku='${sku}'`);
    await addVariant(b, slug, sku);
    const cookies = await b.ctx.cookies();
    expect(cookies.some((c) => c.name.startsWith('__Host-')), 'no __Host- cookies');
    await goto(b, '/checkout');
    await fillAddress(b.page, {});
    const id = await placeOrder(b.page);
    expect(sql(`select status from orders where id='${id}'`) === 'pending_payment', 'order not created');
    await b.page.locator('img[alt="QR Code PIX"]').waitFor();
    state.prodOrderID = id;
  });

  await step(b, 'no CSP or mixed-content violations', async () => {
    expect(violations.length === 0, violations.slice(0, 3).join(' | '));
  });

  await b.ctx.close();
}
