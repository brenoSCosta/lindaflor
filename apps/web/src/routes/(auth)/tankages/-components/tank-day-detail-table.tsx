import { subject } from "@lindaflor/shared/lib/ability/subjects";
import {
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";
import {
  schema,
  tankageBoundsFromDayRows,
  withTankageMeasurementValidation,
  type TankageOutput,
  type TankageTimeWindow,
} from "@lindaflor/shared/schemas/tankage/tankages";
import type { TankTransferOutput } from "@lindaflor/shared/schemas/tankage/transfers";
import { useMutation } from "@tanstack/react-query";
import type { Row } from "@tanstack/react-table";
import { createColumnHelper } from "@tanstack/react-table";
import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import { Button } from "@/components/ui/button";
import { DataTablePagination } from "@/components/ui/data-table/components/toolbar/data-table-pagination";
import { DataTable } from "@/components/ui/data-table/core/data-table";
import { useDataTable } from "@/components/ui/data-table/core/use-data-table";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { Time } from "@/components/ui/time";
import { useTimezone } from "@/context/timezone";
import { useAppAbility } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { formatMeasurementNumber } from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { invalidateTankDay } from "@/routes/(auth)/tankages/-components/invalidate-tank-day";
import { TankTransferRetreatDialog } from "@/routes/(auth)/tankages/-components/tank-transfer-retreat-dialog";
import { TankageRetreatDialog } from "@/routes/(auth)/tankages/-components/tankage-retreat-dialog";

const columnHelper = createColumnHelper<TankageOutput>();

const ALL_HOURS = Array.from({ length: 24 }, (_, index) =>
  String(index).padStart(2, "0"),
);
const ALL_MINUTES = Array.from({ length: 60 }, (_, index) =>
  String(index).padStart(2, "0"),
);

const draftFieldsSchema = z.object({
  measured_at: z.date({ error: "Data/hora inválida" }),
  current_measurement: schema.create.input.shape.current_measurement,
  oil_temperature_c: schema.create.input.shape.oil_temperature_c,
  ambient_temperature_c: schema.create.input.shape.ambient_temperature_c,
  observation: schema.create.input.shape.observation,
});

interface TankDayDetailTableProps {
  tankId: string;
  operationalDay: string;
  rows: TankageOutput[];
  isLoading: boolean;
  /** Tankage ids created by a transferência — excluded from the production quota. */
  transferTankageIds?: ReadonlySet<string>;
  transfersByTankageId?: ReadonlyMap<string, TankTransferOutput>;
  capacityHeightM?: number | null;
  bulletinApproved?: boolean;
}

type TankageUpdateInput = {
  id: string;
  measured_at?: Date;
  current_measurement?: number;
  oil_temperature_c?: number;
  ambient_temperature_c?: number;
  observation?: string;
};

function MeasuredAtDisplay({ measuredAt }: { measuredAt: Date }) {
  return <Time date={measuredAt} formatStr="HH:mm" />;
}

function NumericCell({ value, digits }: { value: number; digits: number }) {
  return <span className="tabular-nums">{value.toFixed(digits)}</span>;
}

function NullableNumericCell({
  value,
  digits,
}: {
  value: number | null;
  digits: number;
}) {
  return (
    <span className="tabular-nums text-muted-foreground">
      {formatMeasurementNumber(value, digits)}
    </span>
  );
}

function ObservationCell({ value }: { value: string }) {
  return <span className="max-w-48 truncate">{value || "—"}</span>;
}

function TankDayDetailEmpty() {
  return (
    <div className="py-12 text-center text-sm text-muted-foreground">
      Nenhuma medida neste dia ainda.
    </div>
  );
}

function minutesOfDay(date: Date, timezone: string): number {
  const parts = zonedParts(date, timezone);
  return Number(parts.hour) * 60 + Number(parts.minute);
}

function allowedClockOptions(args: {
  operationalDay: string;
  timezone: string;
  timeWindow: TankageTimeWindow;
}): { hours: string[]; minutesForHour: (hour: string) => string[] } {
  const { operationalDay, timezone, timeWindow } = args;
  const dayStart = zonedDateTimeToUtc(operationalDay, "00", "00", timezone);
  const dayEnd = zonedDateTimeToUtc(operationalDay, "23", "59", timezone);

  const minExclusive =
    timeWindow.previous_measured_at != null &&
    timeWindow.previous_measured_at.getTime() >= dayStart.getTime()
      ? minutesOfDay(timeWindow.previous_measured_at, timezone)
      : -1;
  const maxExclusive =
    timeWindow.next_measured_at != null &&
    timeWindow.next_measured_at.getTime() <= dayEnd.getTime() + 60_000
      ? minutesOfDay(timeWindow.next_measured_at, timezone)
      : 24 * 60;

  const hours = ALL_HOURS.filter((hour) => {
    const hourStart = Number(hour) * 60;
    const hourEnd = hourStart + 59;
    return hourEnd > minExclusive && hourStart < maxExclusive;
  });

  return {
    hours,
    minutesForHour: (hour: string) => {
      const hourStart = Number(hour) * 60;
      return ALL_MINUTES.filter((minute) => {
        const total = hourStart + Number(minute);
        return total > minExclusive && total < maxExclusive;
      });
    },
  };
}

function buildUpdatePayloadFromDraft(
  row: Row<TankageOutput>,
  values: Record<string, unknown>,
  args: {
    rows: TankageOutput[];
    capacityHeightM: number | null;
    isTransfer: boolean;
  },
): TankageUpdateInput | null {
  const original = row.original;

  const draftSchema = withTankageMeasurementValidation(
    draftFieldsSchema,
    (value) =>
      tankageBoundsFromDayRows({
        rows: args.rows,
        measuredAt: value.measured_at,
        capacityHeightM: args.capacityHeightM,
        excludeId: original.id,
      }),
    { allowDecrease: args.isTransfer },
  );

  const parsed = draftSchema.safeParse({
    measured_at: values.measured_at,
    current_measurement:
      typeof values.current_measurement === "number"
        ? values.current_measurement
        : Number(values.current_measurement),
    oil_temperature_c:
      typeof values.oil_temperature_c === "number"
        ? values.oil_temperature_c
        : Number(values.oil_temperature_c),
    ambient_temperature_c:
      typeof values.ambient_temperature_c === "number"
        ? values.ambient_temperature_c
        : Number(values.ambient_temperature_c),
    observation:
      typeof values.observation === "string" ? values.observation : "",
  });

  if (!parsed.success) {
    toast.error(parsed.error.issues[0]?.message ?? "Revise os campos");
    return null;
  }

  const payload: TankageUpdateInput = { id: original.id };
  const { measured_at, current_measurement, observation } = parsed.data;

  if (measured_at.getTime() !== original.measured_at.getTime()) {
    payload.measured_at = measured_at;
  }
  if (current_measurement !== original.current_measurement) {
    payload.current_measurement = current_measurement;
  }
  for (const key of ["oil_temperature_c", "ambient_temperature_c"] as const) {
    if (parsed.data[key] !== original[key]) {
      payload[key] = parsed.data[key];
    }
  }
  if (observation !== original.observation) {
    payload.observation = observation;
  }

  return payload;
}

function buildColumns(args: {
  canUpdate: boolean;
  operationalDay: string;
  timezone: string;
  rows: TankageOutput[];
  capacityHeightM: number | null;
}) {
  const { canUpdate, operationalDay, timezone, rows, capacityHeightM } = args;

  return [
    columnHelper.accessor("measured_at", {
      header: "Hora",
      meta: {
        variant: "text",
        enableEditing: canUpdate,
        align: "right",
        renderEditCell: ({ cell, table }) => {
          const { rowDraft, setRowDraftValue, cancelEdit } =
            table.tableInstance;
          const raw = rowDraft.measured_at ?? cell.getValue();
          const measuredAt = raw instanceof Date ? raw : new Date();
          const row = cell.row.original;
          const bounds = tankageBoundsFromDayRows({
            rows,
            measuredAt,
            capacityHeightM,
            excludeId: row.id,
          });
          return (
            <MeasuredAtEditCell
              measuredAt={measuredAt}
              operationalDay={operationalDay}
              timezone={timezone}
              timeWindow={bounds}
              onChange={(date) => setRowDraftValue("measured_at", date)}
              onCancel={cancelEdit}
            />
          );
        },
      },
      size: 140,
      cell: ({ row }) => (
        <MeasuredAtDisplay measuredAt={row.original.measured_at} />
      ),
    }),
    columnHelper.accessor("current_measurement", {
      header: "Altura (m)",
      meta: {
        variant: "range",
        align: "right",
        editVariant: "number",
        enableEditing: canUpdate,
        validate: (value) => {
          const n = typeof value === "number" ? value : Number(value);
          if (Number.isNaN(n) || n < 0) return "Altura inválida";
          if (capacityHeightM != null && n > capacityHeightM) {
            return `Máximo ${capacityHeightM.toFixed(3)} m`;
          }
          return undefined;
        },
      },
      size: 110,
      cell: ({ getValue }) => <NumericCell value={getValue()} digits={3} />,
    }),
    columnHelper.accessor("oil_temperature_c", {
      header: "Temp. Óleo (°C)",
      meta: {
        variant: "range",
        align: "right",
        editVariant: "number",
        enableEditing: canUpdate,
      },
      size: 110,
      cell: ({ getValue }) => <NumericCell value={getValue()} digits={1} />,
    }),
    columnHelper.accessor("ambient_temperature_c", {
      header: "Temp. Amb. (°C)",
      meta: {
        variant: "range",
        align: "right",
        editVariant: "number",
        enableEditing: canUpdate,
      },
      size: 110,
      cell: ({ getValue }) => <NumericCell value={getValue()} digits={1} />,
    }),
    columnHelper.accessor("observation", {
      header: "Observação",
      meta: {
        variant: "text",
        editVariant: "text",
        enableEditing: canUpdate,
        validate: (value) => {
          const text = typeof value === "string" ? value.trim() : "";
          if (text.length === 0) return "Informe a observação";
          return undefined;
        },
      },
      size: 200,
      cell: ({ getValue }) => <ObservationCell value={getValue()} />,
    }),
  ];
}

export function TankDayDetailTable({
  tankId,
  operationalDay,
  rows,
  isLoading,
  transferTankageIds,
  transfersByTankageId,
  capacityHeightM = null,
  bulletinApproved = false,
}: TankDayDetailTableProps) {
  const { timezone } = useTimezone();
  const ability = useAppAbility();
  const [retreatTankage, setRetreatTankage] =
    React.useState<TankageOutput | null>(null);
  const [retreatTransfer, setRetreatTransfer] =
    React.useState<TankTransferOutput | null>(null);

  const canUpdate =
    rows.length > 0
      ? ability.can("update", subject("Tankages", rows[0]))
      : false;

  const canRetreatProduction =
    bulletinApproved &&
    rows.some(
      (row) =>
        !(transferTankageIds?.has(row.id) ?? false) &&
        ability.can("retreat", subject("Tankages", row)),
    );
  const canRetreatTransfer =
    bulletinApproved &&
    transfersByTankageId != null &&
    [...transfersByTankageId.values()].some((transfer) =>
      ability.can("retreat", subject("TankTransfers", transfer)),
    );
  const showRetreatActions = canRetreatProduction || canRetreatTransfer;

  const { mutateAsync, isPending } = useMutation(
    orpc.tanks.v1.tankage.update.mutationOptions({
      onSuccess: async () => {
        await invalidateTankDay({ tankId, operationalDay });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const columns = React.useMemo(
    () =>
      buildColumns({
        canUpdate,
        operationalDay,
        timezone,
        rows,
        capacityHeightM,
      }),
    [canUpdate, capacityHeightM, operationalDay, rows, timezone],
  );

  const table = useDataTable({
    data: rows,
    columns,
    getRowId: (row) => row.id,
    isLoading,
    isSaving: isPending,
    enableEditing: canUpdate,
    editDisplayMode: "row",
    enablePagination: rows.length > 10,
    enableTopToolbar: false,
    enableGlobalFilter: false,
    enableColumnFilterModes: false,
    enableAdvancedFilter: false,
    enableColumnOrdering: false,
    enableColumnResizing: false,
    enableRowNumbers: false,
    enableRowSelection: false,
    enableExport: false,
    enableDensityToggle: false,
    enableFullscreenToggle: false,
    enableGrouping: false,
    onSaveRow: ({ row, values, exit }) => {
      void (async () => {
        const isTransfer = transferTankageIds?.has(row.original.id) ?? false;
        const payload = buildUpdatePayloadFromDraft(row, values, {
          rows,
          capacityHeightM,
          isTransfer,
        });
        if (payload === null) return;
        const changedKeys = Object.keys(payload).filter((key) => key !== "id");
        if (changedKeys.length === 0) {
          toast.info("Nenhuma alteração");
          exit();
          return;
        }
        await mutateAsync(payload);
        toast.success("Medição atualizada");
        exit();
      })();
    },
    renderRowActions: showRetreatActions
      ? ({ row }) => {
          const isTransfer = transferTankageIds?.has(row.original.id) ?? false;
          if (isTransfer) {
            const transfer = transfersByTankageId?.get(row.original.id);
            if (transfer == null) return null;
            if (!ability.can("retreat", subject("TankTransfers", transfer))) {
              return null;
            }
            return (
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-8 whitespace-nowrap"
                onClick={() => setRetreatTransfer(transfer)}
              >
                Retratamento
              </Button>
            );
          }
          if (!ability.can("retreat", subject("Tankages", row.original))) {
            return null;
          }
          return (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="h-8 whitespace-nowrap"
              onClick={() => setRetreatTankage(row.original)}
            >
              Retratamento
            </Button>
          );
        }
      : undefined,
    renderDetailPanel: ({ row }) => <DetailPanel row={row.original} />,
    renderBottomToolbar: ({ table: dayTable }) => (
      <div className="px-4 py-2">
        <DataTablePagination table={dayTable} />
      </div>
    ),
    renderEmpty: () => <TankDayDetailEmpty />,
  });

  return (
    <>
      <DataTable table={table} />
      <TankageRetreatDialog
        open={retreatTankage != null}
        onOpenChange={(open) => {
          if (!open) setRetreatTankage(null);
        }}
        tankage={retreatTankage}
        day={operationalDay}
        dayRows={rows}
        capacityHeightM={capacityHeightM}
      />
      <TankTransferRetreatDialog
        open={retreatTransfer != null}
        onOpenChange={(open) => {
          if (!open) setRetreatTransfer(null);
        }}
        transfer={retreatTransfer}
        day={operationalDay}
      />
    </>
  );
}

function MeasuredAtEditCell({
  measuredAt,
  operationalDay,
  timezone,
  timeWindow,
  onChange,
  onCancel,
}: {
  measuredAt: Date;
  operationalDay: string;
  timezone: string;
  timeWindow: TankageTimeWindow;
  onChange: (date: Date) => void;
  onCancel: () => void;
}) {
  const parts = zonedParts(measuredAt, timezone);
  const hour = parts.hour;
  const minute = parts.minute;
  const { hours, minutesForHour } = allowedClockOptions({
    operationalDay,
    timezone,
    timeWindow,
  });
  const minutes = minutesForHour(hour);
  const hourOptions = hours.includes(hour) ? hours : [hour, ...hours];
  const minuteOptions = minutes.includes(minute)
    ? minutes
    : [minute, ...minutes];

  return (
    <div
      className="flex gap-1"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          onCancel();
        }
      }}
    >
      <NativeSelect
        className="h-8 min-w-14 text-xs"
        value={hour}
        aria-label="Hora"
        autoFocus
        onChange={(e) => {
          const nextHour = e.target.value;
          const nextMinutes = minutesForHour(nextHour);
          const nextMinute = nextMinutes.includes(minute)
            ? minute
            : (nextMinutes[0] ?? "00");
          onChange(
            zonedDateTimeToUtc(operationalDay, nextHour, nextMinute, timezone),
          );
        }}
      >
        {hourOptions.map((h) => (
          <NativeSelectOption key={h} value={h}>
            {h}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      <NativeSelect
        className="h-8 min-w-14 text-xs"
        value={minute}
        aria-label="Minuto"
        onChange={(e) => {
          onChange(
            zonedDateTimeToUtc(operationalDay, hour, e.target.value, timezone),
          );
        }}
      >
        {minuteOptions.map((m) => (
          <NativeSelectOption key={m} value={m}>
            {m}
          </NativeSelectOption>
        ))}
      </NativeSelect>
    </div>
  );
}

function DetailPanel({ row }: { row: TankageOutput }) {
  return (
    <div className="grid grid-cols-2 gap-x-8 gap-y-2 sm:grid-cols-3 lg:grid-cols-4">
      <DetailField label="Vol Óleo 20°C (m³)">
        <NullableNumericCell value={row.net_oil_volume_m3_20c} digits={5} />
      </DetailField>
      <DetailField label="Vol Bruto 20°C (m³)">
        <NullableNumericCell value={row.gross_volume_m3_20c} digits={5} />
      </DetailField>
      <DetailField label="Vol Bruto Amb. (m³)">
        <NullableNumericCell value={row.gross_volume_m3} digits={5} />
      </DetailField>
      <DetailField label="CTL">
        <NullableNumericCell value={row.liquid_correction_factor} digits={5} />
      </DetailField>
      <DetailField label="CTSH">
        <NullableNumericCell value={row.shell_correction_factor} digits={5} />
      </DetailField>
      <DetailField label="BSW (%)">
        <NullableNumericCell
          value={row.water_and_sediment_percent}
          digits={5}
        />
      </DetailField>
      <DetailField label="Dens. 20°C (kg/m³)">
        <NullableNumericCell value={row.density_at_20c_kg_m3} digits={2} />
      </DetailField>
      <DetailField label="Operador">
        <span>{row.operator_name || "—"}</span>
      </DetailField>
      <DetailField label="Última alteração">
        <span className="tabular-nums">
          <Time date={row.updated_at} formatStr="dd/MM/yyyy HH:mm" />
        </span>
      </DetailField>
    </div>
  );
}

function DetailField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <span className="text-xs text-muted-foreground">{label}:</span>{" "}
      <span className="text-xs">{children}</span>
    </div>
  );
}
