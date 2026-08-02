import { describe, expect, it, mock } from "bun:test";

import { createRatelimiter } from "@lindaflor/valkey/rate-limiter";

const evalMock = mock((_script: string, _numKeys: number, ..._rest: string[]) =>
  Promise.resolve<unknown>([1, 100, 99, Date.now() + 60_000]),
);

const ratelimiter = createRatelimiter({ eval: evalMock });

describe("createRatelimiter (RedisRatelimiter wired to a client.eval)", () => {
  it("forwards eval calls to the underlying client with the prefixed key", async () => {
    evalMock.mockClear();
    await ratelimiter.limit("user:abc");
    expect(evalMock).toHaveBeenCalled();
    const [script, numKeys, ...rest] = evalMock.mock.calls[0] ?? [];
    expect(typeof script).toBe("string");
    expect(numKeys).toBe(1);
    expect(rest[0]).toBe("ratelimit:user:abc");
  });

  it("returns success=true and the remaining count when the script reports allowed", async () => {
    evalMock.mockClear();
    const resetAt = Date.now() + 60_000;
    evalMock.mockResolvedValueOnce([1, 100, 42, resetAt]);
    const result = await ratelimiter.limit("ip:127.0.0.1");
    expect(result.success).toBe(true);
    expect(result.limit).toBe(100);
    expect(result.remaining).toBe(42);
    expect(result.reset).toBe(resetAt);
  });

  it("returns success=false when the script reports the limit is exhausted", async () => {
    evalMock.mockClear();
    evalMock.mockResolvedValueOnce([0, 100, 0, Date.now() + 60_000]);
    const result = await ratelimiter.limit("ip:127.0.0.1");
    expect(result.success).toBe(false);
    expect(result.remaining).toBe(0);
  });
});
