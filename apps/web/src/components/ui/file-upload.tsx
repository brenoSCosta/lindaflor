import { ark } from "@ark-ui/react/factory";
import {
  FileUpload as ArkFileUpload,
  useFileUploadContext as useArkFileUploadContext,
} from "@ark-ui/react/file-upload";
import { UploadIcon, XIcon } from "lucide-react";
import type React from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export const useFileUpload = useArkFileUploadContext;

export const FileUpload = (
  props: React.ComponentProps<typeof ArkFileUpload.Root>,
) => {
  const { className, children, ...rest } = props;
  return (
    <ArkFileUpload.Root
      className={cn(
        "group/file-upload relative flex flex-col justify-center gap-4",
        className,
      )}
      data-slot="file-upload"
      {...rest}
    >
      {children}
      <ArkFileUpload.HiddenInput />
    </ArkFileUpload.Root>
  );
};

export const FileUploadTrigger = (
  props: React.ComponentProps<typeof ArkFileUpload.Trigger>,
) => <ArkFileUpload.Trigger data-slot="file-upload-trigger" {...props} />;

export const FileUploadDropzone = (
  props: React.ComponentProps<typeof ArkFileUpload.Dropzone>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.Dropzone
      className={cn(
        "[--space:--spacing(6)] flex cursor-pointer flex-col items-center justify-center gap-2 rounded-2xl border-2 border-dashed border-input p-(--space) text-center",
        "data-cover:absolute data-cover:inset-0 data-cover:flex data-cover:items-center data-cover:justify-center",
        "data-dragging:border-primary data-dragging:bg-primary/10",
        "data-invalid:border-destructive dark:data-invalid:border-destructive-foreground",
        className,
      )}
      data-slot="file-upload-dropzone"
      {...rest}
    />
  );
};

export const FileUploadDropzoneIcon = (
  props: React.ComponentProps<typeof ark.div>,
) => {
  const { className, children, ...rest } = props;
  return (
    <ark.div
      className={cn(
        "rounded-full border bg-muted/48 p-3 text-muted-foreground",
        "group-data-dragging/file-upload:border-primary/24 group-data-dragging/file-upload:bg-primary/5 group-data-dragging/file-upload:text-primary",
        "[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
        className,
      )}
      data-slot="file-upload-dropzone-icon"
      {...rest}
    >
      {children ?? <UploadIcon />}
    </ark.div>
  );
};

export const FileUploadTitle = (
  props: React.ComponentProps<typeof ark.div>,
) => {
  const { className, ...rest } = props;
  return (
    <ark.div
      className={cn("text-sm font-medium text-foreground", className)}
      data-slot="file-upload-title"
      {...rest}
    />
  );
};

export const FileUploadDescription = (
  props: React.ComponentProps<typeof ark.div>,
) => {
  const { className, ...rest } = props;
  return (
    <ark.div
      className={cn("text-sm font-medium text-muted-foreground", className)}
      data-slot="file-upload-description"
      {...rest}
    />
  );
};

export const FileUploadHelper = (
  props: React.ComponentProps<typeof ark.div>,
) => {
  const { className, ...rest } = props;
  return (
    <ark.div
      className={cn("text-xs text-muted-foreground", className)}
      data-slot="file-upload-helper"
      {...rest}
    />
  );
};

export const FileUploadItemGroup = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemGroup>,
) => <ArkFileUpload.ItemGroup data-slot="file-upload-item-group" {...props} />;

interface FileUploadListProps extends Omit<
  React.ComponentProps<typeof ArkFileUpload.Item>,
  "file"
> {}

export const FileUploadList = (props: FileUploadListProps) => {
  const { className, ...rest } = props;
  const fileUpload = useFileUpload();
  const files = fileUpload.acceptedFiles;

  if (files.length === 0) {
    return null;
  }

  return (
    <FileUploadItemGroup className="flex flex-col gap-2">
      {files.map((file) => {
        const isImage = file.type.startsWith("image/");
        const key = `${file.name}-${file.lastModified}-${file.size}`;
        const extension = file.name.split(".").pop();

        return (
          <FileUploadItem
            className={cn(
              "flex-1 items-start justify-start gap-4 rounded-xl border bg-card p-2 animate-in fade-in-0 slide-in-from-top-5 motion-reduce:animate-none!",
              className,
            )}
            file={file}
            key={key}
            {...rest}
          >
            <FileUploadItemPreview
              className="size-8"
              {...(isImage ? { type: "image/*" } : { type: ".*" })}
            >
              {isImage ? (
                <FileUploadItemPreviewImage />
              ) : (
                <span className="uppercase">{extension}</span>
              )}
            </FileUploadItemPreview>
            <div className="min-w-0 flex-1 overflow-hidden">
              <FileUploadItemName />
              <FileUploadItemSize />
            </div>
            <FileUploadItemDeleteTrigger
              asChild
              className="me-auto rtl:ms-auto"
            >
              <Button
                className="rounded-lg hover:bg-destructive/10 hover:text-destructive dark:hover:bg-destructive-foreground/10 dark:hover:text-destructive-foreground"
                size="icon-xs"
                variant="ghost"
              >
                <XIcon />
              </Button>
            </FileUploadItemDeleteTrigger>
          </FileUploadItem>
        );
      })}
    </FileUploadItemGroup>
  );
};

export const FileUploadItem = (
  props: React.ComponentProps<typeof ArkFileUpload.Item>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.Item
      className={cn("relative inline-flex", className)}
      data-slot="file-upload-item"
      {...rest}
    />
  );
};

export const FileUploadItemPreview = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemPreview>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.ItemPreview
      className={cn(
        "flex shrink-0 select-none items-center justify-center rounded-full bg-primary/10 font-semibold text-[0.5rem] text-primary",
        className,
      )}
      data-slot="file-upload-item-preview"
      {...rest}
    />
  );
};

export const FileUploadItemPreviewImage = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemPreviewImage>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.ItemPreviewImage
      className={cn(
        "aspect-square size-full rounded-lg object-cover",
        className,
      )}
      data-slot="file-upload-item-preview-image"
      {...rest}
    />
  );
};

export const FileUploadItemName = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemName>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.ItemName
      className={cn(
        "min-w-0 overflow-hidden truncate text-xs font-medium",
        className,
      )}
      data-slot="file-upload-item-name"
      {...rest}
    />
  );
};

export const FileUploadItemSize = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemSizeText>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkFileUpload.ItemSizeText
      className={cn("text-xs text-muted-foreground", className)}
      data-slot="file-upload-item-size"
      {...rest}
    />
  );
};

export const FileUploadItemDeleteTrigger = (
  props: React.ComponentProps<typeof ArkFileUpload.ItemDeleteTrigger>,
) => (
  <ArkFileUpload.ItemDeleteTrigger
    data-slot="file-upload-item-delete-trigger"
    {...props}
  />
);

export const FileUploadClearTrigger = (
  props: React.ComponentProps<typeof ArkFileUpload.ClearTrigger>,
) => (
  <ArkFileUpload.ClearTrigger
    data-slot="file-upload-clear-trigger"
    {...props}
  />
);

export const FileUploadRootProvider = (
  props: React.ComponentProps<typeof ArkFileUpload.RootProvider>,
) => (
  <ArkFileUpload.RootProvider
    data-slot="file-upload-root-provider"
    {...props}
  />
);
