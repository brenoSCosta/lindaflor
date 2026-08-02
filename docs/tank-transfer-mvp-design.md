# Tank transfer (MVP) — design

**Date:** 2026-07-21  
**Branch context:** `feat/tanks-gestao-native`  
**Status:** Draft for review

## Goal

Allow operators to register a **transferência** on the tank day bulletin: document volume leaving a tank with enough data to compute ambient and 20 °C volumes correctly, keep stock consistent, and satisfy inventory drop validation.

Destination may be free text for now — tank-to-tank stock on the destination is out of scope.

## Non-goals (MVP)

- Creating inventory on a destination tank
- Retratamento
- ANP XML / boletim fiscal export changes
- Editing FE for transfers (use `DEFAULT_EFFICIENCY_FACTOR = 1`)
- Recomputing historical transfers when lab dens/BSW changes (same snapshot rule as tancagem)

## Domain rules

1. **Height is not stored on the tank entity.** Level lives on `tankages` measurements (`current_measurement`). Transfer records the outflow and also writes a measurement at the **after** height so current stock stays coherent.
2. **Ambient gross alone is not enough for 20 °C.** Transfer capture must include oil and ambient temperatures (and uses lab dens/BSW when available), same static-tank path as production measurements.
3. **Lab selection** = most recent analysis with `collected_at ≤ transferred_at` (reuse `resolveLabAnalysis`).
4. **Calibration** = arqueação vigente on the operational day (reuse volume context / `grossVolumeFromHeight`).
5. **Operational day** derived server-side from `transferred_at` + `client-timezone` cookie (same as tancagem).

## Volume calculation

Reuse `@lindaflor/shared/functions/tankage/tank-static-volume` and calibration lookup.

Given `height_before_m`, `height_after_m`, temps, lab:

1. `V_amb_before = grossVolumeFromHeight(points, height_before_m)`
2. `V_amb_after = grossVolumeFromHeight(points, height_after_m)`
3. Require `V_amb_before > V_amb_after` (epsilon ~1e-6 m³)
4. `delta_amb = V_amb_before − V_amb_after`
5. If lab present: `correctStaticTankVolume({ gross_volume_m3: delta_amb, oil_temperature_c, ambient_temperature_c, density, bsw })`
   - `delta_gross_20c = gross_volume_m3_20c`
   - `delta_net_oil_20c = net_oil_volume_m3_20c`
   - Persist CTL, CTSH, dens, BSW, lab id
6. If lab missing: persist ambient delta only; 20 °C / factors null (same pattern as tancagem without lab)

Because CTL/CTSH depend only on temps + dens (not on height), correcting the ambient delta equals `V20_before − V20_after` at the same temps.

## Data model

New table `tank_transfers` (name TBD to match drizzle kebab/snake conventions in repo):

| Column                                                                                | Notes                                         |
| ------------------------------------------------------------------------------------- | --------------------------------------------- |
| `id`                                                                                  | uuid                                          |
| `tank_id`                                                                             | source tank                                   |
| `organization_id`                                                                     |                                               |
| `operational_day`                                                                     | `yyyy-MM-dd` client TZ                        |
| `transferred_at`                                                                      | timestamptz                                   |
| `height_before_m`                                                                     |                                               |
| `height_after_m`                                                                      |                                               |
| `oil_temperature_c`                                                                   |                                               |
| `ambient_temperature_c`                                                               |                                               |
| `gross_volume_before_m3`                                                              |                                               |
| `gross_volume_after_m3`                                                               |                                               |
| `gross_volume_out_m3`                                                                 | ambient delta (validation uses this)          |
| `gross_volume_out_m3_20c`                                                             | nullable                                      |
| `net_oil_volume_out_m3_20c`                                                           | nullable                                      |
| `shell_correction_factor` / `liquid_correction_factor` / `combined_correction_factor` | nullable                                      |
| `tank_calibration_id`                                                                 | nullable                                      |
| `lab_oil_analysis_id`                                                                 | nullable                                      |
| `density_at_20c_kg_m3`                                                                | nullable snapshot                             |
| `water_and_sediment_percent`                                                          | nullable snapshot                             |
| `destination_label`                                                                   | text, optional                                |
| `observation`                                                                         | required non-empty                            |
| `tankage_id`                                                                          | FK to the measurement created at after height |
| `created_by_user_id`, `created_at`, `updated_at`                                      |                                               |

CASL subject: extend tankage permissions or add `TankTransfers` mirroring `Tankages` create/read/update/delete for the org.

## API

oRPC under tankage router group, e.g. `tankage.v1.tankTransfers`:

- `create` — input heights, temps, `transferred_at`, optional destination, observation; require client timezone; assert bulletin editable; compute volumes; insert transfer + tankage row in one transaction
- `listByTank` — filter by `tank_id` + optional `operational_day`
- `delete` — only if bulletin open; remove transfer and linked tankage (or block if tankage edited independently — prefer delete both)

Wire `assertTankageMeasurementStock` / `assertInventoryChangeWithinBounds` with `documentedOutflowGrossM3` summed from transfers between prior measurement and the new reading (or including this transfer when creating the after measurement).

## UI

On `tank-day-bulletin-page`:

- Operation **Transferência** shows a dedicated form (not “em breve”)
- Defaults: `height_before` = last measurement height for the tank (or day); `transferred_at` = now in client TZ
- After success: invalidate tankages list, snapshot, day summaries, transfers list
- Detail table: either show transfer rows in detalhamento with a situation label, or a small list under the form — prefer showing the created tankage row in detalhamento (height after) and optionally a compact transfer summary

## Error cases

- No calibration / height outside table → BAD_REQUEST (same messages as tancagem)
- `height_after >= height_before` (no positive outflow) → BAD_REQUEST
- Bulletin approved → blocked
- Day measurement limit: creating the after tankage counts toward `MAX_TANKAGE_MEASUREMENTS_PER_DAY`
- Withdrawal without enough documented outflow when validating other measurements → existing message

## Testing

- Unit: delta ambient → delta @20 with golden temps/dens/BSW (reuse static volume module)
- Unit: inventory bounds with `documentedOutflowGrossM3` covering the drop
- API/integration (or router-level): create transfer persists transfer + tankage; list by day
- No e2e required for MVP unless existing bulletin e2e is cheap to extend

## Open points (resolved in brainstorming)

| Topic          | Decision                                        |
| -------------- | ----------------------------------------------- |
| Destination    | Optional free-text label only                   |
| Volume basis   | Heights + temps → ambient + @20 via shared calc |
| Stock update   | Create tankage at after height                  |
| Lab edit later | Does not recompute past transfers               |

## References

- Spec calc: `apps/web/.../calculo-volume-tanque.md`
- Domain: `apps/web/.../tancagem.md` (operação Transferência)
- Calc module: `packages/api/src/routers/tankage/tank-static-volume.ts`
- Stock hook: `packages/api/src/routers/tankage/tankage-stock-validation.ts`
- UI stub: `apps/web/.../tank-day-bulletin-page.tsx`
