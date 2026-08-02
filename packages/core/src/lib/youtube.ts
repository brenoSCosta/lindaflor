import { env } from "@lindaflor/env/server";
import { ORPCError } from "@orpc/server";
import { z } from "zod";

const YOUTUBE_VIDEO_API_URL = "https://www.googleapis.com/youtube/v3/videos";

const ISO_8601_DURATION_MATCH =
  /^P(?:(\d+)D)?T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?$/;

const youtubeThumbnailsSchema = z.record(
  z.string(),
  z.object({ url: z.string() }),
);

const youtubeVideoItemSchema = z.object({
  snippet: z
    .object({
      title: z.string().optional(),
      thumbnails: youtubeThumbnailsSchema.optional(),
    })
    .optional(),
  contentDetails: z
    .object({
      duration: z.string().optional(),
    })
    .optional(),
});

const youtubeVideoResponseSchema = z.object({
  items: z.array(youtubeVideoItemSchema),
});

function validateYouTubeResponse(data: unknown) {
  const result = youtubeVideoResponseSchema.safeParse(data);
  if (!result.success) {
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: "Resposta inválida da API do YouTube",
    });
  }
  return result.data;
}

function parseISODurationFromApi(duration: string): number {
  const match = ISO_8601_DURATION_MATCH.exec(duration);
  if (!match) {
    return 0;
  }

  const days = Number.parseInt(match[1] ?? "0", 10);
  const hours = Number.parseInt(match[2] ?? "0", 10);
  const minutes = Number.parseInt(match[3] ?? "0", 10);
  const seconds = Number.parseInt(match[4] ?? "0", 10);

  return days * 86_400 + hours * 3_600 + minutes * 60 + seconds;
}

export type YouTubeVideoMetadata = {
  videoId: string;
  title: string;
  durationSeconds: number;
  thumbnailUrl: string | null;
};

export async function fetchYouTubeVideoMetadata(
  videoId: string,
): Promise<YouTubeVideoMetadata> {
  const url = new URL(YOUTUBE_VIDEO_API_URL);
  url.searchParams.set("part", "snippet,contentDetails");
  url.searchParams.set("id", videoId);
  url.searchParams.set("key", env.YOUTUBE_API_KEY);

  const response = await fetch(url);

  if (!response.ok) {
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: "Falha ao buscar metadados do vídeo do YouTube",
    });
  }

  const rawData = await response.json();

  const data = validateYouTubeResponse(rawData);

  const item = data.items[0];
  if (!item) {
    throw new ORPCError("NOT_FOUND", {
      message: "Vídeo do YouTube não encontrado ou não está acessível",
    });
  }

  const title: string = item.snippet?.title ?? "Sem título";
  const durationSeconds = parseISODurationFromApi(
    item.contentDetails?.duration ?? "PT0S",
  );
  const thumbnails = item.snippet?.thumbnails ?? {};
  const thumbnailUrl: string | null =
    thumbnails.maxres?.url ??
    thumbnails.standard?.url ??
    thumbnails.high?.url ??
    thumbnails.medium?.url ??
    thumbnails.default?.url ??
    null;

  return {
    videoId,
    title,
    durationSeconds,
    thumbnailUrl,
  };
}
