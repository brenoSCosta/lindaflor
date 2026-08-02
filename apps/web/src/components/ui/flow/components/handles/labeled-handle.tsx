import { Position, type HandleProps } from "@xyflow/react";
import type { ComponentProps } from "react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { cn } from "@/lib/utils";

const flexDirections = {
  [Position.Top]: "flex-col",
  [Position.Right]: "flex-row-reverse justify-end",
  [Position.Bottom]: "flex-col-reverse justify-end",
  [Position.Left]: "flex-row",
} as const;

export type LabeledHandleProps = HandleProps &
  ComponentProps<"div"> & {
    title: string;
    handleClassName?: string;
    labelClassName?: string;
  };

export function LabeledHandle({
  className,
  labelClassName,
  handleClassName,
  title,
  position,
  ...props
}: LabeledHandleProps) {
  return (
    <div
      data-slot="flow-labeled-handle"
      title={title}
      className={cn(
        "relative flex items-center",
        flexDirections[position],
        className,
      )}
    >
      <BaseHandle position={position} className={handleClassName} {...props} />
      <span className={cn("text-foreground px-3", labelClassName)}>
        {title}
      </span>
    </div>
  );
}
