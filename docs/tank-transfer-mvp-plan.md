# Tank Transfer MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let operators register a tank transfer (altura antes/depois + temps) that computes ambient and 20 °C outflow, persists a transfer row, creates the after-height tankage measurement, and feeds stock validation.

**Architecture:** Pure compute helper for the height delta → volumes (reusing `correctStaticTankVolume` + calibration lookup). New `tank_transfers` table + oRPC router. Create runs in a DB transaction: insert tankage at after height, insert transfer linked by `tankage_id`. Wire `documentedOutflowGrossM3` when validating inventory drops. Bulletin UI swaps “em breve” for a transfer form under the Transferência operation.

**Tech Stack:** Bun, Drizzle (Postgres), oRPC, Zod v4, CASL, TanStack Form/Query, Effect (no new try/catch), existing tankage volume modules.

**Spec:** `docs/tank-transfer-mvp-design.md`

---

## File map

| File                                                            | Responsibility                                        |
| --------------------------------------------------------------- | ----------------------------------------------------- |
| `packages/api/src/routers/tankage/tank-transfer-volume.ts`      | Pure: heights → ambient/20 °C deltas                  |
| `packages/api/src/routers/tankage/tank-transfer-volume.test.ts` | Unit tests for deltas / errors                        |
| `packages/db/src/schema/tankage.ts`                             | `tankTransfers` table                                 |
| `packages/db/src/migrations/*`                                  | Generated migration                                   |
| `packages/api/src/schemas/tankage/transfers.ts`                 | Zod input/output                                      |
| `packages/api/src/lib/ability/subjects.ts` + ability rules      | `TankTransfers` subject (mirror Tankages CRUD in org) |
| `packages/api/src/routers/tankage/transfers.ts`                 | oRPC create / listByTank / delete                     |
| `packages/api/src/routers/tankage/index.ts`                     | Mount `tankTransfers`                                 |
| `packages/api/src/routers/tankage/tankage-stock-validation.ts`  | Sum transfers into `documentedOutflowGrossM3`         |
| `packages/api/src/contract` (regen)                             | Contract after router add                             |
| `apps/web/.../tank-transfer-form.tsx`                           | Form + mutation                                       |
| `apps/web/.../tank-day-bulletin-page.tsx`                       | Show form for operação transferência                  |

---

### Task 1: Pure transfer volume helper (TDD)

**Files:**

- Create: `packages/api/src/routers/tankage/tank-transfer-volume.ts`
- Create: `packages/api/src/routers/tankage/tank-transfer-volume.test.ts`

- [ ] **Step 1: Write failing tests**

```ts
import { describe, expect, test } from "bun:test";
import { computeTransferVolumes } from "@lindaflor/shared/functions/tankage/tank-transfer-volume";
import { correctStaticTankVolume } from "@lindaflor/shared/functions/tankage/tank-static-volume";

const points = [
  { height_cm: 0, volume_m3: 0 },
  { height_cm: 100, volume_m3: 10 },
  { height_cm: 200, volume_m3: 20 },
];

describe("computeTransferVolumes", () => {
  test("computes ambient delta and 20c when lab present", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.5,
      height_after_m: 1.0,
      oil_temperature_c: 32.5,
      ambient_temperature_c: 28,
      lab: {
        id: "018f0000-0000-7000-8000-000000000001",
        density_at_20c: 850,
        water_and_sediment_percent: 2.5,
      },
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.gross_volume_before_m3).toBe(15);
    expect(result.gross_volume_after_m3).toBe(10);
    expect(result.gross_volume_out_m3).toBe(5);
    const expected = correctStaticTankVolume({
      gross_volume_m3: 5,
      oil_temperature_c: 32.5,
      ambient_temperature_c: 28,
      density_at_20c_kg_m3: 850,
      water_and_sediment_percent: 2.5,
    });
    expect(result.gross_volume_out_m3_20c).toBeCloseTo(
      expected.gross_volume_m3_20c,
      12,
    );
    expect(result.net_oil_volume_out_m3_20c).toBeCloseTo(
      expected.net_oil_volume_m3_20c,
      12,
    );
    expect(result.liquid_correction_factor).toBeCloseTo(expected.ctl, 12);
  });

  test("returns ambient-only when lab missing", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.5,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.gross_volume_out_m3).toBe(5);
    expect(result.gross_volume_out_m3_20c).toBeNull();
    expect(result.lab_oil_analysis_id).toBeNull();
  });

  test("fails when after height is not lower", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.0,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("NO_OUTFLOW");
  });

  test("fails when height outside calibration", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 3.0,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("HEIGHT_OUT_OF_TABLE");
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL (module missing)**

```bash
cd packages/api && bun test src/routers/tankage/tank-transfer-volume.test.ts
```

Expected: fail to resolve `computeTransferVolumes`.

- [ ] **Step 3: Implement helper**

```ts
// packages/api/src/routers/tankage/tank-transfer-volume.ts
import type { CalibrationPoint } from "@lindaflor/shared/functions/tankage/calibration-volume";
import { grossVolumeFromHeight } from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { correctStaticTankVolume } from "@lindaflor/shared/functions/tankage/tank-static-volume";

