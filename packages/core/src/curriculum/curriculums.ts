import { db } from "@lindaflor/db";
import { careers } from "@lindaflor/db/schema/career";
import { curriculums } from "@lindaflor/db/schema/curriculum";
import { deleteFile, getFileUrl, uploadFile } from "@lindaflor/s3";
import { PDF_MIME_TYPE } from "@lindaflor/shared/constants";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/curriculum";
import { ORPCError } from "@orpc/server";
import { desc, eq, ilike, or, sql } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import type { z } from "zod";

const CURRICULUMS_PREFIX = "curriculums";

type SubmitCurriculumInput = z.infer<typeof schema.v1.submit.input>;

export async function submitCurriculum(params: {
  input: SubmitCurriculumInput;
}) {
  const { input } = params;
  let headline: string;
  let careerId: string | null = null;

  if (input.career_id) {
    const [career] = await db
      .select({
        title: careers.title,
        is_active: careers.is_active,
      })
      .from(careers)
      .where(eq(careers.id, input.career_id));

    if (!career?.is_active) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Vaga não encontrada ou indisponível",
      });
    }

    headline = career.title;
    careerId = input.career_id;
  } else {
    headline = input.headline?.trim() ?? "";
  }

  const fileKey = `${CURRICULUMS_PREFIX}/${uuidv7()}/${input.file.name}`;
  const buffer = Buffer.from(await input.file.arrayBuffer());

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => uploadFile(fileKey, buffer, input.file.type || PDF_MIME_TYPE),
      catch: () =>
        new ORPCError("SERVICE_UNAVAILABLE", {
          message:
            "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.",
        }),
    }),
  );

  const [created] = await db
    .insert(curriculums)
    .values({
      name: input.name,
      email: input.email,
      phone: input.phone ?? null,
      headline,
      summary: input.summary ?? null,
      skills: input.skills,
      career_id: careerId,
      file_key: fileKey,
      file_name: input.file.name,
      file_size: input.file.size,
      mime_type: input.file.type || PDF_MIME_TYPE,
    })
    .returning();

  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao salvar currículo",
    });
  }

  return schema.v1.submit.output.parse(created);
}

type ListCurriculumsInput = z.infer<typeof schema.v1.list.input>;

export async function listCurriculums(params: { input: ListCurriculumsInput }) {
  const { input } = params;
  const { pageIndex, pageSize } = input.pagination ?? {
    pageIndex: 1,
    pageSize: 9,
  };
  const offset = (pageIndex - 1) * pageSize;

  const searchCondition = input.search
    ? or(
        ilike(curriculums.name, `%${input.search}%`),
        ilike(curriculums.headline, `%${input.search}%`),
        sql`${curriculums.skills}::text ILIKE ${`%${input.search}%`}`,
      )
    : undefined;

  const [data, countResult] = await Promise.all([
    db
      .select()
      .from(curriculums)
      .where(searchCondition)
      .orderBy(desc(curriculums.submitted_at))
      .limit(pageSize)
      .offset(offset),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(curriculums)
      .where(searchCondition)
      .then((rows) => rows[0]?.count ?? 0),
  ]);

  const total = countResult;
  const totalPages = Math.ceil(total / pageSize);

  return schema.v1.list.output.parse({
    data,
    meta: {
      totalPages,
    },
  });
}

type GetCurriculumByIdInput = z.infer<typeof schema.v1.getById.input>;

export async function getCurriculumById(params: {
  input: GetCurriculumByIdInput;
  ability: AppAbility;
}) {
  const [curriculum] = await db
    .select()
    .from(curriculums)
    .where(eq(curriculums.id, params.input.id));

  if (!curriculum) {
    throw new ORPCError("NOT_FOUND", {
      message: "Currículo não encontrado",
    });
  }

  if (params.ability.cannot("read", subject("Curriculum", curriculum))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para visualizar este currículo",
    });
  }

  return schema.v1.getById.output.parse(curriculum);
}

type GetCurriculumDownloadUrlInput = z.infer<
  typeof schema.v1.getDownloadUrl.input
>;

export async function getCurriculumDownloadUrl(params: {
  input: GetCurriculumDownloadUrlInput;
  ability: AppAbility;
}) {
  const [curriculum] = await db
    .select()
    .from(curriculums)
    .where(eq(curriculums.id, params.input.id));

  if (!curriculum) {
    throw new ORPCError("NOT_FOUND", {
      message: "Currículo não encontrado",
    });
  }

  if (params.ability.cannot("read", subject("Curriculum", curriculum))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para baixar este currículo",
    });
  }

  const url = await getFileUrl(curriculum.file_key);
  return { url };
}

type DeleteCurriculumInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteCurriculum(params: {
  input: DeleteCurriculumInput;
  ability: AppAbility;
}) {
  const [existing] = await db
    .select()
    .from(curriculums)
    .where(eq(curriculums.id, params.input.id));

  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Currículo não encontrado",
    });
  }

  if (params.ability.cannot("delete", subject("Curriculum", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para excluir este currículo",
    });
  }

  await deleteFile(existing.file_key);
  await db.delete(curriculums).where(eq(curriculums.id, existing.id));

  return null;
}
