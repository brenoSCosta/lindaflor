import { organizations, users } from "@lindaflor/db/schema/auth";
import {
  lab_oil_sample_types,
  measurement_equipment_types,
  tank_day_bulletin_statuses,
} from "@lindaflor/shared/enums/tankage";
import { relations } from "drizzle-orm";
import {
  boolean,
  date,
  doublePrecision,
  index,
  pgEnum,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";
import { v7 as uuidv7 } from "uuid";

export const lab_oil_sample_type_enum = pgEnum(
  "lab_oil_sample_type",
  lab_oil_sample_types,
);

export const measurement_equipment_type_enum = pgEnum(
  "measurement_equipment_type",
  measurement_equipment_types,
);

export const tank_day_bulletin_status_enum = pgEnum(
  "tank_day_bulletin_status",
  tank_day_bulletin_statuses,
);

export const concessions = pgTable(
  "concessions",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    name: text("name").notNull(),
    state: text("state").notNull(),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("concessions_organization_id_idx").on(table.organization_id),
    index("concessions_created_by_user_id_idx").on(table.created_by_user_id),
  ],
);
export type Concession = typeof concessions.$inferSelect;

export const installations = pgTable(
  "installations",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    name: text("name").notNull(),
    concession_id: uuid("concession_id")
      .notNull()
      .references(() => concessions.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("installations_organization_id_idx").on(table.organization_id),
    index("installations_concession_id_idx").on(table.concession_id),
    index("installations_created_by_user_id_idx").on(table.created_by_user_id),
  ],
);
export type Installation = typeof installations.$inferSelect;

export const measurement_equipments = pgTable(
  "measurement_equipments",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    code: text("code").notNull(),
    description: text("description"),
    type: measurement_equipment_type_enum("type").default("manual").notNull(),
    length_m: doublePrecision("length_m"),
    reference_height_m: doublePrecision("reference_height_m"),
    manufacturer: text("manufacturer"),
    serial_number: text("serial_number"),
    calibrated_at: date("calibrated_at"),
    calibration_valid_until: date("calibration_valid_until"),
    active: boolean("active").default(true).notNull(),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("measurement_equipments_organization_id_code_uidx").on(
      table.organization_id,
      table.code,
    ),
    index("measurement_equipments_organization_id_idx").on(
      table.organization_id,
    ),
    index("measurement_equipments_created_by_user_id_idx").on(
      table.created_by_user_id,
    ),
  ],
);
export type MeasurementEquipment = typeof measurement_equipments.$inferSelect;

export const tanks = pgTable(
  "tanks",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tag: text("tag").notNull(),
    concession_id: uuid("concession_id")
      .notNull()
      .references(() => concessions.id, { onDelete: "cascade" }),
    installation_id: uuid("installation_id")
      .notNull()
      .references(() => installations.id, { onDelete: "cascade" }),
    measurement_equipment_id: uuid("measurement_equipment_id").references(
      () => measurement_equipments.id,
      { onDelete: "set null" },
    ),
    latitude: doublePrecision("latitude"),
    longitude: doublePrecision("longitude"),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("tanks_organization_id_tag_uidx").on(
      table.organization_id,
      table.tag,
    ),
    index("tanks_organization_id_idx").on(table.organization_id),
    index("tanks_concession_id_idx").on(table.concession_id),
    index("tanks_installation_id_idx").on(table.installation_id),
    index("tanks_measurement_equipment_id_idx").on(
      table.measurement_equipment_id,
    ),
    index("tanks_created_by_user_id_idx").on(table.created_by_user_id),
  ],
);
export type Tank = typeof tanks.$inferSelect;

export const concessionRelations = relations(concessions, ({ one, many }) => ({
  organization: one(organizations, {
    fields: [concessions.organization_id],
    references: [organizations.id],
  }),
  created_by: one(users, {
    fields: [concessions.created_by_user_id],
    references: [users.id],
  }),
  installations: many(installations),
  tanks: many(tanks),
  tankages: many(tankages),
}));

export const installationRelations = relations(
  installations,
  ({ one, many }) => ({
    concession: one(concessions, {
      fields: [installations.concession_id],
      references: [concessions.id],
    }),
    organization: one(organizations, {
      fields: [installations.organization_id],
      references: [organizations.id],
    }),
    created_by: one(users, {
      fields: [installations.created_by_user_id],
      references: [users.id],
    }),
    tanks: many(tanks),
    tankages: many(tankages),
  }),
);

export const measurement_equipment_relations = relations(
  measurement_equipments,
  ({ one, many }) => ({
    organization: one(organizations, {
      fields: [measurement_equipments.organization_id],
      references: [organizations.id],
    }),
    created_by: one(users, {
      fields: [measurement_equipments.created_by_user_id],
      references: [users.id],
    }),
    tanks: many(tanks),
  }),
);

