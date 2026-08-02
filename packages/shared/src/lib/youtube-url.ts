import { Effect } from "effect";
import { z } from "zod";

const YOUTUBE_EMBED_BASE_URL = "https://www.youtube.com/embed";
const YOUTUBE_WATCH_BASE_URL = "https://www.youtube.com/watch";
const YOUTUBE_THUMBNAIL_BASE_URL = "https://img.youtube.com/vi";

export type YouTubeThumbnailQuality = "default" | "mqdefault" | "hqdefault";

export function extractYouTubeVideoId(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) {
    return null;
  }

  if (/^[a-zA-Z0-9_-]{11}$/.test(trimmed)) {
    return trimmed;
  }

  return Effect.runSync(
    Effect.gen(function* () {
      const url = yield* Effect.try({
        try: () => new URL(trimmed),
        catch: () => new Error("invalid URL"),
      });

      if (url.hostname === "youtu.be") {
        return url.pathname.slice(1) || null;
      }

      if (
        url.hostname === "www.youtube.com" ||
        url.hostname === "youtube.com" ||
        url.hostname === "m.youtube.com"
      ) {
        if (url.pathname.startsWith("/embed/")) {
          return url.pathname.split("/")[2] || null;
        }

        if (url.pathname === "/watch") {
          return url.searchParams.get("v");
        }
      }

      return null;
    }).pipe(Effect.catchAll(() => Effect.succeed(null))),
  );
}

export function buildYouTubeEmbedUrl(videoId: string): string {
  return `${YOUTUBE_EMBED_BASE_URL}/${encodeURIComponent(videoId)}`;
}

export function buildYouTubeWatchUrl(videoId: string): string {
  return `${YOUTUBE_WATCH_BASE_URL}?v=${encodeURIComponent(videoId)}`;
}

export function buildYouTubeThumbnailUrl(
  input: string,
  quality: YouTubeThumbnailQuality = "hqdefault",
): string | null {
  const videoId = extractYouTubeVideoId(input);

  if (!videoId) {
    return null;
  }

  return `${YOUTUBE_THUMBNAIL_BASE_URL}/${videoId}/${quality}.jpg`;
}

export function normalizeYouTubeUrl(input: string): string {
  const trimmed = input.trim();
  const videoId = extractYouTubeVideoId(trimmed);

  if (!videoId) {
    throw new Error("URL do YouTube inválida");
  }

  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }

  return buildYouTubeWatchUrl(videoId);
}

export const youtubeUrlSchema = z
  .string()
  .min(1)
  .refine((value) => extractYouTubeVideoId(value) !== null, {
    message: "URL do YouTube inválida",
  });
