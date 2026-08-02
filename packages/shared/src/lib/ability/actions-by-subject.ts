import type { ActionsBySubject } from "@lindaflor/shared/lib/ability/subjects";

export const SUBJECT_ACTIONS = {
  Todo: ["create", "read", "update", "delete", "manage"],
  Member: ["create", "read", "update", "delete", "manage"],
  User: ["create", "read", "update", "delete", "manage", "ban", "impersonate"],
  Organization: ["read", "update"],
  Concessions: ["create", "read", "update", "delete", "manage"],
  Installations: ["create", "read", "update", "delete", "manage"],
  MeasurementEquipments: ["create", "read", "update", "delete", "manage"],
  LabOilAnalyses: ["create", "read", "update", "delete", "manage"],
  Tanks: ["create", "read", "update", "delete", "manage"],
  TankCalibrations: ["create", "read", "update", "delete", "manage"],
  Tankages: ["create", "read", "update", "delete", "manage", "retreat"],
  TankTransfers: ["create", "read", "update", "delete", "manage", "retreat"],
  TankDayBulletins: ["read", "approve", "reopen", "delete", "manage"],
  Curriculum: ["create", "read", "update", "delete", "manage"],
  Training: [
    "create",
    "read",
    "update",
    "delete",
    "manage",
    "progress",
    "certificate",
  ],
  TrainingEnrollment: ["create", "read", "update", "delete", "manage"],
  Product: ["create", "read", "update", "delete", "manage"],
  Inventory: ["create", "read", "update", "delete", "manage"],
  Order: ["create", "read", "update", "delete", "manage"],
} as const satisfies {
  [K in keyof ActionsBySubject]: ReadonlyArray<ActionsBySubject[K]>;
};
