/**
 * Supabase transaction pooler (:6543) cannot run DDL such as CREATE SCHEMA.
 * For migrations, prefer DATABASE_URL_DIRECT when set; otherwise use the
 * session pooler (:5432) on the same host.
 */
export function resolveMigrationConnectionString(
  connectionString: string,
): string {
  try {
    const url = new URL(connectionString);

    if (
      url.hostname.includes("pooler.supabase.com") &&
      (url.port === "6543" || url.port === "")
    ) {
      url.port = "5432";
      return url.toString();
    }
  } catch {
    return connectionString.replace(
      "pooler.supabase.com:6543",
      "pooler.supabase.com:5432",
    );
  }

  return connectionString;
}
