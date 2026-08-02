import { recomputeAllTankageVolumesForOrganization } from "@lindaflor/core/tankage/tankage-volume-persist";
import { db } from "@lindaflor/db";
import {
  concessions,
  installations,
  lab_oil_analyses,
  measurement_equipments,
  tankages,
  tank_calibration_points,
  tank_calibrations,
  tank_day_bulletins,
  tanks,
  type Tank,
  type Tankage,
  type TankDayBulletin,
} from "@lindaflor/db/schema/tankage";
import { MAX_TANKAGE_MEASUREMENTS_PER_DAY } from "@lindaflor/shared/constants/tankage";
import type { LabOilSampleType } from "@lindaflor/shared/enums/tankage";
import { lab_oil_sample_types } from "@lindaflor/shared/enums/tankage";
import {
  calendarDateToDayKey,
  dayKeyToCalendarDate,
  operationalDayKey,
  zonedDateTimeToUtc,
} from "@lindaflor/shared/lib/zoned-datetime";
import { Effect } from "effect";

import {
  BATCH_SIZE,
  SEED_CLIENT_TIMEZONE,
  SEED_CONCESSIONS,
  SEED_COUNT,
  SEED_INSTALLATIONS,
  SEED_ORG_IDS,
} from "@/seed/constants";
import {
  getOrgOwner,
  getOrgUserByRole,
  getOrgUsers,
  randomSeedDate,
  randomSeedDateAfter,
  seedIdFor,
} from "@/seed/utils";

const tankagesPerOrg = Math.max(
  1,
  Math.floor(SEED_COUNT / SEED_ORG_IDS.length),
);

const SEED_BUSINESS_DATE_BASE = new Date("2020-01-01T00:00:00.000Z");

/** Must be on or before {@link DEMO_TANK_FIRST_DAY} and bulk seed business days. */
const SEED_CALIBRATION_VALID_FROM = "2019-01-01";

/** Matches seed calibration points 1–170 cm (see seedTankageReferences). */
const SEED_CALIBRATION_MAX_HEIGHT_CM = 170;
const SEED_CALIBRATION_MAX_HEIGHT_M = SEED_CALIBRATION_MAX_HEIGHT_CM / 100;

function randomSeedHeightM(): number {
  const minM = 0.1;
  return Number(
    (minM + Math.random() * (SEED_CALIBRATION_MAX_HEIGHT_M - minM)).toFixed(3),
  );
}

function previousSeedHeightM(currentM: number): number {
  return Number(Math.max(0, currentM - 0.05 - Math.random() * 0.2).toFixed(3));
}

function addDays(base: Date, days: number): Date {
  const result = new Date(base);
  result.setUTCDate(result.getUTCDate() + days);
  return result;
}

function addHours(base: Date, hours: number): Date {
  return new Date(base.getTime() + hours * 3_600_000);
}

function addOperationalDayKey(dayKey: string, days: number): string {
  const base = dayKeyToCalendarDate(dayKey, SEED_CLIENT_TIMEZONE);
  return calendarDateToDayKey(addDays(base, days), SEED_CLIENT_TIMEZONE);
}

function demoHeightM(dayIndex: number, hour: number): number {
  const raw = 0.15 + ((dayIndex * 31 + hour * 17) % 145) / 100;
  return Number(Math.min(raw, SEED_CALIBRATION_MAX_HEIGHT_M - 0.01).toFixed(3));
}

const DEMO_TANK_FIRST_DAY = "2023-05-24";
const DEMO_TANK_DAY_COUNT = 10;
const DEMO_TANK_APPROVED_DAY = "2023-05-24";

export type SeedBulletinPlan = Omit<
  TankDayBulletin,
  "id" | "tank_id" | "organization_id"
> & {
  tank_id: string;
  organization_id: string;
};

