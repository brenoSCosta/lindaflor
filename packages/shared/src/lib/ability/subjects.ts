import {
  subject as caslSubject,
  type AnyAbility,
  type ForcedSubject,
  type MongoAbility,
} from "@casl/ability";
import type { OrgRoles, Roles } from "@lindaflor/shared/lib/roles";
import type { TodoOutput } from "@lindaflor/shared/schemas/todo";

type CrudActions = "create" | "read" | "update" | "delete" | "manage";

// Keys in `Required` must be present on every instance passed to `subject()` so
// CASL can match rule conditions; all other fields stay optional (partial rows,
// stubs, or auth types with looser nullability).
type AbilitySubject<T, Required extends keyof T> = Pick<T, Required> &
  Partial<Omit<T, Required>>;

/** Structural auth rows for CASL — keep drizzle-free so `shared` has no `db` edge. */
export type AbilityMember = {
  id: string;
  organization_id: string;
  user_id: string;
  role: string;
  created_at: Date;
};

export type AbilityUser = {
  id: string;
  name: string;
  email: string;
  email_verified: boolean;
  image: string | null;
  created_at: Date;
  updated_at: Date;
  role: string | null;
  banned: boolean | null;
  ban_reason: string | null;
  ban_expires: Date | null;
  two_factor_enabled: boolean | null;
};

export type AbilityOrganization = {
  id: string;
  name: string;
  slug: string;
  logo: string | null;
  created_at: Date;
  metadata: string | null;
};

export type SubjectMap = {
  Todo: TodoOutput;
  Member: AbilityMember;
  // `id` + `email` required; everything else optional so better-auth's UserWithRole (looser nullability) also fits.
  User: AbilitySubject<AbilityUser, "id" | "email">;
  Organization: AbilityOrganization;
  Product: { id: string };
  Inventory: { variant_id: string };
  Order: { id: string };
};

// Actions are defined per subject. Adding a new action to one subject does not
// leak it into the others — `ability.can('ban', 'Todo')` is a compile error.
export type ActionsBySubject = {
  Todo: CrudActions;
  Member: CrudActions;
  User: CrudActions | "ban" | "impersonate";
  Organization: Extract<CrudActions, "read" | "update">;
  Product: CrudActions;
  Inventory: CrudActions;
  Order: CrudActions;
};

// Discriminated tuple union: each (action, subject) pair must match one branch.
type Abilities = {
  [K in keyof SubjectMap]: [
    ActionsBySubject[K],
    K | (SubjectMap[K] & ForcedSubject<K>),
  ];
}[keyof SubjectMap];

export type AppAbility = MongoAbility<Abilities>;

export type Actions = ActionsBySubject[keyof ActionsBySubject];

export type Subjects = {
  [K in keyof SubjectMap]: K | (SubjectMap[K] & ForcedSubject<K>);
}[keyof SubjectMap];

export type SubjectName = Extract<Subjects, string>;

export function subject<K extends keyof SubjectMap>(
  type: K,
  object: SubjectMap[K],
): SubjectMap[K] & ForcedSubject<K> {
  return caslSubject(type, object);
}

// Caller-facing contract: `(action, subjectName)` must match one branch of
// ActionsBySubject — invalid pairs like ('ban', 'Todo') are compile errors.
// Internally we widen the ability to CASL's `AnyAbility` (permissive can/cannot
// overloads) because TS can't statically prove a discriminated tuple match
// under a generic K. `as AnyAbility` is a widening cast — safe and allowed by
// `no-unsafe-type-assertion`.
export const abilityCan = <K extends keyof ActionsBySubject>(
  ability: AppAbility,
  action: ActionsBySubject[K],
  subjectName: K,
): boolean => (ability as AnyAbility).can(action, subjectName);

export const abilityCannot = <K extends keyof ActionsBySubject>(
  ability: AppAbility,
  action: ActionsBySubject[K],
  subjectName: K,
): boolean => (ability as AnyAbility).cannot(action, subjectName);

export type AbilityActor = {
  userId: string;
  roles: readonly Roles[];
  activeOrganizationId: string | null;
  orgRole: OrgRoles | null;
};
