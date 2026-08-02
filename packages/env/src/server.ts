import path from "node:path";
import { fileURLToPath } from "node:url";

import { createEnv } from "@t3-oss/env-core";
import dotenv from "dotenv";
import { z } from "zod";

const currentDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(currentDir, "../../..");
dotenv.config({ path: path.resolve(projectRoot, ".env"), quiet: true });

export const env = createEnv({
  server: {
    DATABASE_URL: z.string().min(1),
    DATABASE_URL_DIRECT: z.string().min(1).optional(),
    VALKEY_URL: z.url(),
    BETTER_AUTH_SECRET: z.string().min(32),
    BETTER_AUTH_URL: z.url(),
    WEB_ORIGIN: z.url(),
    CORS_ORIGINS: z
      .string()
      .transform((val) => val.split(",").map((o) => o.trim()))
      .pipe(z.url().array()),
    NODE_ENV: z
      .enum(["development", "production", "test"])
      .default("development"),
    SERVER_PORT: z.coerce.number().int().positive(),
    SERVER_HOST: z.string().default("0.0.0.0"),
    TRUSTED_PROXY_COUNT: z.coerce.number().int().positive().default(1),
    RESEND_API_KEY: z
      .string()
      .startsWith("re_", { message: 'RESEND_API_KEY must start with "re_"' })
      .regex(/^[a-zA-Z0-9_]+$/, {
        message: "RESEND_API_KEY must contain only alphanumeric characters",
      })
      .optional(),
    MAIL_ENABLED: z.coerce.boolean().default(false),
    MAIL_FROM: z.string().min(1).default("Linda Flor <noreply@lindaflor.local>"),
    GOOGLE_CLIENT_ID: z
      .string()
      .endsWith(".apps.googleusercontent.com")
      .optional(),
    GOOGLE_CLIENT_SECRET: z.string().startsWith("GOCSPX-").optional(),
    SEED_DEV_PASSWORD: z.string().default("password").optional(),
    S3_ENDPOINT: z.url().optional(),
    S3_REGION: z.string().min(1).default("us-east-1"),
    S3_ACCESS_KEY_ID: z.string().min(1).optional(),
    S3_SECRET_ACCESS_KEY: z.string().min(1).optional(),
    S3_BUCKET: z.string().min(1).default("lindaflor"),
    S3_PUBLIC_URL: z.url().optional(),
    MERCADO_PAGO_ACCESS_TOKEN: z.string().min(1).optional(),
    MERCADO_PAGO_WEBHOOK_SECRET: z.string().min(1).optional(),
    STORE_COUPON_CODE: z.string().min(1).optional(),
    STORE_COUPON_DISCOUNT_PERCENT: z.coerce.number().int().min(1).max(100).default(10),
    ORDER_RESERVATION_HOURS: z.coerce.number().int().positive().default(24),
    INVENTORY_LOW_STOCK_THRESHOLD: z.coerce.number().int().nonnegative().default(5),
    YOUTUBE_API_KEY: z.string().min(1).optional(),
    PS_URL: z.url().default("http://localhost:8080"),
  },
  runtimeEnv: process.env,
  emptyStringAsUndefined: true,
});
