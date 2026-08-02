import type { TankDaySummaryOutput } from "@lindaflor/shared/schemas/tankage/day-summaries";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { createColumnHelper } from "@tanstack/react-table";
import { formatInTimeZone } from "date-fns-tz";
import { Loader2, Plus } from "lucide-react";
import React from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DataTablePagination } from "@/components/ui/data-table/components/toolbar/data-table-pagination";
import { DataTable } from "@/components/ui/data-table/core/data-table";
import { useDataTable } from "@/components/ui/data-table/core/use-data-table";
import { Time } from "@/components/ui/time";
import { useTimezone } from "@/context/timezone";
import { Can } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { formatVolumeM3 } from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { LabAnalysesNavLink } from "@/routes/(auth)/tankages/-components/lab-analyses-page";
import { TankDetailBreadcrumbs } from "@/routes/(auth)/tankages/-components/tank-breadcrumbs";
import { formatTankHeightAtualMaxColumn } from "@/routes/(auth)/tankages/-components/tank-height-display";
import { TankSnapshotCard } from "@/routes/(auth)/tankages/-components/tank-snapshot-card";
import { TankageEntryDialog } from "@/routes/(auth)/tankages/-components/tankage-entry-dialog";
import { TankagesPageShell } from "@/routes/(auth)/tankages/-components/tankages-page-shell";

interface TankDaysPageProps {
  tankId: string;
}

const columnHelper = createColumnHelper<TankDaySummaryOutput>();

function useDayColumns(todayKey: string, maxHeightM: number | null) {
  return React.useMemo(
    () => [
      columnHelper.accessor("last_measured_at", {
        header: "Data",
        meta: { variant: "text" },
        size: 180,
        minSize: 160,
        cell: ({ row }) => {
          const dayKey = row.original.operational_day;
          const isToday = dayKey === todayKey;
          return (
            <span className="inline-flex items-center gap-2 font-medium tabular-nums">
              <Time
                date={row.original.last_measured_at}
                formatStr="dd/MM/yyyy HH:mm"
              />
              {isToday ? <Badge>Hoje</Badge> : null}
            </span>
          );
        },
      }),
      columnHelper.accessor("bulletin_status", {
        header: "Boletim",
        meta: { variant: "select" },
        size: 120,
        minSize: 110,
        cell: ({ getValue }) => {
          const status = getValue();
          return (
            <Badge variant={status === "approved" ? "default" : "secondary"}>
              {status === "approved" ? "Aprovado" : "Aberto"}
            </Badge>
          );
        },
      }),
      columnHelper.accessor("production_gross_volume_m3", {
        header: "Volume bruto",
        meta: { variant: "range", align: "right" },
        size: 150,
        minSize: 130,
        cell: ({ getValue }) => {
          const volumeM3 = getValue();
          if (volumeM3 == null) {
            return <span className="text-muted-foreground">—</span>;
          }
          return (
            <span className="tabular-nums">{formatVolumeM3(volumeM3)}</span>
          );
        },
      }),
      columnHelper.accessor("last_current_measurement", {
        header: "Altura atual / máx. (m)",
        meta: { variant: "range", align: "right" },
        size: 170,
        minSize: 150,
        cell: ({ getValue }) => (
          <span className="tabular-nums">
            {formatTankHeightAtualMaxColumn(getValue(), maxHeightM)}
          </span>
        ),
      }),
      columnHelper.accessor("last_operator_name", {
        header: "Operador",
        meta: { variant: "text" },
        size: 180,
        minSize: 120,
        cell: ({ getValue }) => (
          <span className="text-muted-foreground">{getValue()}</span>
        ),
      }),
    ],
    [todayKey, maxHeightM],
  );
}