export const tankRelations = relations(tanks, ({ one, many }) => ({
  concession: one(concessions, {
    fields: [tanks.concession_id],
    references: [concessions.id],
  }),
  installation: one(installations, {
    fields: [tanks.installation_id],
    references: [installations.id],
  }),
  measurement_equipment: one(measurement_equipments, {
    fields: [tanks.measurement_equipment_id],
    references: [measurement_equipments.id],
  }),
  organization: one(organizations, {
    fields: [tanks.organization_id],
    references: [organizations.id],
  }),
  created_by: one(users, {
    fields: [tanks.created_by_user_id],
    references: [users.id],
  }),
  day_bulletins: many(tank_day_bulletins),
}));

export const tank_calibrations = pgTable(
  "tank_calibrations",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tank_id: uuid("tank_id")
      .notNull()
      .references(() => tanks.id, { onDelete: "cascade" }),
    certificate_number: text("certificate_number").notNull(),
    issued_at: date("issued_at"),
    valid_from: date("valid_from").notNull(),
    valid_until: date("valid_until"),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("tank_calibrations_organization_id_idx").on(table.organization_id),
    index("tank_calibrations_tank_id_idx").on(table.tank_id),
    index("tank_calibrations_tank_id_valid_from_idx").on(
      table.tank_id,
      table.valid_from,
    ),
    index("tank_calibrations_created_by_user_id_idx").on(
      table.created_by_user_id,
    ),
  ],
);
export type TankCalibration = typeof tank_calibrations.$inferSelect;

export const tank_calibration_points = pgTable(
  "tank_calibration_points",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    calibration_id: uuid("calibration_id")
      .notNull()
      .references(() => tank_calibrations.id, { onDelete: "cascade" }),
    height_cm: doublePrecision("height_cm").notNull(),
    volume_m3: doublePrecision("volume_m3").notNull(),
  },
  (table) => [
    uniqueIndex("tank_calibration_points_calibration_id_height_cm_uidx").on(
      table.calibration_id,
      table.height_cm,
    ),
    index("tank_calibration_points_calibration_id_idx").on(
      table.calibration_id,
    ),
  ],
);
export type TankCalibrationPoint = typeof tank_calibration_points.$inferSelect;

export const tankages = pgTable(
  "tankages",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tank_id: uuid("tank_id")
      .notNull()
      .references(() => tanks.id, { onDelete: "restrict" }),
    concession_id: uuid("concession_id")
      .notNull()
      .references(() => concessions.id, { onDelete: "cascade" }),
    installation_id: uuid("installation_id")
      .notNull()
      .references(() => installations.id, { onDelete: "cascade" }),
    measurement_equipment_id: uuid("measurement_equipment_id").references(
      () => measurement_equipments.id,
      { onDelete: "set null" },
    ),
    operator_user_id: uuid("operator_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    measured_at: timestamp("measured_at", {
      withTimezone: true,
      mode: "date",
    }).notNull(),
    operational_day: date("operational_day").notNull(),
    previous_measurement: doublePrecision("previous_measurement").notNull(),
    current_measurement: doublePrecision("current_measurement").notNull(),
    oil_temperature_c: doublePrecision("oil_temperature_c").notNull(),
    ambient_temperature_c: doublePrecision("ambient_temperature_c").notNull(),
    observation: text("observation").notNull(),
    latitude: doublePrecision("latitude"),
    longitude: doublePrecision("longitude"),
    gross_volume_m3: doublePrecision("gross_volume_m3"),
    gross_volume_m3_20c: doublePrecision("gross_volume_m3_20c"),
    net_oil_volume_m3_20c: doublePrecision("net_oil_volume_m3_20c"),
    volume_oil_barrels: doublePrecision("volume_oil_barrels"),
    shell_temperature_c: doublePrecision("shell_temperature_c"),
    shell_correction_factor: doublePrecision("shell_correction_factor"),
    liquid_correction_factor: doublePrecision("liquid_correction_factor"),
    combined_correction_factor: doublePrecision("combined_correction_factor"),
    tank_calibration_id: uuid("tank_calibration_id").references(
      () => tank_calibrations.id,
      { onDelete: "set null" },
    ),
    lab_oil_analysis_id: uuid("lab_oil_analysis_id").references(
      () => lab_oil_analyses.id,
      { onDelete: "set null" },
    ),
    density_at_20c_kg_m3: doublePrecision("density_at_20c_kg_m3"),
    water_and_sediment_percent: doublePrecision("water_and_sediment_percent"),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("tankages_organization_id_idx").on(table.organization_id),
    index("tankages_tank_id_idx").on(table.tank_id),
    index("tankages_concession_id_idx").on(table.concession_id),
    index("tankages_installation_id_idx").on(table.installation_id),
    index("tankages_measurement_equipment_id_idx").on(
      table.measurement_equipment_id,
    ),
    index("tankages_operator_user_id_idx").on(table.operator_user_id),
    index("tankages_measured_at_idx").on(table.measured_at),
    index("tankages_tank_id_measured_at_idx").on(
      table.tank_id,
      table.measured_at,
    ),
    index("tankages_org_tank_operational_day_idx").on(
      table.organization_id,
      table.tank_id,
      table.operational_day,
    ),
    index("tankages_created_by_user_id_idx").on(table.created_by_user_id),
  ],
);
export type Tankage = typeof tankages.$inferSelect;

