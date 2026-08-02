import { loadTankVolumeContext } from "@lindaflor/core/tankage/tank-volume-context";
import { db } from "@lindaflor/db";
import { users } from "@lindaflor/db/schema/auth";
import { tank_day_bulletins, tankages } from "@lindaflor/db/schema/tankage";
import {
  productionFromFirstAndLastMeasurements,
  type TankDayProductionVolumes,
} from "@lindaflor/shared/functions/tankage/tank-day-production";
import type { MeasurementWithPersistedVolumes } from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { and, asc, eq, inArray } from "drizzle-orm";

type TankageMeasurementRow = MeasurementWithPersistedVolumes & {
  operational_day: string;
  created_at: Date;
};

function compareMeasurements(
  a: TankageMeasurementRow,
  b: TankageMeasurementRow,
): number {
  const atDiff = a.measured_at.getTime() - b.measured_at.getTime();
  if (atDiff !== 0) return atDiff;
  return a.created_at.getTime() - b.created_at.getTime();
}

function productionForMeasurementPair(
  tankId: string,
  rows: TankageMeasurementRow[],
  context: Awaited<ReturnType<typeof loadTankVolumeContext>>,
): TankDayProductionVolumes | null {
  if (rows.length < 2) {
    return null;
  }
  const sorted = rows.toSorted(compareMeasurements);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  if (first == null || last == null) {
    return null;
  }
  return productionFromFirstAndLastMeasurements({
    tankId,
    first,
    last,
    context,
  });
}

export async function loadTankDayProductionByTank(args: {
  organizationId: string;
  tankIds: string[];
  operationalDay: string;
}): Promise<Map<string, TankDayProductionVolumes | null>> {
  const result = new Map<string, TankDayProductionVolumes | null>();
  if (args.tankIds.length === 0) {
    return result;
  }

  for (const id of args.tankIds) {
    result.set(id, null);
  }

  const measurements = await db
    .select({
      tank_id: tankages.tank_id,
      current_measurement: tankages.current_measurement,
      measured_at: tankages.measured_at,
      oil_temperature_c: tankages.oil_temperature_c,
      ambient_temperature_c: tankages.ambient_temperature_c,
      operational_day: tankages.operational_day,
      created_at: tankages.created_at,
      gross_volume_m3: tankages.gross_volume_m3,
      net_oil_volume_m3_20c: tankages.net_oil_volume_m3_20c,
      volume_oil_barrels: tankages.volume_oil_barrels,
    })
    .from(tankages)
    .where(
      and(
        eq(tankages.organization_id, args.organizationId),
        eq(tankages.operational_day, args.operationalDay),
        inArray(tankages.tank_id, args.tankIds),
      ),
    )
    .orderBy(asc(tankages.measured_at), asc(tankages.created_at));

  const byTank = new Map<string, TankageMeasurementRow[]>();
  for (const row of measurements) {
    const list = byTank.get(row.tank_id) ?? [];
    list.push(row);
    byTank.set(row.tank_id, list);
  }

  const context = await loadTankVolumeContext({
    organizationId: args.organizationId,
    tankIds: args.tankIds,
    calibrationAt: args.operationalDay,
  });

  for (const [tankId, rows] of byTank) {
    result.set(tankId, productionForMeasurementPair(tankId, rows, context));
  }

  return result;
}

export type TankDaySummaryRow = {
  operational_day: string;
  measurement_count: number;
  last_measured_at: Date;
  last_current_measurement: number;
  last_operator_name: string;
  bulletin_status: "open" | "approved";
  production_gross_volume_m3: number | null;
  production_net_oil_volume_m3_20c: number | null;
  production_volume_oil_barrels: number | null;
};

export async function loadTankDaySummariesForTank(args: {
  organizationId: string;
  tankId: string;
}): Promise<TankDaySummaryRow[]> {
  const measurementsWithOperator = await db
    .select({
      tank_id: tankages.tank_id,
      current_measurement: tankages.current_measurement,
      measured_at: tankages.measured_at,
      oil_temperature_c: tankages.oil_temperature_c,
      ambient_temperature_c: tankages.ambient_temperature_c,
      operational_day: tankages.operational_day,
      created_at: tankages.created_at,
      gross_volume_m3: tankages.gross_volume_m3,
      net_oil_volume_m3_20c: tankages.net_oil_volume_m3_20c,
      volume_oil_barrels: tankages.volume_oil_barrels,
      operator_name: users.name,
    })
    .from(tankages)
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .where(
      and(
        eq(tankages.organization_id, args.organizationId),
        eq(tankages.tank_id, args.tankId),
      ),
    )
    .orderBy(asc(tankages.measured_at), asc(tankages.created_at));

  const byDay = new Map<
    string,
    (TankageMeasurementRow & { operator_name: string })[]
  >();
  for (const row of measurementsWithOperator) {
    const list = byDay.get(row.operational_day) ?? [];
    list.push(row);
    byDay.set(row.operational_day, list);
  }

  const bulletins = await db
    .select({
      operational_day: tank_day_bulletins.operational_day,
      status: tank_day_bulletins.status,
    })
    .from(tank_day_bulletins)
    .where(
      and(
        eq(tank_day_bulletins.organization_id, args.organizationId),
        eq(tank_day_bulletins.tank_id, args.tankId),
      ),
    );

  const bulletinByDay = new Map(
    bulletins.map((b) => [b.operational_day, b.status]),
  );

  const operationalDays = [...byDay.keys()];
  const contextEntries = await Promise.all(
    operationalDays.map((operationalDay) =>
      loadTankVolumeContext({
        organizationId: args.organizationId,
        tankIds: [args.tankId],
        calibrationAt: operationalDay,
      }).then((context) => [operationalDay, context] as const),
    ),
  );
  const contextCache = new Map<
    string,
    Awaited<ReturnType<typeof loadTankVolumeContext>>
  >(contextEntries);

  const summaries: TankDaySummaryRow[] = [];

  for (const [operationalDay, rows] of byDay) {
    const sorted = rows.toSorted(compareMeasurements);
    const last = sorted[sorted.length - 1];
    if (last == null) continue;

    const context = contextCache.get(operationalDay);
    if (context == null) continue;

    const production = productionForMeasurementPair(
      args.tankId,
      sorted,
      context,
    );

    const status = bulletinByDay.get(operationalDay) ?? "open";

    summaries.push({
      operational_day: operationalDay,
      measurement_count: rows.length,
      last_measured_at: last.measured_at,
      last_current_measurement: last.current_measurement,
      last_operator_name: last.operator_name,
      bulletin_status: status,
      production_gross_volume_m3:
        production?.production_gross_volume_m3 ?? null,
      production_net_oil_volume_m3_20c:
        production?.production_net_oil_volume_m3_20c ?? null,
      production_volume_oil_barrels:
        production?.production_volume_oil_barrels ?? null,
    });
  }

  return summaries.toSorted((a, b) =>
    a.operational_day < b.operational_day ? 1 : -1,
  );
}
