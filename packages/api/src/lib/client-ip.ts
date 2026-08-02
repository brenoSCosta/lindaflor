import { z } from "zod";

const ipSchema = z.ipv4().or(z.ipv6());
const UNKNOWN_IP = "unknown";

// Number of trusted reverse proxies in front of the app that append to
// X-Forwarded-For. The genuine client IP is the entry the closest trusted proxy
// appended — i.e. the Nth value counting from the RIGHT. Everything to the LEFT
// of it is supplied by the (untrusted) client and must never be used, otherwise
// the IP — and any rate limit keyed on it — can be spoofed by sending a forged
// X-Forwarded-For header. Default 1 matches a single managed edge (e.g. AWS App
// Runner) that appends the real client IP to whatever the client sent.
const DEFAULT_TRUSTED_PROXY_COUNT = 1;

const parseIp = (value: string | undefined | null): string | null => {
  if (!value) {
    return null;
  }
  const result = ipSchema.safeParse(value);
  return result.success ? result.data : null;
};

export function getClientIp(
  headers: Headers,
  trustedProxyCount: number = DEFAULT_TRUSTED_PROXY_COUNT,
): string {
  const forwardedFor = headers.get("x-forwarded-for");
  if (forwardedFor) {
    const parts = forwardedFor.split(",").flatMap((part) => {
      const trimmed = part.trim();
      return trimmed ? [trimmed] : [];
    });
    // Trust only the entry the closest trusted proxy appended — the Nth value
    // counting from the right. If the header carries fewer hops than there are
    // trusted proxies, the index is negative: there is no trusted entry, so we
    // must fall through rather than clamp to 0 and read a client-supplied value.
    const trustedIndex = parts.length - trustedProxyCount;
    if (trustedIndex >= 0) {
      const parsed = parseIp(parts[trustedIndex]);
      if (parsed) {
        return parsed;
      }
    }
  }

  const realIp = parseIp(headers.get("x-real-ip")?.trim());
  if (realIp) {
    return realIp;
  }

  return UNKNOWN_IP;
}
