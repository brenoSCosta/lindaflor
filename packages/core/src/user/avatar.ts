import { db } from "@lindaflor/db";
import { users } from "@lindaflor/db/schema/auth";
import { deleteFile, getFileUrl, uploadFile } from "@lindaflor/s3";
import { schema } from "@lindaflor/shared/schemas/user";
import { ORPCError } from "@orpc/server";
import { eq } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import type { z } from "zod";

const AVATARS_PREFIX = "avatars";

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

export function extractAvatarS3Key(
  image: string | null | undefined,
): string | null {
  if (!image || !isS3Key(image)) {
    return null;
  }
  return image;
}

async function resolveAvatarUrl(image: string | null): Promise<string | null> {
  if (!image) {
    return null;
  }

  if (!isS3Key(image)) {
    return image;
  }

  return getFileUrl(image);
}

type UpdateAvatarInput = z.infer<typeof schema.v1.avatar.update.input>;

export async function updateAvatar(params: {
  input: UpdateAvatarInput;
  userId: string;
}) {
  const { input, userId } = params;

  const [user] = await db
    .select({ image: users.image })
    .from(users)
    .where(eq(users.id, userId));

  const fileKey = `${AVATARS_PREFIX}/${userId}/${uuidv7()}.${extensionFromMimeType(input.file.type)}`;
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
    .update(users)
    .set({ image: fileKey })
    .where(eq(users.id, userId))
    .returning();

  if (!updated) {
    await deleteFile(fileKey);
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao atualizar avatar",
    });
  }

  const oldKey = extractAvatarS3Key(user?.image);
  if (oldKey && oldKey !== fileKey) {
    await Effect.runPromise(
      Effect.tryPromise({
        try: () => deleteFile(oldKey),
        catch: (e): Error =>
          e instanceof Error ? e : new Error("deleteFile failed"),
      }).pipe(Effect.orElseSucceed(() => undefined)),
    );
  }

  return { image: updated.image ?? fileKey };
}

type GetAvatarUrlInput = z.infer<typeof schema.v1.avatar.get.input>;

export async function getAvatarUrl(params: {
  input: GetAvatarUrlInput;
  sessionUserId: string;
  isAdmin: boolean;
}) {
  const { input, sessionUserId, isAdmin } = params;
  const targetUserId = input.id;

  if (targetUserId !== sessionUserId && !isAdmin) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para visualizar este avatar",
    });
  }

  const [user] = await db
    .select({ image: users.image })
    .from(users)
    .where(eq(users.id, targetUserId));

  const url = await resolveAvatarUrl(user?.image ?? null);

  return { url };
}
