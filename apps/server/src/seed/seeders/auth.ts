import { db } from "@lindaflor/db";
import {
  accounts,
  invitations,
  members,
  sessions,
  users,
} from "@lindaflor/db/schema/auth";
import { RBAC_ROLES } from "@lindaflor/shared/lib/roles";
import { inArray, isNull, notInArray, or } from "drizzle-orm";
import { v7 as uuidv7 } from "uuid";

import { DEV_USERS } from "@/seed/constants";
import { randomSeedDate, randomSeedDateAfter } from "@/seed/utils";

export async function seedDevUsersWithAuth(hashedPassword: string) {
  await DEV_USERS.reduce(
    (p, u) =>
      p.then(async () => {
        const createdAt = randomSeedDate();
        const updatedAt = randomSeedDateAfter(createdAt);

        await db.insert(users).values({
          id: u.id,
          name: u.name,
          email: u.email,
          email_verified: true,
          image: u.image,
          created_at: createdAt,
          updated_at: updatedAt,
          role: u.role,
          banned: false,
          ban_reason: null,
          ban_expires: null,
          two_factor_enabled: false,
        });

        await db.insert(accounts).values({
          id: uuidv7(),
          account_id: u.id,
          provider_id: "credential",
          user_id: u.id,
          password: hashedPassword,
          created_at: createdAt,
          updated_at: updatedAt,
        });

        await db.insert(members).values({
          id: uuidv7(),
          organization_id: u.organizationId,
          user_id: u.id,
          role: u.orgRole,
          created_at: createdAt,
        });
      }),
    Promise.resolve<void>(undefined),
  );
}

export async function cleanupDisallowedUsers() {
  const allowedRoles = [...RBAC_ROLES];
  const disallowedUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(or(isNull(users.role), notInArray(users.role, allowedRoles)));

  const disallowedUserIds = disallowedUsers.map((u) => u.id);

  if (disallowedUserIds.length === 0) {
    return;
  }

  await db.delete(accounts).where(inArray(accounts.user_id, disallowedUserIds));
  await db.delete(sessions).where(inArray(sessions.user_id, disallowedUserIds));
  await db.delete(members).where(inArray(members.user_id, disallowedUserIds));
  await db
    .delete(invitations)
    .where(inArray(invitations.inviter_id, disallowedUserIds));
  await db.delete(users).where(inArray(users.id, disallowedUserIds));
}
