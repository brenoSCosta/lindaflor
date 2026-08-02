import { beforeEach, describe, expect, it, mock } from "bun:test";

import {
  createSessionCache,
  SESSION_CACHE_PREFIX,
  SESSION_CACHE_TTL_SECONDS,
} from "@lindaflor/valkey/session-cache";

const getMock = mock((_key: string) => Promise.resolve<string | null>(null));
const setMock = mock((_key: string, _value: string, _ttl: number) =>
  Promise.resolve<"OK">("OK"),
);
const delMock = mock((_key: string) => Promise.resolve<number>(1));

const cache = createSessionCache({ get: getMock, set: setMock, del: delMock });

describe("createSessionCache", () => {
  beforeEach(() => {
    getMock.mockClear();
    setMock.mockClear();
    delMock.mockClear();
  });

  it("derives a prefixed, hashed key that never contains the raw token", () => {
    const key = cache.cacheKey("super-secret-token");
    expect(key.startsWith(SESSION_CACHE_PREFIX)).toBe(true);
    expect(key).not.toContain("super-secret-token");
    expect(key.slice(SESSION_CACHE_PREFIX.length)).toHaveLength(64);
  });

  it("returns the parsed cached response on a hit", async () => {
    getMock.mockResolvedValueOnce(
      JSON.stringify({ status: 200, body: '{"user":1}' }),
    );
    const result = await cache.read("tok");
    expect(result).toEqual({ status: 200, body: '{"user":1}' });
    expect(getMock).toHaveBeenCalledWith(cache.cacheKey("tok"));
  });

  it("returns null on a miss", async () => {
    getMock.mockResolvedValueOnce(null);
    expect(await cache.read("tok")).toBeNull();
  });

  it("returns null when the cached value is malformed json", async () => {
    getMock.mockResolvedValueOnce("not json");
    expect(await cache.read("tok")).toBeNull();
  });

  it("returns null when the cached value has the wrong shape", async () => {
    getMock.mockResolvedValueOnce(JSON.stringify({ status: "200" }));
    expect(await cache.read("tok")).toBeNull();
  });

  it("writes the serialized response with the default ttl", async () => {
    await cache.write("tok", { status: 200, body: '{"user":1}' });
    expect(setMock).toHaveBeenCalledWith(
      cache.cacheKey("tok"),
      JSON.stringify({ status: 200, body: '{"user":1}' }),
      SESSION_CACHE_TTL_SECONDS,
    );
  });

  it("writes the serialized response with a custom ttl override", async () => {
    await cache.write("tok", { status: 200, body: '{"user":1}' }, 30);
    expect(setMock).toHaveBeenCalledWith(
      cache.cacheKey("tok"),
      JSON.stringify({ status: 200, body: '{"user":1}' }),
      30,
    );
  });

  it("deletes the key on invalidate", async () => {
    await cache.invalidate("tok");
    expect(delMock).toHaveBeenCalledWith(cache.cacheKey("tok"));
  });
});
