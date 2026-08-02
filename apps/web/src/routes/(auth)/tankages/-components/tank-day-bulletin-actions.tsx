import { MAX_TANKAGE_MEASUREMENTS_PER_DAY } from "@lindaflor/shared/constants/tankage";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type { TankDayBulletinOutput } from "@lindaflor/shared/schemas/tankage/day-bulletins";
import type { TankageOutput } from "@lindaflor/shared/schemas/tankage/tankages";
import { useMutation } from "@tanstack/react-query";
import {
  ArrowRightLeft,
  CheckCircle,
  Gauge,
  Trash2,
  Unlock,
} from "lucide-react";
import React from "react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Can, useAppAbility } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { invalidateTankDay } from "@/routes/(auth)/tankages/-components/invalidate-tank-day";
import { TankTransferDialog } from "@/routes/(auth)/tankages/-components/tank-transfer-dialog";
import { TankageEntryDialog } from "@/routes/(auth)/tankages/-components/tankage-entry-dialog";

interface TankDayBulletinActionsProps {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
  productionCount: number;
  dayRows: TankageOutput[];
  capacityHeightM: number | null;
  fallbackPreviousHeightM: number | null;
}

export function TankDayBulletinActions({
  tankId,
  operationalDay,
  bulletin,
  productionCount,
  dayRows,
  capacityHeightM,
  fallbackPreviousHeightM,
}: TankDayBulletinActionsProps) {
  const atDayLimit = productionCount >= MAX_TANKAGE_MEASUREMENTS_PER_DAY;

  return (
    <div className="flex items-center justify-center gap-2">
      <Badge
        variant="secondary"
        className="hidden @sm/card:block font-normal tabular-nums"
      >
        {productionCount}/{MAX_TANKAGE_MEASUREMENTS_PER_DAY}
      </Badge>
      <ProductionAction
        tankId={tankId}
        operationalDay={operationalDay}
        bulletin={bulletin}
        dayRows={dayRows}
        capacityHeightM={capacityHeightM}
        fallbackPreviousHeightM={fallbackPreviousHeightM}
        atDayLimit={atDayLimit}
      />
      <TransferAction
        tankId={tankId}
        operationalDay={operationalDay}
        bulletin={bulletin}
      />
      <DeleteDayAction
        tankId={tankId}
        operationalDay={operationalDay}
        bulletin={bulletin}
      />
      <ApproveBulletinAction
        tankId={tankId}
        operationalDay={operationalDay}
        bulletin={bulletin}
      />
      <ReopenBulletinAction
        tankId={tankId}
        operationalDay={operationalDay}
        bulletin={bulletin}
      />
    </div>
  );
}

function measurementAbilitySubject(bulletin: TankDayBulletinOutput) {
  return {
    organization_id: bulletin.organization_id,
    bulletin_status: bulletin.status,
  };
}

function ProductionAction({
  tankId,
  operationalDay,
  bulletin,
  dayRows,
  capacityHeightM,
  fallbackPreviousHeightM,
  atDayLimit,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
  dayRows: TankageOutput[];
  capacityHeightM: number | null;
  fallbackPreviousHeightM: number | null;
  atDayLimit: boolean;
}) {
  const [open, setOpen] = React.useState(false);
  const ability = useAppAbility();
  const tankageSubject = subject(
    "Tankages",
    measurementAbilitySubject(bulletin),
  );
  const canMutate = ability.can("create", tankageSubject);
  const canAdd = canMutate && !atDayLimit;

  const openProduction = () => {
    if (!canMutate) return;
    setOpen(true);
  };

  if (atDayLimit) {
    return null;
  }

  return (
    <Can I="create" this={tankageSubject}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={!canAdd}
              aria-label="Nova medição de produção"
              onClick={openProduction}
            />
          }
        >
          <Gauge className="size-4" />
        </TooltipTrigger>
        <TooltipContent>
          {atDayLimit
            ? `Limite de ${MAX_TANKAGE_MEASUREMENTS_PER_DAY} medições`
            : "Produção"}
        </TooltipContent>
      </Tooltip>
      <TankageEntryDialog
        open={open}
        onOpenChange={setOpen}
        tankId={tankId}
        day={operationalDay}
        dayRows={dayRows}
        capacityHeightM={capacityHeightM}
        fallbackPreviousHeightM={fallbackPreviousHeightM}
      />
    </Can>
  );
}

