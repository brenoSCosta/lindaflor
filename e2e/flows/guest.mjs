// Flow 4: guest journey — checkout without an account, open the order
// from the emailed link (access token), cancel it, stock released.
import { env, expect, goto, mail, mainText, newBrowser, sql, step, uniq } from '../lib.mjs';
import { addVariant, fillAddress, placeOrder } from './customer.mjs';

export function liveAvailable(sku) {
  return Number(sql(`select l.live_available from inventory_live l join product_variants v on v.id=l.variant_id where v.sku='${sku}'`));
}

export async function guestJourney({ state }) {
  const g = await newBrowser('guest');
  const email = `visitante-${uniq}@example.com`;
  const sku = sql(`select sku from product_variants where product_id='${state.productID}' and size='g'`);
  let orderID;
  let before;

  await step(g, 'guest checkout (Mercado Pago PIX)', async () => {
    before = liveAvailable(sku);
    await addVariant(g, state.productSlug, sku);
    await goto(g, '/checkout');
    expect((await g.page.locator('#save_address').count()) === 0, 'guests must not see "Salvar este endereço"');
    await fillAddress(g.page, { email, name: 'Visitante Teste' });
    orderID = await placeOrder(g.page);
    expect(liveAvailable(sku) === before - 1, `reserved: ${before} → ${liveAvailable(sku)}`);
    expect(sql(`select coalesce(user_id::text,'') from orders where id='${orderID}'`) === '', 'guest order has a user');
  });

  await step(g, 'emailed guest link opens the order (token)', async () => {
    const m = await mail.wait(email, /Pedido #\w+ recebido/);
    const link = mail.link(m, new RegExp(`/pedido/${orderID}\\?t=`));
    // A fresh browser: only the token grants access.
    const other = await newBrowser('guest-link');
    try {
      const res = await goto(other, link);
      expect(res.status() === 200, `guest link → ${res.status()}`);
      expect((await mainText(other)).includes(email), 'order page does not show the guest e-mail');
      other.allow(new RegExp(`/pedido/${orderID}$`));
      const denied = await goto(other, `/pedido/${orderID}`);
      expect(denied.status() === 404, `order without token → ${denied.status()}`);
    } finally {
      await other.ctx.close();
    }
    state.guestLink = link;
  });

  await step(g, 'cancel the unpaid order; stock released', async () => {
    const t0 = Date.now() - 1000;
    await goto(g, state.guestLink);
    const f = g.page.locator(`form[action="/pedido/${orderID}/cancelar"]`);
    g.page.once('dialog', (d) => d.accept());
    await Promise.all([g.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
    await g.page.waitForTimeout(500);
    expect(sql(`select status from orders where id='${orderID}'`) === 'cancelled', `status ${sql(`select status from orders where id='${orderID}'`)}`);
    expect(liveAvailable(sku) === before, `stock after cancel ${liveAvailable(sku)} (was ${before})`);
    await mail.wait(email, 'cancelado', { after: t0 });
  });

  await g.ctx.close();
}
