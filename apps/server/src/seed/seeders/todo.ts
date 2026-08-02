import { db } from "@lindaflor/db";
import { todo } from "@lindaflor/db/schema/todo";
import { labels, priorities, statuses } from "@lindaflor/shared/enums/todo";
import { Effect } from "effect";

import { BATCH_SIZE, SEED_COUNT, SEED_ORG_IDS } from "@/seed/constants";
import {
  pickRandom,
  randomDate,
  randomLorem,
  randomSeedDate,
  randomSeedDateAfter,
  seedIdFor,
} from "@/seed/utils";

export async function seedTodos() {
  let batch: (typeof todo.$inferInsert)[] = [];

  for (let i = 0; i < SEED_COUNT; i++) {
    const createdAt = randomSeedDate();
    batch.push({
      id: seedIdFor(createdAt),
      text: randomLorem(3),
      status: pickRandom(statuses),
      label: pickRandom(labels),
      priority: pickRandom(priorities),
      estimated_hours: Math.floor(Math.random() * 101),
      actual_hours: Math.floor(Math.random() * 101),
      progress: Math.floor(Math.random() * 101),
      cost: Math.floor(Math.random() * 10001),
      due_date: randomDate(new Date(), new Date("2026-12-31")),
      completed_at: randomSeedDateAfter(createdAt),
      created_at: createdAt,
      updated_at: randomSeedDateAfter(createdAt),
      organization_id: pickRandom(SEED_ORG_IDS),
    });

    if (batch.length >= BATCH_SIZE) {
      await db.insert(todo).values(batch);
      const inserted = Math.min(i + 1, SEED_COUNT);
      batch = [];
      if (inserted % 50_000 === 0) {
        Effect.runSync(Effect.log(`  todos: ${inserted}/${SEED_COUNT}`));
      }
    }
  }

  if (batch.length > 0) {
    await db.insert(todo).values(batch);
  }

  Effect.runSync(Effect.log(`  todos: done (${SEED_COUNT} rows)`));
}
