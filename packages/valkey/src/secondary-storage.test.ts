import { beforeEach, describe, expect, it, mock } from "bun:test";

import { createValkeySecondaryStorage } from "@lindaflor/valkey/secondary-storage";

const getMock = mock((_key: string) => Promise.resolve<string | null>(null));
const setMock = mock((_key: string, _value: string, ..._args: unknown[]) =>
  Promise.resolve<"OK">("OK"),
);
const delMock = mock((_key: string) => Promise.resolve<number>(1));

const storage = createValkeySecondaryStorage({
  get: getMock,
  set: setMock,
  del: delMock,
});

describe("createValkeySecondaryStorage", () => {
  beforeEach(() => {
    getMock.mockClear();
    setMock.mockClear();
    delMock.mockClear();
  });

  it("returns the value from the client on get", async () => {
    getMock.mockResolvedValueOnce("hello");
    const result = await storage.get("foo");
    expect(result).toBe("hello");
    expect(getMock).toHaveBeenCalledWith("foo");
  });

  it("returns null when the key is missing", async () => {
    getMock.mockResolvedValueOnce(null);
    const result = await storage.get("missing");
    expect(result).toBeNull();
  });

  it("writes a value without ttl by passing undefined to the client", async () => {
    await storage.set("foo", "bar");
    expect(setMock).toHaveBeenCalledWith("foo", "bar", undefined);
  });

  it("writes a value with ttl by passing the seconds through to the client", async () => {
    await storage.set("foo", "bar", 60);
    expect(setMock).toHaveBeenCalledWith("foo", "bar", 60);
  });

  it("deletes a key", async () => {
    await storage.delete("foo");
    expect(delMock).toHaveBeenCalledWith("foo");
  });
});