function buildTankageRow(args: {
  tank: Tank;
  operatorUserId: string;
  measuredAt: Date;
  createdAt: Date;
  updatedAt: Date;
  observation: string;
  currentMeasurement?: number;
  previousMeasurement?: number;
  oilTemperatureC?: number;
  ambientTemperatureC?: number;
}): Tankage {
  const {
    tank,
    operatorUserId,
    measuredAt,
    createdAt,
    updatedAt,
    observation,
    currentMeasurement,
    previousMeasurement,
    oilTemperatureC,
    ambientTemperatureC,
  } = args;
  const current_measurement = currentMeasurement ?? randomSeedHeightM();
  return {
    id: seedIdFor(createdAt),
    tank_id: tank.id,
    concession_id: tank.concession_id,
    installation_id: tank.installation_id,
    measurement_equipment_id: tank.measurement_equipment_id,
    operator_user_id: operatorUserId,
    measured_at: measuredAt,
    operational_day: operationalDayKey(measuredAt, SEED_CLIENT_TIMEZONE),
    previous_measurement:
      previousMeasurement ?? previousSeedHeightM(current_measurement),
    current_measurement,
    oil_temperature_c:
      oilTemperatureC ?? Number((25 + Math.random() * 15).toFixed(1)),
    ambient_temperature_c:
      ambientTemperatureC ?? Number((20 + Math.random() * 12).toFixed(1)),
    observation,
    latitude: -12.9714 + (Math.random() - 0.5) * 2,
    longitude: -38.5014 + (Math.random() - 0.5) * 2,
    gross_volume_m3: null,
    gross_volume_m3_20c: null,
    net_oil_volume_m3_20c: null,
    volume_oil_barrels: null,
    shell_temperature_c: null,
    shell_correction_factor: null,
    liquid_correction_factor: null,
    combined_correction_factor: null,
    tank_calibration_id: null,
    lab_oil_analysis_id: null,
    density_at_20c_kg_m3: null,
    water_and_sediment_percent: null,
    organization_id: tank.organization_id,
    created_by_user_id: operatorUserId,
    created_at: createdAt,
    updated_at: updatedAt,
  };
}

