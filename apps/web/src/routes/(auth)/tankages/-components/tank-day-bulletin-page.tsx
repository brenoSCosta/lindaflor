import { MAX_TANKAGE_MEASUREMENTS_PER_DAY } from "@lindaflor/shared/constants/tankage";
import { dayKeyToCalendarDate } from "@lindaflor/shared/lib/zoned-datetime";
import type { TankDayBulletinOutput } from "@lindaflor/shared/schemas/tankage/day-bulletins";
import type { TankageOutput } from "@lindaflor/shared/schemas/tankage/tankages";
import type { TankSnapshotOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import type { TankTransferOutput } from "@lindaflor/shared/schemas/tankage/transfers";
import { useQuery } from "@tanstack/react-query";
import { formatInTimeZone } from "date-fns-tz";
import { Loader2, Lock } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Time } from "@/components/ui/time";
import { useTimezone } from "@/context/timezone";
import { orpc } from "@/lib/orpc";
import { formatVolumeM3 } from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { TankAssetMap } from "@/routes/(auth)/tankages/-components/tank-asset-map";
import { TankBulletinBreadcrumbs } from "@/routes/(auth)/tankages/-components/tank-breadcrumbs";
import { TankDayBulletinActions } from "@/routes/(auth)/tankages/-components/tank-day-bulletin-actions";
import { TankDayBulletinTrace } from "@/routes/(auth)/tankages/-components/tank-day-bulletin-trace";
import { TankDayDetailTable } from "@/routes/(auth)/tankages/-components/tank-day-detail-table";
import { TankSnapshotCard } from "@/routes/(auth)/tankages/-components/tank-snapshot-card";
import { TankagesPageShell } from "@/routes/(auth)/tankages/-components/tankages-page-shell";

interface TankDayBulletinPageProps {
  tankId: string;
  date: string;
}

export function TankDayBulletinPage({
  tankId,
  date,
}: TankDayBulletinPageProps) {
  const { timezone } = useTimezone();

  const { data: snapshot, isPending: snapshotPending } = useQuery({
    ...orpc.tanks.v1.tank.get.snapshot.queryOptions({
      input: { id: tankId },
    }),
  });

  const { data: bulletin, isPending: bulletinPending } = useQuery({
    ...orpc.tanks.v1.bulletin.getBy.day.queryOptions({
      input: { tank_id: tankId, operational_day: date },
    }),
  });

  const { data: eventsResult, isPending: eventsPending } = useQuery({
    ...orpc.tanks.v1.bulletin.event.listBy.day.queryOptions({
      input: { tank_id: tankId, operational_day: date },
    }),
  });
  const events = eventsResult?.data ?? [];

  const { data: rows = [], isPending: rowsPending } = useQuery({
    ...orpc.tanks.v1.tankage.listBy.tank.queryOptions({
      input: { tank_id: tankId, measured_on: date },
      select: (result) => result.data,
    }),
  });

  const { data: transfers = [] } = useQuery({
    ...orpc.tanks.v1.transfer.listBy.tank.queryOptions({
      input: { tank_id: tankId, operational_day: date },
      select: (result) => result.data,
    }),
  });

  const existing = rows[0] ?? null;
  const dayAnchor =
    existing?.measured_at ?? dayKeyToCalendarDate(date, timezone);
  const isApproved = bulletin?.status === "approved";
  const transferTankageIds = new Set(transfers.map((row) => row.tankage_id));
  const transfersByTankageId = new Map(
    transfers.map((transfer) => [transfer.tankage_id, transfer]),
  );
  const productionCount = rows.filter(
    (row) => !transferTankageIds.has(row.id),
  ).length;
  const latestOilTemperatureC =
    rows.find((row) => row.oil_temperature_c != null)?.oil_temperature_c ??
    null;
  const stockM3 =
    snapshot?.current_net_oil_volume_m3_20c ??
    snapshot?.current_volume_m3 ??
    null;

  if (snapshotPending || bulletinPending || !snapshot || !bulletin) {
    return (
      <div className="flex justify-center py-16 text-muted-foreground">
        <Loader2 className="size-5 animate-spin" />
      </div>
    );
  }

  return (
    <TankagesPageShell className="max-w-7xl gap-5 sm:gap-6">
      <div className="flex flex-col gap-3">
        <TankBulletinBreadcrumbs
          tankId={tankId}
          tag={snapshot.tag}
          dayLabel={formatInTimeZone(dayAnchor, timezone, "dd/MM/yyyy")}
        />
        <BulletinMobileHeader
          tag={snapshot.tag}
          isApproved={isApproved}
          stockM3={stockM3}
          capacityVolumeM3={snapshot.capacity_volume_m3}
        />
        <BulletinDesktopHeader
          tag={snapshot.tag}
          concessionName={snapshot.concession_name}
          installationName={snapshot.installation_name}
          dayAnchor={dayAnchor}
          isApproved={isApproved}
        />
      </div>

      <TankSnapshotCard
        snapshot={snapshot}
        layout="dashboard"
        latestOilTemperatureC={latestOilTemperatureC}
      />

      <div className="grid gap-4 lg:grid-cols-12 lg:items-start">
        <section className="min-w-0 lg:col-span-8">
          <BulletinDetailSection
            tankId={tankId}
            operationalDay={date}
            bulletin={bulletin}
            snapshot={snapshot}
            productionCount={productionCount}
            dayRows={rows}
            rowsPending={rowsPending}
            transferTankageIds={transferTankageIds}
            transfersByTankageId={transfersByTankageId}
            isApproved={isApproved}
          />
        </section>

        <aside className="flex flex-col gap-4 lg:sticky lg:top-4 lg:col-span-4">
          <TankAssetMap
            latitude={snapshot.latitude}
            longitude={snapshot.longitude}
            tag={snapshot.tag}
            className="order-1"
          />

          <TankDayBulletinTrace
            bulletin={bulletin}
            events={events}
            isPending={eventsPending}
          />
        </aside>
      </div>
    </TankagesPageShell>
  );
}

