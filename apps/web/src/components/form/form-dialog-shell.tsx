import type { ReactNode } from "react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

type FormDialogShellProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  contentClassName?: string;
};

export function FormDialogShell({
  open,
  onOpenChange,
  title,
  description,
  children,
  contentClassName,
}: FormDialogShellProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className={cn(
          "flex max-h-[min(90vh,52rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl",
          contentClassName,
        )}
      >
        <DialogHeader className="shrink-0 gap-1 border-b px-6 py-4 pr-12">
          <DialogTitle className="text-lg font-semibold tracking-tight">
            {title}
          </DialogTitle>
          {description != null ? (
            <DialogDescription>{description}</DialogDescription>
          ) : null}
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

type FormDialogBodyProps = {
  children: ReactNode;
  className?: string;
};

export function FormDialogBody({ children, className }: FormDialogBodyProps) {
  return (
    <div className={cn("min-h-0 flex-1 overflow-y-auto px-6 py-5", className)}>
      {children}
    </div>
  );
}

type FormDialogSectionProps = {
  title: string;
  description?: string;
  children: ReactNode;
  className?: string;
};

export function FormDialogSection({
  title,
  description,
  children,
  className,
}: FormDialogSectionProps) {
  return (
    <section className={cn("space-y-4", className)}>
      <div className="space-y-1">
        <h3 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
          {title}
        </h3>
        {description != null ? (
          <p className="text-sm text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="grid gap-4 sm:grid-cols-2">{children}</div>
    </section>
  );
}