export async function seedTankageReferences() {
  const orgConcessions: (typeof concessions.$inferSelect)[] = [];
  for (const organizationId of SEED_ORG_IDS) {
    for (const concessionItem of SEED_CONCESSIONS) {
      const createdAt = randomSeedDate();
      const updatedAt = randomSeedDateAfter(createdAt);
      orgConcessions.push({
        id: seedIdFor(createdAt),
        name: concessionItem.name,
        state: concessionItem.state,
        organization_id: organizationId,
        created_by_user_id: getOrgOwner(organizationId),
        created_at: createdAt,
        updated_at: updatedAt,
      });
    }
  }

  await db.insert(concessions).values(orgConcessions);

  const orgInstallations: (typeof installations.$inferSelect)[] = [];
  for (const orgId of SEED_ORG_IDS) {
    const orgConcessionRows = orgConcessions.filter(
      (c) => c.organization_id === orgId,
    );
    for (let i = 0; i < SEED_INSTALLATIONS.length; i++) {
      const template = SEED_INSTALLATIONS[i];
      if (!template) continue;
      const parentConcession = orgConcessionRows[i % orgConcessionRows.length];
      if (!parentConcession) continue;
      const concession_id = parentConcession.id;
      if (!concession_id) continue;
      const createdAt = randomSeedDate();
      const updatedAt = randomSeedDateAfter(createdAt);
      orgInstallations.push({
        id: seedIdFor(createdAt),
        name: template.name,
        concession_id,
        organization_id: orgId,
        created_by_user_id: getOrgOwner(orgId),
        created_at: createdAt,
        updated_at: updatedAt,
      });
    }
  }

  await db.insert(installations).values(orgInstallations);

  const orgEquipments: (typeof measurement_equipments.$inferSelect)[] = [];
  for (const orgId of SEED_ORG_IDS) {
    for (let i = 0; i < 3; i++) {
      const createdAt = randomSeedDate();
      const updatedAt = randomSeedDateAfter(createdAt);
      orgEquipments.push({
        id: seedIdFor(createdAt),
        code: `TR-${String(i + 1).padStart(3, "0")}`,
        description: `Trena ${i + 1}`,
        type: i % 2 === 0 ? "manual" : "electronic",
        length_m: 10 + i * 5,
        reference_height_m: i === 0 ? 0 : null,
        manufacturer: "Seed Co.",
        serial_number: `SN-${String(i + 1).padStart(4, "0")}`,
        calibrated_at: "2025-01-15",
        calibration_valid_until: "2026-01-15",
        active: true,
        organization_id: orgId,
        created_by_user_id: getOrgOwner(orgId),
        created_at: createdAt,
        updated_at: updatedAt,
      });
    }
  }
  if (orgEquipments.length > 0) {
    await db.insert(measurement_equipments).values(orgEquipments);
  }

  const orgTanks: Tank[] = [];
  for (const orgId of SEED_ORG_IDS) {
    const orgInstallationRows = orgInstallations.filter(
      (i) => i.organization_id === orgId,
    );
    const orgEquipmentRows = orgEquipments.filter(
      (e) => e.organization_id === orgId,
    );
    for (let i = 0; i < orgInstallationRows.length; i++) {
      const installation = orgInstallationRows[i];
      if (!installation) continue;
      const equipment = orgEquipmentRows[i % orgEquipmentRows.length];
      const createdAt = randomSeedDate();
      const updatedAt = randomSeedDateAfter(createdAt);
      orgTanks.push({
        id: seedIdFor(createdAt),
        tag: `TQ-${String(i + 1).padStart(4, "0")}`,
        concession_id: installation.concession_id,
        installation_id: installation.id,
        measurement_equipment_id: equipment?.id ?? null,
        latitude: -12.9714 + (Math.random() - 0.5) * 0.1,
        longitude: -38.5014 + (Math.random() - 0.5) * 0.1,
        organization_id: orgId,
        created_by_user_id: getOrgOwner(orgId),
        created_at: createdAt,
        updated_at: updatedAt,
      });
    }
  }

  if (orgTanks.length > 0) {
    await db.insert(tanks).values(orgTanks);
  }

  const orgCalibrations: (typeof tank_calibrations.$inferSelect)[] = [];
  const orgCalibrationPoints: (typeof tank_calibration_points.$inferInsert)[] =
    [];
  for (const tank of orgTanks) {
    const createdAt = randomSeedDate();
    const updatedAt = randomSeedDateAfter(createdAt);
    const calibrationId = seedIdFor(createdAt);
    orgCalibrations.push({
      id: calibrationId,
      tank_id: tank.id,
      certificate_number: `ARQ-${tank.tag}`,
      issued_at: SEED_CALIBRATION_VALID_FROM,
      valid_from: SEED_CALIBRATION_VALID_FROM,
      valid_until: null,
      organization_id: tank.organization_id,
      created_by_user_id: tank.created_by_user_id,
      created_at: createdAt,
      updated_at: updatedAt,
    });
    for (let cm = 1; cm <= SEED_CALIBRATION_MAX_HEIGHT_CM; cm++) {
      orgCalibrationPoints.push({
        id: seedIdFor(addDays(createdAt, cm + 1)),
        calibration_id: calibrationId,
        height_cm: cm,
        volume_m3: Number(((cm / 100) * 18.5).toFixed(4)),
      });
    }
  }
  if (orgCalibrations.length > 0) {
    await db.insert(tank_calibrations).values(orgCalibrations);
  }
  if (orgCalibrationPoints.length > 0) {
    await db.insert(tank_calibration_points).values(orgCalibrationPoints);
  }

  return { orgConcessions, orgInstallations, orgTanks, orgEquipments };
}

function findFirstTankForOrg(
  orgTanks: Tank[],
  organizationId: string,
): Tank | null {
  return (
    orgTanks.find(
      (t) => t.organization_id === organizationId && t.tag === "TQ-0001",
    ) ??
    orgTanks.find((t) => t.organization_id === organizationId) ??
    null
  );
}

