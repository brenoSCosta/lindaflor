import { z } from "zod";

/**
 * UTC instant from JSON / PowerSync SQLite TEXT (ISO-8601) or an existing Date.
 */
export function jsonInstantSchema() {
  return z
    .union([z.date(), z.string()])
    .refine(
      (value) => {
        if (value instanceof Date) {
          return !Number.isNaN(value.getTime());
        }
        return !Number.isNaN(Date.parse(value));
      },
      { message: "Invalid date" },
    )
    .transform((value) => (value instanceof Date ? value : new Date(value)));
}
