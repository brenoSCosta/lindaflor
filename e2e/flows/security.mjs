// Flow 6: account security — enable 2FA from settings (secret read off the
// setup page), TOTP login, backup-code login, revoke other sessions, change
// password, change e-mail (mail link), delete the account (mail link).
import { env, expect, goto, mail, mainText, newBrowser, sleep, sql, step, totp, uniq, waitFor } from '../lib.mjs';
import { signUp } from './customer.mjs';

// A TOTP code is accepted once; wait for a fresh 30 s window when needed.
const used = new Set();
async function freshCode(secret) {
  for (;;) {
    const c = totp(secret);
    const left = 30 - (Math.floor(Date.now() / 1000) % 30);
    if (!used.has(c) && left > 3) {
      used.add(c);
      return c;
    }
    await sleep(left * 1000 + 300);
  }
}

async function loginWith2FA(b, email, password, second) {
  await goto(b, '/login');
  await b.page.locator('form[action="/login"] input[name=email]').fill(email);
  await b.page.locator('form[action="/login"] input[name=password]').fill(password);
  await Promise.all([b.page.waitForURL(/\/two-factor/, { timeout: 15000 }), b.page.locator('form[action="/login"] button[type=submit]').click()]);
  if (second.backup) {
    await goto(b, '/two-factor?method=backup');
    await b.page.fill('#backup_code', second.backup);
  } else {
    await b.page.fill('#code', second.code);
  }
  await Promise.all([b.page.waitForURL((u) => !u.pathname.startsWith('/two-factor') && !u.pathname.startsWith('/login'), { timeout: 15000 }), b.page.locator('main form button[type=submit], form button[type=submit]').last().click()]);
  await b.page.waitForLoadState('networkidle').catch(() => {});
}

async function settingsSubmit(b, tab, action, fill) {
  await goto(b, `/settings?tab=${tab}`);
  const f = b.page.locator(`form:has(input[name=action][value="${action}"])`).first();
  expect(await f.count(), `no ${action} form on tab ${tab}`);
  await fill(f);
  await Promise.all([b.page.waitForLoadState('networkidle'), f.locator('button[type=submit]').click()]);
  await b.page.waitForTimeout(600);
}

