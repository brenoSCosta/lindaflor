import { valkey } from "@lindaflor/valkey";
import type { SecondaryStorage } from "better-auth";

// Minimal structural deps the factory needs. iovalkey's Redis class has
// heavily overloaded signatures that don't structurally narrow to a single
// callable type, so we keep the deps shape simple and wrap the real client
// at the binding site below.
type SecondaryStorageDeps = {
  get(key: string): Promise<string | null | undefined>;
  set(key: string, value: string, ttlSeconds?: number): Promise<unknown>;
  del(key: string): Promise<unknown>;
};

// Factory takes a deps object so unit tests can pass a stub directly without
// resorting to `mock.module('@lindaflor/valkey', ...)`. Module mocks leak across
// files in the same Bun worker, which previously caused tests to receive a
// cached stub instead of the real implementation.
export const createValkeySecondaryStorage = (
  deps: SecondaryStorageDeps,
): SecondaryStorage => ({
  get: async (key) => {
    const value = await deps.get(key);
    return value;
  },
  set: async (key, value, ttl) => {
    await deps.set(key, value, ttl);
  },
  delete: async (key) => {
    await deps.del(key);
  },
});

export const valkeySecondaryStorage: SecondaryStorage =
  createValkeySecondaryStorage({
    get: (key) => valkey.get(key),
    set: (key, value, ttlSeconds) =>
      ttlSeconds === undefined
        ? valkey.set(key, value)
        : valkey.set(key, value, "EX", ttlSeconds),
    del: (key) => valkey.del(key),
  });
