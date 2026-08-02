import { beforeEach, describe, expect, it, mock } from "bun:test";

import {
  createAuthRequestHandler,
  extractSessionToken,
} from "@/lib/auth-session-cache";

const readMock = mock((_token: string) =>
  Promise.resolve<{ status: number; body: string } | null>(null),
);
const writeMock = mock(
  (_token: string, _res: { status: number; body: string }, _ttl?: number) =>
    Promise.resolve(),
);
const invalidateMock = mock((_token: string) => Promise.resolve());

const makeHandler = (
  authHandler: (request: Request) => Promise<Response>,
  getToken: (request: Request) => string | null = () => "tok",
) =>
  createAuthRequestHandler({
    authHandler,
    cache: { read: readMock, write: writeMock, invalidate: invalidateMock },
    getToken,
    ttlSeconds: 60,
  });

const getSession = (search = "") =>
  new Request(`https://x.test/api/auth/get-session${search}`, {
    method: "GET",
  });

describe("createAuthRequestHandler", () => {
  beforeEach(() => {
    readMock.mockClear();
    writeMock.mockClear();
    invalidateMock.mockClear();
  });

  it("serves a cache hit without calling the auth handler", async () => {
    readMock.mockResolvedValueOnce({ status: 200, body: '{"user":1}' });
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("SHOULD NOT RUN", { status: 200 })),
    );
    const response = await makeHandler(authHandler)(getSession());
    expect(await response.text()).toBe('{"user":1}');
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(authHandler).not.toHaveBeenCalled();
  });

  it("calls the auth handler on a miss and caches a 200 non-null body", async () => {
    readMock.mockResolvedValueOnce(null);
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response('{"user":1}', { status: 200 })),
    );
    const response = await makeHandler(authHandler)(getSession());
    expect(authHandler).toHaveBeenCalledTimes(1);
    expect(await response.text()).toBe('{"user":1}');
    expect(writeMock).toHaveBeenCalledWith(
      "tok",
      { status: 200, body: '{"user":1}' },
      60,
    );
  });

  it("does not cache a null (unauthenticated) body", async () => {
    readMock.mockResolvedValueOnce(null);
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("null", { status: 200 })),
    );
    await makeHandler(authHandler)(getSession());
    expect(writeMock).not.toHaveBeenCalled();
  });

  it("does not cache a non-200 response", async () => {
    readMock.mockResolvedValueOnce(null);
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("rate limited", { status: 429 })),
    );
    await makeHandler(authHandler)(getSession());
    expect(writeMock).not.toHaveBeenCalled();
  });

  it("bypasses the cache when get-session has query params", async () => {
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response('{"user":1}', { status: 200 })),
    );
    await makeHandler(authHandler)(getSession("?disableCookieCache=true"));
    expect(readMock).not.toHaveBeenCalled();
    expect(authHandler).toHaveBeenCalledTimes(1);
    expect(writeMock).not.toHaveBeenCalled();
  });

  it("does not read the cache when there is no token", async () => {
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("null", { status: 200 })),
    );
    await makeHandler(authHandler, () => null)(getSession());
    expect(readMock).not.toHaveBeenCalled();
    expect(writeMock).not.toHaveBeenCalled();
    expect(authHandler).toHaveBeenCalledTimes(1);
  });

  it("invalidates the token cache after a non-GET auth request", async () => {
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("ok", { status: 200 })),
    );
    const request = new Request("https://x.test/api/auth/sign-out", {
      method: "POST",
    });
    await makeHandler(authHandler)(request);
    expect(invalidateMock).toHaveBeenCalledWith("tok");
  });

  it("does not invalidate a non-GET request without a token", async () => {
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("ok", { status: 200 })),
    );
    const request = new Request("https://x.test/api/auth/sign-out", {
      method: "POST",
    });
    await makeHandler(authHandler, () => null)(request);
    expect(invalidateMock).not.toHaveBeenCalled();
  });

  it("fails open to the auth handler when the cache read throws", async () => {
    readMock.mockRejectedValueOnce(new Error("valkey down"));
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response('{"user":1}', { status: 200 })),
    );
    const response = await makeHandler(authHandler)(getSession());
    expect(authHandler).toHaveBeenCalledTimes(1);
    expect(await response.text()).toBe('{"user":1}');
  });

  it("fails open when the cache write throws on a miss", async () => {
    readMock.mockResolvedValueOnce(null);
    writeMock.mockRejectedValueOnce(new Error("valkey down"));
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response('{"user":1}', { status: 200 })),
    );
    const response = await makeHandler(authHandler)(getSession());
    expect(authHandler).toHaveBeenCalledTimes(1);
    expect(await response.text()).toBe('{"user":1}');
  });

  it("fails open when the cache invalidate throws on a non-GET request", async () => {
    invalidateMock.mockRejectedValueOnce(new Error("valkey down"));
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("ok", { status: 200 })),
    );
    const request = new Request("https://x.test/api/auth/sign-out", {
      method: "POST",
    });
    const response = await makeHandler(authHandler)(request);
    expect(invalidateMock).toHaveBeenCalledWith("tok");
    expect(await response.text()).toBe("ok");
  });

  it("returns 405 for unsupported methods", async () => {
    const authHandler = mock((_r: Request) =>
      Promise.resolve(new Response("ok", { status: 200 })),
    );
    const request = new Request("https://x.test/api/auth/get-session", {
      method: "DELETE",
    });
    const response = await makeHandler(authHandler)(request);
    expect(response.status).toBe(405);
    expect(response.headers.get("Allow")).toBe("GET, POST");
    expect(authHandler).not.toHaveBeenCalled();
  });
});

describe("extractSessionToken", () => {
  it("reads a bearer token from the Authorization header", () => {
    const request = new Request("https://x.test/api/auth/get-session", {
      headers: { authorization: "Bearer abc.def" },
    });
    expect(extractSessionToken(request)).toBe("abc.def");
  });

  it("returns null when no cookie or bearer token is present", () => {
    const request = new Request("https://x.test/api/auth/get-session");
    expect(extractSessionToken(request)).toBeNull();
  });

  it("reads the session token from the better-auth session cookie", () => {
    const request = new Request("https://x.test/api/auth/get-session", {
      headers: { cookie: "better-auth.session_token=tok-value-123" },
    });
    expect(extractSessionToken(request)).toBe("tok-value-123");
  });
});
