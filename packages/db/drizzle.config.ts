import { config } from "dotenv";
import { defineConfig } from "drizzle-kit";

config({ path: "../../.env", quiet: true });

const migrationUrl =
  process.env.DATABASE_URL_DIRECT ?? process.env.DATABASE_URL;

if (!migrationUrl) {
  throw new Error(
    "DATABASE_URL or DATABASE_URL_DIRECT must be set for drizzle-kit",
  );
}

export default defineConfig({
  schema: ["./src/schema/auth.ts", "./src/schema/commerce.ts"],
  out: "./src/migrations",
  dialect: "postgresql",
  dbCredentials: {
    url: migrationUrl,
  },
});