function firstTankForOrg(orgTanks: Tank[], organizationId: string): Tank {
  const tank = findFirstTankForOrg(orgTanks, organizationId);
  if (!tank) {
    throw new Error(`No tank found for org ${organizationId}`);
  }
  return tank;
}

function buildDemoForOrg(
  orgId: string,
  tank: Tank,
): { rows: Tankage[]; bulletins: SeedBulletinPlan[] } {
  const supervisorId = getOrgUserByRole(orgId, "supervisor");
  const operatorIds = getOrgUsers(orgId);
  if (operatorIds.length === 0) {
    throw new Error(`No users for org ${orgId}`);
  }

  const rows: Tankage[] = [];

  for (let dayIndex = 0; dayIndex < DEMO_TANK_DAY_COUNT; dayIndex++) {
    const dayKey = addOperationalDayKey(DEMO_TANK_FIRST_DAY, dayIndex);
    for (let hour = 0; hour < MAX_TANKAGE_MEASUREMENTS_PER_DAY; hour++) {
      const minute = (hour * 17 + dayIndex * 3) % 60;
      const measuredAt = zonedDateTimeToUtc(
        dayKey,
        String(hour).padStart(2, "0"),
        String(minute).padStart(2, "0"),
        SEED_CLIENT_TIMEZONE,
      );
      const createdAt = randomSeedDate();
      const height = demoHeightM(dayIndex, hour);
      const slot = dayIndex * MAX_TANKAGE_MEASUREMENTS_PER_DAY + hour;
      const operatorUserId = operatorIds[slot % operatorIds.length];
      if (operatorUserId == null) {
        throw new Error(`No operator for org ${orgId}`);
      }

      rows.push(
        buildTankageRow({
          tank,
          operatorUserId,
          measuredAt,
          createdAt,
          updatedAt: randomSeedDateAfter(createdAt),
          observation: `Seed produção (${dayKey} h${hour})`,
          currentMeasurement: height,
          previousMeasurement: previousSeedHeightM(height),
          oilTemperatureC: Number((26 + ((dayIndex + hour) % 12)).toFixed(1)),
          ambientTemperatureC: Number(
            (22 + ((dayIndex + hour) % 8)).toFixed(1),
          ),
        }),
      );
    }
  }

  const approvedAt = zonedDateTimeToUtc(
    DEMO_TANK_APPROVED_DAY,
    "23",
    "00",
    SEED_CLIENT_TIMEZONE,
  );

  const bulletins: SeedBulletinPlan[] = [
    {
      tank_id: tank.id,
      organization_id: orgId,
      operational_day: DEMO_TANK_APPROVED_DAY,
      status: "approved",
      approved_at: approvedAt,
      approved_by_user_id: supervisorId,
      reopened_at: null,
      reopened_by_user_id: null,
      created_at: approvedAt,
      updated_at: approvedAt,
    },
  ];

  return { rows, bulletins };
}

const SEED_LAB_COLLECTED_AT_BASE = new Date("2019-06-01T12:00:00.000Z");

function buildLabOilAnalysisRow(args: {
  tank: Tank;
  collectedAt: Date;
  sampleType: LabOilSampleType;
  densityAt20c: number;
  waterAndSedimentPercent: number;
  certificateNumber: string;
}): typeof lab_oil_analyses.$inferInsert {
  const createdAt = randomSeedDate();
  const updatedAt = randomSeedDateAfter(createdAt);
  const issuedDay = calendarDateToDayKey(
    args.collectedAt,
    SEED_CLIENT_TIMEZONE,
  );

  return {
    id: seedIdFor(createdAt),
    tank_id: args.tank.id,
    sample_type: args.sampleType,
    collected_at: args.collectedAt,
    issued_at: issuedDay,
    certificate_number: args.certificateNumber,
    laboratory_name: "Laboratório Seed",
    method_density: "ASTM D1298",
    method_basic_sediment_water: "ASTM D4007",
    density_at_20c: args.densityAt20c,
    water_and_sediment_percent: args.waterAndSedimentPercent,
    salinity: Number((Math.random() * 50).toFixed(1)),
    organization_id: args.tank.organization_id,
    created_by_user_id: getOrgOwner(args.tank.organization_id),
    created_at: createdAt,
    updated_at: updatedAt,
  };
}

