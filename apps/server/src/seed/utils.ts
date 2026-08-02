import { v7 as uuidv7 } from "uuid";

import {
  DEV_USERS,
  LOREM_WORDS,
  SEED_NOW,
  SEED_WINDOW_START,
} from "@/seed/constants";

export function randomLorem(sentencesCount: number): string {
  const sentences: string[] = [];
  for (let s = 0; s < sentencesCount; s++) {
    const wordCount = 5 + Math.floor(Math.random() * 10);
    const words: string[] = [];
    for (let w = 0; w < wordCount; w++) {
      const word = pickRandom(LOREM_WORDS);
      words.push(word);
    }
    const sentence = words.join(" ") + ".";
    sentences.push(sentence.charAt(0).toUpperCase() + sentence.slice(1));
  }
  return sentences.join(" ");
}

export function pickRandom<T>(arr: readonly T[]): T {
  const item = arr[Math.floor(Math.random() * arr.length)];
  if (item === undefined) throw new Error("pickRandom: empty array");
  return item;
}

export function randomDate(min: Date, max: Date): Date {
  const minMs = min.getTime();
  const maxMs = max.getTime();
  return new Date(minMs + Math.random() * (maxMs - minMs));
}

/**
 * Random date within the shared seed window [SEED_WINDOW_START, SEED_NOW].
 * Use for created_at-style timestamps so seeded rows span >= 6 months of
 * history instead of all sharing a single "now".
 */
export function randomSeedDate(): Date {
  return randomDate(SEED_WINDOW_START, SEED_NOW);
}

/**
 * Random date in (after, SEED_NOW]. Use for updated_at and other timestamps
 * that must postdate a row's created_at while still falling inside the window.
 */
export function randomSeedDateAfter(after: Date): Date {
  return randomDate(after, SEED_NOW);
}

export function getOrgUsers(organizationId: string) {
  return DEV_USERS.filter((u) => u.organizationId === organizationId).map(
    (u) => u.id,
  );
}

export function getOrgOwner(organizationId: string) {
  const owner = DEV_USERS.find(
    (u) => u.organizationId === organizationId && u.orgRole === "owner",
  );
  const fallback = DEV_USERS[0];
  if (!fallback) throw new Error("No dev users configured for seed");
  return owner?.id ?? fallback.id;
}

export function getOrgUserByRole(
  organizationId: string,
  orgRole: (typeof DEV_USERS)[number]["orgRole"],
) {
  const user = DEV_USERS.find(
    (u) => u.organizationId === organizationId && u.orgRole === orgRole,
  );
  if (!user) {
    throw new Error(
      `No dev user with role ${orgRole} for org ${organizationId}`,
    );
  }
  return user.id;
}

export function getOrgOperatorId(organizationId: string) {
  const operator = DEV_USERS.find(
    (u) => u.organizationId === organizationId && u.orgRole === "operator",
  );
  return operator?.id ?? getOrgOwner(organizationId);
}

export function getOrgAdminId(organizationId: string) {
  const admin = DEV_USERS.find(
    (u) => u.organizationId === organizationId && u.orgRole === "admin",
  );
  return admin?.id ?? getOrgOwner(organizationId);
}

/**
 * UUID v7 whose leading 48-bit timestamp matches createdAt, so ORDER BY id
 * reproduces the same chronological order as ORDER BY created_at. Leverages
 * the PK index and keeps id sort consistent with the created_at column.
 */
export function seedIdFor(createdAt: Date): string {
  return uuidv7({ msecs: createdAt.getTime() });
}
