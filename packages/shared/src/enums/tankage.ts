// Kept in a drizzle-free module so client code (apps/web) can import these
// constants without pulling `drizzle-orm/pg-core` into the browser bundle.
// Do not inline back into ./tankage.ts — that file imports drizzle.

export const measurement_equipment_types = ["manual", "electronic"] as const;
export type MeasurementEquipmentType =
  (typeof measurement_equipment_types)[number];

export const lab_oil_sample_types = [
  "top",
  "middle",
  "bottom",
  "inline",
  "running",
  "residual",
] as const;
export type LabOilSampleType = (typeof lab_oil_sample_types)[number];

export const tank_day_bulletin_statuses = ["open", "approved"] as const;
export type TankDayBulletinStatus = (typeof tank_day_bulletin_statuses)[number];

export const labOilSampleTypeLabels: Record<LabOilSampleType, string> = {
  top: "Topo",
  middle: "Meio",
  bottom: "Base",
  inline: "Em linha",
  running: "Amostra corrida",
  residual: "Residual",
};
