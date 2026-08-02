export const RBAC_ROLES = ["admin", "moderator", "user"] as const;
export type Roles = (typeof RBAC_ROLES)[number];

const rbacRoleSet = new Set<string>(RBAC_ROLES);

export function isRole(s: string): s is Roles {
  return rbacRoleSet.has(s);
}

export const parseRoles = (raw: string | null | undefined): readonly Roles[] =>
  (raw ?? "user")
    .split(",")
    .map((r) => r.trim())
    .filter((r): r is Roles => r !== "" && isRole(r));

export const ORGANIZATION_ROLES = [
  "owner",
  "admin",
  "supervisor",
  "operator",
  "member",
] as const;
export type OrgRoles = (typeof ORGANIZATION_ROLES)[number];

const orgRoleSet = new Set<string>(ORGANIZATION_ROLES);

export function isOrgRole(s: string): s is OrgRoles {
  return orgRoleSet.has(s);
}

export const toOrgRole = (raw: string | null | undefined): OrgRoles | null =>
  raw != null && isOrgRole(raw) ? raw : null;
