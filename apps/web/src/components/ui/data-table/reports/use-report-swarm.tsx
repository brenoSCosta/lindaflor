import { Effect } from "effect";
import React from "react";
import { toast } from "sonner";

import { BRAND } from "@/components/ui/data-table/reports/render-helpers";
import {
  ReportAbortedError,
  spawnReportWorker,
} from "@/components/ui/data-table/reports/spawn-report-worker";
import type {
  OrgReportTheme,
  ReportColumn,
  ReportFormat,
} from "@/components/ui/data-table/reports/types";
import { Progress } from "@/components/ui/progress";
import { useTimeFormat } from "@/context/time-format";
import { useTimezone } from "@/context/timezone";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

export interface ReportJobInput {
  readonly format: ReportFormat;
  readonly filename: string;
  readonly title: string;
  readonly columns: readonly ReportColumn[];
  readonly fetchRows: () => Promise<string[][]>;
}

export interface ReportJobState {
  readonly id: string;
  readonly format: ReportFormat;
  readonly percent: number;
  readonly step: string;
}

interface JobContext {
  format: ReportFormat;
  percent: number;
  target: number;
  step: string;
  toastId: string | number;
  controller: AbortController;
}

function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}

export function ReportProgressBar({
  percent,
  step,
  label,
  className,
}: {
  percent: number;
  step: string;
  label?: string;
  className?: string;
}): React.ReactElement {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="flex min-w-0 items-center gap-1.5 font-medium">
          {label && (
            <span className="shrink-0 text-muted-foreground">{label}</span>
          )}
          <span className="truncate">{step || "Gerando relatório…"}</span>
        </span>
        <span className="shrink-0 tabular-nums text-muted-foreground">
          {percent}%
        </span>
      </div>
      <Progress value={percent} />
    </div>
  );
}

