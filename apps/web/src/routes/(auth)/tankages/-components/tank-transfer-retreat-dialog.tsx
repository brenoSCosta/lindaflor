import type { TankTransferOutput } from "@lindaflor/shared/schemas/tankage/transfers";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TankTransferRetreatForm } from "@/routes/(auth)/tankages/-components/tank-transfer-retreat-form";

interface TankTransferRetreatDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  transfer: TankTransferOutput | null;
  day: string;
}

export function TankTransferRetreatDialog({
  open,
  onOpenChange,
  transfer,
  day,
}: TankTransferRetreatDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[92vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Retratamento · Transferência</DialogTitle>
          <DialogDescription>
            Corrija a transferência no boletim aprovado. Informe a
            justificativa; o boletim permanece aprovado.
          </DialogDescription>
        </DialogHeader>
        {open && transfer != null ? (
          <TankTransferRetreatForm
            key={transfer.id}
            transfer={transfer}
            day={day}
            onSubmitted={() => {
              onOpenChange(false);
            }}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
