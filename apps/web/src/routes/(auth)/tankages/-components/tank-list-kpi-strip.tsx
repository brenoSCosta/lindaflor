import type { TankSnapshotOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { AlertTriangle, Cylinder, Droplets, TrendingUp } from "lucide-react";
import React from "react";

import { formatVolumeM3 } from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { TankKpiCard } from "@/routes/(auth)/tankages/-components/tank-kpi-card";

function aggregateTankListKpis(tanks: TankSnapshotOutput[]) {
  let stockM3 = 0;
  let productionTodayM3 = 0;
  let capacityM3 = 0;
  let expiredCount = 0;
  let hasStock = false;
  let hasProduction = false;
  let hasCapacity = false;

  for (const tank of tanks) {
    if (tank.current_volume_m3 != null) {
      stockM3 += tank.current_volume_m3;
      hasStock = true;
    }
    if (tank.today_production_gross_volume_m3 != null) {
      productionTodayM3 += tank.today_production_gross_volume_m3;
      hasProduction = true;
    }
    if (tank.capacity_volume_m3 != null) {
      capacityM3 += tank.capacity_volume_m3;
      hasCapacity = true;
    }
    if (tank.calibration_status === "expired") {
      expiredCount += 1;
    }
  }

  return {
    total: tanks.length,
    stockM3: hasStock ? stockM3 : null,
    productionTodayM3: hasProduction ? productionTodayM3 : null,
    capacityM3: hasCapacity ? capacityM3 : null,
    expiredCount,
  };
}

type TankListKpiStripProps = {
  tanks: TankSnapshotOutput[];
};

export function TankListKpiStrip({ tanks }: TankListKpiStripProps) {
  const kpis = React.useMemo(() => aggregateTankListKpis(tanks), [tanks]);

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      <TankKpiCard
        label="Total de tanques"
        value={String(kpis.total)}
        icon={Cylinder}
      />
      <TankKpiCard
        label="Capacidade total"
        value={kpis.capacityM3 != null ? formatVolumeM3(kpis.capacityM3) : "—"}
        icon={Droplets}
      />
      <TankKpiCard
        label="Estoque agregado"
        value={kpis.stockM3 != null ? formatVolumeM3(kpis.stockM3) : "—"}
        hint={
          kpis.capacityM3 != null && kpis.stockM3 != null && kpis.capacityM3 > 0
            ? `${Math.round((kpis.stockM3 / kpis.capacityM3) * 100)}% da capacidade`
            : undefined
        }
        icon={TrendingUp}
        tone="success"
      />
      <TankKpiCard
        label="Arqueação expirada"
        value={String(kpis.expiredCount)}
        hint={
          kpis.productionTodayM3 != null
            ? `Produção hoje: ${formatVolumeM3(kpis.productionTodayM3)}`
            : "Produção hoje: —"
        }
        icon={AlertTriangle}
        tone={kpis.expiredCount > 0 ? "warning" : "default"}
      />
    </div>
  );
}
