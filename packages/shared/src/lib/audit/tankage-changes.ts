export const TANKAGE_AUDIT_EDITABLE_FIELDS = [
  "measured_at",
  "current_measurement",
  "oil_temperature_c",
  "ambient_temperature_c",
  "observation",
  "operator_user_id",
  "measurement_equipment_id",
] as const;

export type TankageAuditEditableField =
  (typeof TANKAGE_AUDIT_EDITABLE_FIELDS)[number];

export type TankageAuditChange = {
  field: TankageAuditEditableField;
  from: string | number | null;
  to: string | number | null;
};

type TankageAuditSnapshot = Partial<
  Record<TankageAuditEditableField, string | number | Date | null | undefined>
>;

function normalizeAuditValue(
  value: string | number | Date | null | undefined,
): string | number | null {
  if (value == null) return null;
  if (value instanceof Date) return value.toISOString();
  return value;
}

export function buildTankageAuditChanges(args: {
  before: TankageAuditSnapshot;
  after: TankageAuditSnapshot;
}): TankageAuditChange[] {
  const changes: TankageAuditChange[] = [];
  for (const field of TANKAGE_AUDIT_EDITABLE_FIELDS) {
    if (!(field in args.after)) continue;
    const from = normalizeAuditValue(args.before[field]);
    const to = normalizeAuditValue(args.after[field]);
    if (from === to) continue;
    changes.push({ field, from, to });
  }
  return changes;
}
