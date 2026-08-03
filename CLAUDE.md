# Core Principles

Write code that is **accessible, performant, type-safe, and maintainable**. Focus on clarity and explicit intent over brevity.

## Runtime & Scripts

- **Bun** only (`packageManager: bun@1.3.10`). No npm, pnpm, or yarn.
- **Turborepo monorepo** — run all scripts from the repository root with `bun run <script>`. Workspace tasks: `turbo -F @lindaflor/web dev`.
- Install dependencies with `bun install` from the root only. One-off executables: `bun x` (not `npx`).
- **After code changes:** `bun run check`, `bun run check-types`, and `bun test`. Web UI code changes also require `bun run test-e2e`. Skip for non-code changes. Fix all warnings and errors before finishing.

## Project Conventions

**Imports:** Never use relative imports (`./`, `../`). Use package aliases (`@lindaflor/api/...`, `@lindaflor/shared/...`, `@lindaflor/core/...`) inside packages; use `@/` in `apps/web`. Named exports only. **kebab-case** filenames. Never edit `*.gen.ts` manually.

**Index files:** `index.ts` holds real implementation only — never barrel re-exports (`export * from './x'`). Import features via deep alias paths (e.g. `@lindaflor/api/casl/middleware`, not `@lindaflor/api`).

**Type boundaries:** Never use unsafe `as SomeUnion` on dynamic values (event args, JSON, query params). Parse with Zod (load `zod-4` skill) or write type guards. See [oxlint `no-unsafe-type-assertion`](https://oxc.rs/docs/guide/usage/linter/rules/typescript/no-unsafe-type-assertion.html).

## Dates & timezones

**Database storage is always UTC.** Timestamps are absolute instants, not local wall-clock strings. Use `timestamp(..., { withTimezone: true, mode: "date" })` in Drizzle for user-facing event times. Plain `timestamp` columns without timezone are for server-side audit fields (`created_at`, `updated_at`) — still written as UTC instants.

**API boundary:** oRPC handlers receive and return JavaScript `Date` values (UTC instants). Do not persist business logic as ad-hoc pairs of separate date strings and time strings; store one instant plus, when needed, a derived calendar date (below).

**Client timezone:** The web app sends the user's effective IANA timezone in the `client-timezone` cookie (`CLIENT_TIMEZONE_COOKIE_NAME` in `@lindaflor/shared/constants`). `useTimezone()` in `apps/web` reads it; the cookie is always set to the resolved IANA zone (including when the user picks "System"). Persist that cookie **synchronously** on first load (before the first oRPC call) so the API receives the same zone the UI uses for pickers. The API reads timezone from this cookie only — not from `Intl` on the server.

| Layer              | Rule                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Display**        | Format UTC instants in the user's timezone. Use `<Time>` (`@/components/ui/time`) or `formatInTimeZone` from `date-fns-tz` with `useTimezone()`.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| **Date pickers**   | Calendar + hour/minute inputs represent wall-clock time in the user's timezone. Convert to UTC before sending to the API via `@lindaflor/shared/lib/zoned-datetime` (`zonedDateTimeToUtc`, `dayKeyToCalendarDate`, `zonedParts`, `operationalDayKey`).                                                                                                                                                                                                                                                                                                                         |
| **Day boundaries** | Route params and filters use `yyyy-MM-dd` day keys in the **client timezone**, not UTC midnight. When logic groups or limits data by calendar day (daily registers, quotas, bulletins), add a `date` column for that business day (`operational_day` or equivalent), computed on the server with `operationalDayKey(instant, context.client.timezone)` — never accept it from the client. Query and aggregate by that column. Use `dateFilterToCondition` (`@lindaflor/core/lib/date-filter`) on timestamp columns only for report-style ranges, always with `clientTimezone`. |
| **Mutations**      | When which calendar day an instant falls on affects rules (limits, locks, grouping), require `context.client.timezone` (e.g. `requireClientTimezone` helper pattern in API routers). Reject the request if the cookie is missing.                                                                                                                                                                                                                                                                                                                                              |

**Do not:** use `datetime-local`, `toLocaleDateString` for persistence, assume the browser timezone matches the user's selected timezone without `useTimezone()`, store per-row user timezones, use fixed UTC offsets, or derive daily buckets with `DATE(timestamp_column)` in UTC.

## Non-negotiable Patterns

| Concern          | Rule                                                 | Skill                                           |
| ---------------- | ---------------------------------------------------- | ----------------------------------------------- |
| Errors & logging | `effect` package; no new `try/catch`, no `console.*` | `effect-error-handling`                         |
| Authorization    | CASL only — no custom permission checks              | `casl-authorization`                            |
| API handlers     | oRPC + `ORPCError`                                   | `orpc-guide`; see also `packages/api/AGENTS.md` |
| Validation       | Zod v4 syntax                                        | `zod-4`                                         |
| Web UI structure | Mutation co-location; route `-components/`           | see `apps/web/AGENTS.md`                        |

## Skills Index

Load skills when working in these areas:

| Area                          | Skills                                                                                                                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/web`                    | `apps/web/AGENTS.md`; skills: `web-design-system`, `tanstack-form`, `tanstack-query-best-practices`, `tanstack-table`, `tanstack-router-best-practices` |
| `packages/api`, `apps/server` | `orpc-guide`, `casl-authorization`, `drizzle-orm-patterns`, `effect-error-handling`                                                                     |
| `packages/core`               | `drizzle-orm-patterns`, `effect-error-handling`, `casl-authorization`                                                                                   |
| `packages/shared`             | `zod-4`, `casl-authorization`                                                                                                                           |
| Auth / permissions            | `better-auth-best-practices`, `casl-authorization`                                                                                                      |
| DB schema / queries           | `drizzle-orm-patterns`                                                                                                                                  |
| Valkey / cache                | `valkey`                                                                                                                                                |
| Commits / PRs                 | `atomic-semantic-commits`, `github-pull-request`                                                                                                        |
| Monorepo layout               | `docs/MONOREPO.md`                                                                                                                                      |

## Human Review Focus

Oxlint + Oxfmt catch style and many TypeScript issues. Focus review on business logic correctness, architecture, edge cases, and UX.
