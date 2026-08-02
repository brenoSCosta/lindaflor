import {
  CopyObjectCommand,
  CreateBucketCommand,
  DeleteObjectCommand,
  GetObjectCommand,
  HeadBucketCommand,
  PutBucketLifecycleConfigurationCommand,
  S3Client,
} from "@aws-sdk/client-s3";
import { Upload } from "@aws-sdk/lib-storage";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
import { env } from "@lindaflor/env/server";
import { Effect } from "effect";

export const TRAINING_TMP_PREFIX = "training/tmp";
export const TRAINING_LECTURES_PREFIX = "training/lectures";
export const PRODUCTS_IMAGES_PREFIX = "products";

const PRESIGNED_URL_EXPIRATION_SECONDS = 60 * 60 * 24;

export const s3Client = new S3Client({
  endpoint: env.S3_ENDPOINT ?? "http://localhost:9012",
  region: env.S3_REGION,
  credentials: {
    accessKeyId: env.S3_ACCESS_KEY_ID ?? "minioadmin",
    secretAccessKey: env.S3_SECRET_ACCESS_KEY ?? "minioadmin",
  },
  forcePathStyle: true,
});

export function ensureBucket(): Effect.Effect<void, Error> {
  return Effect.tryPromise({
    try: () => s3Client.send(new HeadBucketCommand({ Bucket: env.S3_BUCKET })),
    catch: () => new Error("bucket-not-found"),
  }).pipe(
    Effect.catchAll(() =>
      Effect.gen(function* () {
        yield* Effect.logWarning(
          `Bucket "${env.S3_BUCKET}" not found — creating it`,
        );

        yield* Effect.tryPromise({
          try: () =>
            s3Client.send(new CreateBucketCommand({ Bucket: env.S3_BUCKET })),
          catch: (err) => new Error(`bucket creation failed: ${String(err)}`),
        });

        yield* Effect.tryPromise({
          try: () =>
            s3Client.send(
              new PutBucketLifecycleConfigurationCommand({
                Bucket: env.S3_BUCKET,
                LifecycleConfiguration: {
                  Rules: [
                    {
                      ID: "expire-training-tmp",
                      Status: "Enabled",
                      Filter: { Prefix: `${TRAINING_TMP_PREFIX}/` },
                      Expiration: { Days: 1 },
                    },
                  ],
                },
              }),
            ),
          catch: (err) => new Error(`bucket lifecycle failed: ${String(err)}`),
        });

        yield* Effect.log(`Bucket "${env.S3_BUCKET}" created`);
      }),
    ),
    Effect.asVoid,
  );
}

export async function uploadFile(
  key: string,
  buffer: Buffer,
  mimeType: string,
): Promise<void> {
  const upload = new Upload({
    client: s3Client,
    params: {
      Bucket: env.S3_BUCKET,
      Key: key,
      Body: buffer,
      ContentType: mimeType,
    },
  });

  await upload.done();
}

export async function getPublicFileUrl(key: string): Promise<string> {
  if (env.S3_PUBLIC_URL) {
    return `${env.S3_PUBLIC_URL.replace(/\/$/, "")}/${key}`;
  }

  return getFileUrl(key);
}

export async function getFileUrl(key: string): Promise<string> {
  const command = new GetObjectCommand({
    Bucket: env.S3_BUCKET,
    Key: key,
  });

  return getSignedUrl(s3Client, command, {
    expiresIn: PRESIGNED_URL_EXPIRATION_SECONDS,
  });
}

export async function getFileStream(
  key: string,
): Promise<ReadableStream<Uint8Array>> {
  const command = new GetObjectCommand({
    Bucket: env.S3_BUCKET,
    Key: key,
  });

  const response = await s3Client.send(command);
  const body = response.Body;

  if (!body) {
    throw new Error("Empty response body from S3");
  }

  return body.transformToWebStream();
}

export async function deleteFile(key: string): Promise<void> {
  const command = new DeleteObjectCommand({
    Bucket: env.S3_BUCKET,
    Key: key,
  });

  await s3Client.send(command);
}

export async function copyFile(
  sourceKey: string,
  destKey: string,
): Promise<void> {
  const command = new CopyObjectCommand({
    Bucket: env.S3_BUCKET,
    Key: destKey,
    CopySource: `${env.S3_BUCKET}/${sourceKey}`,
  });

  await s3Client.send(command);
}
