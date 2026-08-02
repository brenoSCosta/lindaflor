import { describe, expect, it } from "bun:test";

import { resolveVerificationCallbackPath } from "@lindaflor/auth/lib/safe-callback";

const ORIGIN = "https://app.example.com";

const verificationUrl = (callbackURL: string | null): string => {
  const url = new URL("https://api.example.com/api/auth/verify-email");
  url.searchParams.set("token", "tok");
  if (callbackURL !== null) {
    url.searchParams.set("callbackURL", callbackURL);
  }
  return url.toString();
};

describe("resolveVerificationCallbackPath", () => {
  it("honors a same-origin /accept-invitation callback with its query", () => {
    const callback = `${ORIGIN}/accept-invitation?id=inv-1&email=b%40x.com`;
    expect(
      resolveVerificationCallbackPath(verificationUrl(callback), ORIGIN),
    ).toBe("/accept-invitation?id=inv-1&email=b%40x.com");
  });

  it("honors a relative /accept-invitation callback", () => {
    const callback = "/accept-invitation?id=inv-2";
    expect(
      resolveVerificationCallbackPath(verificationUrl(callback), ORIGIN),
    ).toBe("/accept-invitation?id=inv-2");
  });

  it("falls back when no callbackURL is present", () => {
    expect(resolveVerificationCallbackPath(verificationUrl(null), ORIGIN)).toBe(
      "/verify-email",
    );
  });

  it("rejects a cross-origin callback (open redirect)", () => {
    const callback = "https://evil.com/accept-invitation?id=x";
    expect(
      resolveVerificationCallbackPath(verificationUrl(callback), ORIGIN),
    ).toBe("/verify-email");
  });

  it("rejects a protocol-relative callback", () => {
    expect(
      resolveVerificationCallbackPath(
        verificationUrl("//evil.com/accept-invitation"),
        ORIGIN,
      ),
    ).toBe("/verify-email");
  });

  it("rejects a non-allowlisted same-origin path", () => {
    const callback = `${ORIGIN}/admin`;
    expect(
      resolveVerificationCallbackPath(verificationUrl(callback), ORIGIN),
    ).toBe("/verify-email");
  });

  it("falls back when the verification url is malformed", () => {
    expect(resolveVerificationCallbackPath("not a url", ORIGIN)).toBe(
      "/verify-email",
    );
  });
});
