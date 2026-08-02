import type { LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

type TankKpiCardProps = {
  label: string;
  value: string;
  hint?: string;
  icon: LucideIcon;
  tone?: "default" | "warning" | "success";
};

export function TankKpiCard({
  label,
  value,
  hint,
  icon: Icon,
  tone = "default",
}: TankKpiCardProps) {
  return (
    <div className="rounded-xl border bg-card p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
            {label}
          </p>
          <p className="text-2xl font-semibold tracking-tight tabular-nums">
            {value}
          </p>
          {hint ? (
            <p className="text-xs text-muted-foreground">{hint}</p>
          ) : null}
        </div>
        <div
          className={cn(
            "flex size-10 shrink-0 items-center justify-center rounded-lg",
            tone === "warning" && "bg-destructive/10 text-destructive",
            tone === "success" && "bg-success/10 text-success",
            tone === "default" && "bg-primary/10 text-primary",
          )}
        >
          <Icon className="size-5" aria-hidden />
        </div>
      </div>
    </div>
  );
}
