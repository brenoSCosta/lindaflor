export const TRANSFER_AUDIT_EDITABLE_FIELDS = [
  "transferred_at",
  "height_before_m",
  "height_after_m",
  "oil_temperature_c",
  "ambient_temperature_c",
  "destination_label",
  "observation",
] as const;

export type TransferAuditEditableField =
  (typeof TRANSFER_AUDIT_EDITABLE_FIELDS)[number];

export type TransferAuditChange = {
  field: TransferAuditEditableField;
  from: string | number | null;
  to: string | number | null;
};

type TransferAuditSnapshot = Partial<
  Record<TransferAuditEditableField, string | number | Date | null | undefined>
>;

function normalizeAuditValue(
  value: string | number | Date | null | undefined,
): string | number | null {
  if (value == null) return null;
  if (value instanceof Date) return value.toISOString();
  return value;
}

export function buildTransferAuditChanges(args: {
  before: TransferAuditSnapshot;
  after: TransferAuditSnapshot;
}): TransferAuditChange[] {
  const changes: TransferAuditChange[] = [];
  for (const field of TRANSFER_AUDIT_EDITABLE_FIELDS) {
    if (!(field in args.after)) continue;
    const from = normalizeAuditValue(args.before[field]);
    const to = normalizeAuditValue(args.after[field]);
    if (from === to) continue;
    changes.push({ field, from, to });
  }
  return changes;
}
