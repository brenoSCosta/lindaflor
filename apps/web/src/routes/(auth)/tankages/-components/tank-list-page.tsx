import type { TankSnapshotOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";

import { Badge } from "@/components/ui/badge";
import { DataTable } from "@/components/ui/data-table/core/data-table";
import { useDataTable } from "@/components/ui/data-table/core/use-data-table";
import { orpc } from "@/lib/orpc";
import {
  formatStockM3OilBarrels,
  formatVolumeM3,
} from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { TankagesListBreadcrumbs } from "@/routes/(auth)/tankages/-components/tank-breadcrumbs";
import { formatTankHeightM } from "@/routes/(auth)/tankages/-components/tank-height-display";
import { TankListKpiStrip } from "@/routes/(auth)/tankages/-components/tank-list-kpi-strip";
import { TankagesPageShell } from "@/routes/(auth)/tankages/-components/tankages-page-shell";

const columns: ColumnDef<TankSnapshotOutput>[] = [
  {
    accessorKey: "tag",
    header: "TAG",
    meta: { variant: "text" },
    size: 120,
    minSize: 100,
    maxSize: 180,
    cell: ({ row }) => <Badge variant="outline">{row.original.tag}</Badge>,
  },
  {
    accessorKey: "concession_name",
    header: "Concessão",
    meta: { variant: "text" },
    size: 180,
    cell: ({ row }) => (
      <span className="text-muted-foreground">
        {row.original.concession_name}
      </span>
    ),
  },
  {
    accessorKey: "installation_name",
    header: "Instalação",
    meta: { variant: "text" },
    size: 200,
    cell: ({ row }) => (
      <span className="text-muted-foreground">
        {row.original.installation_name}
      </span>
    ),
  },
  {
    id: "capacity_volume_m3",
    header: "Capacidade",
    meta: { variant: "range", align: "right" },
    size: 120,
    cell: ({ row }) => (
      <div className="text-right">
        <span className="tabular-nums text-muted-foreground">
          {row.original.capacity_volume_m3 != null
            ? formatVolumeM3(row.original.capacity_volume_m3)
            : "—"}
        </span>
        {row.original.capacity_height_m != null ? (
          <p className="text-xs tabular-nums text-muted-foreground/80">
            máx. arqueação {formatTankHeightM(row.original.capacity_height_m)}
          </p>
        ) : null}
      </div>
    ),
  },
  {
    id: "current_stock",
    header: "Estoque (m³ / barris)",
    meta: { variant: "range", align: "right" },
    size: 200,
    cell: ({ row }) => (
      <span className="tabular-nums text-muted-foreground">
        {formatStockM3OilBarrels(
          row.original.current_volume_m3,
          row.original.current_net_oil_volume_m3_20c,
          row.original.current_volume_oil_barrels,
        )}
      </span>
    ),
  },
  {
    id: "today_production",
    header: "Produção hoje (m³ / barris)",
    meta: { variant: "range", align: "right" },
    size: 200,
    cell: ({ row }) => (
      <span className="tabular-nums text-muted-foreground">
        {formatStockM3OilBarrels(
          row.original.today_production_gross_volume_m3,
          row.original.today_production_net_oil_volume_m3_20c,
          row.original.today_production_volume_oil_barrels,
        )}
      </span>
    ),
  },
  {
    accessorKey: "measurement_equipment_code",
    header: "Trena",
    meta: { variant: "text" },
    size: 100,
    cell: ({ row }) => (
      <span className="text-muted-foreground">
        {row.original.measurement_equipment_code ?? "—"}
      </span>
    ),
  },
];

export function TankagesTankListPage() {
  const navigate = useNavigate();

  const {
    data: { data: tanks = [] } = {},
    isPending,
    isError,
    error,
  } = useQuery(
    orpc.tanks.v1.tank.list.snapshot.queryOptions({
      select: (result) => result,
    }),
  );

  const table = useDataTable({
    data: tanks,
    columns,
    getRowId: (row) => row.id,
    isLoading: isPending,
    enableGlobalFilter: true,
    enableColumnOrdering: true,
    enableColumnResizing: true,
    enableRowNumbers: false,
    enableRowSelection: false,
    enablePagination: true,
    enableExport: false,
    enableTopToolbar: true,
    enableBottomToolbar: true,
    enableDensityToggle: false,
    enableFullscreenToggle: false,
    enableColumnFilterModes: false,
    enableAdvancedFilter: false,
    enableGrouping: false,
    initialState: { pagination: { pageSize: 20 } },
    onRowClick: ({ row }) => {
      void navigate({
        to: "/tankages/$tankId",
        params: { tankId: row.original.id },
      });
    },
    renderEmpty: () => (
      <div className="py-16 text-center text-sm text-muted-foreground">
        Nenhum tanque encontrado
      </div>
    ),
  });

  if (isError) {
    return (
      <TankagesPageShell>
        <TankagesListBreadcrumbs />
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Gestão de tanques
          </h1>
          <p className="text-sm text-muted-foreground">
            Selecione um tanque para ver as produções por dia.
          </p>
        </div>
        <p className="text-sm text-destructive">
          {error?.message ?? "Falha ao carregar tanques"}
        </p>
      </TankagesPageShell>
    );
  }

  return (
    <TankagesPageShell>
      <TankagesListBreadcrumbs />
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          Gestão de tanques
        </h1>
        <p className="max-w-3xl text-sm text-muted-foreground">
          Capacidade, estoque atual e produção do dia operacional (m³ e barris
          quando houver análise de laboratório). Selecione um tanque para ver os
          boletins diários.
        </p>
      </div>

      {!isPending && tanks.length > 0 ? (
        <TankListKpiStrip tanks={tanks} />
      ) : null}

      <div className="min-w-0 rounded-xl border bg-card shadow-sm">
        <DataTable table={table} />
      </div>
    </TankagesPageShell>
  );
}
