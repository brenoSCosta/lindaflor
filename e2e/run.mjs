// End-to-end suite for the lindaflor store against real services
// (Postgres, Valkey, S3, Mailpit, the fake Mercado Pago). See e2e/README.md.
//
//   node e2e/run.mjs [flow ...]   flows: admin customer fulfil guest jobs security (default) and prod
//
// Normally started by e2e/stack.sh (`make e2e`), which boots the stack and
// sets the E2E_* variables. State shared between flows (product, customer,
// order ids) is kept in E2E_STATE so flows can also run one at a time.
import fs from 'node:fs';
import { closeBrowser, env, expect, flow, goto, launch, login, mail, newBrowser, summary } from './lib.mjs';
import { adminCatalog } from './flows/admin.mjs';
import { customerJourney } from './flows/customer.mjs';
import { fulfilment } from './flows/fulfil.mjs';
import { guestJourney } from './flows/guest.mjs';
import { backgroundJobs } from './flows/jobs.mjs';
import { accountSecurity } from './flows/security.mjs';
import { prodSmoke } from './flows/prod.mjs';

const all = ['admin', 'customer', 'fulfil', 'guest', 'jobs', 'security'];
const wanted = process.argv.slice(2).length ? process.argv.slice(2) : all;

await launch();
const stateFile = process.env.E2E_STATE || 'e2e/.out/state.json';
let state = {};
try {
  state = JSON.parse(fs.readFileSync(stateFile, 'utf8'));
} catch {}
const ctx = { env, expect, goto, login, mail, newBrowser, state };
const save = () => {
  fs.mkdirSync(stateFile.replace(/\/[^/]*$/, ''), { recursive: true });
  fs.writeFileSync(stateFile, JSON.stringify(state, null, 2));
};
try {
  for (const name of wanted) {
    flow(name);
    switch (name) {
      case 'admin':
        await adminCatalog(ctx);
        break;
      case 'customer':
        await customerJourney(ctx);
        break;
      case 'fulfil':
        await fulfilment(ctx);
        break;
      case 'guest':
        await guestJourney(ctx);
        break;
      case 'jobs':
        await backgroundJobs(ctx);
        break;
      case 'security':
        await accountSecurity(ctx);
        break;
      case 'prod':
        await prodSmoke(ctx);
        break;
      default:
        throw new Error(`unknown flow ${name}`);
    }
    save();
  }
} finally {
  await closeBrowser();
}
process.exit(summary() ? 0 : 1);
