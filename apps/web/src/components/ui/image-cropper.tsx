import {
  ImageCropper as ArkImageCropper,
  useImageCropper as useArkImageCropper,
} from "@ark-ui/react/image-cropper";
import type React from "react";

import { cn } from "@/lib/utils";

export const useImageCropper = useArkImageCropper;

interface ImageCropperProps extends React.ComponentProps<
  typeof ArkImageCropper.Root
> {}

export const ImageCropper = (props: ImageCropperProps) => {
  const { className, children, ...rest } = props;
  return (
    <ArkImageCropper.Root
      className={cn(
        "[--cropper-accent:var(--color-white)] [--cropper-handler-size:--spacing(2)] [--cropper-handler-width:--spacing(1)] relative aspect-video w-full",
        className,
      )}
      data-slot="image-cropper"
      {...rest}
    >
      <ArkImageCropper.Viewport
        className={cn("size-full overflow-hidden")}
        data-slot="image-cropper-viewport"
      >
        {children}
      </ArkImageCropper.Viewport>
    </ArkImageCropper.Root>
  );
};

export const ImageCropperRootProvider = (
  props: React.ComponentProps<typeof ArkImageCropper.RootProvider>,
) => {
  const { className, children, ...rest } = props;
  return (
    <ArkImageCropper.RootProvider
      className={cn(
        "[--cropper-accent:var(--color-white)] [--cropper-handler-size:--spacing(2)] [--cropper-handler-width:--spacing(1)] relative aspect-video w-full",
        className,
      )}
      data-slot="image-cropper-root-provider"
      {...rest}
    >
      <ArkImageCropper.Viewport
        className={cn("size-full overflow-hidden")}
        data-slot="image-cropper-viewport"
      >
        {children}
      </ArkImageCropper.Viewport>
    </ArkImageCropper.RootProvider>
  );
};

export const ImageCropperImage = (
  props: React.ComponentProps<typeof ArkImageCropper.Image>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkImageCropper.Image
      className={cn(
        "pointer-events-none absolute top-0 left-0 size-full origin-center select-none object-contain backface-hidden",
        className,
      )}
      data-slot="image-cropper-image"
      {...rest}
    />
  );
};

interface ImageCropperSelectionProps extends React.ComponentProps<
  typeof ArkImageCropper.Selection
> {
  axis?: "horizontal" | "vertical" | "both";
}

export const ImageCropperSelection = (props: ImageCropperSelectionProps) => {
  const { axis = "both", className, children, ...rest } = props;
  return (
    <ArkImageCropper.Selection
      className={cn(
        "cursor-move border-2 border-white/64 shadow-[0_0_0_9999px_rgb(0_0_0/0.5)] backface-visibility-hidden outline-none data-[shape=circle]:rounded-full data-disabled:cursor-default data-dragging:cursor-grabbing data-dragging:border-white/84 focus-visible:border-(--cropper-accent)",
        className,
      )}
      data-slot="image-cropper-selection"
      {...rest}
    >
      {children}
      {(axis === "horizontal" || axis === "both") && (
        <ImageCropperGrid axis="horizontal" />
      )}
      {(axis === "vertical" || axis === "both") && (
        <ImageCropperGrid axis="vertical" />
      )}
      <ImageCropperHandle position="n" />
      <ImageCropperHandle position="e" />
      <ImageCropperHandle position="s" />
      <ImageCropperHandle position="w" />
      <ImageCropperHandle position="ne" />
      <ImageCropperHandle position="se" />
      <ImageCropperHandle position="sw" />
      <ImageCropperHandle position="nw" />
    </ArkImageCropper.Selection>
  );
};

export const ImageCropperHandle = (
  props: React.ComponentProps<typeof ArkImageCropper.Handle>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkImageCropper.Handle
      className={cn(
        "absolute flex touch-none items-center justify-center border-(--cropper-accent) data-disabled:hidden",
        "h-[calc(var(--cropper-handler-size)+8px)] w-[calc(var(--cropper-handler-size)+8px)]",
        "data-[position=n]:cursor-ns-resize data-[position=s]:cursor-ns-resize data-[position=e]:cursor-ew-resize data-[position=w]:cursor-ew-resize",
        "data-[position=ne]:cursor-nesw-resize data-[position=nw]:cursor-nwse-resize data-[position=se]:cursor-nwse-resize data-[position=sw]:cursor-nesw-resize",
        "[&>span]:bg-(--cropper-accent) [&>span]:shadow-[0_1px_3px_rgb(0_0_0/0.3)]",
        "[&[data-position=nw]_*]:size-(--cropper-handler-size) [&[data-position=nw]_*]:border-l-[length:(--cropper-handler-width)] [&[data-position=nw]_*]:border-t-[length:(--cropper-handler-width)] [&[data-position=nw]_*]:bg-(--cropper-accent) data-[position=nw]:hover:**:scale-110",
        "[&[data-position=ne]_*]:size-(--cropper-handler-size) [&[data-position=ne]_*]:border-r-[length:(--cropper-handler-width)] [&[data-position=ne]_*]:border-t-[length:(--cropper-handler-width)] [&[data-position=ne]_*]:bg-(--cropper-accent) data-[position=ne]:hover:**:scale-110",
        "[&[data-position=se]_*]:size-(--cropper-handler-size) [&[data-position=se]_*]:border-r-[length:(--cropper-handler-width)] [&[data-position=se]_*]:border-b-[length:(--cropper-handler-width)] [&[data-position=se]_*]:bg-(--cropper-accent) data-[position=se]:hover:**:scale-110",
        "[&[data-position=sw]_*]:size-(--cropper-handler-size) [&[data-position=sw]_*]:border-l-[length:(--cropper-handler-width)] [&[data-position=sw]_*]:border-b-[length:(--cropper-handler-width)] [&[data-position=sw]_*]:bg-(--cropper-accent) data-[position=sw]:hover:**:scale-110",
        "[&[data-position=n]_*]:size-1.5 [&[data-position=n]_*]:opacity-0 data-[position=n]:hover:**:opacity-100",
        "[&[data-position=s]_*]:size-1.5 [&[data-position=s]_*]:bg-(--cropper-accent) data-[position=s]:hover:**:opacity-100",
        "[&[data-position=e]_*]:size-1.5 [&[data-position=e]_*]:bg-(--cropper-accent) data-[position=e]:hover:**:opacity-100",
        "[&[data-position=w]_*]:size-1.5 [&[data-position=w]_*]:bg-(--cropper-accent) data-[position=w]:hover:**:opacity-100",
        className,
      )}
      data-slot="image-cropper-handle"
      {...rest}
    >
      <span aria-hidden className="block size-(--cropper-handler-size)" />
    </ArkImageCropper.Handle>
  );
};

export const ImageCropperGrid = (
  props: React.ComponentProps<typeof ArkImageCropper.Grid>,
) => {
  const { className, ...rest } = props;
  return (
    <ArkImageCropper.Grid
      className={cn(
        "pointer-events-none absolute opacity-0 transition-opacity duration-200 motion-reduce:transition-none! data-dragging:opacity-100 data-panning:opacity-100",
        "data-[axis=horizontal]:inset-[33.33%_0] data-[axis=horizontal]:border-y data-[axis=horizontal]:border-white/40",
        "data-[axis=vertical]:inset-0_[33.33%] data-[axis=vertical]:border-x data-[axis=vertical]:border-white/40",
        className,
      )}
      data-slot="image-cropper-grid"
      {...rest}
    />
  );
};
