import { db } from "@lindaflor/db";
import { members, organizations } from "@lindaflor/db/schema/auth";
import { deleteFile, getFileUrl, uploadFile } from "@lindaflor/s3";
import { schema } from "@lindaflor/shared/schemas/organization";
import { ORPCError } from "@orpc/server";
import { and, eq } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import type { z } from "zod";

const LOGOS_PREFIX = "organizations";

function isOwnerOrAdmin(role: string): boolean {
  return role === "owner" || role === "admin";
}

function extensionFromMimeType(mimeType: string): string {
  switch (mimeType) {
    case "image/jpeg":
      return "jpg";
    case "image/png":
      return "png";
    case "image/webp":
      return "webp";
    default:
      return "png";
  }
}

function isS3Key(value: string): boolean {
  return !value.includes("://");
}

function extractLogoS3Key(logo: string | null | undefined): string | null {
  if (!logo || !isS3Key(logo)) {
    return null;
  }
  return logo;
}

async function resolveLogoUrl(logo: string | null): Promise<string | null> {
  if (!logo) {
    return null;
  }

  if (!isS3Key(logo)) {
    return logo;
  }

  return getFileUrl(logo);
}

type UpdateOrganizationLogoInput = z.infer<typeof schema.v1.logo.update.input>;

export async function updateOrganizationLogo(params: {
  input: UpdateOrganizationLogoInput;
  userId: string;
}) {
  const { input, userId } = params;
  const orgId = input.id;

  const [membership] = await db
    .select()
    .from(members)
    .where(and(eq(members.organization_id, orgId), eq(members.user_id, userId)))
    .limit(1);

  if (!membership || !isOwnerOrAdmin(membership.role)) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "Apenas administradores e proprietários podem atualizar a logo da organização",
    });
  }

  const [org] = await db
    .select()
    .from(organizations)
    .where(eq(organizations.id, orgId));

  if (!org) {
    throw new ORPCError("NOT_FOUND", {
      message: "Organização não encontrada",
    });
  }

  const fileKey = `${LOGOS_PREFIX}/${orgId}/${uuidv7()}.${extensionFromMimeType(input.file.type)}`;
  const buffer = Buffer.from(await input.file.arrayBuffer());

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => uploadFile(fileKey, buffer, input.file.type),
      catch: () =>
        new ORPCError("SERVICE_UNAVAILABLE", {
          message:
            "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.",
        }),
    }),
  );

  const [updated] = await db
    .update(organizations)
    .set({ logo: fileKey })
    .where(eq(organizations.id, orgId))
    .returning();

  if (!updated) {
    await deleteFile(fileKey);
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao atualizar logo da organização",
    });
  }

  const oldKey = extractLogoS3Key(org?.logo);
  if (oldKey && oldKey !== fileKey) {
    await Effect.runPromise(
      Effect.tryPromise({
        try: () => deleteFile(oldKey),
        catch: (e): Error =>
          e instanceof Error ? e : new Error("deleteFile failed"),
      }).pipe(Effect.orElseSucceed(() => undefined)),
    );
  }

  return { logo: updated.logo ?? fileKey };
}

type GetOrganizationLogoUrlInput = z.infer<typeof schema.v1.logo.get.input>;

export async function getOrganizationLogoUrl(params: {
  input: GetOrganizationLogoUrlInput;
  userId: string;
}) {
  const { input, userId } = params;

  const [membership] = await db
    .select({ id: members.id })
    .from(members)
    .where(
      and(eq(members.organization_id, input.id), eq(members.user_id, userId)),
    )
    .limit(1);

  if (!membership) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para visualizar esta organização",
    });
  }

  const [org] = await db
    .select()
    .from(organizations)
    .where(eq(organizations.id, input.id));

  const url = await resolveLogoUrl(org?.logo ?? null);

  return { url };
}