export async function seedLabOilAnalyses(orgTanks: Tank[]) {
  if (orgTanks.length === 0) return;

  const rows: (typeof lab_oil_analyses.$inferInsert)[] = [];

  for (let i = 0; i < orgTanks.length; i++) {
    const tank = orgTanks[i];
    if (!tank) continue;

    const sampleType =
      lab_oil_sample_types[i % lab_oil_sample_types.length] ?? "top";
    const densityAt20c = Number(
      (820 + (i % 7) * 5 + Math.random() * 3).toFixed(1),
    );
    const waterAndSedimentPercent = Number((0.5 + (i % 5) * 0.4).toFixed(2));

    rows.push(
      buildLabOilAnalysisRow({
        tank,
        collectedAt: addDays(SEED_LAB_COLLECTED_AT_BASE, i % 14),
        sampleType,
        densityAt20c,
        waterAndSedimentPercent,
        certificateNumber: `LAB-${tank.tag}-BASE`,
      }),
    );

    if (tank.tag === "TQ-0001") {
      rows.push(
        buildLabOilAnalysisRow({
          tank,
          collectedAt: zonedDateTimeToUtc(
            "2023-05-01",
            "10",
            "00",
            SEED_CLIENT_TIMEZONE,
          ),
          sampleType: "middle",
          densityAt20c: 852.3,
          waterAndSedimentPercent: 1.25,
          certificateNumber: `LAB-${tank.tag}-DEMO`,
        }),
      );
    }
  }

  for (let i = 0; i < rows.length; i += BATCH_SIZE) {
    await db.insert(lab_oil_analyses).values(rows.slice(i, i + BATCH_SIZE));
  }

  Effect.runSync(Effect.log(`  lab_oil_analyses: ${rows.length} rows`));
}

export async function seedTankageDemoMeasurements(
  orgTanks: Tank[],
): Promise<SeedBulletinPlan[]> {
  const allRows: Tankage[] = [];
  const allBulletins: SeedBulletinPlan[] = [];

  for (const orgId of SEED_ORG_IDS) {
    const tank = findFirstTankForOrg(orgTanks, orgId);
    if (!tank) continue;

    const { rows, bulletins } = buildDemoForOrg(orgId, tank);
    allRows.push(...rows);
    allBulletins.push(...bulletins);

    Effect.runSync(
      Effect.log(
        `  tankages demo (${orgId}, ${tank.tag}): ${rows.length} rows (${MAX_TANKAGE_MEASUREMENTS_PER_DAY}/day × ${DEMO_TANK_DAY_COUNT} days, ${DEMO_TANK_FIRST_DAY}..)`,
      ),
    );
  }

  if (allRows.length > 0) {
    await db.insert(tankages).values(allRows);
  }

  return allBulletins;
}

