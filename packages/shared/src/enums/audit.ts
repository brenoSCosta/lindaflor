export const audit_event_actions = [
  "create",
  "update",
  "delete",
  "approve",
  "reopen",
  "retreat",
] as const;
export type AuditEventAction = (typeof audit_event_actions)[number];

export const audit_entity_types = [
  "tank_day_bulletin",
  "tankage",
  "tank_transfer",
] as const;
export type AuditEntityType = (typeof audit_entity_types)[number];

export const audit_aggregate_types = ["tank_day_bulletin"] as const;
export type AuditAggregateType = (typeof audit_aggregate_types)[number];
