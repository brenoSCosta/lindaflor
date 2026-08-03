import type { ActionsBySubject } from "@lindaflor/shared/lib/ability/subjects";

export const SUBJECT_ACTIONS = {
  Todo: ["create", "read", "update", "delete", "manage"],
  Member: ["create", "read", "update", "delete", "manage"],
  User: ["create", "read", "update", "delete", "manage", "ban", "impersonate"],
  Organization: ["read", "update"],
  Product: ["create", "read", "update", "delete", "manage"],
  Inventory: ["create", "read", "update", "delete", "manage"],
  Order: ["create", "read", "update", "delete", "manage"],
} as const satisfies {
  [K in keyof ActionsBySubject]: ReadonlyArray<ActionsBySubject[K]>;
};