function TransferAction({
  tankId,
  operationalDay,
  bulletin,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
}) {
  const [open, setOpen] = React.useState(false);
  const transferSubject = subject(
    "TankTransfers",
    measurementAbilitySubject(bulletin),
  );

  return (
    <Can I="create" this={transferSubject}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              size="icon"
              variant="outline"
              aria-label="Nova transferência"
              onClick={() => setOpen(true)}
            />
          }
        >
          <ArrowRightLeft className="size-4" />
        </TooltipTrigger>
        <TooltipContent>Transferência</TooltipContent>
      </Tooltip>
      <TankTransferDialog
        open={open}
        onOpenChange={setOpen}
        tankId={tankId}
        day={operationalDay}
      />
    </Can>
  );
}

function DeleteDayAction({
  tankId,
  operationalDay,
  bulletin,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
}) {
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const bulletinSubject = subject("TankDayBulletins", bulletin);

  const deleteDayMutation = useMutation(
    orpc.tanks.v1.bulletin.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Operações do dia excluídas");
        setConfirmOpen(false);
        await invalidateTankDay({ tankId, operationalDay });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <Can I="delete" this={bulletinSubject}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={deleteDayMutation.isPending}
              aria-label="Excluir operações do dia"
              onClick={() => setConfirmOpen(true)}
            />
          }
        >
          <Trash2 className="size-4 text-destructive" />
        </TooltipTrigger>
        <TooltipContent>Excluir Operações do Dia</TooltipContent>
      </Tooltip>
      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Excluir operações do dia?</AlertDialogTitle>
            <AlertDialogDescription>
              Todas as medições e transferências deste dia serão removidas
              permanentemente. Esta ação não pode ser desfeita.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancelar</AlertDialogCancel>
            <AlertDialogAction
              onClick={() =>
                deleteDayMutation.mutate({
                  tank_id: tankId,
                  operational_day: operationalDay,
                })
              }
            >
              Excluir
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Can>
  );
}

function ApproveBulletinAction({
  tankId,
  operationalDay,
  bulletin,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
}) {
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const bulletinSubject = subject("TankDayBulletins", bulletin);

  const approveMutation = useMutation(
    orpc.tanks.v1.bulletin.approve.mutationOptions({
      onSuccess: async () => {
        toast.success("Boletim aprovado");
        setConfirmOpen(false);
        await invalidateTankDay({ tankId, operationalDay });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <Can I="approve" this={bulletinSubject}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={approveMutation.isPending}
              aria-label="Aprovar boletim"
              onClick={() => setConfirmOpen(true)}
            />
          }
        >
          <CheckCircle className="size-4 text-success" />
        </TooltipTrigger>
        <TooltipContent>Aprovar</TooltipContent>
      </Tooltip>
      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Aprovar boletim?</AlertDialogTitle>
            <AlertDialogDescription>
              Após a aprovação, as medições deste dia não poderão mais ser
              alteradas até uma reabertura administrativa.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancelar</AlertDialogCancel>
            <AlertDialogAction
              onClick={() =>
                approveMutation.mutate({
                  tank_id: tankId,
                  operational_day: operationalDay,
                })
              }
            >
              Aprovar
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Can>
  );
}

function ReopenBulletinAction({
  tankId,
  operationalDay,
  bulletin,
}: {
  tankId: string;
  operationalDay: string;
  bulletin: TankDayBulletinOutput;
}) {
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const bulletinSubject = subject("TankDayBulletins", bulletin);

  const reopenMutation = useMutation(
    orpc.tanks.v1.bulletin.reopen.mutationOptions({
      onSuccess: async () => {
        toast.success("Boletim reaberto para edição");
        setConfirmOpen(false);
        await invalidateTankDay({ tankId, operationalDay });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <Can I="reopen" this={bulletinSubject}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={reopenMutation.isPending}
              aria-label="Reabrir boletim"
              onClick={() => setConfirmOpen(true)}
            />
          }
        >
          <Unlock className="size-4" />
        </TooltipTrigger>
        <TooltipContent>Reabrir (Adm)</TooltipContent>
      </Tooltip>
      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Reabrir boletim?</AlertDialogTitle>
            <AlertDialogDescription>
              O boletim voltará a ficar aberto para edição das medições do dia.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancelar</AlertDialogCancel>
            <AlertDialogAction
              onClick={() =>
                reopenMutation.mutate({
                  tank_id: tankId,
                  operational_day: operationalDay,
                })
              }
            >
              Reabrir
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Can>
  );
}
