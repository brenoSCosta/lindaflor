import type { TankCalibrationListItem } from "@lindaflor/shared/schemas/tankage/calibrations";
import { useMutation } from "@tanstack/react-query";
import { Download, Info, Plus, Save, Trash2, Upload } from "lucide-react";
import React from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { orpc, queryClient } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import {
  type CalibrationEditablePoint,
  newCalibrationPoint,
  toEditablePoints,
} from "@/routes/(auth)/arqueacao/-components/calibration-points";
import {
  ALL_HEIGHT_BANDS_KEY,
  buildHeightBands,
  heightBandKey,
  heightBounds,
  rangeForBandKey,
  resolveSelectedBandKey,
} from "@/routes/(auth)/arqueacao/-components/height-range-filter";
import { invalidateTankCalibrations } from "@/routes/(auth)/arqueacao/-components/invalidate-tank-calibrations";
import {
  downloadCalibrationCsvExample,
  parseCalibrationCsv,
} from "@/routes/(auth)/arqueacao/-components/parse-calibration-csv";

interface CalibrationPointsEditorProps {
  calibrationId: string;
  tankId: string;
  points: CalibrationEditablePoint[];
  canManage: boolean;
  readOnlyHint?: boolean;
  onChange: (points: CalibrationEditablePoint[]) => void;
}

function formatDelta(current: number, previous: number | null): string {
  if (previous == null) return "—";
  const delta = current - previous;
  return delta.toLocaleString("pt-BR", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 3,
    signDisplay: "exceptZero",
  });
}