/** Worker-swarm hook for pre-formatted tabular exports (data-table and reuse). */
export function useReportSwarm() {
  const { timezone } = useTimezone();
  const { formatStr } = useTimeFormat();
  const [jobs, setJobs] = React.useState<ReadonlyMap<string, ReportJobState>>(
    new Map(),
  );
  const jobsRef = React.useMemo(
    () => ({ current: new Map<string, JobContext>() }),
    [],
  );
  const counterRef = React.useRef(0);

  const { data: activeOrganization } = authClient.useActiveOrganization();
  const orgThemeRef = React.useRef<OrgReportTheme | undefined>(undefined);

  const resolveOrgTheme = React.useCallback(async (): Promise<
    OrgReportTheme | undefined
  > => {
    if (!activeOrganization) {
      return undefined;
    }

    let logoUrl: string | null = null;
    if (activeOrganization.logo) {
      if (activeOrganization.logo.includes("://")) {
        logoUrl = activeOrganization.logo;
      } else {
        const logoResult = await Effect.runPromise(
          Effect.tryPromise({
            try: () =>
              orpc.organization.v1.logo.get.call({ id: activeOrganization.id }),
            catch: (e): Error =>
              e instanceof Error ? e : new Error("Failed to fetch logo"),
          }).pipe(Effect.catchAll(() => Effect.succeed(null))),
        );
        logoUrl = logoResult?.url ?? null;
      }
    }

    let primary = BRAND.primary;
    let secondary = BRAND.secondary;
    if (activeOrganization.metadata) {
      const parsed = Effect.runSync(
        Effect.try({
          try: () => JSON.parse(activeOrganization.metadata),
          catch: () => new Error("invalid metadata"),
        }).pipe(Effect.catchAll(() => Effect.succeed(null))),
      );
      if (
        parsed &&
        typeof parsed === "object" &&
        !Array.isArray(parsed) &&
        "reportPrimary" in parsed &&
        "reportSecondary" in parsed
      ) {
        const { reportPrimary, reportSecondary } = parsed;
        if (
          Array.isArray(reportPrimary) &&
          reportPrimary.length === 3 &&
          reportPrimary.every((v: unknown) => typeof v === "number")
        ) {
          primary = [reportPrimary[0], reportPrimary[1], reportPrimary[2]];
        }
        if (
          Array.isArray(reportSecondary) &&
          reportSecondary.length === 3 &&
          reportSecondary.every((v: unknown) => typeof v === "number")
        ) {
          secondary = [
            reportSecondary[0],
            reportSecondary[1],
            reportSecondary[2],
          ];
        }
      }
    }

    return {
      name: activeOrganization.name,
      logoUrl,
      primary,
      secondary,
    };
  }, [activeOrganization]);

  React.useEffect(() => {
    if (!activeOrganization) {
      orgThemeRef.current = undefined;
      return;
    }
    void resolveOrgTheme().then((theme) => {
      orgThemeRef.current = theme;
    });
  }, [activeOrganization, resolveOrgTheme]);

  const isGenerating = jobs.size > 0;

  const updateSnapshot = React.useCallback(() => {
    const snapshot = new Map<string, ReportJobState>();
    for (const [id, ctx] of jobsRef.current) {
      snapshot.set(id, {
        id,
        format: ctx.format,
        percent: ctx.percent,
        step: ctx.step,
      });
    }
    setJobs(snapshot);
  }, [jobsRef]);

  React.useEffect(() => {
    if (jobs.size === 0) {
      return undefined;
    }
    const intervalId = setInterval(() => {
      let changed = false;
      for (const ctx of jobsRef.current.values()) {
        if (ctx.percent < ctx.target || ctx.target < 60 || ctx.percent >= 99) {
          continue;
        }
        ctx.percent = Math.min(99, ctx.percent + 1);
        changed = true;
        toast.loading(ctx.step || "Gerando relatório…", {
          id: ctx.toastId,
          action: {
            label: "Cancelar",
            onClick: () => ctx.controller.abort(),
          },
        });
      }
      if (changed) {
        updateSnapshot();
      }
    }, 300);
    return () => clearInterval(intervalId);
  }, [jobs.size, updateSnapshot, jobsRef]);

  const abortJob = React.useCallback(
    (jobId: string) => {
      const ctx = jobsRef.current.get(jobId);
      if (ctx) {
        ctx.controller.abort();
      }
    },
    [jobsRef],
  );

  const enqueue = React.useCallback(
    (input: ReportJobInput) => {
      counterRef.current += 1;
      const jobId = String(counterRef.current);
      const controller = new AbortController();

      const loadingToast = toast.loading("Iniciando relatório…", {
        action: {
          label: "Cancelar",
          onClick: () => controller.abort(),
        },
      });
      jobsRef.current.set(jobId, {
        format: input.format,
        percent: 0,
        target: 0,
        step: "",
        toastId: loadingToast,
        controller,
      });
      updateSnapshot();

      const handleProgress = (percent: number, step: string) => {
        const ctx = jobsRef.current.get(jobId);
        if (!ctx) {
          return;
        }
        ctx.percent = percent;
        ctx.target = percent;
        ctx.step = step;
        toast.loading(step, {
          id: ctx.toastId,
          action: {
            label: "Cancelar",
            onClick: () => ctx.controller.abort(),
          },
        });
        updateSnapshot();
      };

      const program = Effect.tryPromise({
        try: async () => {
          const rows = await Effect.runPromise(
            Effect.tryPromise({
              try: () => input.fetchRows(),
              catch: (error) =>
                error instanceof Error
                  ? error
                  : new Error("Falha ao carregar dados do relatório"),
            }),
          );
          return spawnReportWorker(
            {
              format: input.format,
              title: input.title,
              columns: input.columns,
              rows,
              timezone,
              datetimeFormat: formatStr,
              orgTheme: orgThemeRef.current,
            },
            handleProgress,
            controller.signal,
          );
        },
        catch: (error) =>
          error instanceof Error
            ? error
            : new Error("Falha ao gerar relatório"),
      }).pipe(
        Effect.tap((blob) => {
          const ctx = jobsRef.current.get(jobId);
          if (ctx) {
            ctx.percent = 100;
            ctx.step = "Concluído";
            toast.loading("Concluído", {
              id: ctx.toastId,
              action: {
                label: "Cancelar",
                onClick: () => ctx.controller.abort(),
              },
            });
          }
          triggerDownload(blob, input.filename);
          toast.dismiss(loadingToast);
          toast.success("Relatório gerado com sucesso");
        }),
        Effect.catchAll((error) => {
          toast.dismiss(loadingToast);
          if (error instanceof ReportAbortedError) {
            toast.info("Geração cancelada");
          } else {
            const message =
              error instanceof Error
                ? error.message
                : "Falha ao gerar relatório";
            toast.error(message);
          }
          return Effect.void;
        }),
        Effect.tap(() => {
          jobsRef.current.delete(jobId);
          updateSnapshot();
        }),
      );

      void Effect.runPromise(program);
    },
    [timezone, formatStr, updateSnapshot, jobsRef, orgThemeRef],
  );

  return { enqueue, isGenerating, jobs, abortJob };
}
