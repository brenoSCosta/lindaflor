# DB Package Conventions

## Schema Naming

**Snake_case for all JS identifiers:** table consts, relation consts, enum consts, and index callback keys are all snake_case.

```ts
// Correct
export const tank_calibrations = pgTable("tank_calibrations", { ... });

// Wrong
export const tankCalibrations = pgTable("tank_calibrations", { ... });
```

**PascalCase for TypeScript type aliases** (`TankCalibration`, `TrainingCourse`).

## Exceptions

- `jwkss` column properties (`publicKey`, `privateKey`, `createdAt`, `expiresAt`) stay camelCase — Better Auth JWT plugin accesses them by JS property name with no `fieldName` mapping.
- `twoFactors` key in Better Auth schema object stays `twoFactors: two_factors`.

## Migration Generation

Run `bun run db-gen --name <name>` from repo root

## Drizzle Patterns

- `uuid("id").primaryKey().$defaultFn(() => uuidv7())` for primary keys
- `timestamp("created_at").defaultNow().notNull()` for audit timestamps
- `timestamp("updated_at").defaultNow().$onUpdate(() => /* @__PURE__ */ new Date()).notNull()`
- User-facing instants: `timestamp("col", { withTimezone: true, mode: "date" })`
- Calendar dates: `date("col")`
- Indexes: array callback `(table) => [index("name").on(table.col)]`
- FKs inline with `.references(() => Table.id, { onDelete: "..." })`

## Relations

- `relations(Table, ({ one, many }) => ({ ... }))`
- `relationName` for multiple FK paths to the same table
- Relation keys are snake_case (matches the table const name pattern)
