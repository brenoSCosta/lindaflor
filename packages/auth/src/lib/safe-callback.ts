import { Effect } from "effect";

/**
 * After verifying their email, Better Auth redirects the browser to the
 * `callbackURL` embedded in the verification link. We pin that destination to a
 * small allowlist so a forged sign-up `callbackURL` can't turn the verification
 * link into an open redirect. Anything not explicitly allowed falls back to the
 * standard post-verification page.
 */
const DEFAULT_VERIFICATION_PATH = "/verify-email";
const ALLOWED_CALLBACK_PATHS = new Set<string>(["/accept-invitation"]);

/**
 * Resolve the safe frontend path to send a freshly verified user to.
 *
 * @param verificationUrl - Better Auth's verification URL (contains the
 *   client-supplied `callbackURL` query param).
 * @param allowedOrigin - the trusted frontend origin (`CORS_ORIGIN`).
 * @returns a same-origin, allowlisted `pathname + search`, or
 *   `/verify-email` when the callback is missing, cross-origin, or not allowed.
 */
export function resolveVerificationCallbackPath(
  verificationUrl: string,
  allowedOrigin: string,
): string {
  const program = Effect.gen(function* () {
    const rawCallback = yield* Effect.try({
      try: () => new URL(verificationUrl).searchParams.get("callbackURL"),
      catch: () => new Error("invalid verification URL"),
    });
    if (!rawCallback) {
      return DEFAULT_VERIFICATION_PATH;
    }

    const expectedOrigin = yield* Effect.try({
      try: () => new URL(allowedOrigin).origin,
      catch: () => new Error("invalid allowed origin"),
    });

    // Resolve relative callbacks against the trusted origin; absolute callbacks
    // keep their own origin so cross-origin values are rejected below.
    const parsed = yield* Effect.try({
      try: () => new URL(rawCallback, expectedOrigin),
      catch: () => new Error("invalid callback URL"),
    });

    if (parsed.origin !== expectedOrigin) {
      return DEFAULT_VERIFICATION_PATH;
    }

    if (!ALLOWED_CALLBACK_PATHS.has(parsed.pathname)) {
      return DEFAULT_VERIFICATION_PATH;
    }

    return parsed.pathname + parsed.search;
  });

  return Effect.runSync(
    program.pipe(
      Effect.catchAll(() => Effect.succeed(DEFAULT_VERIFICATION_PATH)),
    ),
  );
}
