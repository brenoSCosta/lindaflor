import { describe, expect, it } from "bun:test";

import { os } from "@orpc/server";
import { RPCHandler } from "@orpc/server/fetch";

import { BodyLimitPlugin } from "@/lib/body-limit";

const noopProcedure = os.handler(async () => ({ ok: true }));
const router = {
  noop: noopProcedure,
  large: noopProcedure,
  other: noopProcedure,
  a: { submit: noopProcedure },
  b: { submit: noopProcedure },
};

function createRpcRequest(path: string, body: string): Request {
  return new Request(`https://test${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body,
  });
}

function rpcBody(dataSize: number): string {
  return JSON.stringify({
    json: { data: "x".repeat(dataSize) },
    meta: [],
  });
}

async function handle(
  path: string,
  bodySize: number,
  defaultMaxBodySize: number,
  overrides: Record<string, number> = {},
): Promise<Response> {
  const body = rpcBody(bodySize);
  const handler = new RPCHandler(router, {
    plugins: [
      new BodyLimitPlugin({
        defaultMaxBodySize,
        overrides,
      }),
    ],
    strictGetMethodPluginEnabled: false,
  });
  const { response } = await handler.handle(createRpcRequest(path, body), {
    prefix: "/",
    context: {},
  });
  return response ?? new Response("Not Found", { status: 404 });
}

describe("BodyLimitPlugin", () => {
  const DEFAULT = 150;
  const OVERRIDE = 500;

  it("blocks a body that exceeds the default limit", async () => {
    const response = await handle("/noop", 200, DEFAULT);
    expect(response.status).toBe(413);
  });

  it("allows a body under the default limit", async () => {
    const response = await handle("/noop", 50, DEFAULT);
    expect(response.status).toBe(200);
  });

  it("allows a body that exceeds the default but is under the override", async () => {
    const response = await handle("/large", 300, DEFAULT, {
      large: OVERRIDE,
    });
    expect(response.status).toBe(200);
  });

  it("blocks a body that exceeds the override limit", async () => {
    const response = await handle("/large", 600, DEFAULT, {
      large: OVERRIDE,
    });
    expect(response.status).toBe(413);
  });

  it("uses the default for a path not in overrides", async () => {
    const response = await handle("/other", 300, DEFAULT, {
      large: OVERRIDE,
    });
    expect(response.status).toBe(413);
  });

  it("applies multiple path overrides independently", async () => {
    // Both under their respective overrides
    const resA = await handle("/a/submit", 400, DEFAULT, {
      "a.submit": 500,
      "b.submit": 800,
    });
    expect(resA.status).toBe(200);

    // /a exceeds its override, /b doesn't
    const resA2 = await handle("/a/submit", 600, DEFAULT, {
      "a.submit": 500,
      "b.submit": 800,
    });
    expect(resA2.status).toBe(413);

    // /b still under its higher override
    const resB = await handle("/b/submit", 700, DEFAULT, {
      "a.submit": 500,
      "b.submit": 800,
    });
    expect(resB.status).toBe(200);
  });
});
