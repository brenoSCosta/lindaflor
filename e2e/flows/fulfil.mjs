// Flow 3: admin fulfilment — processing → shipped (tracking) → delivered
// with the customer mails, a review approved into the product page's
// JSON-LD, a return approved/received/refunded through Mercado Pago, and
// the reports page + CSV export.
import { env, expect, fakemp, goto, login, mail, mainText, newBrowser, shot, sql, step, uniq } from '../lib.mjs';

async function orderAction(a, orderID, status, extra = {}) {
  await goto(a, `/admin/pedidos/${orderID}`);
  const p = a.page;
  const form = p.locator(`form:has(input[name=status][value="${status}"])`).first();
  expect(await form.count(), `no "${status}" action on the order page: ${(await mainText(a)).slice(0, 300)}`);
  for (const [k, v] of Object.entries(extra)) await form.locator(`[name="${k}"]`).fill(v);
  await Promise.all([p.waitForLoadState('networkidle'), form.locator('button[type=submit]').click()]);
  await p.waitForTimeout(300);
}

function orderStatus(id) {
  return sql(`select status from orders where id='${id}'`);
}

export async function fulfilment({ state }) {
  const a = await newBrowser('admin');
  const c = await newBrowser('customer');
  const email = state.customerEmail;
  const orderID = state.orderID;
  const code = sql(`select upper(left(id::text, 8)) from orders where id='${orderID}'`);

  await step(a, 'admin + customer login', async () => {
    await login(a, env.adminEmail, env.adminPassword);
    await login(c, email, state.customerPassword);
  });

  await step(a, 'paid → processing', async () => {
    await orderAction(a, orderID, 'processing');
    expect(orderStatus(orderID) === 'processing', `status ${orderStatus(orderID)}`);
  });

  await step(a, 'processing → shipped with tracking (mail)', async () => {
    const t0 = Date.now() - 1000;
    await orderAction(a, orderID, 'shipped', { tracking_code: 'QB123456789BR' });
    expect(orderStatus(orderID) === 'shipped', `status ${orderStatus(orderID)}`);
    const m = await mail.wait(email, 'enviado', { after: t0 });
    expect((m.Text || m.HTML).includes('QB123456789BR'), 'tracking code missing from the shipped mail');
  });

  await step(a, 'shipped → delivered', async () => {
    await orderAction(a, orderID, 'delivered');
    expect(orderStatus(orderID) === 'delivered', `status ${orderStatus(orderID)}`);
    await goto(c, `/pedido/${orderID}`);
    expect(/Entregue/i.test(await c.page.locator('h1').first().innerText()), 'customer order page not delivered');
  });

  await step(c, 'customer leaves a review', async () => {
    await goto(c, `/produtos/${state.productSlug}`);
    const f = c.page.locator('form[action="/avaliacoes/enviar"]');
    expect(await f.count(), 'no review form for a buyer of a delivered order');
    await f.locator('input[name=rating][value="5"]').check({ force: true });
    await f.locator('input[name=title]').fill(`Amei ${uniq}`);
    await f.locator('textarea[name=body]').fill('Tecido ótimo, caimento perfeito e chegou rápido.');
    await Promise.all([c.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
    await c.page.waitForTimeout(400);
    const st = sql(`select status from product_reviews where product_id='${state.productID}' and title='Amei ${uniq}'`);
    expect(st === 'pending', `review status ${st}`);
  });

  await step(a, 'admin approves the review; JSON-LD has aggregateRating', async () => {
    await goto(a, '/admin/avaliacoes');
    const id = sql(`select id from product_reviews where title='Amei ${uniq}'`);
    const f = a.page.locator(`form:has(input[name=review_id][value="${id}"]):has(input[name=action][value=approve])`);
    await Promise.all([a.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
    expect(sql(`select status from product_reviews where id='${id}'`) === 'approved', 'not approved');
    await goto(c, `/produtos/${state.productSlug}`);
    expect((await mainText(c)).includes(`Amei ${uniq}`), 'review not on product page');
    const ld = await c.page.locator('script[type="application/ld+json"]').allTextContents();
    const product = ld.map((t) => JSON.parse(t)).flatMap((j) => (Array.isArray(j) ? j : j['@graph'] || [j])).find((j) => j['@type'] === 'Product');
    expect(product?.aggregateRating, `no aggregateRating in JSON-LD: ${ld.join(' ').slice(0, 300)}`);
    expect(Number(product.aggregateRating.ratingValue) === 5 && Number(product.aggregateRating.reviewCount) === 1, `aggregateRating ${JSON.stringify(product.aggregateRating)}`);
  });

  let returnID;
  await step(c, 'customer requests a return', async () => {
    const t0 = Date.now() - 1000;
    await goto(c, `/pedido/${orderID}/troca`);
    // Return the whole order so the admin can refund it in full through
    // Mercado Pago (a partial return is a manually recorded refund).
    const qtys = c.page.locator('select[name^="qty_"]');
    expect(await qtys.count(), `no return form: ${(await mainText(c)).slice(0, 300)}`);
    for (let i = 0; i < (await qtys.count()); i++) {
      const max = await qtys.nth(i).locator('option').last().getAttribute('value');
      await qtys.nth(i).selectOption(max);
    }
    const kinds = await c.page.locator('#kind option').evaluateAll((os) => os.map((o) => o.value));
    await c.page.selectOption('#kind', kinds.includes('return') ? 'return' : kinds[0]);
    await c.page.fill('#reason', 'Ficou grande, prefiro devolver.');
    await Promise.all([c.page.waitForLoadState('networkidle'), c.page.getByRole('button', { name: 'Enviar solicitação' }).click()]);
    returnID = sql(`select id from return_requests where order_id='${orderID}' order by created_at desc limit 1`);
    expect(returnID, 'return request not created');
    await mail.wait(email, 'Solicitação de troca/devolução', { after: t0 });
  });

  await step(a, 'admin approves, receives and refunds via Mercado Pago', async () => {
    const t0 = Date.now() - 1000;
    const act = async (action) => {
      await goto(a, `/admin/trocas/${returnID}`);
      const f = a.page.locator(`form:has(input[name=action][value="${action}"])`).first();
      expect(await f.count(), `no "${action}" action: ${(await mainText(a)).slice(0, 300)}`);
      await Promise.all([a.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
      await a.page.waitForTimeout(300);
    };
    await act('approve');
    expect(sql(`select status from return_requests where id='${returnID}'`) === 'approved', 'not approved');
    await mail.wait(email, 'Troca/devolução aprovada', { after: t0 });
    await act('receive');
    expect(sql(`select status from return_requests where id='${returnID}'`) === 'received', 'not received');
    await act('mp_refund');
    const st = sql(`select status from return_requests where id='${returnID}'`);
    expect(st === 'resolved' || st === 'refunded', `return status ${st}`);
    const pays = await fakemp.payments();
    const mp = pays.find((x) => x.external_reference === orderID);
    expect(mp && mp.refunds >= 1, `fake MP saw no refund: ${JSON.stringify(mp)}`);
    await mail.wait(email, /Reembolso|concluída/, { after: t0 });
    await shot(a, 'fulfil-return-resolved');
  });

  await step(a, 'reports page and CSV export include the order', async () => {
    await goto(a, '/admin/relatorios');
    expect((await mainText(a)).length > 100, 'empty reports page');
    const res = await a.page.request.get(`${env.base}/admin/relatorios/pedidos.csv`);
    expect(res.status() === 200, `csv ${res.status()}`);
    const csv = await res.text();
    expect(csv.includes(orderID) || csv.toUpperCase().includes(code), 'order missing from the CSV');
  });

  await a.ctx.close();
  await c.ctx.close();
}
