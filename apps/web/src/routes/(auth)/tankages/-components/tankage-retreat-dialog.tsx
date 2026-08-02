import type { TankageOutput } from "@lindaflor/shared/schemas/tankage/tankages";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TankageRetreatForm } from "@/routes/(auth)/tankages/-components/tankage-retreat-form";

interface TankageRetreatDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tankage: TankageOutput | null;
  day: string;
  dayRows: readonly TankageOutput[];
  capacityHeightM?: number | null;
}

export function TankageRetreatDialog({
  open,
  onOpenChange,
  tankage,
  day,
  dayRows,
  capacityHeightM,
}: TankageRetreatDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[92vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Retratamento · Produção</DialogTitle>
          <DialogDescription>
            Corrija a medição no boletim aprovado. Informe a justificativa; o
            boletim permanece aprovado.
          </DialogDescription>
        </DialogHeader>
        {open && tankage != null ? (
          <TankageRetreatForm
            key={tankage.id}
            tankage={tankage}
            day={day}
            dayRows={dayRows}
            capacityHeightM={capacityHeightM}
            onSubmitted={() => {
              onOpenChange(false);
            }}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