export const lab_oil_analyses = pgTable(
  "lab_oil_analyses",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tank_id: uuid("tank_id")
      .notNull()
      .references(() => tanks.id, { onDelete: "cascade" }),
    sample_type: lab_oil_sample_type_enum("sample_type").notNull(),
    collected_at: timestamp("collected_at", {
      withTimezone: true,
      mode: "date",
    }).notNull(),
    issued_at: date("issued_at").notNull(),
    certificate_number: text("certificate_number").notNull(),
    laboratory_name: text("laboratory_name").notNull(),
    method_density: text("method_density"),
    method_basic_sediment_water: text("method_basic_sediment_water"),
    density_at_20c: doublePrecision("density_at_20c").notNull(),
    water_and_sediment_percent: doublePrecision(
      "water_and_sediment_percent",
    ).notNull(),
    salinity: doublePrecision("salinity"),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("lab_oil_analyses_organization_id_idx").on(table.organization_id),
    index("lab_oil_analyses_tank_id_idx").on(table.tank_id),
    index("lab_oil_analyses_tank_id_collected_at_idx").on(
      table.tank_id,
      table.collected_at,
    ),
    index("lab_oil_analyses_created_by_user_id_idx").on(
      table.created_by_user_id,
    ),
  ],
);

export type LabOilAnalysis = typeof lab_oil_analyses.$inferSelect;

export const tank_day_bulletins = pgTable(
  "tank_day_bulletins",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tank_id: uuid("tank_id")
      .notNull()
      .references(() => tanks.id, { onDelete: "cascade" }),
    operational_day: date("operational_day").notNull(),
    status: tank_day_bulletin_status_enum("status").notNull().default("open"),
    approved_at: timestamp("approved_at"),
    approved_by_user_id: uuid("approved_by_user_id").references(
      () => users.id,
      {
        onDelete: "set null",
      },
    ),
    reopened_at: timestamp("reopened_at"),
    reopened_by_user_id: uuid("reopened_by_user_id").references(
      () => users.id,
      {
        onDelete: "set null",
      },
    ),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("tank_day_bulletins_org_tank_day_uidx").on(
      table.organization_id,
      table.tank_id,
      table.operational_day,
    ),
    index("tank_day_bulletins_organization_id_idx").on(table.organization_id),
    index("tank_day_bulletins_tank_id_idx").on(table.tank_id),
    index("tank_day_bulletins_tank_id_operational_day_idx").on(
      table.tank_id,
      table.operational_day,
    ),
  ],
);

export type TankDayBulletin = typeof tank_day_bulletins.$inferSelect;

