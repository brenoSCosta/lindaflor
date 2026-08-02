import { resolveVerificationCallbackPath } from "@lindaflor/auth/lib/safe-callback";
import { db } from "@lindaflor/db";
import {
  accounts,
  invitations,
  jwkss,
  members,
  organizations,
  sessions,
  two_factors,
  users,
  verifications,
} from "@lindaflor/db/schema/auth";
import { env } from "@lindaflor/env/server";
import { sendOrganizationCreatedEmail } from "@lindaflor/mail/templates";
import { sendChangeEmailConfirmation } from "@lindaflor/mail/templates/change-email";
import { sendDeleteAccountEmail } from "@lindaflor/mail/templates/delete-account";
import { sendOrganizationInvitationEmail } from "@lindaflor/mail/templates/organization-invitation";
import { sendPasswordResetEmail } from "@lindaflor/mail/templates/password-reset";
import { sendVerifyEmail } from "@lindaflor/mail/templates/verify-email";
import {
  adminAC,
  adminOrganization,
  adminRBAC,
  memberOrganization,
  moderatorRBAC,
  operatorOrganization,
  organizationAC,
  ownerOrganization,
  supervisorOrganization,
  userRBAC,
} from "@lindaflor/shared/lib/permissions";
import {
  isOrgRole,
  type OrgRoles,
  type Roles,
} from "@lindaflor/shared/lib/roles";
import { valkeySecondaryStorage } from "@lindaflor/valkey/secondary-storage";
import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import {
  admin as adminPlugin,
  bearer,
  haveIBeenPwned,
  organization as organizationPlugin,
  twoFactor as twoFactorPlugin,
} from "better-auth/plugins";
import { jwt } from "better-auth/plugins/jwt";
import { eq as drizzleEq } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";

const adminPluginRoles = {
  admin: adminRBAC,
  moderator: moderatorRBAC,
  user: userRBAC,
} satisfies Record<Roles, unknown>;

const organizationPluginRoles = {
  member: memberOrganization,
  operator: operatorOrganization,
  supervisor: supervisorOrganization,
  admin: adminOrganization,
  owner: ownerOrganization,
} satisfies Record<OrgRoles, unknown>;

const userFieldMap = {
  createdAt: "created_at",
  updatedAt: "updated_at",
  emailVerified: "email_verified",
  twoFactorEnabled: "two_factor_enabled",
  banReason: "ban_reason",
  banExpires: "ban_expires",
} as const;

const sessionFieldMap = {
  expiresAt: "expires_at",
  createdAt: "created_at",
  updatedAt: "updated_at",
  ipAddress: "ip_address",
  userAgent: "user_agent",
  userId: "user_id",
  impersonatedBy: "impersonated_by",
  activeOrganizationId: "active_organization_id",
} as const;

const accountFieldMap = {
  accountId: "account_id",
  providerId: "provider_id",
  userId: "user_id",
  accessToken: "access_token",
  refreshToken: "refresh_token",
  idToken: "id_token",
  accessTokenExpiresAt: "access_token_expires_at",
  refreshTokenExpiresAt: "refresh_token_expires_at",
  createdAt: "created_at",
  updatedAt: "updated_at",
} as const;

const verificationFieldMap = {
  expiresAt: "expires_at",
  createdAt: "created_at",
  updatedAt: "updated_at",
} as const;

const organizationPluginFieldMap = {
  organization: {
    fields: {
      createdAt: "created_at",
    },
  },
  member: {
    fields: {
      organizationId: "organization_id",
      userId: "user_id",
      createdAt: "created_at",
    },
  },
  invitation: {
    fields: {
      organizationId: "organization_id",
      inviterId: "inviter_id",
      expiresAt: "expires_at",
      createdAt: "created_at",
    },
  },
  session: {
    fields: {
      activeOrganizationId: "active_organization_id",
    },
  },
} as const;

const twoFactorPluginFieldMap = {
  user: {
    fields: {
      twoFactorEnabled: "two_factor_enabled",
    },
  },
  twoFactor: {
    fields: {
      backupCodes: "backup_codes",
      userId: "user_id",
    },
  },
} as const;

const adminPluginFieldMap = {
  user: {
    fields: {
      banReason: "ban_reason",
      banExpires: "ban_expires",
    },
  },
} as const;

