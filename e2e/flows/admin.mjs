// Flow 1: admin catalog against real S3 — product with images, variants
// and stock, edit (delete an image, change the price), a set with slots, a
// home banner, presigned image URLs on the storefront, avatar cropper.
import { env, expect, goto, login, mainText, newBrowser, pngFile, s3List, shot, sql, step, uniq, waitFor } from '../lib.mjs';

// imgsLoaded asserts every <img> under sel finished loading (naturalWidth
// > 0) and that its src is a presigned S3 URL when s3 is true.
async function imgsLoaded(b, sel, { s3 = true, min = 1 } = {}) {
  const imgs = b.page.locator(sel);
  await waitFor(async () => (await imgs.count()) >= min, { what: `${min} images at ${sel}` });
  await b.page.waitForFunction((s) => [...document.querySelectorAll(s)].every((i) => i.complete), sel, { timeout: 15000 }).catch(() => {});
  const info = await imgs.evaluateAll((els) => els.map((i) => ({ src: i.currentSrc || i.src, w: i.naturalWidth })));
  for (const i of info) {
    expect(i.w > 0, `image did not load: ${i.src.slice(0, 120)}`);
    if (s3) expect(/X-Amz-Signature=/.test(i.src) && i.src.startsWith(env.s3), `not a presigned S3 URL: ${i.src.slice(0, 120)}`);
  }
  // And fetch them directly: 200 with an image content type.
  for (const i of info.slice(0, 4)) {
    const r = await fetch(i.src);
    expect(r.status === 200 && /^image\//.test(r.headers.get('content-type') || ''), `GET ${i.src.slice(0, 80)} → ${r.status} ${r.headers.get('content-type')}`);
  }
  return info;
}

export async function adminCatalog({ state }) {
  const a = await newBrowser('admin');
  const slug = `maio-e2e-${uniq}`;
  const setSlug = `conjunto-e2e-${uniq}`;
  state.productSlug = slug;
  state.setSlug = setSlug;

  await step(a, 'admin login', async () => {
    await login(a, env.adminEmail, env.adminPassword);
    expect(!a.page.url().includes('/login'), `still on ${a.page.url()}`);
  });

  await step(a, 'create product with 3 images and 4 variants', async () => {
    await goto(a, '/admin/produtos/novo');
    const p = a.page;
    await p.fill('#name', `Maiô E2E ${uniq}`);
    await p.fill('#slug', slug);
    await p.fill('#description', 'Maiô criado pelo teste ponta a ponta.');
    await p.fill('#price', '159.90');
    await p.fill('#compare_at_price', '199.90');
    await p.selectOption('#category', { index: 1 });
    await p.check('#featured');
    await p.setInputFiles('#images', [pngFile('frente.png', [220, 60, 90]), pngFile('costas.png', [40, 120, 200]), pngFile('detalhe.png', [250, 190, 40])]);
    // The form renders one variant row; the handler takes arrays, so add
    // more rows the way a browser would submit them.
    await p.evaluate(() => {
      const row = document.querySelector('input[name="variant_sku[]"]').closest('.grid');
      for (let i = 0; i < 3; i++) row.parentElement.appendChild(row.cloneNode(true));
    });
    const variants = [
      ['p', 'Coral', '#ff7f50', '5'],
      ['m', 'Coral', '#ff7f50', '3'],
      ['g', 'Azul', '#1e90ff', '2'],
      ['m', 'Azul', '#1e90ff', '0'],
    ];
    const rows = p.locator('input[name="variant_sku[]"]');
    for (let i = 0; i < variants.length; i++) {
      const [size, color, hex, qty] = variants[i];
      await rows.nth(i).fill(`E2E-${uniq}-${size}-${color}`.toUpperCase());
      await p.locator('select[name="variant_size[]"]').nth(i).selectOption(size);
      await p.locator('input[name="variant_color[]"]').nth(i).fill(color);
      await p.locator('input[name="variant_color_hex[]"]').nth(i).fill(hex);
      await p.locator('input[name="variant_quantity[]"]').nth(i).fill(qty);
    }
    await Promise.all([p.waitForURL(/\/admin\/produtos\/[0-9a-f-]{36}/, { timeout: 20000 }), p.getByRole('button', { name: 'Criar produto' }).click()]);
    state.productID = p.url().match(/[0-9a-f-]{36}/)[0];
    const n = sql(`select count(*) from product_images where product_id='${state.productID}'`);
    expect(n === '3', `product_images = ${n}`);
    const stock = sql(`select string_agg(v.size||'/'||v.color||'='||i.quantity, ',' order by v.sku) from product_variants v join inventory i on i.variant_id=v.id where v.product_id='${state.productID}'`);
    expect(stock.split(',').length === 4, `variants/stock: ${stock}`);
    await imgsLoaded(a, 'main img[alt="Imagem do produto"]', { min: 3 });
  });

  await step(a, 'images are in the bucket', async () => {
    const keys = sql(`select string_agg(url, ' ') from product_images where product_id='${state.productID}'`).split(' ');
    const listed = await s3List();
    for (const k of keys) expect(listed.includes(k), `S3 is missing ${k}`);
    state.productImageKeys = keys;
  });

  await step(a, 'edit: delete an image, change the price, add a variant', async () => {
    await goto(a, `/admin/produtos/${state.productID}`);
    const p = a.page;
    await p.locator('input[name="delete_image_id"]').first().check();
    await p.fill('#price', '149.90');
    // The blank last row adds a variant (stock 0, restocked in the jobs flow).
    const last = (sel) => p.locator(sel).last();
    await last('input[name="variant_sku[]"]').fill(`E2E-${uniq}-GG-AZUL`.toUpperCase());
    await last('input[name="variant_size[]"]').fill('gg');
    await last('input[name="variant_color[]"]').fill('Azul');
    await last('input[name="variant_color_hex[]"]').fill('#1e90ff');
    await Promise.all([p.waitForLoadState('networkidle'), p.getByRole('button', { name: 'Salvar alterações' }).click()]);
    await p.waitForTimeout(500);
    const row = sql(`select price_in_cents from products where id='${state.productID}'`);
    expect(row === '14990', `price_in_cents = ${row}`);
    const n = sql(`select count(*) from product_images where product_id='${state.productID}'`);
    expect(n === '2', `images after delete = ${n}`);
    const nv = sql(`select count(*) from product_variants v join inventory i on i.variant_id=v.id where v.product_id='${state.productID}'`);
    expect(nv === '5', `variants after edit = ${nv}`);
    const keys = sql(`select string_agg(url, ' ') from product_images where product_id='${state.productID}'`).split(' ');
    const gone = state.productImageKeys.find((k) => !keys.includes(k));
    const listed = await s3List();
    expect(!listed.includes(gone), `deleted image still in S3: ${gone}`);
  });

  await step(a, 'storefront product renders presigned images', async () => {
    await goto(a, `/produtos/${slug}`);
    expect((await mainText(a)).includes('149,90'), 'new price not shown');
    await imgsLoaded(a, 'main img[src*="X-Amz-Signature"]', { min: 1 });
    await shot(a, 'admin-product-storefront');
  });

  await step(a, 'create a set and add slots', async () => {
    await goto(a, '/admin/sets');
    const p = a.page;
    await p.fill('#name', `Conjunto E2E ${uniq}`);
    await p.fill('#slug', setSlug);
    await p.fill('#description', 'Conjunto criado pelo teste.');
    await p.fill('#price', '249.90');
    await p.setInputFiles('#images', [pngFile('set.png', [30, 160, 90])]);
    await Promise.all([p.waitForLoadState('networkidle'), p.getByRole('button', { name: /Criar/ }).click()]);
    state.setID = sql(`select id from sets where slug='${setSlug}'`);
    expect(state.setID, 'set not created');
    await goto(a, `/admin/sets/${state.setID}`);
    // Two members: our product and a seeded active product.
    const boxes = p.locator('input[name="member_product_ids[]"]');
    const ours = p.locator(`input[name="member_product_ids[]"][value="${state.productID}"]`);
    if (await ours.count()) await ours.check();
    await boxes.first().check();
    await Promise.all([p.waitForLoadState('networkidle'), p.getByRole('button', { name: 'Salvar alterações' }).click()]);
    await goto(a, `/admin/sets/${state.setID}`);
    // Fill slots: one per blank row, saving in between.
    for (let i = 0; i < 2; i++) {
      const labels = p.locator('input[name="slot_label[]"]');
      const last = (await labels.count()) - 1;
      await labels.nth(last).fill(i === 0 ? 'Parte de cima' : 'Parte de baixo');
      const sel = p.locator('select[name="slot_variant[]"]').nth(last);
      const opts = await sel.locator('option').evaluateAll((os) => os.map((o) => o.value).filter(Boolean));
      expect(opts.length > 1, 'no variants to choose for slots');
      await sel.selectOption(opts[i]);
      await Promise.all([p.waitForLoadState('networkidle'), p.getByRole('button', { name: 'Salvar alterações' }).click()]);
      await goto(a, `/admin/sets/${state.setID}`);
    }
    const slots = sql(`select count(*) from set_slots where set_id='${state.setID}'`);
    expect(Number(slots) >= 2, `set_slots = ${slots}`);
    await goto(a, `/sets/${setSlug}`);
    await imgsLoaded(a, 'main img[src*="X-Amz-Signature"]', { min: 1 });
  });

  await step(a, 'create a home banner with an image', async () => {
    await goto(a, '/admin/banners');
    const p = a.page;
    await p.fill('#title', `Verão E2E ${uniq}`);
    await p.fill('#subtitle', 'Coleção nova');
    await p.fill('#cta_label', 'Ver coleção');
    await p.fill('#link_url', '/produtos?sort=novos');
    await p.setInputFiles('#images', [pngFile('banner.png', [120, 60, 200], 1200, 500)]);
    await Promise.all([p.waitForLoadState('networkidle'), p.locator('form[action="/admin/banners/create"] button[type=submit]').click()]);
    const key = sql(`select image_url from home_banners where title='Verão E2E ${uniq}'`);
    expect(key, 'banner not created');
    expect((await s3List()).includes(key), `banner image ${key} not in S3`);
    await goto(a, '/');
    expect((await mainText(a)).includes(`Verão E2E ${uniq}`), 'banner not on home');
    await imgsLoaded(a, 'main img[src*="X-Amz-Signature"]', { min: 1 });
    await shot(a, 'admin-home-banner');
  });

  await step(a, 'avatar upload with the cropper', async () => {
    await goto(a, '/settings');
    const p = a.page;
    await p.click('#avatar-cropper-open');
    await p.setInputFiles('#avatar-cropper-file', [pngFile('avatar.png', [200, 100, 50], 400, 400)]);
    await p.locator('#avatar-cropper-apply:not([disabled])').waitFor({ timeout: 10000 });
    await p.click('#avatar-cropper-zoom-in');
    await Promise.all([p.waitForResponse((r) => r.url().includes('/settings') && r.request().method() === 'POST', { timeout: 15000 }), p.click('#avatar-cropper-apply')]);
    await p.waitForLoadState('networkidle');
    await p.waitForTimeout(800);
    await goto(a, '/settings');
    const img = sql(`select coalesce(image,'') from users where email='${env.adminEmail}'`);
    expect(img, 'user.image not set');
    // Sidebar avatar (app shell) loads from S3.
    await imgsLoaded(a, 'img[src*="/avatars/"]', { min: 2 });
    await shot(a, 'admin-avatar');
  });

  await a.ctx.close();
}
