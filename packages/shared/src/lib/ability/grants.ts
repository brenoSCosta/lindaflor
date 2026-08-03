import type { AbilityBuilder, AnyAbility } from "@casl/ability";
import type {
  AbilityActor,
  ActionsBySubject,
  AppAbility,
  SubjectMap,
} from "@lindaflor/shared/lib/ability/subjects";
import type { OrgRoles } from "@lindaflor/shared/lib/roles";

type OrgScopeKind = "organization_id" | "organization_id_as_id";

type GrantMode = keyof Pick<AbilityBuilder<AppAbility>, "can" | "cannot">;

type SubjectKey = keyof SubjectMap;

type AbilityBuilderApi = Pick<AbilityBuilder<AppAbility>, "can" | "cannot">;

type OrgAbilityGrant<S extends SubjectKey = SubjectKey> = {
  mode?: GrantMode;
  actions: ActionsBySubject[S] | readonly ActionsBySubject[S][];
  subject: S;
  scope?: OrgScopeKind;
  conditions?: (
    actor: AbilityActor,
    org: OrgScopeContext,
  ) => Record<string, unknown>;
};

type AbilityGrant<S extends SubjectKey = SubjectKey> = {
  mode?: GrantMode;
  actions: ActionsBySubject[S] | readonly ActionsBySubject[S][];
  subject: S;
  scope?: OrgScopeKind;
  conditions?: (
    actor: AbilityActor,
    org: OrgScopeContext | null,
  ) => Record<string, unknown>;
};

export type OrgScopeContext = {
  organization_id: string;
  orgIdScope: { id: string };
};

type ApplyCaslRule = {
  (
    action: string | string[],
    subject: string,
    conditions: Record<string, unknown>,
  ): void;
  (action: string | string[], subject: string): void;
};

// Same widening pattern as `abilityCan` in subjects.ts.
const caslRules = (builder: AbilityBuilderApi): AnyAbility =>
  // oxlint-disable-next-line no-unsafe-type-assertion -- AbilityBuilder rule overloads cannot accept declarative grant unions
  builder as unknown as AnyAbility;

const caslApply = (casl: AnyAbility, mode: GrantMode): ApplyCaslRule =>
  // oxlint-disable-next-line no-unsafe-type-assertion -- loose rule API for declarative grants
  (mode === "can" ? casl.can : casl.cannot) as unknown as ApplyCaslRule;

function applyRule(
  builder: AbilityBuilderApi,
  mode: GrantMode,
  actions: AbilityGrant["actions"],
  subject: keyof SubjectMap,
  conditions?: Record<string, unknown>,
): void {
  const apply = caslApply(caslRules(builder), mode);
  const ruleActions =
    // oxlint-disable-next-line no-unsafe-type-assertion -- grant action unions are validated at grant definition time
    (Array.isArray(actions) ? [...actions] : actions) as string | string[];
  if (conditions !== undefined) {
    apply(ruleActions, subject, conditions);
  } else {
    apply(ruleActions, subject);
  }
}

const orgScopeGrant = <S extends SubjectKey>(
  actions: OrgAbilityGrant<S>["actions"],
  subject: S,
  scope: OrgScopeKind = "organization_id",
): OrgAbilityGrant<S> => ({ actions, subject, scope });

const RBAC_ADMIN_CAN: readonly AbilityGrant[] = [
  { actions: "manage", subject: "User" },
  { actions: "manage", subject: "Product" },
  { actions: "manage", subject: "Inventory" },
  { actions: "manage", subject: "Order" },
];

const RBAC_ADMIN_CANNOT: readonly AbilityGrant[] = [
  {
    mode: "cannot",
    actions: ["delete", "ban", "impersonate"],
    subject: "User",
    conditions: (actor) => ({ id: actor.userId }),
  },
];

const RBAC_MODERATOR_CAN: readonly AbilityGrant[] = [];

