import { getPublicFileUrl } from "@lindaflor/s3";

function isS3Key(value: string) {
  return !value.includes("://");
}

export async function resolveImageUrl(url: string) {
  if (!isS3Key(url)) {
    return url;
  }
  return getPublicFileUrl(url);
}

export async function resolveImageUrls<T extends { url: string }>(images: T[]) {
  return Promise.all(
    images.map(async (image) => ({
      ...image,
      url: await resolveImageUrl(image.url),
    })),
  );
}