const customRulesRateLimit = {
  "/sign-in/email": {
    window: 10,
    max: 3,
  },
  "/sign-in/social": {
    window: 10,
    max: 5,
  },
  "/request-password-reset": {
    window: 60,
    max: 3,
  },
  "/reset-password": {
    window: 60,
    max: 5,
  },
  "/send-verification-email": {
    window: 60,
    max: 3,
  },
  "/verify-email": {
    window: 60,
    max: 10,
  },
  "/change-email": {
    window: 60,
    max: 3,
  },
  "/delete-user": {
    window: 60,
    max: 3,
  },
  "/two-factor/enable": {
    window: 60,
    max: 5,
  },
  "/two-factor/disable": {
    window: 60,
    max: 5,
  },
  "/two-factor/verify-totp": {
    window: 10,
    max: 5,
  },
  "/two-factor/verify-backup-code": {
    window: 60,
    max: 5,
  },
  "/two-factor/generate-backup-codes": {
    window: 60,
    max: 5,
  },
  "/get-session": {
    window: 60,
    max: 60,
  },
  "/organization/invite-member": {
    window: 60,
    max: 10,
  },
} as const;

const schema = {
  users,
  accounts,
  sessions,
  verifications,
  organizations,
  members,
  invitations,
  twoFactors: two_factors,
  jwkss,
};

// Rewrite it to the frontend origin.
function withFrontendCallback(url: string, path: string): string {
  const parsed = new URL(url);
  parsed.searchParams.set(
    "callbackURL",
    new URL(path, env.WEB_ORIGIN).toString(),
  );
  return parsed.toString();
}