export async function accountSecurity() {
  const a = await newBrowser('sec-a');
  let email = `seguranca-${uniq}@example.com`;
  let password = 'Areia-Branca-2026!y';
  let secret;
  let backups;

  await step(a, 'sign up and enable 2FA from settings', async () => {
    await signUp(a, 'Segura Teste', email, password);
    await goto(a, '/settings?tab=security');
    const f = a.page.locator('form[action="/settings/two-factor"]');
    await f.locator('input[name=password]').fill(password);
    await Promise.all([a.page.waitForURL(/\/settings\/two-factor/), f.locator('button[type=submit]').click()]);
    secret = (await a.page.locator('code.font-mono').first().innerText()).replace(/\s+/g, '');
    backups = await a.page.locator('ul.font-mono li').allInnerTexts();
    expect(/^[A-Z2-7]{16,}$/.test(secret), `secret ${secret}`);
    expect(backups.length >= 8, `backup codes: ${backups.length}`);
    await a.page.fill('#setup-code', await freshCode(secret));
    await Promise.all([a.page.waitForLoadState('networkidle'), a.page.getByRole('button', { name: 'Confirmar 2FA' }).click()]);
    await a.page.waitForTimeout(500);
    expect(sql(`select two_factor_enabled from users where email='${email}'`) === 't', '2FA not enabled');
  });

  const b = await newBrowser('sec-b');
  const c = await newBrowser('sec-c');
  await step(b, 'log in with a TOTP code', async () => {
    await loginWith2FA(b, email, password, { code: await freshCode(secret) });
    expect(b.page.url().includes('/dashboard'), `after 2FA: ${b.page.url()}`);
  });

  await step(c, 'log in with a backup code (single use)', async () => {
    await loginWith2FA(c, email, password, { backup: backups[0] });
    expect(c.page.url().includes('/dashboard'), `after backup code: ${c.page.url()}`);
    const d = await newBrowser('sec-d');
    try {
      d.allow(/\/two-factor/);
      await goto(d, '/login');
      await d.page.locator('form[action="/login"] input[name=email]').fill(email);
      await d.page.locator('form[action="/login"] input[name=password]').fill(password);
      await Promise.all([d.page.waitForURL(/\/two-factor/), d.page.locator('form[action="/login"] button[type=submit]').click()]);
      await goto(d, '/two-factor?method=backup');
      await d.page.fill('#backup_code', backups[0]);
      await d.page.locator('form button[type=submit]').last().click();
      await d.page.waitForTimeout(1500);
      expect(d.page.url().includes('/two-factor'), `reused backup code accepted: ${d.page.url()}`);
    } finally {
      await d.ctx.close();
    }
  });

  await step(a, 'revoke other sessions', async () => {
    const n = Number(sql(`select count(*) from sessions s join users u on u.id=s.user_id where u.email='${email}'`));
    expect(n >= 3, `sessions ${n}`);
    await goto(a, '/settings?tab=sessions');
    await Promise.all([a.page.waitForResponse((r) => r.url().includes('revoke-other-sessions')), a.page.getByRole('button', { name: /Sair de todos os outros/ }).click()]);
    await a.page.waitForTimeout(500);
    expect(sql(`select count(*) from sessions s join users u on u.id=s.user_id where u.email='${email}'`) === '1', 'other sessions still alive');
    await goto(b, '/dashboard');
    expect(b.page.url().includes('/login'), `revoked session still works: ${b.page.url()}`);
  });

  await step(a, 'change password', async () => {
    const next = 'Coqueiral-Azul-2027!z';
    await settingsSubmit(a, 'account', 'change_password', async (f) => {
      await f.locator('#current_password').fill(password);
      await f.locator('#new_password').fill(next);
      await f.locator('#confirm_password').fill(next);
    });
    password = next;
    await loginWith2FA(b, email, password, { code: await freshCode(secret) });
    expect(b.page.url().includes('/dashboard'), `login with the new password: ${b.page.url()}`);
  });

  await step(a, 'change e-mail (confirm via mail)', async () => {
    const t0 = Date.now() - 1000;
    const next = `seguranca2-${uniq}@example.com`;
    await settingsSubmit(a, 'account', 'change_email', async (f) => {
      await f.locator('#new_email').fill(next);
      if (await f.locator('#change_email-password').count()) await f.locator('#change_email-password').fill(password);
      if (await f.locator('#change_email-code').count()) await f.locator('#change_email-code').fill(await freshCode(secret));
    });
    const m = await mail.wait(next, 'Confirme seu novo e-mail', { after: t0 }).catch(async () => mail.wait(email, 'Confirme seu novo e-mail', { after: t0 }));
    const link = mail.link(m, /confirmar-email\?token=/);
    await goto(a, link);
    await Promise.all([a.page.waitForLoadState('networkidle'), a.page.getByRole('button', { name: 'Confirmar e-mail' }).click()]);
    await waitFor(() => sql(`select count(*) from users where email='${next}'`) === '1', { what: 'e-mail changed' });
    email = next;
  });

  await step(a, 'delete the account from settings (step-up)', async () => {
    await loginWith2FA(a, email, password, { code: await freshCode(secret) });
    await settingsSubmit(a, 'danger', 'delete_user', async (f) => {
      await f.locator('#confirm_email').fill(email);
      await f.locator('#delete_password').fill(password);
      if (await f.locator('#delete_code').count()) await f.locator('#delete_code').fill(await freshCode(secret));
    });
    await waitFor(() => sql(`select count(*) from users where email='${email}'`) === '0', { what: 'user deleted' });
  });

  // The confirm-by-mail deletion is the JSON API's path (POST
  // /api/auth/delete-user without a token mails a link to /excluir-conta).
  const e = await newBrowser('sec-e');
  await step(e, 'delete an account via the mail link', async () => {
    const t0 = Date.now() - 1000;
    const em = `apagar-${uniq}@example.com`;
    const pw = 'Mangue-Seco-2026!q';
    await signUp(e, 'Apagar Teste', em, pw);
    const res = await e.page.evaluate(async (pw) => {
      const r = await fetch('/api/auth/delete-user', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password: pw }) });
      return [r.status, await r.text()];
    }, pw);
    expect(res[0] === 200, `delete-user request: ${res.join(' ')}`);
    const m = await mail.wait(em, 'Confirme a exclusão', { after: t0 });
    await goto(e, mail.link(m, /excluir-conta\?token=/));
    await Promise.all([e.page.waitForLoadState('networkidle'), e.page.getByRole('button', { name: 'Excluir minha conta' }).click()]);
    await waitFor(() => sql(`select count(*) from users where email='${em}'`) === '0', { what: 'user deleted' });
    expect(!/Erro/.test(await mainText(e)), 'error page after delete');
  });
  await e.ctx.close();

  for (const x of [a, b, c]) await x.ctx.close();
}
