import { describe, expect, it } from "bun:test";

import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type {
  AbilityMember,
  AbilityOrganization,
  AbilityUser,
} from "@lindaflor/shared/lib/ability/subjects";
import type { CurriculumOutput } from "@lindaflor/shared/schemas/curriculum";
import type { TankCalibrationOutput } from "@lindaflor/shared/schemas/tankage/calibrations";
import type { TankDayBulletinOutput } from "@lindaflor/shared/schemas/tankage/day-bulletins";
import type { TankageOutput } from "@lindaflor/shared/schemas/tankage/tankages";
import type { TankOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import type { TankTransferOutput } from "@lindaflor/shared/schemas/tankage/transfers";
import type { TodoOutput } from "@lindaflor/shared/schemas/todo";
import type { TrainingCourseOutput } from "@lindaflor/shared/schemas/training";

const tankDayBulletinInOrg = (
  organization_id: string,
  overrides: Partial<TankDayBulletinOutput> = {},
): TankDayBulletinOutput => ({
  id: null,
  tank_id: "00000000-0000-4000-8000-000000000002",
  operational_day: "1970-01-01",
  status: "open",
  approved_at: null,
  approved_by_user_id: null,
  approved_by_name: null,
  reopened_at: null,
  reopened_by_user_id: null,
  reopened_by_name: null,
  organization_id,
  ...overrides,
});

const tankageInOrg = (
  organization_id: string,
  overrides: Partial<TankageOutput> = {},
): TankageOutput => ({
  id: "00000000-0000-4000-8000-000000000001",
  tank_id: "00000000-0000-4000-8000-000000000002",
  tag: "TQ-000",
  concession_id: "00000000-0000-4000-8000-000000000003",
  concession_name: "Concession",
  installation_id: "00000000-0000-4000-8000-000000000004",
  installation_name: "Installation",
  measurement_equipment_id: null,
  measurement_equipment_code: null,
  operator_user_id: "00000000-0000-4000-8000-000000000005",
  operator_name: "Operator",
  measured_at: new Date(0),
  operational_day: "1970-01-01",
  previous_measurement: 0,
  current_measurement: 0,
  oil_temperature_c: 0,
  ambient_temperature_c: 0,
  observation: "",
  latitude: null,
  longitude: null,
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
  created_by_user_id: "00000000-0000-4000-8000-000000000006",
  created_at: new Date(0),
  updated_at: new Date(0),
  organization_id,
  bulletin_status: "open",
  ...overrides,
});

const tankTransferInOrg = (
  organization_id: string,
  overrides: Partial<TankTransferOutput> = {},
): TankTransferOutput => ({
  id: "00000000-0000-4000-8000-000000000001",
  tank_id: "00000000-0000-4000-8000-000000000002",
  organization_id,
  operational_day: "1970-01-01",
  transferred_at: new Date(0),
  height_before_m: 0,
  height_after_m: 0,
  oil_temperature_c: 0,
  ambient_temperature_c: 0,
  gross_volume_before_m3: 0,
  gross_volume_after_m3: 0,
  gross_volume_out_m3: 0,
  gross_volume_out_m3_20c: null,
  net_oil_volume_out_m3_20c: null,
  shell_temperature_c: null,
  shell_correction_factor: null,
  liquid_correction_factor: null,
  combined_correction_factor: null,
  tank_calibration_id: null,
  lab_oil_analysis_id: null,
  density_at_20c_kg_m3: null,
  water_and_sediment_percent: null,
  destination_label: null,
  observation: "",
  tankage_id: "00000000-0000-4000-8000-000000000003",
  created_by_user_id: "00000000-0000-4000-8000-000000000004",
  created_at: new Date(0),
  updated_at: new Date(0),
  bulletin_status: "open",
  ...overrides,
});

const todoInOrg = (organization_id: string, id = "todo-1"): TodoOutput => ({
  id,
  text: "test todo",
  status: "todo",
  label: "documentation",
  priority: "medium",
  estimated_hours: 0,
  actual_hours: 0,
  progress: 0,
  cost: 0,
  due_date: null,
  completed_at: null,
  created_at: new Date(),
  updated_at: new Date(),
  organization_id,
});

const tankInOrg = (organization_id: string, id = "tank-1"): TankOutput => ({
  id,
  tag: "TQ-001",
  concession_id: "concession-1",
  concession_name: "Concession",
  installation_id: "installation-1",
  installation_name: "Installation",
  measurement_equipment_id: null,
  measurement_equipment_code: null,
  latitude: null,
  longitude: null,
  organization_id,
  created_by_user_id: "user-x",
  created_at: new Date(),
  updated_at: new Date(),
});

const tankCalibrationInOrg = (
  organization_id: string,
  overrides: Partial<TankCalibrationOutput> = {},
): TankCalibrationOutput => ({
  id: "cal-1",
  tank_id: "tank-1",
  certificate_number: "CERT-001",
  issued_at: null,
  valid_from: "2026-01-01",
  valid_until: "2026-12-31",
  organization_id,
  created_by_user_id: "user-x",
  created_at: new Date(),
  updated_at: new Date(),
  is_expired: false,
  ...overrides,
});

const memberInOrg = (
  organization_id: string,
  id = "member-1",
): AbilityMember => ({
  id,
  organization_id,
  user_id: "user-x",
  role: "member",
  created_at: new Date(),
});

const trainingCourseInOrg = (
  organization_id: string,
  id = "course-1",
  overrides: Partial<TrainingCourseOutput> = {},
): TrainingCourseOutput => ({
  id,
  title: "Course",
  description: null,
  is_published: true,
  enrolled: false,
  organization_id,
  created_by_user_id: "user-x",
  created_at: new Date(),
  updated_at: new Date(),
  ...overrides,
});
const curriculumRow = (id = "curriculum-1"): CurriculumOutput => ({
  id,
  name: "Candidate",
  email: "candidate@example.com",
  phone: null,
  headline: "Técnico de Operação",
  summary: null,
  skills: [],
  career_id: null,
  file_key: "curriculums/test.pdf",
  file_name: "test.pdf",
  file_size: 1,
  mime_type: "application/pdf",
  submitted_at: new Date(),
});

const userRow = (id = "other-user"): AbilityUser => ({
  id,
  name: "Test User",
  email: `${id}@example.com`,
  email_verified: true,
  image: null,
  created_at: new Date(),
  updated_at: new Date(),
  role: "user",
  banned: false,
  ban_reason: null,
  ban_expires: null,
  two_factor_enabled: false,
});

const orgRow = (id = "org-1"): AbilityOrganization => ({
  id,
  name: "Test Organization",
  slug: `test-org-${id}`,
  logo: null,
  created_at: new Date(),
  metadata: null,
});

describe("defineAbilityFor", () => {
  describe("admin RBAC role", () => {
    it("with no active org, cannot touch org-scoped Todos", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-x")))).toBe(
        false,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-x")))).toBe(
        false,
      );
    });

    it("who is owner of active org can manage that org Todos and still manage Users", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("read", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
      expect(ability.can("manage", "User")).toBe(true);
    });

    it("who is member of active org gets member-level Todo access", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
      expect(ability.can("manage", "User")).toBe(true);
    });
  });

  describe("org owner", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "owner" as const,
    };

    it("can manage Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("update", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });

    it("cannot touch Todo from a different org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
    });

    it("can manage Tanks in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "Tanks")).toBe(true);
      expect(ability.can("delete", subject("Tanks", tankInOrg("org-1")))).toBe(
        true,
      );
    });

    it("can manage MeasurementEquipments in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "MeasurementEquipments")).toBe(true);
    });

    it("can manage TankCalibrations in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "TankCalibrations")).toBe(true);
      const active = subject(
        "TankCalibrations",
        tankCalibrationInOrg("org-1", { is_expired: false }),
      );
      expect(ability.can("update", active)).toBe(true);
      expect(ability.can("delete", active)).toBe(true);
      const expired = subject(
        "TankCalibrations",
        tankCalibrationInOrg("org-1", {
          is_expired: true,
          valid_until: "2020-01-01",
        }),
      );
      expect(ability.can("update", expired)).toBe(false);
      expect(ability.can("delete", expired)).toBe(false);
      expect(ability.can("read", expired)).toBe(true);
    });

    it("can manage LabOilAnalyses in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "LabOilAnalyses")).toBe(true);
    });

    it("can manage TankTransfers in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "TankTransfers")).toBe(true);
    });

    it("cannot delete TankDayBulletins when approved", () => {
      const ability = defineAbilityFor(actor);
      const approved = subject(
        "TankDayBulletins",
        tankDayBulletinInOrg("org-1", { status: "approved" }),
      );
      expect(ability.can("delete", approved)).toBe(false);
    });

    it("can retreat Tankages and TankTransfers only when bulletin is approved", () => {
      const ability = defineAbilityFor(actor);
      const openTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "approved" }),
      );
      const openTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("retreat", approvedTankage)).toBe(true);
      expect(ability.can("retreat", openTankage)).toBe(false);
      expect(ability.can("retreat", approvedTransfer)).toBe(true);
      expect(ability.can("retreat", openTransfer)).toBe(false);
    });
  });

  describe("org admin", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "admin" as const,
    };

    it("can create, read, update Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("update", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });

    it("cannot delete Todo (owner-only)", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
    });

    it("can manage Tanks in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "Tanks")).toBe(true);
      expect(ability.can("delete", subject("Tanks", tankInOrg("org-1")))).toBe(
        true,
      );
    });

    it("can manage MeasurementEquipments in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "MeasurementEquipments")).toBe(true);
    });

    it("can manage TankCalibrations in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("manage", "TankCalibrations")).toBe(true);
      const expired = subject(
        "TankCalibrations",
        tankCalibrationInOrg("org-1", {
          is_expired: true,
          valid_until: "2020-01-01",
        }),
      );
      expect(ability.can("update", expired)).toBe(false);
      expect(ability.can("delete", expired)).toBe(false);
    });

    it("can create, read, update, and delete LabOilAnalyses in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "LabOilAnalyses")).toBe(true);
      expect(ability.can("read", "LabOilAnalyses")).toBe(true);
      expect(ability.can("update", "LabOilAnalyses")).toBe(true);
      expect(ability.can("delete", "LabOilAnalyses")).toBe(true);
    });

    it("can reopen TankDayBulletins but not approve", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "TankDayBulletins")).toBe(true);
      expect(ability.can("reopen", "TankDayBulletins")).toBe(true);
      expect(ability.can("approve", "TankDayBulletins")).toBe(false);
    });

    it("can reopen approved bulletins but not open ones", () => {
      const ability = defineAbilityFor(actor);
      const openBulletin = subject(
        "TankDayBulletins",
        tankDayBulletinInOrg("org-1", { status: "open" }),
      );
      const approvedBulletin = subject(
        "TankDayBulletins",
        tankDayBulletinInOrg("org-1", { status: "approved" }),
      );
      expect(ability.can("reopen", openBulletin)).toBe(false);
      expect(ability.can("reopen", approvedBulletin)).toBe(true);
    });

    it("cannot retreat Tankages or TankTransfers", () => {
      const ability = defineAbilityFor(actor);
      const approvedTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "approved" }),
      );
      const approvedTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("retreat", "Tankages")).toBe(false);
      expect(ability.can("retreat", approvedTankage)).toBe(false);
      expect(ability.can("retreat", approvedTransfer)).toBe(false);
    });
  });

  describe("org operator", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "operator" as const,
    };

    it("can create, read, update Tankages, Concessions, Installations in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Tankages")).toBe(true);
      expect(ability.can("read", "Tankages")).toBe(true);
      expect(ability.can("update", "Tankages")).toBe(true);
      expect(ability.can("create", "LabOilAnalyses")).toBe(true);
      expect(ability.can("read", "LabOilAnalyses")).toBe(true);
      expect(ability.can("update", "LabOilAnalyses")).toBe(true);
      expect(ability.can("create", "Concessions")).toBe(true);
      expect(ability.can("read", "Concessions")).toBe(true);
      expect(ability.can("update", "Concessions")).toBe(true);
      expect(ability.can("create", "Installations")).toBe(true);
      expect(ability.can("read", "Installations")).toBe(true);
      expect(ability.can("update", "Installations")).toBe(true);
    });

    it("cannot delete Tankages, Concessions, or Installations", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("delete", "Tankages")).toBe(false);
      expect(ability.can("delete", "LabOilAnalyses")).toBe(false);
      expect(ability.can("delete", "Concessions")).toBe(false);
      expect(ability.can("delete", "Installations")).toBe(false);
    });

    it("can read Tanks but cannot create, update, or delete them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "Tanks")).toBe(true);
      expect(ability.can("create", "Tanks")).toBe(false);
      expect(ability.can("update", "Tanks")).toBe(false);
      expect(ability.can("delete", subject("Tanks", tankInOrg("org-1")))).toBe(
        false,
      );
    });

    it("can read MeasurementEquipments but cannot manage them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "MeasurementEquipments")).toBe(true);
      expect(ability.can("create", "MeasurementEquipments")).toBe(false);
      expect(ability.can("manage", "MeasurementEquipments")).toBe(false);
    });

    it("can read TankCalibrations but cannot manage them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "TankCalibrations")).toBe(true);
      expect(ability.can("create", "TankCalibrations")).toBe(false);
      expect(ability.can("manage", "TankCalibrations")).toBe(false);
    });

    it("can read members but not manage them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("create", "Member")).toBe(false);
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
    });

    it("can mutate Tankages when bulletin is open but not when approved", () => {
      const ability = defineAbilityFor(actor);
      const openDay = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedDay = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("create", openDay)).toBe(true);
      expect(ability.can("update", openDay)).toBe(true);
      expect(ability.can("create", approvedDay)).toBe(false);
      expect(ability.can("update", approvedDay)).toBe(false);
    });

    it("can mutate TankTransfers when bulletin is open but not when approved", () => {
      const ability = defineAbilityFor(actor);
      const openDay = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedDay = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("create", openDay)).toBe(true);
      expect(ability.can("update", openDay)).toBe(true);
      expect(ability.can("create", approvedDay)).toBe(false);
      expect(ability.can("update", approvedDay)).toBe(false);
    });

    it("cannot retreat Tankages or TankTransfers", () => {
      const ability = defineAbilityFor(actor);
      const approvedTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "approved" }),
      );
      const approvedTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("retreat", "Tankages")).toBe(false);
      expect(ability.can("retreat", approvedTankage)).toBe(false);
      expect(ability.can("retreat", approvedTransfer)).toBe(false);
    });
  });

  describe("org supervisor", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "supervisor" as const,
    };

    it("can read org-scoped tankage resources but not create or update them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "Concessions")).toBe(true);
      expect(ability.can("create", "Concessions")).toBe(false);
      expect(ability.can("update", "Concessions")).toBe(false);
      expect(ability.can("read", "Installations")).toBe(true);
      expect(ability.can("read", "MeasurementEquipments")).toBe(true);
      expect(ability.can("read", "Tanks")).toBe(true);
      expect(ability.can("read", "TankCalibrations")).toBe(true);
      expect(ability.can("read", "Tankages")).toBe(true);
      expect(ability.can("create", "Tankages")).toBe(false);
    });

    it("can read members and organization but not manage them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("create", "Member")).toBe(false);
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(false);
    });

    it("cannot access LabOilAnalyses or Training", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "LabOilAnalyses")).toBe(false);
      expect(ability.can("read", "Training")).toBe(false);
    });

    it("can read and approve TankDayBulletins but not reopen", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "TankDayBulletins")).toBe(true);
      expect(ability.can("approve", "TankDayBulletins")).toBe(true);
      expect(ability.can("reopen", "TankDayBulletins")).toBe(false);
    });

    it("can approve open bulletins but not already approved ones", () => {
      const ability = defineAbilityFor(actor);
      const openBulletin = subject(
        "TankDayBulletins",
        tankDayBulletinInOrg("org-1", { status: "open" }),
      );
      const approvedBulletin = subject(
        "TankDayBulletins",
        tankDayBulletinInOrg("org-1", { status: "approved" }),
      );
      expect(ability.can("approve", openBulletin)).toBe(true);
      expect(ability.can("approve", approvedBulletin)).toBe(false);
    });

    it("can retreat Tankages and TankTransfers only when bulletin is approved", () => {
      const ability = defineAbilityFor(actor);
      const openTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedTankage = subject(
        "Tankages",
        tankageInOrg("org-1", { bulletin_status: "approved" }),
      );
      const openTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "open" }),
      );
      const approvedTransfer = subject(
        "TankTransfers",
        tankTransferInOrg("org-1", { bulletin_status: "approved" }),
      );
      expect(ability.can("retreat", "Tankages")).toBe(true);
      expect(ability.can("retreat", "TankTransfers")).toBe(true);
      expect(ability.can("retreat", openTankage)).toBe(false);
      expect(ability.can("retreat", approvedTankage)).toBe(true);
      expect(ability.can("retreat", openTransfer)).toBe(false);
      expect(ability.can("retreat", approvedTransfer)).toBe(true);
      expect(ability.can("update", approvedTankage)).toBe(false);
      expect(ability.can("update", approvedTransfer)).toBe(false);
    });
  });

  describe("org member", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "member" as const,
    };

    it("can create and read Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });

    it("can read Tanks but cannot create them", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", "Tanks")).toBe(true);
      expect(ability.can("create", "Tanks")).toBe(false);
    });

    it("can create and read LabOilAnalyses but cannot update or delete", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "LabOilAnalyses")).toBe(true);
      expect(ability.can("read", "LabOilAnalyses")).toBe(true);
      expect(ability.can("update", "LabOilAnalyses")).toBe(false);
      expect(ability.can("delete", "LabOilAnalyses")).toBe(false);
    });

    it("cannot update or delete Todo", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("update", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
    });
  });

  describe("no active org", () => {
    it("cannot do anything on Todo", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Todo")).toBe(false);
      expect(ability.can("create", "Todo")).toBe(false);
    });
  });

  describe("active org but not a member of it", () => {
    it("cannot do anything on Todo", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: null,
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
    });
  });

  describe("Member subject", () => {
    it("admin RBAC cannot manage members in any org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(
        ability.can("update", subject("Member", memberInOrg("org-x"))),
      ).toBe(false);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-y"))),
      ).toBe(false);
      expect(ability.can("read", subject("Member", memberInOrg("org-x")))).toBe(
        false,
      );
    });

    it("org owner can invite, update, and remove members in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("create", "Member")).toBe(true);
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
    });

    it("org owner cannot touch members in a different org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("update", subject("Member", memberInOrg("org-2"))),
      ).toBe(false);
    });

    it("org admin can create (invite) and read but not update or delete members", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "admin",
      });
      // The invite button gates on the type-level check.
      expect(ability.can("create", "Member")).toBe(true);
      expect(
        ability.can("create", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
      expect(
        ability.can("create", subject("Member", memberInOrg("org-2"))),
      ).toBe(false);
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
    });

    it("org member can only read members (cannot invite)", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("create", "Member")).toBe(false);
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
    });
  });

  describe("moderator RBAC role", () => {
    it("can manage Curriculum without active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["moderator"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Curriculum")).toBe(true);
      expect(
        ability.can("delete", subject("Curriculum", curriculumRow())),
      ).toBe(true);
    });

    it("cannot manage Users", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["moderator"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", subject("User", userRow()))).toBe(false);
      expect(ability.can("manage", "User")).toBe(false);
    });
  });

  describe("Curriculum subject", () => {
    it("admin can manage Curriculum", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("manage", "Curriculum")).toBe(true);
    });

    it("regular user cannot access Curriculum", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("read", "Curriculum")).toBe(false);
      expect(ability.can("read", subject("Curriculum", curriculumRow()))).toBe(
        false,
      );
    });
  });

  describe("Training subject", () => {
    it("org member who is enrolled can progress and request certificate", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-1"),
        enrolled: true,
      });
      expect(ability.can("progress", course)).toBe(true);
      expect(ability.can("certificate", course)).toBe(true);
    });

    it("org member who is NOT enrolled cannot progress or request certificate", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-1"),
        enrolled: false,
      });
      expect(ability.can("progress", course)).toBe(false);
      expect(ability.can("certificate", course)).toBe(false);
    });

    it("org member enrolled in any course still cannot progress on a different org's course", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-2"),
        enrolled: true,
      });
      expect(ability.can("progress", course)).toBe(false);
      expect(ability.can("certificate", course)).toBe(false);
    });

    it("org member can read a published course in the active org without enrolling", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(
        ability.can("read", subject("Training", trainingCourseInOrg("org-1"))),
      ).toBe(true);
    });

    it("org owner bypasses enrollment — can progress and request certificate even when enrolled is false", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-1"),
        enrolled: false,
      });
      expect(ability.can("progress", course)).toBe(true);
      expect(ability.can("certificate", course)).toBe(true);
    });

    it("org admin bypasses enrollment — can progress and request certificate", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "admin",
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-1"),
        enrolled: false,
      });
      expect(ability.can("progress", course)).toBe(true);
      expect(ability.can("certificate", course)).toBe(true);
    });

    it("global admin bypasses enrollment (no active org needed)", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: "org-1",
        orgRole: null,
      });
      const course = subject("Training", {
        ...trainingCourseInOrg("org-1"),
        enrolled: false,
      });
      expect(ability.can("progress", course)).toBe(true);
      expect(ability.can("certificate", course)).toBe(true);
    });

    it("org member without enrolled flag falls back to no progress / no certificate", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(
        ability.can(
          "progress",
          subject("Training", trainingCourseInOrg("org-1")),
        ),
      ).toBe(false);
      expect(
        ability.can(
          "certificate",
          subject("Training", trainingCourseInOrg("org-1")),
        ),
      ).toBe(false);
    });
  });

  describe("User subject", () => {
    const adminAbility = defineAbilityFor({
      userId: "admin-1",
      roles: ["admin"],
      activeOrganizationId: null,
      orgRole: null,
    });

    it("admin can update, ban, delete, impersonate other users", () => {
      const other = userRow("other-1");
      expect(adminAbility.can("update", subject("User", other))).toBe(true);
      expect(adminAbility.can("ban", subject("User", other))).toBe(true);
      expect(adminAbility.can("delete", subject("User", other))).toBe(true);
      expect(adminAbility.can("impersonate", subject("User", other))).toBe(
        true,
      );
    });

    it("admin can read and update themselves", () => {
      const self = userRow("admin-1");
      expect(adminAbility.can("read", subject("User", self))).toBe(true);
      expect(adminAbility.can("update", subject("User", self))).toBe(true);
    });

    it("admin cannot delete, ban, or impersonate themselves", () => {
      const self = userRow("admin-1");
      expect(adminAbility.can("delete", subject("User", self))).toBe(false);
      expect(adminAbility.can("ban", subject("User", self))).toBe(false);
      expect(adminAbility.can("impersonate", subject("User", self))).toBe(
        false,
      );
    });

    it("non-admin RBAC user has no User access", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("read", subject("User", userRow("any")))).toBe(false);
      expect(ability.can("update", subject("User", userRow("any")))).toBe(
        false,
      );
      expect(ability.can("ban", subject("User", userRow("any")))).toBe(false);
      expect(ability.can("delete", subject("User", userRow("any")))).toBe(
        false,
      );
      expect(ability.can("impersonate", subject("User", userRow("any")))).toBe(
        false,
      );
    });
  });

  describe("Organization subject", () => {
    it("org owner can read and update Organization in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
    });

    it("org owner cannot update Organization in a different org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("update", subject("Organization", orgRow("org-2"))),
      ).toBe(false);
    });

    it("org admin can read and update Organization in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "admin",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
    });

    it("org operator can only read, not update Organization", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "operator",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(false);
    });

    it("org member can only read, not update Organization", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(false);
    });

    it("no active org cannot access Organization", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Organization")).toBe(false);
      expect(ability.can("update", "Organization")).toBe(false);
    });

    it("global admin cannot access Organization without active org role", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Organization")).toBe(false);
      expect(ability.can("update", "Organization")).toBe(false);
    });
  });
});