export async function seedTankageMeasurements(
  orgTanks: Tank[],
): Promise<SeedBulletinPlan[]> {
  const bulkBulletins: SeedBulletinPlan[] = [];

  for (const orgId of SEED_ORG_IDS) {
    const tankRows = orgTanks.filter((t) => t.organization_id === orgId);
    const userIds = getOrgUsers(orgId);

    if (tankRows.length === 0 || userIds.length === 0) continue;

    const demoTank = firstTankForOrg(orgTanks, orgId);
    const supervisorId = getOrgUserByRole(orgId, "supervisor");

    let batch: Tankage[] = [];
    let totalInserted = 0;

    for (let i = 0; i < tankagesPerOrg; i++) {
      const tank = tankRows[i % tankRows.length];
      if (!tank) continue;
      if (tank.id === demoTank.id) continue;
      const operator_user_id = userIds[i % userIds.length] ?? userIds[0];
      if (!operator_user_id) continue;
      const createdAt = randomSeedDate();
      const updatedAt = randomSeedDateAfter(createdAt);
      const dayOffset = Math.floor(i / tankRows.length);
      const businessDay = addDays(SEED_BUSINESS_DATE_BASE, dayOffset);
      const dayKey = calendarDateToDayKey(businessDay, SEED_CLIENT_TIMEZONE);
      const hour = 8 + (i % 10);
      const minute = (i * 7) % 60;
      const measured_at = zonedDateTimeToUtc(
        dayKey,
        String(hour).padStart(2, "0"),
        String(minute).padStart(2, "0"),
        SEED_CLIENT_TIMEZONE,
      );

      if (tank.id === demoTank.id && dayOffset > 0 && dayOffset % 10 === 0) {
        const approvedAt = addHours(measured_at, 2);
        bulkBulletins.push({
          tank_id: tank.id,
          organization_id: orgId,
          operational_day: dayKey,
          status: "approved",
          approved_at: approvedAt,
          approved_by_user_id: supervisorId,
          reopened_at: null,
          reopened_by_user_id: null,
          created_at: approvedAt,
          updated_at: approvedAt,
        });
      }

      batch.push(
        buildTankageRow({
          tank,
          operatorUserId: operator_user_id,
          measuredAt: measured_at,
          createdAt,
          updatedAt,
          observation: "Seed produção",
        }),
      );

      if (batch.length >= BATCH_SIZE) {
        await db.insert(tankages).values(batch);
        totalInserted += batch.length;
        batch = [];

        if (totalInserted % 50_000 === 0) {
          Effect.runSync(
            Effect.log(
              `  tankages (${orgId}): ${totalInserted}/${tankagesPerOrg}`,
            ),
          );
        }
      }
    }

    if (batch.length > 0) {
      await db.insert(tankages).values(batch);
      totalInserted += batch.length;
    }

    Effect.runSync(
      Effect.log(`  tankages (${orgId}): done (${totalInserted} rows)`),
    );
  }

  return bulkBulletins;
}

export async function seedTankDayBulletins(plans: SeedBulletinPlan[]) {
  if (plans.length === 0) return;

  const rows: TankDayBulletin[] = plans.map((plan) => {
    const createdAt = plan.created_at ?? plan.approved_at ?? new Date();
    return {
      id: seedIdFor(createdAt),
      tank_id: plan.tank_id,
      operational_day: plan.operational_day,
      status: plan.status,
      approved_at: plan.approved_at ?? null,
      approved_by_user_id: plan.approved_by_user_id ?? null,
      reopened_at: plan.reopened_at ?? null,
      reopened_by_user_id: plan.reopened_by_user_id ?? null,
      organization_id: plan.organization_id,
      created_at: plan.created_at ?? createdAt,
      updated_at: plan.updated_at ?? createdAt,
    };
  });

  for (let i = 0; i < rows.length; i += BATCH_SIZE) {
    await db.insert(tank_day_bulletins).values(rows.slice(i, i + BATCH_SIZE));
  }

  Effect.runSync(Effect.log(`  tank_day_bulletins: ${rows.length} rows`));
}

export async function seedTankageData(orgTanks: Tank[]) {
  await seedLabOilAnalyses(orgTanks);
  const bulkBulletins = await seedTankageMeasurements(orgTanks);
  const demoBulletins = await seedTankageDemoMeasurements(orgTanks);
  await seedTankDayBulletins([...bulkBulletins, ...demoBulletins]);

  const orgIds = [...new Set(orgTanks.map((t) => t.organization_id))];
  for (const organizationId of orgIds) {
    const count =
      await recomputeAllTankageVolumesForOrganization(organizationId);
    Effect.runSync(
      Effect.log(
        `  tankage volume backfill (${organizationId}): ${count} rows`,
      ),
    );
  }
}
