import { z } from "zod";

type DuplicateInTuple<
  T extends readonly unknown[],
  Seen = never,
> = T extends readonly [infer Head, ...infer Tail]
  ? Head extends Seen
    ? Head
    : DuplicateInTuple<Tail, Seen | Head>
  : never;

type UniqueReadonlyArray<T extends readonly string[]> =
  DuplicateInTuple<T> extends never
    ? T
    : T & { readonly __duplicate_table: DuplicateInTuple<T> };

const crudOpSchema = z.enum(["PUT", "PATCH", "DELETE"]);

export type CrudOp = z.infer<typeof crudOpSchema>;

/** Sync tables kept as a hand-maintained list (no `@lindaflor/db` dependency). */
const powersyncSyncTablesSource = [
  "concessions",
  "installations",
  "tankages",
  "members",
  "users",
  "measurement_equipments",
  "tanks",
  "tank_calibrations",
  "tank_calibration_points",
] as const;

export const POWERSYNC_SYNC_TABLES: UniqueReadonlyArray<
  typeof powersyncSyncTablesSource
> = powersyncSyncTablesSource;

export type PowerSyncSyncTable = (typeof POWERSYNC_SYNC_TABLES)[number];

const powersyncUploadTablesSource = [
  "tankages",
  "tanks",
] as const satisfies readonly PowerSyncSyncTable[];

export const POWERSYNC_UPLOAD_TABLES: UniqueReadonlyArray<
  typeof powersyncUploadTablesSource
> = powersyncUploadTablesSource;

export type PowerSyncUploadTable = (typeof POWERSYNC_UPLOAD_TABLES)[number];

const syncTableSchema = z.enum(POWERSYNC_SYNC_TABLES);

const uploadErrorSchema = z.object({
  id: z.string(),
  table: syncTableSchema,
  message: z.string(),
});

export type UploadError = z.infer<typeof uploadErrorSchema>;

export const uploadOperationStatusSchema = z.enum([
  "success",
  "failed",
  "forbidden",
  "invalid",
  "unsupported",
]);

export type UploadOperationStatus = z.infer<typeof uploadOperationStatusSchema>;

export const uploadOperationClientPayloadSchema = z.object({
  opData: z.record(z.string(), z.unknown()).optional(),
  transactionId: z.number().optional(),
});

export type UploadOperationClientPayload = z.infer<
  typeof uploadOperationClientPayloadSchema
>;

export const uploadOperationServerPayloadSchema = z.record(
  z.string(),
  z.unknown(),
);

export type UploadOperationServerPayload = z.infer<
  typeof uploadOperationServerPayloadSchema
>;

export const crudEntrySchema = z.object({
  id: z.string().min(1),
  op: crudOpSchema,
  table: syncTableSchema,
  opData: z.record(z.string(), z.unknown()).optional(),
  transactionId: z.number().optional(),
});

export type CrudEntry = z.infer<typeof crudEntrySchema>;

export const schema = {
  v1: {
    credentials: {
      output: z.object({
        token: z.string(),
        powersync_url: z.url(),
      }),
    },
    upload: {
      input: z.object({
        operations: z.array(crudEntrySchema).min(1),
      }),
      output: z.object({
        success: z.boolean(),
        errors: z.array(uploadErrorSchema).optional(),
      }),
    },
  },
};
