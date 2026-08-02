import type { TankCalibrationListItem } from "@lindaflor/shared/schemas/tankage/calibrations";
import type { VariantProps } from "class-variance-authority";

import { badgeVariants } from "@/components/ui/badge";

export type CalibrationDisplayBadge = {
  label: string;
  className: string;
  variant: VariantProps<typeof badgeVariants>["variant"];
};

export function calibrationStatus(
  item: TankCalibrationListItem,
  today: string,
): "current" | "expired" | "future" {
  if (item.valid_from > today) return "future";
  if (item.is_expired) return "expired";
  return "current";
}

function daysUntil(dateIso: string, today: string): number {
  const msPerDay = 24 * 60 * 60 * 1000;
  const end = new Date(`${dateIso}T00:00:00`);
  const start = new Date(`${today}T00:00:00`);
  return Math.ceil((end.getTime() - start.getTime()) / msPerDay);
}

export function formatValidityRange(
  validFrom: string,
  validUntil: string | null,
): string {
  return `${validFrom} → ${validUntil ?? "sem fim"}`;
}

export function formatVolumeM3(value: number): string {
  return `${value.toLocaleString("pt-BR", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 3,
  })} m³`;
}

export function formatVolumeOilBarrels(value: number): string {
  return `${value.toLocaleString("pt-BR", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 3,
  })} barris`;
}

/** Numeric cell for tankage detalhamento (pt-BR, fixed fraction digits). */
export function formatMeasurementNumber(
  value: number | null | undefined,
  fractionDigits: number,
): string {
  if (value == null) {
    return "—";
  }
  return value.toLocaleString("pt-BR", {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
}

export function formatStockM3OilBarrels(
  ambientM3: number | null,
  netOilM3_20c: number | null,
  oilBarrels: number | null,
): string {
  const displayM3 = netOilM3_20c ?? ambientM3;
  if (displayM3 == null) {
    return "—";
  }
  const m3Part = formatVolumeM3(displayM3);
  if (oilBarrels == null) {
    return `${m3Part} / —`;
  }
  return `${m3Part} / ${formatVolumeOilBarrels(oilBarrels)}`;
}

export function certificateDisplayBadge(
  item: TankCalibrationListItem,
  today: string,
): CalibrationDisplayBadge {
  const status = calibrationStatus(item, today);

  if (status === "expired") {
    return {
      label: "Expirado",
      variant: "destructive",
      className: "",
    };
  }

  if (status === "future") {
    return {
      label: "Futuro",
      variant: "secondary",
      className: "",
    };
  }

  if (item.valid_until != null) {
    const days = daysUntil(item.valid_until, today);
    if (days >= 0 && days <= 30) {
      return {
        label: `Expira em ${days} dia${days === 1 ? "" : "s"}`,
        variant: "outline",
        className: "border-warning/40 bg-warning/10 text-warning",
      };
    }
  }

  return {
    label: "Ativo",
    variant: "outline",
    className: "border-success/40 bg-success/10 text-success",
  };
}
