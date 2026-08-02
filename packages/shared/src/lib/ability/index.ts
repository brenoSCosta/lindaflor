import { AbilityBuilder, createMongoAbility } from "@casl/ability";
import {
  applyOrgRoleGrants,
  applyRbacGrants,
  applyGrants,
  GLOBAL_CANNOT_GRANTS,
} from "@lindaflor/shared/lib/ability/grants";
import type {
  AbilityActor,
  AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";

export function defineAbilityFor(actor: AbilityActor): AppAbility {
  const builder = new AbilityBuilder<AppAbility>(createMongoAbility);

  applyRbacGrants(builder, actor);
  applyOrgRoleGrants(builder, actor);
  applyGrants(builder, GLOBAL_CANNOT_GRANTS, actor, null);

  return builder.build();
}
