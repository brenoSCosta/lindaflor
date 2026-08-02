import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type { TankOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import React from "react";

import { Card, CardContent } from "@/components/ui/card";
import { useAppAbility } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { todayIsoDate } from "@/lib/utils";
import { CalibrationMetaForm } from "@/routes/(auth)/arqueacao/-components/calibration-meta-form";
import {
  type CalibrationEditablePoint,
  toEditablePoints,
} from "@/routes/(auth)/arqueacao/-components/calibration-points";
import { CalibrationPointsEditor } from "@/routes/(auth)/arqueacao/-components/calibration-points-editor";
import {
  calibrationStatus,
  certificateDisplayBadge,
} from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { CertificateHistorySidebar } from "@/routes/(auth)/arqueacao/-components/certificate-history-sidebar";
import { CreateCertificateForm } from "@/routes/(auth)/arqueacao/-components/create-certificate-form";
import { TankHeroCard } from "@/routes/(auth)/arqueacao/-components/tank-hero-card";

interface TankCalibrationPanelProps {
  tank: TankOutput;
}

export function TankCalibrationPanel({ tank }: TankCalibrationPanelProps) {
  const ability = useAppAbility();
  const today = todayIsoDate();
  const [creating, setCreating] = React.useState(false);
  const [draftPoints, setDraftPoints] = React.useState<
    CalibrationEditablePoint[]
  >([]);
  const [pointsSourceId, setPointsSourceId] = React.useState<string | null>(
    null,
  );
  const [selectedId, setSelectedId] = React.useState<string | null>(null);

  const { data: list = [], isPending: listPending } = useQuery({
    ...orpc.tanks.v1.calibration.listBy.tank.queryOptions({
      input: { tank_id: tank.id },
      select: (result) => result.data,
    }),
  });

  const currentId =
    list.find((item) => calibrationStatus(item, today) === "current")?.id ??
    null;
  const effectiveSelectedId = creating ? null : (selectedId ?? currentId);

  const { data: detail, isPending: detailPending } = useQuery({
    ...orpc.tanks.v1.calibration.getBy.id.queryOptions({
      input: { id: effectiveSelectedId ?? "" },
    }),
    enabled: effectiveSelectedId != null,
    placeholderData: (previous) => previous,
  });

  // Load draft only when switching certificate — never after a local save.
  if (
    detail != null &&
    detail.id === effectiveSelectedId &&
    detail.id !== pointsSourceId
  ) {
    setPointsSourceId(detail.id);
    setDraftPoints(toEditablePoints(detail.points));
  }

  if (effectiveSelectedId == null && pointsSourceId != null) {
    setPointsSourceId(null);
    setDraftPoints([]);
  }

  // Pin auto-selected "vigente" so refetch does not jump selection.
  if (!creating && selectedId == null && currentId != null) {
    setSelectedId(currentId);
  }

  const selectCertificate = (id: string) => {
    setCreating(false);
    setSelectedId(id);
    setPointsSourceId(null);
    setDraftPoints([]);
  };

  const startCreate = () => {
    setCreating(true);
    setSelectedId(null);
    setPointsSourceId(null);
    setDraftPoints([]);
  };

  const handleCreated = (id: string) => {
    setCreating(false);
    setSelectedId(id);
    setPointsSourceId(null);
    setDraftPoints([]);
  };

  const handleDeleted = () => {
    setSelectedId(null);
    setPointsSourceId(null);
    setDraftPoints([]);
  };

  const currentItem =
    list.find((item) => item.id === currentId) ??
    list.find((item) => calibrationStatus(item, today) === "current") ??
    null;

  const capacityPoints =
    detail != null && detail.id === effectiveSelectedId ? draftPoints : [];
  const maxVolume =
    capacityPoints.length > 0
      ? Math.max(...capacityPoints.map((p) => p.volume_m3))
      : null;

  const heroStatus = currentItem
    ? certificateDisplayBadge(currentItem, today)
    : null;

  const calibrationSubject =
    detail != null && detail.id === effectiveSelectedId
      ? subject("TankCalibrations", detail)
      : null;
  const canEdit =
    calibrationSubject != null
      ? ability.can("update", calibrationSubject)
      : false;
  const canDelete =
    calibrationSubject != null
      ? ability.can("delete", calibrationSubject)
      : false;
  const isExpiredReadOnly = detail != null && detail.is_expired && !canEdit;

  return (
    <div className="flex flex-col gap-4">
      <TankHeroCard tank={tank} maxVolume={maxVolume} heroStatus={heroStatus} />

      <div className="grid h-[min(72dvh,52rem)] flex-1 gap-4 overflow-hidden md:grid-cols-[280px_1fr]">
        <CertificateHistorySidebar
          list={list}
          listPending={listPending}
          selectedId={effectiveSelectedId}
          creating={creating}
          onSelect={selectCertificate}
          onCreate={startCreate}
        />

        <div className="flex h-full min-h-0 flex-col gap-4 overflow-hidden">
          {creating ? (
            <CreateCertificateForm
              tankId={tank.id}
              onCreated={handleCreated}
              onCancel={() => setCreating(false)}
            />
          ) : effectiveSelectedId == null ? (
            <Card>
              <CardContent className="py-10 text-center text-sm text-muted-foreground">
                Selecione um certificado ou crie um novo.
              </CardContent>
            </Card>
          ) : detailPending ? (
            <div className="flex flex-1 items-center justify-center">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : detail == null ? (
            <Card>
              <CardContent className="py-10 text-center text-sm text-destructive">
                Certificado não encontrado
              </CardContent>
            </Card>
          ) : (
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden">
              <CalibrationMetaForm
                key={detail.id}
                detail={detail}
                canManage={canEdit}
                canDelete={canDelete}
                readOnlyHint={isExpiredReadOnly}
                tankId={tank.id}
                onDeleted={handleDeleted}
              />

              <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                <CalibrationPointsEditor
                  calibrationId={detail.id}
                  tankId={tank.id}
                  points={draftPoints}
                  canManage={canEdit}
                  readOnlyHint={isExpiredReadOnly}
                  onChange={setDraftPoints}
                />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
