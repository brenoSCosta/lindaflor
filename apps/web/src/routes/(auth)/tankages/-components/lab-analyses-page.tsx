import { labOilSampleTypeLabels } from "@lindaflor/shared/enums/tankage";
import type { LabOilAnalysisOutput } from "@lindaflor/shared/schemas/lab-oil-analysis";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createColumnHelper } from "@tanstack/react-table";
import { format, parseISO } from "date-fns";
import { ChevronLeft, FlaskConical, Loader2, Plus } from "lucide-react";
import React from "react";

import { Button, buttonVariants } from "@/components/ui/button";
import { DataTable } from "@/components/ui/data-table/core/data-table";
import { useDataTable } from "@/components/ui/data-table/core/use-data-table";
import { Time } from "@/components/ui/time";
import { Can, useAppAbility } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { LabAnalysisFormDialog } from "@/routes/(auth)/tankages/-components/lab-analysis-form-dialog";
import { TankSnapshotCard } from "@/routes/(auth)/tankages/-components/tank-snapshot-card";

interface LabAnalysesPageProps {
  tankId: string;
}

const columnHelper = createColumnHelper<LabOilAnalysisOutput>();

function useColumns() {
  return React.useMemo(
    () => [
      columnHelper.accessor("collected_at", {
        header: "Coleta",
        meta: { variant: "text" },
        size: 160,
        cell: ({ row }) => (
          <Time date={row.original.collected_at} formatStr="dd/MM/yyyy HH:mm" />
        ),
      }),
      columnHelper.accessor("sample_type", {
        header: "Amostra",
        meta: { variant: "text" },
        size: 130,
        cell: ({ getValue }) => labOilSampleTypeLabels[getValue()],
      }),
      columnHelper.accessor("density_at_20c", {
        header: "ρ₂₀ (kg/m³)",
        meta: { variant: "range", align: "right" },
        size: 120,
        cell: ({ getValue }) => (
          <span className="tabular-nums">{getValue().toFixed(1)}</span>
        ),
      }),
      columnHelper.accessor("water_and_sediment_percent", {
        header: "Água e sedimentos (%)",
        meta: { variant: "range", align: "right" },
        size: 100,
        cell: ({ getValue }) => (
          <span className="tabular-nums">{getValue().toFixed(2)}</span>
        ),
      }),
      columnHelper.accessor("certificate_number", {
        header: "Certificado",
        meta: { variant: "text" },
        size: 140,
      }),
      columnHelper.accessor("laboratory_name", {
        header: "Laboratório",
        meta: { variant: "text" },
        size: 160,
      }),
      columnHelper.accessor("issued_at", {
        header: "Emissão",
        meta: { variant: "text" },
        size: 120,
        cell: ({ getValue }) => {
          const value = getValue();
          return (
            <span className="tabular-nums">
              {format(parseISO(value), "dd/MM/yyyy")}
            </span>
          );
        },
      }),
    ],
    [],
  );
}

export function LabAnalysesPage({ tankId }: LabAnalysesPageProps) {
  const ability = useAppAbility();
  const canUpdate = ability.can("update", "LabOilAnalyses");
  const columns = useColumns();
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [selected, setSelected] = React.useState<LabOilAnalysisOutput | null>(
    null,
  );

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

  const {
    data: analyses = [],
    isPending: analysesPending,
    isError: analysesError,
    error: analysesErr,
  } = useQuery({
    ...orpc.labOilAnalyses.v1.listByTank.queryOptions({
      input: { tank_id: tankId },
      select: (result) => result.data,
    }),
  });

  const table = useDataTable({
    data: analyses,
    columns,
    isLoading: analysesPending,
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
    enableBottomToolbar: false,
    enableDensityToggle: false,
    enableFullscreenToggle: false,
    enableGrouping: false,
    getRowId: (row) => row.id,
    renderTopToolbar: () => (
      <div className="flex w-full flex-wrap items-start justify-between gap-3 py-1">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight">
            Análises de laboratório
          </h2>
          <p className="text-sm text-muted-foreground">
            Densidade a 20 °C e água e sedimentos vinculados a este tanque.
            {canUpdate ? " Clique para editar." : ""}
          </p>
        </div>
        <Can I="create" a="LabOilAnalyses">
          <Button
            size="sm"
            onClick={() => {
              setSelected(null);
              setDialogOpen(true);
            }}
          >
            <Plus className="size-4" />
            Nova análise
          </Button>
        </Can>
      </div>
    ),
    renderEmpty: () => (
      <div className="py-10 text-center text-sm text-muted-foreground">
        Nenhuma análise cadastrada
      </div>
    ),
    onRowClick: canUpdate
      ? ({ row }) => {
          setSelected(row.original);
          setDialogOpen(true);
        }
      : undefined,
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
      <div className="mx-auto max-w-5xl space-y-4 px-4 py-8">
        <Link
          to="/tankages"
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="size-4" />
          Tanques
        </Link>
        <p className="text-sm text-destructive">
          {tankErr?.message ?? "Tanque não encontrado"}
        </p>
      </div>
    );
  }

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8">
      <div className="space-y-3">
        <Link
          to="/tankages/$tankId"
          params={{ tankId }}
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="size-4" />
          {snapshot.tag}
        </Link>
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">
            Análises de laboratório
          </h1>
          <p className="text-sm text-muted-foreground">
            {snapshot.tag} · {snapshot.concession_name} ·{" "}
            {snapshot.installation_name}
          </p>
        </div>
      </div>

      <TankSnapshotCard snapshot={snapshot} layout="compact" />

      {analysesError ? (
        <p className="text-sm text-destructive">
          {analysesErr?.message ?? "Falha ao carregar análises"}
        </p>
      ) : (
        <DataTable table={table} />
      )}

      <LabAnalysisFormDialog
        open={dialogOpen}
        onOpenChange={(open) => {
          setDialogOpen(open);
          if (!open) setSelected(null);
        }}
        tankId={tankId}
        analysis={selected}
      />
    </div>
  );
}

export function LabAnalysesNavLink({ tankId }: { tankId: string }) {
  return (
    <Can I="read" a="LabOilAnalyses">
      <Link
        to="/tankages/$tankId/analises-laboratorio"
        params={{ tankId }}
        className={buttonVariants({ variant: "outline", size: "sm" })}
      >
        <FlaskConical className="size-4" />
        Análises de laboratório
      </Link>
    </Can>
  );
}