export function TankDaysPage({ tankId }: TankDaysPageProps) {
  const { timezone } = useTimezone();
  const todayKey = formatInTimeZone(new Date(), timezone, "yyyy-MM-dd");
  const navigate = useNavigate();
  const [entryOpen, setEntryOpen] = React.useState(false);

  const {
    data: snapshot,
    isPending: tankPending,
    isError: tankError,
    error: tankErr,
  } = useQuery({
    ...orpc.tanks.v1.tank.get.snapshot.queryOptions({
      input: { id: tankId },
    }),
  });

  const columns = useDayColumns(todayKey, snapshot?.capacity_height_m ?? null);

  const {
    data: daySummaries = [],
    isPending: daysPending,
    isError: daysError,
    error: daysErr,
  } = useQuery({
    ...orpc.tanks.v1.summary.listBy.tank.queryOptions({
      input: { tank_id: tankId },
      select: (result) => result.data,
    }),
  });

  const table = useDataTable({
    data: daySummaries,
    columns,
    getRowId: (row) => row.operational_day,
    isLoading: daysPending,
    enableGlobalFilter: true,
    enableColumnFilterModes: false,
    enableAdvancedFilter: false,
    enableColumnOrdering: false,
    enableColumnResizing: false,
    enableRowNumbers: false,
    enableRowSelection: false,
    enablePagination: true,
    enableExport: false,
    enableTopToolbar: true,
    enableToolbarInternalActions: false,
    enableColumnActions: false,
    enableDensityToggle: false,
    enableFullscreenToggle: false,
    enableGrouping: false,
    renderTopToolbar: () => (
      <div className="flex w-full flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
        <div className="space-y-0.5">
          <h2 className="text-base font-semibold tracking-tight">
            Boletins diários
          </h2>
          <p className="text-sm text-muted-foreground">
            Clique em uma linha para abrir o boletim do dia.
          </p>
        </div>
        <Can I="create" a="Tankages">
          <Button size="sm" onClick={() => setEntryOpen(true)}>
            <Plus className="size-4" />
            Nova medição
          </Button>
        </Can>
      </div>
    ),
    renderBottomToolbar: ({ table: dayTable }) => (
      <div className="px-4 py-2">
        <DataTablePagination table={dayTable} />
      </div>
    ),
    renderEmpty: () => (
      <div className="py-10 text-center text-sm text-muted-foreground">
        Nenhuma produção registrada
      </div>
    ),
    onRowClick: ({ row }) => {
      void navigate({
        to: "/tankages/$tankId/$date",
        params: {
          tankId,
          date: row.original.operational_day,
        },
      });
    },
  });

  if (tankPending) {
    return (
      <div className="flex justify-center py-16 text-muted-foreground">
        <Loader2 className="size-5 animate-spin" />
      </div>
    );
  }

  if (tankError || snapshot == null) {
    return (
      <TankagesPageShell>
        <TankDetailBreadcrumbs tag="Tanque" />
        <p className="text-sm text-destructive">
          {tankErr?.message ?? "Tanque não encontrado"}
        </p>
      </TankagesPageShell>
    );
  }

  return (
    <TankagesPageShell>
      <div className="space-y-3">
        <TankDetailBreadcrumbs tag={snapshot.tag} />
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">
              {snapshot.tag}
            </h1>
            <p className="text-sm text-muted-foreground">
              Produções e boletins diários · {snapshot.concession_name} ·{" "}
              {snapshot.installation_name}
            </p>
          </div>
          <LabAnalysesNavLink tankId={tankId} />
        </div>
      </div>

      <TankSnapshotCard snapshot={snapshot} layout="dashboard" />

      {daysError ? (
        <p className="text-sm text-destructive">
          {daysErr?.message ?? "Falha ao carregar produções"}
        </p>
      ) : (
        <div className="min-w-0 overflow-hidden rounded-xl border bg-card shadow-sm">
          <DataTable
            table={table}
            className="gap-0"
            surfaceClassName="rounded-none border-0"
          />
        </div>
      )}

      <TankageEntryDialog
        open={entryOpen}
        onOpenChange={setEntryOpen}
        tankId={tankId}
        capacityHeightM={snapshot.capacity_height_m}
        fallbackPreviousHeightM={snapshot.current_height_m}
      />
    </TankagesPageShell>
  );
}