export const auth = betterAuth({
  database: drizzleAdapter(db, {
    provider: "pg",
    schema,
    camelCase: false,
    usePlural: true,
  }),
  secondaryStorage: valkeySecondaryStorage,
  databaseHooks: {
    session: {
      create: {
        before: async (session) => {
          const memberRows = session.userId
            ? await db.query.members.findMany({
                where: (table, { eq }) => eq(table.user_id, session.userId),
              })
            : [];

          // Prefer higher-privileged org roles so the default active org is
          // useful immediately after login (e.g. seeded admin owners).
          const rolePriority = {
            owner: 0,
            admin: 1,
            supervisor: 2,
            operator: 3,
            member: 4,
          } as const satisfies Record<OrgRoles, number>;

          const sorted = memberRows.toSorted(
            (a, b) =>
              (isOrgRole(a.role)
                ? rolePriority[a.role]
                : Number.MAX_SAFE_INTEGER) -
              (isOrgRole(b.role)
                ? rolePriority[b.role]
                : Number.MAX_SAFE_INTEGER),
          );

          return {
            data: {
              activeOrganizationId: sorted[0]?.organization_id ?? null,
            },
          };
        },
      },
    },
  },
  session: {
    fields: sessionFieldMap,
    expiresIn: 60 * 60 * 24 * 30, // 30 days
    updateAge: 60 * 60 * 24, // extend session daily on activity
    cookieCache: {
      enabled: true,
      maxAge: 5 * 60,
    },
  },
  account: {
    fields: accountFieldMap,
  },
  verification: {
    fields: verificationFieldMap,
  },
  user: {
    fields: userFieldMap,
    changeEmail: {
      enabled: true,
      sendChangeEmailConfirmation: async ({ user: u, newEmail, url }) => {
        await sendChangeEmailConfirmation({
          to: u.email,
          recipientName: u.name ?? "there",
          newEmail,
          confirmUrl: withFrontendCallback(url, "/settings"),
        });
      },
    },
    deleteUser: {
      enabled: true,
      sendDeleteAccountVerification: async ({ user: u, url }) => {
        await sendDeleteAccountEmail({
          to: u.email,
          recipientName: u.name ?? "there",
          deleteUrl: withFrontendCallback(url, "/"),
        });
      },
      deleteTokenExpiresIn: 60 * 60,
    },
  },
  emailVerification: {
    sendVerificationEmail: async ({ user: u, url }) => {
      await sendVerifyEmail({
        to: u.email,
        recipientName: u.name ?? "there",
        verifyUrl: withFrontendCallback(
          url,
          resolveVerificationCallbackPath(url, env.WEB_ORIGIN),
        ),
      });
    },
    sendOnSignUp: env.NODE_ENV === "production",
    autoSignInAfterVerification: true,
    expiresIn: 60 * 60,
  },
  socialProviders:
    env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET
      ? {
          google: {
            clientId: env.GOOGLE_CLIENT_ID,
            clientSecret: env.GOOGLE_CLIENT_SECRET,
          },
        }
      : {},
  trustedOrigins: env.CORS_ORIGINS,
  emailAndPassword: {
    enabled: true,
    requireEmailVerification: env.NODE_ENV === "production",
    minPasswordLength: 8,
    revokeSessionsOnPasswordReset: true,
    sendResetPassword: async ({ user: u, url }) => {
      await sendPasswordResetEmail({
        to: u.email,
        recipientName: u.name ?? "there",
        resetUrl: url,
      });
    },
    onPasswordReset: async ({ user: u }) => {
      if (env.NODE_ENV !== "production") {
        Effect.runSync(
          Effect.log("[auth] password reset completed", { userId: u.id }),
        );
      }
    },
    resetPasswordTokenExpiresIn: 60 * 60,
  },
  advanced: {
    database: {
      generateId: () => uuidv7(),
    },
    defaultCookieAttributes: {
      sameSite: env.NODE_ENV === "production" ? "none" : "lax",
      secure: env.NODE_ENV === "production",
      httpOnly: true,
      // TODO: Reconfigure API/WEB to share the same origin
      partitioned: env.NODE_ENV === "production",
    },
    useSecureCookies: env.NODE_ENV === "production",
    disableCSRFCheck: false,
    disableOriginCheck: false,
  },
  logger: {
    level: env.NODE_ENV === "production" ? "error" : "debug",
    log(level, message, ...args) {
      Effect.runSync(Effect.log(`[${level}] ${message}`, ...args));
    },
  },
  telemetry: {
    enabled: false,
  },
  rateLimit: {
    enabled: env.NODE_ENV === "production",
    window: 60,
    max: 100,
    storage: "secondary-storage",
    customRules: customRulesRateLimit,
  },
  plugins: [
    bearer({ requireSignature: false }),
    jwt({
      jwt: {
        expirationTime: "60min",
        audience: env.PS_URL,
        definePayload: (session) => ({
          active_organization_id: session.session.activeOrganizationId ?? null,
        }),
      },
      jwks: {
        jwksPath: "/.well-known/jwks.json",
        keyPairConfig: {
          alg: "RS256",
        },
      },
    }),
    haveIBeenPwned({
      enabled: env.NODE_ENV === "production",
    }),
    twoFactorPlugin({
      issuer: "lindaflor",
      schema: twoFactorPluginFieldMap,
    }),
    adminPlugin({
      ac: adminAC,
      roles: adminPluginRoles,
      schema: adminPluginFieldMap,
    }),
    organizationPlugin({
      ac: organizationAC,
      roles: organizationPluginRoles,
      schema: organizationPluginFieldMap,
      allowUserToCreateOrganization: async (_user) => {
        // TODO: Change this latter, for now we only allow admins to create organizations
        return _user.role === "admin";
      },
      sendInvitationEmail: async (data) => {
        const acceptUrl = new URL("/accept-invitation", env.WEB_ORIGIN);
        acceptUrl.searchParams.set("id", data.id);
        acceptUrl.searchParams.set("email", data.email);
        await sendOrganizationInvitationEmail({
          to: data.email,
          inviterName: data.inviter.user.name ?? "A teammate",
          organizationName: data.organization.name,
          role: data.role,
          acceptUrl: acceptUrl.toString(),
        });
      },
      organizationHooks: {
        afterCreateOrganization: async ({
          organization: org,
          user: createdUser,
        }) => {
          void sendOrganizationCreatedEmail({
            to: createdUser.email,
            recipientName: createdUser.name ?? "there",
            organizationName: org.name,
            organizationSlug: org.slug,
          });
        },
        afterDeleteOrganization: async ({ organization }) => {
          await db
            .update(sessions)
            .set({ active_organization_id: null })
            .where(drizzleEq(sessions.active_organization_id, organization.id));
        },
      },
    }),
  ],
});
