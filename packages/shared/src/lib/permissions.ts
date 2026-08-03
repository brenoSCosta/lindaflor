import { createAccessControl } from "better-auth/plugins/access";
import {
  adminAc as adminAcRBAC,
  defaultStatements as defaultStatementRBAC,
  userAc as userAcRBAC,
} from "better-auth/plugins/admin/access";
import {
  adminAc,
  defaultStatements,
  memberAc,
  ownerAc,
} from "better-auth/plugins/organization/access";

/**
 * make sure to use `as const` so typescript can infer the type correctly
 */
const statementRBAC = {
  ...defaultStatementRBAC,
} as const;

const statementOrganization = {
  ...defaultStatements,
} as const;

export const organizationAC = createAccessControl(statementOrganization);

export const adminAC = createAccessControl(statementRBAC);

export const adminRBAC = adminAC.newRole({
  ...adminAcRBAC.statements,
});

export const moderatorRBAC = adminAC.newRole({
  ...userAcRBAC.statements,
});

export const userRBAC = adminAC.newRole({
  ...userAcRBAC.statements,
});

export const memberOrganization = organizationAC.newRole({
  ...memberAc.statements,
});

export const adminOrganization = organizationAC.newRole({
  ...adminAc.statements,
});

export const ownerOrganization = organizationAC.newRole({
  ...ownerAc.statements,
});

type NonEmptyArray<T> = [T, ...T[]];

type RBACStatements = typeof statementRBAC;
type OrganizationStatements = typeof statementOrganization;

type RBACPermissionInputs = {
  [K in keyof RBACStatements]: NonEmptyArray<RBACStatements[K][number]>;
};

type OrganizationPermissionInputs = {
  [K in keyof OrganizationStatements]: NonEmptyArray<
    OrganizationStatements[K][number]
  >;
};

/** At least one key required; each key's value must be a non-empty array */
export type OrganizationPermissionInputsAtLeastOne = {
  [K in keyof OrganizationPermissionInputs]: Pick<
    OrganizationPermissionInputs,
    K
  > &
    Partial<OrganizationPermissionInputs>;
}[keyof OrganizationPermissionInputs];

export type RBACPermissionInputsAtLeastOne = {
  [K in keyof RBACPermissionInputs]: Pick<RBACPermissionInputs, K> &
    Partial<RBACPermissionInputs>;
}[keyof RBACPermissionInputs];
