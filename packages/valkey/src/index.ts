import { env } from "@lindaflor/env/server";
import { Effect } from "effect";
import { Redis as Valkey } from "iovalkey";

const RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_ATTEMPTS = 10;

export const valkey: Valkey = new Valkey(env.VALKEY_URL, {
  lazyConnect: true,
  // Railway private DNS (valkey.railway.internal) resolves to IPv6 only in
  // legacy environments. iovalkey defaults to IPv4 (A records), so lookups fail
  // unless dual-stack is enabled. Harmless elsewhere.
  // https://docs.railway.com/databases/troubleshooting/enotfound-redis-railway-internal
  family: 0,
  maxRetriesPerRequest: 3,
  retryStrategy: (times) =>
    times <= MAX_RECONNECT_ATTEMPTS ? times * RECONNECT_DELAY_MS : null,
  reconnectOnError: (error) => {
    const message = error.message ?? String(error);
    return message.includes("READONLY") || message.includes("ETIMEDOUT");
  },
});

valkey.on("error", (error) => {
  const detail =
    error instanceof Error
      ? error.message
      : error === undefined
        ? "connection failed (no error details)"
        : String(error);

  Effect.runSync(Effect.logError("[valkey] client error:", detail));
});

export function connectValkey(): Effect.Effect<void, Error> {
  if (valkey.status === "ready" || valkey.status === "connect") {
    return Effect.void;
  }

  return Effect.tryPromise({
    try: () => valkey.connect(),
    catch: (e): Error =>
      e instanceof Error ? e : new Error("Valkey connection failed"),
  }).pipe(
    Effect.tap(() => Effect.log("[valkey] connected")),
    Effect.tapError((error) =>
      Effect.logError("[valkey] failed to connect:", error),
    ),
  );
}
