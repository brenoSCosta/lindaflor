# Architecture

- **oRPC** for all endpoints. Every handler is an oRPC procedure with typed input/output via zod schemas.
- **CASL** for all access control. Never write custom access-checking functions — use `authorize()` middleware, `ability.can()`, or `ability.cannot()` with `subject()`.
- **Zod schemas** in `src/schemas/` define the API contract (input + output types). Output enrichment (derived fields, joins) lives in the schema layer via transforms or Drizzle column maps — not in router handler code.

## Authorization pattern

Access control is always done through CASL — load the **`casl-authorization`** skill for subject setup, middleware, instance checks, and frontend patterns.

```ts
// Subject-level gate (no instance data needed):
.use(authorize("manage", "Training"))

// Instance-level check (with row data):
if (ability.cannot("read", subject("Training", { ...course, enrolled }))) {
  throw new ORPCError("NOT_FOUND", { message: "Curso não encontrado" });
}
```

The subject object must include every condition the CASL rules reference (e.g., `enrolled`, `is_published`, `organization_id`).

## Schema-driven output enrichment

Enriching query results with derived/computed fields:

1. Define a Drizzle column map in the handler or a columns file
2. Use SQL subqueries (e.g., `EXISTS`) for boolean derivations like `enrolled`
3. Add zod `.transform()` on output schemas for JS-only computations (e.g., thumbnail URLs)
4. Complex business logic (e.g., course completion) stays in dedicated helpers

## Nested router keys

Prefer **nested router objects** over flat camelCase procedure names so client paths stay short and context is not repeated in every key.

- **App router**: one top-level key per domain (matches `appRouter` in `src/routers/index.ts`). Version and resources sit underneath (`orpc.<domain>.v1.<resource>.…`).
- **Resource keys**: singular nouns where possible (`course`, `enrollment`, not `coursesRouter` in the client tree).
- **Drop redundant words** already implied by the parent path (e.g. `listBy.tank` under a tank-scoped module, not `listByTank` on a flat router).
- **Casing**: lowercase dot segments for groups and leaves (`list.all`, `get.snapshot`, `replace.point`). The **only camelCase object keys** are **`getBy`** and **`listBy`**, with lowercase children (`getBy.id`, `listBy.tank`).
- **HTTP**: `.route({ path })` stays the REST path; nesting is TypeScript + generated contract shape only.
- **Large domain routers**: use an explicit router type and a small factory/helper when composing `o.prefix().router()` so declaration emit does not hit TS7056 (see domain `index.ts` patterns).
- **After changing router shape**: `bun run contract-gen` in `@lindaflor/api`.

Nested examples in the codebase: `training` (`courses.certificate.get`, `lectures.pdf.upload`), `tanks` (`tank.list.all`, `tankage.listBy.tank`).

## Schema module exports

Each schema module exports one procedure contract tree as **`export const schema`** (see `src/schemas/concession.ts`, `src/schemas/lab-oil-analysis.ts`).

| Export                                           | Name                                                                                                                             |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| Procedure contract (nested `.input` / `.output`) | `schema`                                                                                                                         |
| Row DTOs, enums, facets helpers, inferred types  | Descriptive names (`rowSchema`, `ConcessionOutput`, `defaultFacets`, …)                                                          |
| Composing several entities in one index file     | `import { schema as courses } from "…/courses"` then `export const schema = { v1: { courses, … } }`                              |
| One contract in a consumer file                  | `import { schema } from "@lindaflor/shared/schemas/…"`                                                                          |
| Multiple contracts in one file                   | Local alias only: `import { schema as otherSchema } from "…/other"` — do not export prefixed `*Schema` names from schema modules |

## oRPC patterns

```ts
import { schema } from "@lindaflor/shared/schemas/training";

export const coursesRouter = {
  list: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "Training"))
    .route({ method: "GET", path: "/training/courses", description: "...", summary: "..." })
    .input(schema.v1.courses.list.input)
    .output(schema.v1.courses.list.output)
    .handler(async ({ input, context }) => { ... }),
};
```

- Use `.errors({ FORBIDDEN: { message: "..." } })` for middleware-level error customization
- Parse output through the matching `schema.v1.….output.parse(result)` path for type safety
- Use `Effect.tryPromise()` + `.pipe(Effect.catchAll(...))` for fallible I/O — never `try/catch`

## Performance & Big O notation

Always consider time and space complexity when writing router handlers, middleware, and schema transforms:

- **Prefer O(n) over O(n²):** Avoid nested loops over potentially large arrays. Use `Map`/`Set` for lookups instead of `Array.includes()` or `Array.find()` inside loops.
- **N+1 queries:** Use Drizzle's `with` (eager loading) or batched queries instead of per-row fetches in loops.
- **Early returns:** Structure access control and validation checks to fail fast before expensive operations.
- **Unnecessary copies:** Avoid spread syntax in accumulators; prefer `push()` or direct mutation in local scope.
- **Schema transforms:** Expensive `.transform()` calls on output schemas run on every response — keep them O(n) or less.
- **CASL rule evaluation:** Subject objects should include only the fields rules reference. Extra fields don't hurt correctness but be mindful of deeply nested condition evaluation.

When reviewing a handler, trace the hot path and ask: can this be done with fewer passes over the data?

## Error handling

- Throw `ORPCError` with explicit status code in handlers
- NOT_FOUND for missing resources, FORBIDDEN for access denied, BAD_REQUEST for validation
- Never write `try/catch` — use the `effect` package for fallible operations
