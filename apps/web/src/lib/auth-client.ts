import {
  adminAC,
  adminOrganization,
  adminRBAC,
  memberOrganization,
  moderatorRBAC,
  organizationAC,
  ownerOrganization,
  userRBAC,
} from "@lindaflor/shared/lib/permissions";
import {
  adminClient,
  jwtClient,
  organizationClient,
  twoFactorClient,
} from "better-auth/client/plugins";
import { createAuthClient } from "better-auth/react";

import { getApiBaseUrl } from "@/lib/api-base-url";

export const authClient = createAuthClient({
  baseURL: getApiBaseUrl(),
  plugins: [
    twoFactorClient({
      onTwoFactorRedirect: () => {
        window.location.href = "/two-factor";
      },
    }),
    jwtClient(),
    adminClient({
      ac: adminAC,
      roles: {
        admin: adminRBAC,
        moderator: moderatorRBAC,
        user: userRBAC,
      },
    }),
    organizationClient({
      ac: organizationAC,
      roles: {
        member: memberOrganization,
        admin: adminOrganization,
        owner: ownerOrganization,
      },
    }),
  ],
});
