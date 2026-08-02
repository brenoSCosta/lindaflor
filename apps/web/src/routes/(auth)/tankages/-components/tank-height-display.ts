import type { TankSnapshotOutput } from "@lindaflor/shared/schemas/tankage/tanks";

export function formatTankHeightM(heightM: number): string {
  return `${heightM.toFixed(3)} m`;
}

type TankHeightSnapshot = Pick<
  TankSnapshotOutput,
  "current_height_m" | "capacity_height_m"
>;

/** `capacity_height_m` = maior altura (m) da tabela de arqueação vigente (max height_cm ÷ 100). */
export function formatTankHeightAtualMax(snapshot: TankHeightSnapshot): string {
  const currentText =
    snapshot.current_height_m != null
      ? formatTankHeightM(snapshot.current_height_m)
      : "—";
  const maxText =
    snapshot.capacity_height_m != null
      ? formatTankHeightM(snapshot.capacity_height_m)
      : "—";
  return `Atual ${currentText} · Máx. arqueação ${maxText}`;
}

export function formatTankHeightAtualMaxColumn(
  currentM: number,
  maxM: number | null | undefined,
): string {
  if (maxM != null) {
    return `${currentM.toFixed(3)} / ${maxM.toFixed(3)}`;
  }
  return `${currentM.toFixed(3)} / —`;
}

export function formatCalibrationMaxHeightHint(
  maxHeightM: number | null | undefined,
): string | undefined {
  if (maxHeightM == null) {
    return undefined;
  }
  return `Máx. arqueação: ${formatTankHeightM(maxHeightM)}`;
}