function BulletinMobileHeader({
  tag,
  isApproved,
  stockM3,
  capacityVolumeM3,
}: {
  tag: string;
  isApproved: boolean;
  stockM3: number | null;
  capacityVolumeM3: number | null;
}) {
  return (
    <div className="flex items-start justify-between gap-3 lg:hidden">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight">{tag}</h1>
        <div className="mt-1 flex flex-wrap items-center gap-2">
          <Badge
            variant={isApproved ? "default" : "secondary"}
            className="gap-1.5 font-normal"
          >
            <span
              className={
                isApproved
                  ? "size-1.5 rounded-full bg-primary-foreground"
                  : "size-1.5 animate-pulse rounded-full bg-success"
              }
              aria-hidden
            />
            {isApproved ? "Aprovado" : "Monitorando"}
          </Badge>
          {isApproved ? (
            <Badge variant="outline" className="gap-1 font-normal">
              <Lock className="size-3" aria-hidden />
              Bloqueado
            </Badge>
          ) : null}
        </div>
      </div>
      <div className="shrink-0 text-right">
        <p className="text-[10px] font-bold tracking-wider text-muted-foreground uppercase">
          Estoque / Cap
        </p>
        <p className="font-mono text-sm tabular-nums">
          <span className="font-semibold text-primary">
            {stockM3 != null ? formatVolumeM3(stockM3).replace(" m³", "") : "—"}
          </span>
          <span className="text-muted-foreground">
            /
            {capacityVolumeM3 != null
              ? formatVolumeM3(capacityVolumeM3).replace(" m³", "")
              : "—"}
          </span>
          <span className="ml-0.5 text-[10px] text-muted-foreground">m³</span>
        </p>
      </div>
    </div>
  );
}

function BulletinDesktopHeader({
  tag,
  concessionName,
  installationName,
  dayAnchor,
  isApproved,
}: {
  tag: string;
  concessionName: string;
  installationName: string;
  dayAnchor: Date;
  isApproved: boolean;
}) {
  return (
    <div className="hidden flex-wrap items-center gap-3 lg:flex">
      <div className="min-w-0 flex-1">
        <h1 className="text-2xl font-semibold tracking-tight md:text-3xl">
          Boletim · {tag}
        </h1>
        <p className="text-sm text-muted-foreground">
          {concessionName} · {installationName} ·{" "}
          <Time date={dayAnchor} formatStr="dd/MM/yyyy" />
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={isApproved ? "default" : "secondary"}>
          {isApproved ? "Aprovado" : "Aberto para edição"}
        </Badge>
        {isApproved ? (
          <Badge variant="outline" className="gap-1 font-normal">
            <Lock className="size-3" aria-hidden />
            Medições bloqueadas
          </Badge>
        ) : null}
      </div>
    </div>
  );
}

function BulletinDetailSection({
  tankId,
  operationalDay,
  bulletin,
  snapshot,
  productionCount,
  dayRows,
  rowsPending,
  transferTankageIds,
  transfersByTankageId,
  isApproved,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
  snapshot: TankSnapshotOutput;
  productionCount: number;
  dayRows: TankageOutput[];
  rowsPending: boolean;
  transferTankageIds: ReadonlySet<string>;
  transfersByTankageId: ReadonlyMap<string, TankTransferOutput>;
  isApproved: boolean;
}) {
  return (
    <Card className="@container/card gap-0 overflow-hidden rounded-md py-0 shadow-sm">
      <CardHeader className="gap-3 py-3 sm:flex-row sm:items-center">
        <CardTitle className="text-base font-semibold sm:text-lg">
          Detalhamento
        </CardTitle>
        <Badge
          variant="secondary"
          className="@sm/card:hidden font-normal tabular-nums"
        >
          {productionCount}/{MAX_TANKAGE_MEASUREMENTS_PER_DAY}
        </Badge>

        <CardAction className="flex max-w-full flex-wrap items-center gap-2">
          <TankDayBulletinActions
            tankId={tankId}
            operationalDay={operationalDay}
            bulletin={bulletin}
            productionCount={productionCount}
            dayRows={dayRows}
            capacityHeightM={snapshot.capacity_height_m}
            fallbackPreviousHeightM={snapshot.current_height_m}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="p-0">
        {rowsPending ? (
          <div className="flex justify-center py-10 text-muted-foreground">
            <Loader2 className="size-5 animate-spin" />
          </div>
        ) : (
          <TankDayDetailTable
            tankId={tankId}
            operationalDay={operationalDay}
            rows={dayRows}
            isLoading={false}
            transferTankageIds={transferTankageIds}
            transfersByTankageId={transfersByTankageId}
            capacityHeightM={snapshot.capacity_height_m}
            bulletinApproved={isApproved}
          />
        )}
      </CardContent>
    </Card>
  );
}