export function CalibrationPointsEditor({
  calibrationId,
  tankId,
  points,
  canManage,
  readOnlyHint = false,
  onChange,
}: CalibrationPointsEditorProps) {
  const fileInputRef = React.useRef<HTMLInputElement>(null);
  const bounds = heightBounds(points);
  const fullRange: [number, number] = [bounds.min, bounds.max];
  const heightBands = buildHeightBands(bounds.min, bounds.max);

  const [selectedBandKey, setSelectedBandKey] =
    React.useState(ALL_HEIGHT_BANDS_KEY);

  const effectiveBandKey = resolveSelectedBandKey(selectedBandKey, heightBands);
  const [rangeFrom, rangeTo] = rangeForBandKey(
    effectiveBandKey,
    heightBands,
    fullRange,
  );

  const heightBandSelectItems = {
    [ALL_HEIGHT_BANDS_KEY]: "Todos",
    ...Object.fromEntries(
      heightBands.map((band) => [
        heightBandKey(band.from, band.to),
        band.label,
      ]),
    ),
  };

  const replacePointsMutation = useMutation(
    orpc.tanks.v1.calibration.replace.point.mutationOptions({
      onSuccess: async (updated) => {
        toast.success("Tabela de arqueação salva");
        onChange(toEditablePoints(updated.points));
        queryClient.setQueryData(
          orpc.tanks.v1.calibration.getBy.id.key({
            input: { id: updated.id },
          }),
          updated,
        );
        queryClient.setQueryData(
          orpc.tanks.v1.calibration.listBy.tank.key({
            input: { tank_id: tankId },
          }),
          (prev: { data: TankCalibrationListItem[] } | undefined) => {
            if (prev == null) return prev;
            return {
              data: prev.data.map((item) =>
                item.id === updated.id
                  ? { ...item, points_count: updated.points.length }
                  : item,
              ),
            };
          },
        );
        await invalidateTankCalibrations(tankId, updated.id);
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const filteredPoints = React.useMemo(
    () =>
      points.filter(
        (point) => point.height_cm >= rangeFrom && point.height_cm <= rangeTo,
      ),
    [points, rangeFrom, rangeTo],
  );

  const addRow = () => {
    const last = points[points.length - 1];
    const nextHeight = last == null ? 1 : last.height_cm + 1;
    onChange([
      ...points,
      newCalibrationPoint(nextHeight, last?.volume_m3 ?? 0.001),
    ]);
  };

  const updatePoint = (
    id: string,
    patch: Partial<Pick<CalibrationEditablePoint, "height_cm" | "volume_m3">>,
  ) => {
    onChange(points.map((p) => (p.id === id ? { ...p, ...patch } : p)));
  };

  const removePoint = (id: string) => {
    onChange(points.filter((p) => p.id !== id));
  };

  const handleImportCsv = (file: File) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => {
      const text = typeof reader.result === "string" ? reader.result : "";
      const parsed = parseCalibrationCsv(text);
      if (parsed.error) {
        toast.error(parsed.error);
        return;
      }
      setSelectedBandKey(ALL_HEIGHT_BANDS_KEY);
      onChange(toEditablePoints(parsed.points));
      toast.success(`${parsed.points.length} pontos importados`);
    });
    reader.readAsText(file);
  };

  const handleSave = () => {
    const heights = new Set(points.map((p) => p.height_cm));
    if (heights.size !== points.length) {
      toast.error("Alturas duplicadas na tabela de arqueação");
      return;
    }
    if (points.some((point) => point.height_cm <= 0 || point.volume_m3 <= 0)) {
      toast.error("Altura e volume devem ser maiores que zero");
      return;
    }
    replacePointsMutation.mutate({
      id: calibrationId,
      points: points.map(({ height_cm, volume_m3 }) => ({
        height_cm,
        volume_m3,
      })),
    });
  };

  const gridCols = canManage
    ? "grid-cols-[minmax(0,1.4fr)_1fr_1fr_2.5rem]"
    : "grid-cols-[minmax(0,1.4fr)_1fr_1fr]";

  const isSaving = replacePointsMutation.isPending;

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden rounded-xl bg-card ring-1 ring-foreground/10">
      <div className="shrink-0 border-b p-4">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0 space-y-3">
            <div className="space-y-0.5">
              <h3 className="text-sm font-semibold text-foreground">
                Tabela de arqueação
              </h3>
              {readOnlyHint ? (
                <p className="text-xs text-muted-foreground">
                  Certificado expirado. Somente leitura.
                </p>
              ) : null}
              <p className="text-xs text-muted-foreground">
                Altura em centímetros e volume acumulado em m³
              </p>
            </div>
            {canManage ? (
              <>
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    ref={fileInputRef}
                    type="file"
                    accept=".csv,text/csv"
                    className="hidden"
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      if (file) handleImportCsv(file);
                      e.target.value = "";
                    }}
                  />
                  <Button
                    type="button"
                    size="sm"
                    variant="default"
                    onClick={() => fileInputRef.current?.click()}
                  >
                    <Upload className="size-4" />
                    Importar CSV
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={downloadCalibrationCsvExample}
                  >
                    <Download className="size-4" />
                    Baixar exemplo
                  </Button>
                </div>
                <div className="flex gap-2 rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
                  <Info
                    className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/80"
                    aria-hidden
                  />
                  <p>
                    Colunas do CSV:{" "}
                    <code className="rounded-sm bg-background/80 px-1 py-0.5 font-mono text-[0.7rem] text-foreground">
                      height_cm,volume_m3
                    </code>
                    . Para tabelas grandes, use importação em vez de editar
                    linha a linha.
                  </p>
                </div>
              </>
            ) : null}
          </div>

          <div className="flex shrink-0 flex-wrap items-center gap-2 lg:pt-0.5">
            <div className="flex min-w-0 flex-1 items-center gap-2 sm:flex-initial">
              <Label
                htmlFor="calibration-height-band"
                className="shrink-0 text-xs text-muted-foreground"
              >
                Intervalo
              </Label>
              <Select
                items={heightBandSelectItems}
                value={effectiveBandKey}
                disabled={points.length === 0}
                onValueChange={(value) => {
                  if (typeof value === "string") {
                    setSelectedBandKey(value);
                  }
                }}
              >
                <SelectTrigger
                  id="calibration-height-band"
                  size="sm"
                  className="w-full min-w-40 sm:w-48"
                >
                  <SelectValue placeholder="Filtrar altura" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL_HEIGHT_BANDS_KEY}>Todos</SelectItem>
                  {heightBands.map((band) => {
                    const key = heightBandKey(band.from, band.to);
                    return (
                      <SelectItem key={key} value={key}>
                        {band.label}
                      </SelectItem>
                    );
                  })}
                </SelectContent>
              </Select>
            </div>
            <Badge
              variant="secondary"
              className="h-8 px-2.5 font-normal tabular-nums"
            >
              {filteredPoints.length}/{points.length} pontos
            </Badge>
          </div>
        </div>
      </div>

      <div
        className={cn(
          "grid shrink-0 gap-2 border-b bg-muted/30 px-4 py-2.5 text-xs font-medium tracking-wide text-muted-foreground uppercase",
          gridCols,
        )}
      >
        <span>Altura (cm)</span>
        <span>Volume (m³)</span>
        <span>Diferencial (Δ)</span>
        {canManage ? <span className="sr-only">Ações</span> : null}
      </div>

      <ScrollArea className="min-h-0 max-h-86 flex-1 overflow-hidden">
        {filteredPoints.length === 0 ? (
          <p className="px-4 py-10 text-center text-sm text-muted-foreground">
            {points.length === 0
              ? "Nenhum ponto. Importe um CSV ou adicione linhas."
              : "Nenhum ponto neste intervalo de altura."}
          </p>
        ) : (
          <div className="w-full">
            {filteredPoints.map((point, index) => {
              const previous = filteredPoints[index - 1] ?? null;
              return (
                <div
                  key={point.id}
                  className={cn(
                    "grid w-full gap-2 border-b px-4 py-1.5 transition-colors hover:bg-muted/25",
                    gridCols,
                  )}
                >
                  <Input
                    type="number"
                    step="1"
                    min={1}
                    className="h-8"
                    value={point.height_cm}
                    disabled={!canManage}
                    onChange={(e) => {
                      const value = Number(e.target.value);
                      updatePoint(point.id, {
                        height_cm: Number.isFinite(value) ? value : 1,
                      });
                    }}
                  />
                  <Input
                    type="number"
                    step="0.001"
                    min={0.001}
                    className="h-8"
                    value={point.volume_m3}
                    disabled={!canManage}
                    onChange={(e) => {
                      const value = Number(e.target.value);
                      updatePoint(point.id, {
                        volume_m3: Number.isFinite(value) ? value : 0.001,
                      });
                    }}
                  />
                  <div className="flex h-8 items-center text-sm text-muted-foreground tabular-nums">
                    {formatDelta(point.volume_m3, previous?.volume_m3 ?? null)}
                  </div>
                  {canManage ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="size-8"
                      aria-label="Remover ponto"
                      onClick={() => removePoint(point.id)}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}
      </ScrollArea>

      {canManage ? (
        <div className="flex shrink-0 items-center justify-between gap-2 border-t px-4 py-2">
          <Button type="button" size="sm" variant="outline" onClick={addRow}>
            <Plus className="size-4" />
            Novo ponto (+1 cm)
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={isSaving || points.length === 0}
            onClick={handleSave}
          >
            {isSaving ? (
              "Salvando…"
            ) : (
              <>
                <Save className="size-4" />
                Salvar tabela
              </>
            )}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
