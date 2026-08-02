import { describe, expect, it } from "bun:test";

import { getClientIp } from "@lindaflor/api/lib/client-ip";

const headers = (entries: Record<string, string>) => new Headers(entries);

describe("getClientIp", () => {
  it("uses the entry appended by the trusted proxy (rightmost by default)", () => {
    const ip = getClientIp(
      headers({ "x-forwarded-for": "203.0.113.7, 10.0.0.1, 198.51.100.9" }),
    );
    expect(ip).toBe("198.51.100.9");
  });

  it("ignores client-spoofed leftmost entries", () => {
    const ip = getClientIp(
      headers({ "x-forwarded-for": "1.2.3.4, 203.0.113.7" }),
    );
    expect(ip).toBe("203.0.113.7");
  });

  it("honors trustedProxyCount when several proxies are present", () => {
    const ip = getClientIp(
      headers({ "x-forwarded-for": "203.0.113.7, 10.0.0.1, 10.0.0.2" }),
      2,
    );
    expect(ip).toBe("10.0.0.1");
  });

  it("trims whitespace around the trusted entry", () => {
    const ip = getClientIp(headers({ "x-forwarded-for": "  203.0.113.7  " }));
    expect(ip).toBe("203.0.113.7");
  });

  it("falls back to x-real-ip when x-forwarded-for is absent", () => {
    const ip = getClientIp(headers({ "x-real-ip": "198.51.100.42" }));
    expect(ip).toBe("198.51.100.42");
  });

  it("prefers x-forwarded-for over x-real-ip", () => {
    const ip = getClientIp(
      headers({
        "x-forwarded-for": "203.0.113.7",
        "x-real-ip": "198.51.100.42",
      }),
    );
    expect(ip).toBe("203.0.113.7");
  });

  it('returns "unknown" when neither header is set', () => {
    expect(getClientIp(headers({}))).toBe("unknown");
  });

  it('returns "unknown" when x-forwarded-for is an empty string', () => {
    expect(getClientIp(headers({ "x-forwarded-for": "" }))).toBe("unknown");
  });

  it('returns "unknown" when x-forwarded-for is only whitespace and commas', () => {
    expect(getClientIp(headers({ "x-forwarded-for": " , , " }))).toBe(
      "unknown",
    );
  });

  it("accepts an IPv6 address", () => {
    expect(getClientIp(headers({ "x-forwarded-for": "2001:db8::1" }))).toBe(
      "2001:db8::1",
    );
  });

  it("accepts the IPv6 loopback in x-real-ip", () => {
    expect(getClientIp(headers({ "x-real-ip": "::1" }))).toBe("::1");
  });

  it('returns "unknown" when the trusted x-forwarded-for entry is not a valid IP', () => {
    expect(getClientIp(headers({ "x-forwarded-for": "not.an.ip" }))).toBe(
      "unknown",
    );
  });

  it('returns "unknown" when x-real-ip is not a valid IP', () => {
    expect(getClientIp(headers({ "x-real-ip": "999.999.999.999" }))).toBe(
      "unknown",
    );
  });

  it("falls through to x-real-ip when the trusted x-forwarded-for entry is invalid", () => {
    const ip = getClientIp(
      headers({
        "x-forwarded-for": "not.an.ip",
        "x-real-ip": "198.51.100.42",
      }),
    );
    expect(ip).toBe("198.51.100.42");
  });

  it("does not trust client-supplied entries when fewer hops than trustedProxyCount are present", () => {
    // With 2 trusted proxies, each appends its own entry, so a single value
    // can only have come from the (untrusted) client. The index counted from
    // the right is negative here, so there is no trusted entry to use — the
    // code must not clamp the index to 0 and read the spoofed leftmost value.
    const ip = getClientIp(headers({ "x-forwarded-for": "1.2.3.4" }), 2);
    expect(ip).toBe("unknown");
  });

  it("falls back to x-real-ip when x-forwarded-for carries fewer hops than trustedProxyCount", () => {
    // The spoofed '1.2.3.4' sits left of any trusted entry and must be ignored;
    // the real client IP comes from x-real-ip instead.
    const ip = getClientIp(
      headers({ "x-forwarded-for": "1.2.3.4", "x-real-ip": "198.51.100.42" }),
      2,
    );
    expect(ip).toBe("198.51.100.42");
  });
});