const OWNER_GRANTS: readonly OrgAbilityGrant[] = [
  orgScopeGrant("manage", "Member"),
  orgScopeGrant(["read", "update"], "Organization", "organization_id_as_id"),
];

const ADMIN_GRANTS: readonly OrgAbilityGrant[] = [
  // `create` on Member means inviting; admins may invite and cancel
  // invitations, but updating roles / removing members stays owner-only.
  orgScopeGrant(["create", "read"], "Member"),
  orgScopeGrant(["read", "update"], "Organization", "organization_id_as_id"),
];

const MEMBER_GRANTS: readonly OrgAbilityGrant[] = [
  orgScopeGrant("read", "Member"),
  orgScopeGrant("read", "Organization", "organization_id_as_id"),
  // {
  //   actions: "read",
  //   subject: "",
  //   conditions: (_actor, org) => ({
  //     organization_id: org.organization_id,
  //     flag: true,
  //   }),
  // }
];

export const ORG_ROLE_GRANTS: Record<OrgRoles, readonly OrgAbilityGrant[]> = {
  owner: OWNER_GRANTS,
  admin: ADMIN_GRANTS,
  member: MEMBER_GRANTS,
};

export const GLOBAL_CANNOT_GRANTS: readonly AbilityGrant[] = [
  // {
  //   mode: "cannot",
  //   actions: "",
  //   subject: "",
  //   conditions: () => ({ status: "sold-out" }),
  // },
];

function resolveOrgGrantConditions(
  grant: OrgAbilityGrant,
  actor: AbilityActor,
  org: OrgScopeContext,
): Record<string, unknown> | undefined {
  if (grant.conditions) {
    return grant.conditions(actor, org);
  }
  if (grant.scope === "organization_id_as_id") {
    return org.orgIdScope;
  }
  return { organization_id: org.organization_id };
}

function resolveGrantConditions(
  grant: AbilityGrant,
  actor: AbilityActor,
  org: OrgScopeContext | null,
): Record<string, unknown> | undefined {
  if (grant.conditions) {
    return grant.conditions(actor, org);
  }
  if (!org) {
    return undefined;
  }
  if (grant.scope === "organization_id_as_id") {
    return org.orgIdScope;
  }
  return { organization_id: org.organization_id };
}

export function applyGrants(
  builder: AbilityBuilderApi,
  grants: readonly AbilityGrant[],
  actor: AbilityActor,
  org: OrgScopeContext | null,
): void {
  for (const grant of grants) {
    const mode = grant.mode ?? "can";
    const conditions = resolveGrantConditions(grant, actor, org);
    applyRule(builder, mode, grant.actions, grant.subject, conditions);
  }
}

export function applyOrgGrants(
  builder: AbilityBuilderApi,
  grants: readonly OrgAbilityGrant[],
  actor: AbilityActor,
  org: OrgScopeContext,
): void {
  for (const grant of grants) {
    const mode = grant.mode ?? "can";
    const conditions = resolveOrgGrantConditions(grant, actor, org);
    applyRule(builder, mode, grant.actions, grant.subject, conditions);
  }
}

export function applyRbacGrants(
  builder: AbilityBuilderApi,
  actor: AbilityActor,
): void {
  if (actor.roles.includes("admin")) {
    applyGrants(builder, RBAC_ADMIN_CAN, actor, null);
    applyGrants(builder, RBAC_ADMIN_CANNOT, actor, null);
  }
  if (actor.roles.includes("moderator")) {
    applyGrants(builder, RBAC_MODERATOR_CAN, actor, null);
  }
}

export function applyOrgRoleGrants(
  builder: AbilityBuilderApi,
  actor: AbilityActor,
): void {
  if (!actor.activeOrganizationId || !actor.orgRole) {
    return;
  }
  const org: OrgScopeContext = {
    organization_id: actor.activeOrganizationId,
    orgIdScope: { id: actor.activeOrganizationId },
  };
  applyOrgGrants(builder, ORG_ROLE_GRANTS[actor.orgRole], actor, org);
}
