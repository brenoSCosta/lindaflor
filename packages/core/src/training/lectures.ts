import { buildTmpPdfKey } from "@lindaflor/core/training/lecture-pdf";
import { verifyTrainingProgress } from "@lindaflor/core/training/utils";
import { db } from "@lindaflor/db";
import {
  training_lecture_progress,
  training_lectures,
  type TrainingLectureProgress,
} from "@lindaflor/db/schema/training";
import { getFileUrl, uploadFile } from "@lindaflor/s3";
import { PDF_MIME_TYPE } from "@lindaflor/shared/constants";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/training";
import { ORPCError } from "@orpc/server";
import { and, eq } from "drizzle-orm";
import { Effect } from "effect";
import type { z } from "zod";

type UploadLecturePdfInput = z.infer<
  typeof schema.v1.lectures.pdf.upload.input
>;

export async function uploadLecturePdf(params: {
  input: UploadLecturePdfInput;
}) {
  const { input } = params;
  const fileKey = buildTmpPdfKey(input.file.name);
  const buffer = Buffer.from(await input.file.arrayBuffer());
  const mimeType = input.file.type || PDF_MIME_TYPE;

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => uploadFile(fileKey, buffer, mimeType),
      catch: () =>
        new ORPCError("SERVICE_UNAVAILABLE", {
          message:
            "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.",
        }),
    }),
  );

  return {
    file_key: fileKey,
    file_name: input.file.name,
    file_size: input.file.size,
    mime_type: mimeType,
  };
}

type DownloadLecturePdfInput = z.infer<
  typeof schema.v1.lectures.pdf.download.input
>;

export async function downloadLecturePdf(params: {
  input: DownloadLecturePdfInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;

  const { url, file_name } = await db.transaction(async (tx) => {
    const [lecture] = await tx
      .select({
        file_key: training_lectures.file_key,
        file_name: training_lectures.file_name,
        type: training_lectures.type,
      })
      .from(training_lectures)
      .where(eq(training_lectures.id, input.lecture_id))
      .limit(1);

    if (!lecture) {
      throw new ORPCError("NOT_FOUND", {
        message: "Aula não encontrada",
      });
    }

    if (lecture.type !== "pdf" || !lecture.file_key) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Esta aula não possui arquivo PDF",
      });
    }

    await verifyTrainingProgress(
      tx,
      ability,
      input.lecture_id,
      organizationId,
      userId,
    );

    const signedUrl = await getFileUrl(lecture.file_key);
    return { url: signedUrl, file_name: lecture.file_name };
  });

  return { url, file_name };
}

type MarkLectureProgressInput = z.infer<
  typeof schema.v1.lectures.progress.mark.input
>;

export async function markLectureProgress(params: {
  input: MarkLectureProgressInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;

  const progress = await db.transaction(async (tx) => {
    const [lecture] = await tx
      .select()
      .from(training_lectures)
      .where(eq(training_lectures.id, input.lecture_id))
      .limit(1);

    if (!lecture) {
      throw new ORPCError("NOT_FOUND", {
        message: "Aula não encontrada",
      });
    }

    await verifyTrainingProgress(
      tx,
      ability,
      input.lecture_id,
      organizationId,
      userId,
    );

    const completedAt = input.status === "completed" ? new Date() : null;
    const now = new Date();

    const [existing] = await tx
      .select()
      .from(training_lecture_progress)
      .where(
        and(
          eq(training_lecture_progress.lecture_id, input.lecture_id),
          eq(training_lecture_progress.user_id, userId),
          eq(training_lecture_progress.organization_id, organizationId),
        ),
      )
      .limit(1);

    let result: TrainingLectureProgress;

    if (existing) {
      await tx
        .update(training_lecture_progress)
        .set({
          status: input.status,
          completed_at: completedAt,
          updated_at: now,
        })
        .where(eq(training_lecture_progress.id, existing.id));

      result = {
        ...existing,
        status: input.status,
        completed_at: completedAt,
        updated_at: now,
      };
    } else {
      const [inserted] = await tx
        .insert(training_lecture_progress)
        .values({
          lecture_id: input.lecture_id,
          user_id: userId,
          organization_id: organizationId,
          status: input.status,
          completed_at: completedAt,
          updated_at: now,
        })
        .returning();

      if (!inserted) {
        throw new ORPCError("INTERNAL_SERVER_ERROR", {
          message: "Falha ao criar progresso",
        });
      }

      result = inserted;
    }

    return result;
  });

  return schema.v1.lectures.progress.mark.output.parse(progress);
}