const VOLUME_EPSILON_M3 = 1e-6;

export type TransferLabInput = {
  id: string;
  density_at_20c: number;
  water_and_sediment_percent: number;
} | null;

export type TransferVolumeOk = {
  ok: true;
  gross_volume_before_m3: number;
  gross_volume_after_m3: number;
  gross_volume_out_m3: number;
  gross_volume_out_m3_20c: number | null;
  net_oil_volume_out_m3_20c: number | null;
  shell_temperature_c: number | null;
  shell_correction_factor: number | null;
  liquid_correction_factor: number | null;
  combined_correction_factor: number | null;
  lab_oil_analysis_id: string | null;
  density_at_20c_kg_m3: number | null;
  water_and_sediment_percent: number | null;
};

export type TransferVolumeErr = {
  ok: false;
  code: "HEIGHT_OUT_OF_TABLE" | "NO_OUTFLOW";
};

export function computeTransferVolumes(args: {
  calibrationPoints: readonly CalibrationPoint[];
  height_before_m: number;
  height_after_m: number;
  oil_temperature_c: number;
  ambient_temperature_c: number;
  lab: TransferLabInput;
}): TransferVolumeOk | TransferVolumeErr {
  const before = grossVolumeFromHeight(
    args.calibrationPoints,
    args.height_before_m,
  );
  const after = grossVolumeFromHeight(
    args.calibrationPoints,
    args.height_after_m,
  );
  if (before == null || after == null) {
    return { ok: false, code: "HEIGHT_OUT_OF_TABLE" };
  }
  const out = before - after;
  if (out <= VOLUME_EPSILON_M3) {
    return { ok: false, code: "NO_OUTFLOW" };
  }
  if (args.lab == null) {
    return {
      ok: true,
      gross_volume_before_m3: before,
      gross_volume_after_m3: after,
      gross_volume_out_m3: out,
      gross_volume_out_m3_20c: null,
      net_oil_volume_out_m3_20c: null,
      shell_temperature_c: null,
      shell_correction_factor: null,
      liquid_correction_factor: null,
      combined_correction_factor: null,
      lab_oil_analysis_id: null,
      density_at_20c_kg_m3: null,
      water_and_sediment_percent: null,
    };
  }
  const corrected = correctStaticTankVolume({
    gross_volume_m3: out,
    oil_temperature_c: args.oil_temperature_c,
    ambient_temperature_c: args.ambient_temperature_c,
    density_at_20c_kg_m3: args.lab.density_at_20c,
    water_and_sediment_percent: args.lab.water_and_sediment_percent,
  });
  return {
    ok: true,
    gross_volume_before_m3: before,
    gross_volume_after_m3: after,
    gross_volume_out_m3: out,
    gross_volume_out_m3_20c: corrected.gross_volume_m3_20c,
    net_oil_volume_out_m3_20c: corrected.net_oil_volume_m3_20c,
    shell_temperature_c: corrected.shell_temperature_c,
    shell_correction_factor: corrected.ctsh,
    liquid_correction_factor: corrected.ctl,
    combined_correction_factor: corrected.combined_correction_factor,
    lab_oil_analysis_id: args.lab.id,
    density_at_20c_kg_m3: args.lab.density_at_20c,
    water_and_sediment_percent: args.lab.water_and_sediment_percent,
  };
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
cd packages/api && bun test src/routers/tankage/tank-transfer-volume.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add packages/api/src/routers/tankage/tank-transfer-volume.ts packages/api/src/routers/tankage/tank-transfer-volume.test.ts
git commit -m "feat(tankage): add transfer volume compute helper"
```

---

### Task 2: Schema + migration

**Files:**

- Modify: `packages/db/src/schema/tankage.ts`
- Create: migration via `bun run db-gen -- tank_transfers` (from repo root)

- [ ] **Step 1: Add `tankTransfers` table** next to `tankages` / `labOilAnalyses` in `packages/db/src/schema/tankage.ts`

Use the same patterns as `tankages`: `uuid` + `uuidv7`, `timestamp(..., { withTimezone: true, mode: "date" })` for `transferred_at`, plain `timestamp` for audit, `date("operational_day")`, FKs to `tanks`, `organizations`, `users`, `tankages` (`tankage_id` unique), optional FKs to calibrations/lab.

Columns per `docs/tank-transfer-mvp-design.md` data model. Indexes: `(organization_id, tank_id, operational_day)`, `tank_id`, `transferred_at`.

- [ ] **Step 2: Export table** from whatever schema barrel/index this package already uses (follow existing tankage exports — no new barrel files).

- [ ] **Step 3: Generate migration**

```bash
bun run db-gen -- tank_transfers
```

- [ ] **Step 4: Apply locally** (dev stack up)

```bash
bun run db-migrate:run
```

- [ ] **Step 5: Commit**

```bash
git add packages/db/src/schema/tankage.ts packages/db/src/migrations
git commit -m "feat(db): add tank_transfers table"
```

---

### Task 3: Zod schema + CASL subject

**Files:**

- Create: `packages/api/src/schemas/tankage/transfers.ts`
- Modify: `packages/api/src/lib/ability/subjects.ts`
- Modify: `packages/api/src/lib/ability/index.ts` (rules for org CRUD on `TankTransfers`, same as `Tankages`)
- Modify: `packages/api/src/lib/ability/index.test.ts` — one assertion org member can create/read

- [ ] **Step 1: Add Zod schemas**

```ts
// packages/api/src/schemas/tankage/transfers.ts
import { z } from "zod";

const rowSchema = z.object({
  id: z.guid(),
  tank_id: z.guid(),
  organization_id: z.guid(),
  operational_day: z.iso.date(),
  transferred_at: z.date(),
  height_before_m: z.number(),
  height_after_m: z.number(),
  oil_temperature_c: z.number(),
  ambient_temperature_c: z.number(),
  gross_volume_before_m3: z.number(),
  gross_volume_after_m3: z.number(),
  gross_volume_out_m3: z.number(),
  gross_volume_out_m3_20c: z.number().nullable(),
  net_oil_volume_out_m3_20c: z.number().nullable(),
  shell_temperature_c: z.number().nullable(),
  shell_correction_factor: z.number().nullable(),
  liquid_correction_factor: z.number().nullable(),
  combined_correction_factor: z.number().nullable(),
  tank_calibration_id: z.guid().nullable(),
  lab_oil_analysis_id: z.guid().nullable(),
  density_at_20c_kg_m3: z.number().nullable(),
  water_and_sediment_percent: z.number().nullable(),
  destination_label: z.string().nullable(),
  observation: z.string(),
  tankage_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type TankTransferOutput = z.infer<typeof rowSchema>;

export const tankTransfersSchema = {
  v1: {
    listByTank: {
      input: z.object({
        tank_id: z.guid(),
        operational_day: z.iso.date().optional(),
      }),
      output: z.object({ data: z.array(rowSchema) }),
    },
    create: {
      input: z.object({
        tank_id: z.guid(),
        transferred_at: z.date(),
        height_before_m: z.number().min(0),
        height_after_m: z.number().min(0),
        oil_temperature_c: z.number(),
        ambient_temperature_c: z.number(),
        destination_label: z.string().trim().nullable().optional(),
        observation: z
          .string()
          .min(1, { message: "Informe a observação" })
          .trim(),
        operator_user_id: z.guid(),
        measurement_equipment_id: z.guid().nullable().optional(),
      }),
      output: rowSchema,
    },
    delete: {
      input: z.object({ id: z.guid() }),
      output: z.null(),
    },
  },
};
```

- [ ] **Step 2: Register CASL** — add `TankTransfers: TankTransferOutput` to subjects + `CrudActions`; in `defineAbilityFor`, grant org-scoped manage/CRUD like `Tankages`.

- [ ] **Step 3: Extend ability test** for `TankTransfers`.

- [ ] **Step 4: Commit**

```bash
git add packages/api/src/schemas/tankage/transfers.ts packages/api/src/lib/ability
git commit -m "feat(api): add tank transfer schema and CASL subject"
```

---

### Task 4: Stock validation — sum documented outflows

**Files:**

- Modify: `packages/api/src/routers/tankage/tankage-stock-validation.ts`
- Modify: `packages/api/src/routers/tankage/tankage-stock-validation.test.ts`

- [ ] **Step 1: Write failing test** — when prior gross 20 → new gross 10 and `documentedOutflowGrossM3 = 10`, assert passes; when documented = 5, assert throws with the existing Portuguese message.

(Reuse `assertInventoryChangeWithinBounds` unit tests; add async helper test if you introduce `sumDocumentedOutflowGrossM3`.)

- [ ] **Step 2: Implement `sumTankTransferOutflowGrossM3`**

Query `tank_transfers` for `tank_id` + org where `transferred_at` is after prior measurement time and `<= measuredAt` of the new reading; `sum(gross_volume_out_m3)`.

- [ ] **Step 3: In `assertTankageMeasurementStock`**, if caller did not pass `documentedOutflowGrossM3`, load the sum and pass it into `assertInventoryChangeWithinBounds` (always enforce once transfers exist; when sum is 0, large drops without transfers still only fail if you choose strict mode — **MVP rule:** always pass the sum; if sum is 0, any positive withdrawal still allowed until product tightens — OR enforce: withdrawal must be `<= sum` when sum is used.

**MVP decision (lock this):** Always compute sum S. Call `assertInventoryChangeWithinBounds` with `documentedOutflowGrossM3: S`. That means **any** unexplained drop fails. Production increases (new > prior) still OK. This matches the design error case.

When **creating** the transfer’s after-measurement in the same transaction, pass `documentedOutflowGrossM3` including this transfer’s `gross_volume_out_m3` (or insert transfer first then validate — prefer compute volumes first, validate with that delta as documented, then insert both).

- [ ] **Step 4: Run**

```bash
cd packages/api && bun test src/routers/tankage/tankage-stock-validation.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add packages/api/src/routers/tankage/tankage-stock-validation.ts packages/api/src/routers/tankage/tankage-stock-validation.test.ts
git commit -m "feat(tankage): enforce documented transfer outflows on stock drops"
```

---

### Task 5: oRPC router `tankTransfers`

**Files:**

- Create: `packages/api/src/routers/tankage/transfers.ts`
- Modify: `packages/api/src/routers/tankage/index.ts`
- Modify: `packages/api/src/routers/index.ts` if needed (usually via tankage router only)
- Run: `bun run contract-gen` from root after router mount

- [ ] **Step 1: Implement `create`**

Flow (mirror `tankages.create` patterns for timezone, tank-in-org, bulletin editable, day capacity):

1. `requireClientTimezone`
2. Load tank in org; `operationalDay = operationalDayKey(transferred_at, tz)`
3. `assertTankDayBulletinEditable`
4. `assertTankDayMeasurementCapacity` (after-row counts as a tankage)
5. `loadVolumeContextForTank` → points, calibration id, labs
6. `resolveLabAnalysis(labs, transferred_at)`
7. `computeTransferVolumes(...)` — map codes to `ORPCError("BAD_REQUEST", ...)`
8. Build after-measurement volume columns via `volumeAuditFromMeasurement` for height_after (stock snapshot on the tankage row)
9. `assertTankageMeasurementStock` with `documentedOutflowGrossM3: volumes.gross_volume_out_m3` (and any prior transfers in window — helper from Task 4)
10. `db.transaction`: insert `tankages` (previous_measurement = height_before, current = height_after, measured_at = transferred_at, observation from input, volumes for after height); insert `tank_transfers` with `tankage_id`
11. Return transfer row parsed by schema

Use Effect for fallible DB where existing routers do; no `console.*` / bare try/catch.

- [ ] **Step 2: Implement `listByTank`** — authorize read; filter org + tank + optional day; order by `transferred_at`.

- [ ] **Step 3: Implement `delete`** — load transfer; authorize; bulletin editable; transaction delete transfer then linked tankage (or tankage then transfer if FK requires).

- [ ] **Step 4: Mount**

```ts
// index.ts
tankTransfers: tankTransfersRouter,
```

- [ ] **Step 5: Contract gen + typecheck**

```bash
bun run contract-gen
bun run check-types
```

- [ ] **Step 6: Commit**

```bash
git add packages/api/src/routers/tankage apps/web/src/orpc # if generated paths differ, add whatever contract-gen touches
git commit -m "feat(api): add tankTransfers create/list/delete"
```

---

### Task 6: Web transfer form + bulletin wiring

**Files:**

- Create: `apps/web/src/routes/(auth)/tankages/-components/tank-transfer-form.tsx`
- Create: `apps/web/src/routes/(auth)/tankages/-components/tank-transfer-panel.tsx` (optional thin wrapper like entry panel)
- Modify: `apps/web/src/routes/(auth)/tankages/-components/tank-day-bulletin-page.tsx`

- [ ] **Step 1: Build `TankTransferForm`** with `useAppForm` + Zod matching create input (omit server-only fields). Fields: transferred_at (reuse MeasuredAtField pattern), height_before, height_after, oil/ambient temps, destination_label optional, observation. Default height_before from last `listByTank` row’s `current_measurement` (or 0). Default `operator_user_id` from session like production form.

Mutation: `orpc.tanks.v1.transfer.create.mutationOptions({ onSuccess, onError })` — toast; invalidate:

- `tankages.listByTank`
- `tanks.getSnapshot` / `listSnapshots`
- `tankDaySummaries.listByTank`
- `tankTransfers.listByTank`

- [ ] **Step 2: Panel** — `<Can I="create" a="TankTransfers">` wrapping form; title “Transferência”.

- [ ] **Step 3: Bulletin page** — when `operation === "transferencia"` and `canAddMeasurement`, render panel instead of `ComingSoonPanel`; remove toast “em breve” on Transferência click.

- [ ] **Step 4: Manual check** — `bun run check` from root; fix oxlint/oxfmt.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/\(auth\)/tankages/-components
git commit -m "feat(web): add tank transfer form on day bulletin"
```

---

### Task 7: Verification pass

- [ ] **Step 1: Unit tests**

```bash
cd packages/api && bun test src/routers/tankage/tank-transfer-volume.test.ts src/routers/tankage/tankage-stock-validation.test.ts src/routers/tankage/tank-static-volume.test.ts
```

- [ ] **Step 2: Repo checks**

```bash
bun run check
bun run check-types
bun test
```

- [ ] **Step 3: Smoke (manual)** — with `docker-dev` + `bun run dev`: open boletim → Transferência → register drop with temps → detalhamento shows new height; second production measurement with unexplained drop fails if applicable.

---

## Spec coverage checklist

| Spec item                        | Task       |
| -------------------------------- | ---------- |
| Heights + temps → ambient + @20  | 1, 5       |
| Free-text destination            | 3, 5, 6    |
| Persist transfer + after tankage | 2, 5       |
| Operational day + timezone       | 5          |
| Lab resolve by collected_at      | 5          |
| Bulletin editable / day limit    | 5          |
| documentedOutflowGrossM3         | 4, 5       |
| UI replaces em breve             | 6          |
| No destination tank stock        | — non-goal |
| Snapshot on lab edit             | — non-goal |

## Placeholder scan

None intentional. Table name locked: `tank_transfers` / Drizzle `tankTransfers`.
