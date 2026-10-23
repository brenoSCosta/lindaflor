// Shared helpers for the end-to-end suite: browser contexts with error
// capture, Mailpit, the fake Mercado Pago, the server log, Postgres and S3
// probes, TOTP and tiny PNG generation. No dependencies beyond Playwright
// and Node's standard library.
import { createHmac } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import zlib from 'node:zlib';

export const env = {
  base: process.env.E2E_BASE_URL || 'http://127.0.0.1:4720',
  mailpit: process.env.E2E_MAILPIT_URL || 'http://127.0.0.1:4716',
  fakemp: process.env.E2E_FAKEMP_URL || 'http://127.0.0.1:4717',
  appLog: process.env.E2E_APP_LOG || '',
  databaseURL: process.env.DATABASE_URL || '',
  s3: process.env.S3_ENDPOINT || '',
  bucket: process.env.S3_BUCKET || 'lindaflor',
  shots: process.env.E2E_SHOTS || 'e2e/.out/shots',
  adminEmail: process.env.E2E_ADMIN_EMAIL || 'admin@lindaflor.test',
  adminPassword: process.env.E2E_ADMIN_PASSWORD || 'Adm1n-Sup3r-Secret!',
  chromium: process.env.E2E_CHROMIUM || '',
  ignoreHTTPSErrors: process.env.E2E_IGNORE_HTTPS_ERRORS === '1',
};

const pwModule = process.env.PLAYWRIGHT_MODULE || 'playwright';
const { chromium } = await import(pwModule);

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

export const results = [];
let currentFlow = '';

export function flow(name) {
  currentFlow = name;
  console.log(`\n=== ${name} ===`);
}

export class Check extends Error {}

export function expect(cond, msg) {
  if (!cond) throw new Check(msg);
}

export function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

export async function waitFor(fn, { timeout = 15000, interval = 300, what = 'condition' } = {}) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await sleep(interval);
  }
  throw new Check(`timed out waiting for ${what}${last instanceof Error ? `: ${last.message}` : ''}`);
}

