import {
  Can as BaseCan,
  AbilityProvider as CaslAbilityProvider,
  useAbility,
  type CanProps,
} from "@casl/react";
import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { parseRoles, toOrgRole } from "@lindaflor/shared/lib/roles";
import { useMemo, type ReactNode } from "react";

import { authClient } from "@/lib/auth-client";

const emptyAbility = (): AppAbility =>
  defineAbilityFor({
    userId: "",
    roles: [],
    activeOrganizationId: null,
    orgRole: null,
  });

export const useAppAbility = (): AppAbility => useAbility<AppAbility>();

export function Can(props: CanProps<AppAbility>) {
  return <BaseCan<AppAbility> {...props} />;
}

export function AbilityProvider({ children }: { children: ReactNode }) {
  const { data: session } = authClient.useSession();
  const { data: activeMember } = authClient.useActiveMember();

  const ability = useMemo(() => {
    if (!session?.user) return emptyAbility();
    return defineAbilityFor({
      userId: session.user.id,
      roles: parseRoles(session.user.role),
      activeOrganizationId: session.session.activeOrganizationId ?? null,
      orgRole: toOrgRole(activeMember?.role),
    });
  }, [session, activeMember]);

  return <CaslAbilityProvider value={ability}>{children}</CaslAbilityProvider>;
}