export const tank_transfers = pgTable(
  "tank_transfers",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    tank_id: uuid("tank_id")
      .notNull()
      .references(() => tanks.id, { onDelete: "restrict" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    operational_day: date("operational_day").notNull(),
    transferred_at: timestamp("transferred_at", {
      withTimezone: true,
      mode: "date",
    }).notNull(),
    height_before_m: doublePrecision("height_before_m").notNull(),
    height_after_m: doublePrecision("height_after_m").notNull(),
    oil_temperature_c: doublePrecision("oil_temperature_c").notNull(),
    ambient_temperature_c: doublePrecision("ambient_temperature_c").notNull(),
    gross_volume_before_m3: doublePrecision("gross_volume_before_m3").notNull(),
    gross_volume_after_m3: doublePrecision("gross_volume_after_m3").notNull(),
    gross_volume_out_m3: doublePrecision("gross_volume_out_m3").notNull(),
    gross_volume_out_m3_20c: doublePrecision("gross_volume_out_m3_20c"),
    net_oil_volume_out_m3_20c: doublePrecision("net_oil_volume_out_m3_20c"),
    shell_temperature_c: doublePrecision("shell_temperature_c"),
    shell_correction_factor: doublePrecision("shell_correction_factor"),
    liquid_correction_factor: doublePrecision("liquid_correction_factor"),
    combined_correction_factor: doublePrecision("combined_correction_factor"),
    tank_calibration_id: uuid("tank_calibration_id").references(
      () => tank_calibrations.id,
      { onDelete: "set null" },
    ),
    lab_oil_analysis_id: uuid("lab_oil_analysis_id").references(
      () => lab_oil_analyses.id,
      { onDelete: "set null" },
    ),
    density_at_20c_kg_m3: doublePrecision("density_at_20c_kg_m3"),
    water_and_sediment_percent: doublePrecision("water_and_sediment_percent"),
    destination_label: text("destination_label"),
    observation: text("observation").notNull(),
    tankage_id: uuid("tankage_id")
      .notNull()
      .references(() => tankages.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("tank_transfers_tankage_id_uidx").on(table.tankage_id),
    index("tank_transfers_organization_id_idx").on(table.organization_id),
    index("tank_transfers_tank_id_idx").on(table.tank_id),
    index("tank_transfers_transferred_at_idx").on(table.transferred_at),
    index("tank_transfers_org_tank_operational_day_idx").on(
      table.organization_id,
      table.tank_id,
      table.operational_day,
    ),
    index("tank_transfers_created_by_user_id_idx").on(table.created_by_user_id),
  ],
);

export type TankTransfer = typeof tank_transfers.$inferSelect;

export const lab_oil_analysis_relations = relations(
  lab_oil_analyses,
  ({ one }) => ({
    tank: one(tanks, {
      fields: [lab_oil_analyses.tank_id],
      references: [tanks.id],
    }),
    organization: one(organizations, {
      fields: [lab_oil_analyses.organization_id],
      references: [organizations.id],
    }),
    created_by: one(users, {
      fields: [lab_oil_analyses.created_by_user_id],
      references: [users.id],
    }),
  }),
);

export const tank_calibration_relations = relations(
  tank_calibrations,
  ({ one, many }) => ({
    tank: one(tanks, {
      fields: [tank_calibrations.tank_id],
      references: [tanks.id],
    }),
    organization: one(organizations, {
      fields: [tank_calibrations.organization_id],
      references: [organizations.id],
    }),
    created_by: one(users, {
      fields: [tank_calibrations.created_by_user_id],
      references: [users.id],
    }),
    points: many(tank_calibration_points),
  }),
);

export const tank_calibration_point_relations = relations(
  tank_calibration_points,
  ({ one }) => ({
    calibration: one(tank_calibrations, {
      fields: [tank_calibration_points.calibration_id],
      references: [tank_calibrations.id],
    }),
  }),
);

export const tankageRelations = relations(tankages, ({ one }) => ({
  tank: one(tanks, {
    fields: [tankages.tank_id],
    references: [tanks.id],
  }),
  concession: one(concessions, {
    fields: [tankages.concession_id],
    references: [concessions.id],
  }),
  installation: one(installations, {
    fields: [tankages.installation_id],
    references: [installations.id],
  }),
  measurement_equipment: one(measurement_equipments, {
    fields: [tankages.measurement_equipment_id],
    references: [measurement_equipments.id],
  }),
  operator: one(users, {
    fields: [tankages.operator_user_id],
    references: [users.id],
  }),
  organization: one(organizations, {
    fields: [tankages.organization_id],
    references: [organizations.id],
  }),
  created_by: one(users, {
    fields: [tankages.created_by_user_id],
    references: [users.id],
  }),
  lab_oil_analysis: one(lab_oil_analyses, {
    fields: [tankages.lab_oil_analysis_id],
    references: [lab_oil_analyses.id],
  }),
}));

export const tank_day_bulletin_relations = relations(
  tank_day_bulletins,
  ({ one }) => ({
    tank: one(tanks, {
      fields: [tank_day_bulletins.tank_id],
      references: [tanks.id],
    }),
    organization: one(organizations, {
      fields: [tank_day_bulletins.organization_id],
      references: [organizations.id],
    }),
    approved_by: one(users, {
      fields: [tank_day_bulletins.approved_by_user_id],
      references: [users.id],
      relationName: "tank_day_bulletin_approved_by",
    }),
    reopened_by: one(users, {
      fields: [tank_day_bulletins.reopened_by_user_id],
      references: [users.id],
      relationName: "tank_day_bulletin_reopened_by",
    }),
  }),
);

export const tank_transfer_relations = relations(tank_transfers, ({ one }) => ({
  tank: one(tanks, {
    fields: [tank_transfers.tank_id],
    references: [tanks.id],
  }),
  organization: one(organizations, {
    fields: [tank_transfers.organization_id],
    references: [organizations.id],
  }),
  tankage: one(tankages, {
    fields: [tank_transfers.tankage_id],
    references: [tankages.id],
  }),
  calibration: one(tank_calibrations, {
    fields: [tank_transfers.tank_calibration_id],
    references: [tank_calibrations.id],
  }),
  lab_oil_analysis: one(lab_oil_analyses, {
    fields: [tank_transfers.lab_oil_analysis_id],
    references: [lab_oil_analyses.id],
  }),
  created_by: one(users, {
    fields: [tank_transfers.created_by_user_id],
    references: [users.id],
  }),
}));
