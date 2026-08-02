import { db } from "@lindaflor/db";
import {
  training_lectures,
  training_modules,
  training_sections,
} from "@lindaflor/db/schema/training";
import {
  copyFile,
  deleteFile,
  TRAINING_LECTURES_PREFIX,
  TRAINING_TMP_PREFIX,
} from "@lindaflor/s3";
import { ORPCError } from "@orpc/server";
import { eq, inArray } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";

export function isTmpPdfKey(fileKey: string | undefined | null): boolean {
  return (
    typeof fileKey === "string" && fileKey.startsWith(`${TRAINING_TMP_PREFIX}/`)
  );
}

export type PdfLectureMeta = {
  file_key: string;
  file_name: string;
  file_size: number;
  mime_type: string;
};

export type ResolvedPdfLecture = {
  file_key: string | null;
  file_name: string | null;
  file_size: number | null;
  mime_type: string | null;
};

export const EMPTY_PDF_FIELDS: ResolvedPdfLecture = {
  file_key: null,
  file_name: null,
  file_size: null,
  mime_type: null,
};

export function buildTmpPdfKey(fileName: string): string {
  return `${TRAINING_TMP_PREFIX}/${uuidv7()}/${fileName}`;
}

export function buildPermanentPdfKey(fileName: string): string {
  return `${TRAINING_LECTURES_PREFIX}/${uuidv7()}/${fileName}`;
}

export async function moveTmpPdfToPermanent(
  tmpKey: string,
  fileName: string,
): Promise<string> {
  if (!isTmpPdfKey(tmpKey)) {
    return tmpKey;
  }

  const permanentKey = buildPermanentPdfKey(fileName);

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => copyFile(tmpKey, permanentKey),
      catch: () =>
        new ORPCError("SERVICE_UNAVAILABLE", {
          message:
            "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.",
        }),
    }),
  );

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => deleteFile(tmpKey),
      catch: (e): Error =>
        e instanceof Error ? e : new Error("deleteFile failed"),
    }).pipe(Effect.orElseSucceed(() => undefined)),
  );

  return permanentKey;
}

export async function bestEffortDeleteFile(
  fileKey: string | null | undefined,
): Promise<void> {
  if (!fileKey || fileKey.length === 0) {
    return;
  }

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => deleteFile(fileKey),
      catch: (e): Error =>
        e instanceof Error ? e : new Error("deleteFile failed"),
    }).pipe(Effect.orElseSucceed(() => undefined)),
  );
}

export type LectureFileRow = {
  type: string;
  file_key: string | null;
};

type Queryable = typeof db;

export const deleteLectureFilesByCourse = (courseId: string) =>
  Effect.gen(function* () {
    const rows = yield* Effect.tryPromise({
      try: () =>
        db
          .select({ file_key: training_lectures.file_key })
          .from(training_lectures)
          .innerJoin(
            training_modules,
            eq(training_lectures.module_id, training_modules.id),
          )
          .innerJoin(
            training_sections,
            eq(training_modules.section_id, training_sections.id),
          )
          .where(eq(training_sections.course_id, courseId)),
      catch: (e): Error => (e instanceof Error ? e : new Error(String(e))),
    });

    yield* Effect.all(
      rows.map((row) =>
        Effect.tryPromise({
          try: () => bestEffortDeleteFile(row.file_key),
          catch: (e): Error => (e instanceof Error ? e : new Error(String(e))),
        }),
      ),
    );
  }).pipe(Effect.orElseSucceed(() => undefined));

export async function deleteLectureFilesBySections(
  sectionIds: string[],
  tx: Queryable = db,
): Promise<void> {
  if (sectionIds.length === 0) {
    return;
  }

  const rows = await tx
    .select({ file_key: training_lectures.file_key })
    .from(training_lectures)
    .innerJoin(
      training_modules,
      eq(training_lectures.module_id, training_modules.id),
    )
    .where(inArray(training_modules.section_id, sectionIds));

  await Promise.all(rows.map((row) => bestEffortDeleteFile(row.file_key)));
}

export async function deleteLectureFilesByModules(
  moduleIds: string[],
  tx: Queryable = db,
): Promise<void> {
  if (moduleIds.length === 0) {
    return;
  }

  const rows = await tx
    .select({ file_key: training_lectures.file_key })
    .from(training_lectures)
    .where(inArray(training_lectures.module_id, moduleIds));

  await Promise.all(rows.map((row) => bestEffortDeleteFile(row.file_key)));
}