// step runs fn, records OK/FAIL plus any console errors, failed requests
// and server-log errors that happened during it.
export async function step(b, name, fn) {
  const logMark = serverLog.mark();
  b.drain();
  const t0 = Date.now();
  let error;
  try {
    await fn();
  } catch (e) {
    error = e;
  }
  await sleep(150);
  const noise = b.drain();
  const logErrors = serverLog.errorsSince(logMark);
  const problems = [...noise, ...logErrors.map((l) => `server: ${l}`)];
  const ok = !error && problems.length === 0;
  const res = { flow: currentFlow, name, ok, ms: Date.now() - t0, error: error ? String(error.message || error).split('\n')[0] : '', problems };
  results.push(res);
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${name}${res.error ? ` — ${res.error}` : ''}`);
  for (const p of problems) console.log(`     ${p}`);
  if (error && b.page) {
    fs.mkdirSync(env.shots, { recursive: true });
    await b.page.screenshot({ path: `${env.shots}/fail-${currentFlow}-${name}.png`.replace(/[^\w./-]+/g, '_'), fullPage: true }).catch(() => {});
  }
  return !error;
}

export function summary() {
  const failed = results.filter((r) => !r.ok);
  console.log(`\n${results.length - failed.length}/${results.length} steps passed`);
  for (const f of failed) console.log(`  FAIL [${f.flow}] ${f.name}${f.error ? ` — ${f.error}` : ''}${f.problems.length ? ` (+${f.problems.length} problems)` : ''}`);
  return failed.length === 0;
}

// ---------------------------------------------------------------------------
// Browser
// ---------------------------------------------------------------------------

let browser;

export async function launch() {
  const opts = { args: ['--no-sandbox'] };
  if (env.chromium) opts.executablePath = env.chromium;
  browser = await chromium.launch(opts);
  return browser;
}

export async function closeBrowser() {
  await browser?.close();
}

// Expected 4xx responses (validation re-renders, probes, ...) can be
// whitelisted per browser with b.allow(regexp).
export async function newBrowser(label) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 }, ignoreHTTPSErrors: env.ignoreHTTPSErrors, locale: 'pt-BR' });
  const page = await ctx.newPage();
  // Seed data points at images.unsplash.com; keep the run hermetic by
  // answering external requests with an empty response.
  const local = [new URL(env.base).host, env.s3 ? new URL(env.s3).host : ''].filter(Boolean);
  await ctx.route(
    (u) => !local.includes(u.host) && !u.protocol.startsWith('data') && !u.protocol.startsWith('blob'),
    (route) => route.fulfill({ status: 204, body: '' }),
  );
  const b = { label, ctx, page, noise: [], allowed: [/\/favicon\.ico$/] };
  b.allow = (re) => b.allowed.push(re);
  b.drain = () => b.noise.splice(0);
  const watch = (p) => {
    p.on('console', (m) => {
      if (m.type() === 'error') b.noise.push(`[${label}] console.${m.type()}: ${m.text()} @ ${p.url()}`);
    });
    p.on('pageerror', (e) => b.noise.push(`[${label}] pageerror: ${e.message} @ ${p.url()}`));
    p.on('requestfailed', (r) => {
      const f = r.failure()?.errorText || '';
      if (f.includes('ERR_ABORTED')) return; // navigations/htmx swaps cancel in-flight requests
      b.noise.push(`[${label}] request failed: ${r.method()} ${r.url()} ${f}`);
    });
    p.on('response', (r) => {
      const s = r.status();
      if (s < 400) return;
      if (s === 422 || b.allowed.some((re) => re.test(r.url()))) return;
      b.noise.push(`[${label}] HTTP ${s} ${r.request().method()} ${r.url()}`);
    });
  };
  ctx.on('page', watch);
  watch(page);
  return b;
}

export async function goto(b, path) {
  const res = await b.page.goto(path.startsWith('http') ? path : env.base + path);
  await b.page.waitForLoadState('networkidle').catch(() => {});
  return res;
}

export async function shot(b, name) {
  fs.mkdirSync(env.shots, { recursive: true });
  await b.page.screenshot({ path: `${env.shots}/${name}.png`, fullPage: true }).catch(() => {});
}

export async function mainText(b) {
  return (await b.page.locator('main').first().innerText().catch(() => '')).replace(/\s+/g, ' ');
}

export async function login(b, email, password) {
  await goto(b, '/login');
  await b.page.locator('form[action="/login"] input[name=email]').fill(email);
  await b.page.locator('form[action="/login"] input[name=password]').fill(password);
  await Promise.all([
    b.page.waitForURL((u) => !u.pathname.startsWith('/login'), { timeout: 15000 }),
    b.page.locator('form[action="/login"] button[type=submit]').click(),
  ]);
  await b.page.waitForLoadState('networkidle').catch(() => {});
}

// Toast text currently on screen (toast.js renders into [data-toast]).
export async function toasts(b) {
  return (await b.page.locator('[data-sonner-toast], [data-toast], [role=status]').allInnerTexts().catch(() => [])).join(' | ');
}

// ---------------------------------------------------------------------------
// Mailpit
// ---------------------------------------------------------------------------

export const mail = {
  async list() {
    const r = await fetch(`${env.mailpit}/api/v1/messages?limit=500`);
    return (await r.json()).messages || [];
  },
  async search(q) {
    const r = await fetch(`${env.mailpit}/api/v1/search?query=${encodeURIComponent(q)}&limit=200`);
    return (await r.json()).messages || [];
  },
  async get(id) {
    const r = await fetch(`${env.mailpit}/api/v1/message/${id}`);
    return r.json();
  },
  // wait returns the newest message to `to` whose subject matches.
  async wait(to, subject, { timeout = 30000, after = 0 } = {}) {
    return waitFor(
      async () => {
        const msgs = await mail.search(`to:"${to}"`);
        const m = msgs.find((x) => (subject instanceof RegExp ? subject.test(x.Subject) : x.Subject.includes(subject)) && Date.parse(x.Created) >= after);
        return m ? mail.get(m.ID) : null;
      },
      { timeout, interval: 500, what: `mail "${subject}" to ${to}` },
    );
  },
  links(msg) {
    const html = msg.HTML || '';
    const out = new Set();
    for (const m of html.matchAll(/href="([^"]+)"/g)) out.add(m[1].replace(/&amp;/g, '&'));
    for (const m of (msg.Text || '').matchAll(/https?:\/\/\S+/g)) out.add(m[0]);
    return [...out];
  },
  link(msg, re) {
    const l = mail.links(msg).find((u) => re.test(u));
    if (!l) throw new Check(`no link matching ${re} in "${msg.Subject}": ${mail.links(msg).join(' ')}`);
    return l;
  },
  async subjectsTo(to) {
    return (await mail.search(`to:"${to}"`)).map((m) => m.Subject);
  },
};

// ---------------------------------------------------------------------------
// Fake Mercado Pago
// ---------------------------------------------------------------------------

export const fakemp = {
  async payments() {
    return (await fetch(`${env.fakemp}/_fake/payments`)).json();
  },
  async approve(orderID) {
    const r = await fetch(`${env.fakemp}/_fake/approve?external_reference=${orderID}`, { method: 'POST' });
    return r.json();
  },
};

// ---------------------------------------------------------------------------
// Server log
// ---------------------------------------------------------------------------

export const serverLog = {
  mark() {
    if (!env.appLog) return 0;
    try {
      return fs.statSync(env.appLog).size;
    } catch {
      return 0;
    }
  },
  since(mark) {
    if (!env.appLog) return [];
    try {
      const fd = fs.openSync(env.appLog, 'r');
      const size = fs.fstatSync(fd).size;
      const buf = Buffer.alloc(Math.max(0, size - mark));
      fs.readSync(fd, buf, 0, buf.length, mark);
      fs.closeSync(fd);
      return buf.toString('utf8').split('\n').filter(Boolean);
    } catch {
      return [];
    }
  },
  // Errors: level=ERROR, 5xx responses, panics. WARN lines are reported
  // except the ones listed as expected.
  expectedWarn: [/mercado pago webhook rejected/, /rate limited/],
  errorsSince(mark) {
    return serverLog.since(mark).filter((l) => {
      if (/level=ERROR|panic|"level":"ERROR"/.test(l)) return true;
      if (/status=5\d\d\b|"status":5\d\d/.test(l)) return true;
      if (/level=WARN|"level":"WARN"/.test(l)) return !serverLog.expectedWarn.some((re) => re.test(l));
      return false;
    });
  },
};

// ---------------------------------------------------------------------------
// Postgres (psql) and S3 probes
// ---------------------------------------------------------------------------

export function sql(query) {
  if (!env.databaseURL) throw new Check('DATABASE_URL not set');
  return execFileSync('psql', [env.databaseURL, '-XAtq', '-F', '\t', '-c', query], { encoding: 'utf8' }).trim();
}

// s3List lists the bucket's object keys (ListObjectsV2, SigV4-signed).
export async function s3List(prefix = '') {
  const url = new URL(`${env.s3}/${env.bucket}`);
  url.searchParams.set('list-type', '2');
  if (prefix) url.searchParams.set('prefix', prefix);
  const res = await fetch(url, { headers: await sigv4Headers('GET', url) });
  const body = await res.text();
  if (!res.ok) throw new Check(`S3 list ${res.status}: ${body.slice(0, 200)}`);
  return [...body.matchAll(/<Key>([^<]+)<\/Key>/g)].map((m) => m[1]);
}

async function sigv4Headers(method, url) {
  const access = process.env.S3_ACCESS_KEY_ID;
  const secret = process.env.S3_SECRET_ACCESS_KEY;
  const region = process.env.S3_REGION || 'us-east-1';
  const { createHash } = await import('node:crypto');
  const now = new Date();
  const amzDate = now.toISOString().replace(/[:-]|\.\d{3}/g, '');
  const date = amzDate.slice(0, 8);
  const payloadHash = createHash('sha256').update('').digest('hex');
  const q = [...url.searchParams.entries()].sort(([a], [b]) => (a < b ? -1 : 1)).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
  const headers = { host: url.host, 'x-amz-content-sha256': payloadHash, 'x-amz-date': amzDate };
  const signed = Object.keys(headers).sort().join(';');
  const canonHeaders = Object.keys(headers)
    .sort()
    .map((k) => `${k}:${headers[k]}\n`)
    .join('');
  const canon = [method, url.pathname, q, canonHeaders, signed, payloadHash].join('\n');
  const scope = `${date}/${region}/s3/aws4_request`;
  const sts = ['AWS4-HMAC-SHA256', amzDate, scope, createHash('sha256').update(canon).digest('hex')].join('\n');
  let k = createHmac('sha256', `AWS4${secret}`).update(date).digest();
  for (const p of [region, 's3', 'aws4_request']) k = createHmac('sha256', k).update(p).digest();
  const sig = createHmac('sha256', k).update(sts).digest('hex');
  return { 'x-amz-content-sha256': payloadHash, 'x-amz-date': amzDate, authorization: `AWS4-HMAC-SHA256 Credential=${access}/${scope}, SignedHeaders=${signed}, Signature=${sig}` };
}

// ---------------------------------------------------------------------------
// TOTP (RFC 6238, SHA-1, 6 digits, 30 s)
// ---------------------------------------------------------------------------

function base32Decode(s) {
  const alpha = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  let bits = '';
  for (const c of s.replace(/[\s=]/g, '').toUpperCase()) bits += alpha.indexOf(c).toString(2).padStart(5, '0');
  const bytes = [];
  for (let i = 0; i + 8 <= bits.length; i += 8) bytes.push(parseInt(bits.slice(i, i + 8), 2));
  return Buffer.from(bytes);
}

export function totp(secret, t = Date.now()) {
  const counter = Math.floor(t / 1000 / 30);
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64BE(BigInt(counter));
  const h = createHmac('sha1', base32Decode(secret)).update(buf).digest();
  const o = h[h.length - 1] & 0xf;
  const n = ((h[o] & 0x7f) << 24) | (h[o + 1] << 16) | (h[o + 2] << 8) | h[o + 3];
  return String(n % 1_000_000).padStart(6, '0');
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}

// png returns a w×h checkerboard PNG in the given RGB colour.
export function png([r, g, b], w = 320, h = 400) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const dark = (Math.floor(x / 40) + Math.floor(y / 40)) % 2 === 0;
      const o = y * (w * 3 + 1) + 1 + x * 3;
      raw[o] = dark ? r >> 1 : r;
      raw[o + 1] = dark ? g >> 1 : g;
      raw[o + 2] = dark ? b >> 1 : b;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

export function pngFile(name, rgb, w, h) {
  return { name, mimeType: 'image/png', buffer: png(rgb, w, h) };
}

export const uniq = Date.now().toString(36);
